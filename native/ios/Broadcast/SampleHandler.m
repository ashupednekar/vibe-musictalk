#import <ReplayKit/ReplayKit.h>
#import <AVFoundation/AVFoundation.h>
#import <CoreMedia/CoreMedia.h>
#import <sys/socket.h>
#import <netinet/in.h>
#import <unistd.h>
#import <stdatomic.h>
#import "../CaptureSession.h"

static BOOL MTWriteAll(int fd, const void *buffer, size_t size) {
    const uint8_t *bytes = buffer;
    while (size) {
        ssize_t count = send(fd, bytes, size, 0);
        if (count <= 0) return NO;
        bytes += count; size -= count;
    }
    return YES;
}

@interface SampleHandler : RPBroadcastSampleHandler {
    atomic_int _pending;
    atomic_bool _finished;
}
@property(nonatomic) int connection;
@property(nonatomic, strong) AVAudioConverter *converter;
@property(nonatomic, strong) AVAudioFormat *outputFormat;
@property(nonatomic, strong) dispatch_queue_t audioQueue;
@property(nonatomic) CFAbsoluteTime lastSessionRead;
@property(nonatomic) BOOL hostForeground;
@end

@implementation SampleHandler
- (void)broadcastStartedWithSetupInfo:(NSDictionary *)setupInfo {
    self.connection = -1; atomic_store(&_pending, 0); atomic_store(&_finished, false);
    self.audioQueue = dispatch_queue_create("dev.musictalk.capture", DISPATCH_QUEUE_SERIAL);
    self.outputFormat = [[AVAudioFormat alloc] initWithCommonFormat:AVAudioPCMFormatInt16 sampleRate:48000 channels:2 interleaved:YES];
    NSDictionary *session = MTReadSession();
    NSData *token = [[NSData alloc] initWithBase64EncodedString:session[@"token"] ?: @"" options:0];
    if (token.length != 32 || !session[@"port"]) { [self fail:@"Open MusicTalk and join a call before starting the broadcast."]; return; }
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    int noSignal = 1;
    setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &noSignal, sizeof(noSignal));
    struct timeval timeout = {0, 500000};
    setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout));
    struct sockaddr_in address = {0};
    address.sin_len = sizeof(address); address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK); address.sin_port = htons([session[@"port"] unsignedShortValue]);
    if (fd < 0 || connect(fd, (struct sockaddr *)&address, sizeof(address)) || !MTWriteAll(fd, token.bytes, token.length)) {
        if (fd >= 0) close(fd);
        [self fail:@"Could not reach the MusicTalk call. Join the call again."]; return;
    }
    self.connection = fd; self.hostForeground = YES;
    // The host closes the authenticated local connection when the call ends.
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_UTILITY, 0), ^{
        char command;
        recv(fd, &command, 1, 0);
        [self fail:@"The MusicTalk call ended."];
    });
}
- (void)fail:(NSString *)message {
    if (atomic_exchange(&_finished, true)) return;
    [self finishBroadcastWithError:[NSError errorWithDomain:@"MusicTalk" code:1 userInfo:@{NSLocalizedDescriptionKey:message}]];
}
- (void)broadcastFinished {
    atomic_store(&_finished, true);
    if (self.connection >= 0) { shutdown(self.connection, SHUT_RDWR); close(self.connection); self.connection = -1; }
}
- (void)broadcastPaused { self.converter = nil; }
- (void)broadcastResumed { self.converter = nil; }
- (void)processSampleBuffer:(CMSampleBufferRef)sample withType:(RPSampleBufferType)type {
    // Discard video and ReplayKit's microphone immediately. WebRTC owns the microphone.
    if (type != RPSampleBufferTypeAudioApp || atomic_load(&_finished)) return;
    if (atomic_fetch_add(&_pending, 1) >= 4) { atomic_fetch_sub(&_pending, 1); return; }
    CFRetain(sample);
    dispatch_async(self.audioQueue, ^{
        @autoreleasepool { [self sendAudio:sample]; }
        CFRelease(sample); atomic_fetch_sub(&self->_pending, 1);
    });
}
- (void)sendAudio:(CMSampleBufferRef)sample {
    if (atomic_load(&_finished) || self.connection < 0) return;
    CFAbsoluteTime now = CFAbsoluteTimeGetCurrent();
    if (now - self.lastSessionRead > 0.25) {
        NSDictionary *session = MTReadSession();
        if (!session) { [self fail:@"The MusicTalk call ended."]; return; }
        self.hostForeground = [session[@"hostForeground"] boolValue]; self.lastSessionRead = now;
    }
    // Never re-send MusicTalk's received voice/music. Capture only when another app is foreground.
    if (self.hostForeground) return;
    CMAudioFormatDescriptionRef description = CMSampleBufferGetFormatDescription(sample);
    const AudioStreamBasicDescription *asbd = CMAudioFormatDescriptionGetStreamBasicDescription(description);
    if (!asbd || asbd->mFormatID != kAudioFormatLinearPCM) return;
    AVAudioFormat *format = [[AVAudioFormat alloc] initWithStreamDescription:asbd];
    AVAudioFrameCount frames = (AVAudioFrameCount)CMSampleBufferGetNumSamples(sample);
    if (!format || !frames) return;
    size_t size = 0;
    CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(sample, &size, NULL, 0, NULL, NULL, 0, NULL);
    AudioBufferList *buffers = malloc(size);
    CMBlockBufferRef retained = NULL;
    OSStatus status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(sample, NULL, buffers, size, NULL, NULL, kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment, &retained);
    if (status != noErr) { free(buffers); return; }
    AVAudioPCMBuffer *input = [[AVAudioPCMBuffer alloc] initWithPCMFormat:format frameCapacity:frames];
    input.frameLength = frames;
    AudioBufferList *destination = input.mutableAudioBufferList;
    BOOL valid = destination->mNumberBuffers == buffers->mNumberBuffers;
    for (UInt32 i = 0; valid && i < buffers->mNumberBuffers; i++) {
        if (buffers->mBuffers[i].mDataByteSize > destination->mBuffers[i].mDataByteSize) { valid = NO; break; }
        memcpy(destination->mBuffers[i].mData, buffers->mBuffers[i].mData, buffers->mBuffers[i].mDataByteSize);
    }
    free(buffers); if (retained) CFRelease(retained);
    if (!valid) return;
    if (!self.converter || ![self.converter.inputFormat isEqual:format]) {
        self.converter = [[AVAudioConverter alloc] initFromFormat:format toFormat:self.outputFormat];
    }
    AVAudioFrameCount capacity = (AVAudioFrameCount)ceil(frames * 48000.0 / format.sampleRate) + 64;
    AVAudioPCMBuffer *output = [[AVAudioPCMBuffer alloc] initWithPCMFormat:self.outputFormat frameCapacity:capacity];
    __block BOOL supplied = NO;
    NSError *error = nil;
    [self.converter convertToBuffer:output error:&error withInputFromBlock:^AVAudioBuffer *(AVAudioPacketCount count, AVAudioConverterInputStatus *status) {
        if (supplied) { *status = AVAudioConverterInputStatus_NoDataNow; return nil; }
        supplied = YES; *status = AVAudioConverterInputStatus_HaveData; return input;
    }];
    if (error || !output.frameLength) return;
    uint32_t bytes = output.frameLength * 4;
    if (bytes > 65536) return;
    uint32_t prefix = htonl(bytes);
    if (!MTWriteAll(self.connection, &prefix, sizeof(prefix)) || !MTWriteAll(self.connection, output.int16ChannelData[0], bytes)) {
        [self fail:@"The audio connection ended."];
    }
}
@end
