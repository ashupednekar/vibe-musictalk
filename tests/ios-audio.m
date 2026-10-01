#import <Foundation/Foundation.h>
#import <sys/socket.h>
#import <poll.h>
#import <math.h>
#import "../native/ios/Broadcast/SampleHandler.m"

static void check(BOOL condition, NSString *message) {
    if (!condition) { fprintf(stderr,"FAIL %s\n", message.UTF8String); exit(1); }
}
static CMSampleBufferRef makeSample(double rate, BOOL planar) {
    AVAudioFormat *format = [[AVAudioFormat alloc] initWithCommonFormat:AVAudioPCMFormatFloat32 sampleRate:rate channels:planar ? 2 : 1 interleaved:!planar];
    AVAudioPCMBuffer *pcm = [[AVAudioPCMBuffer alloc] initWithPCMFormat:format frameCapacity:480];
    pcm.frameLength = 480;
    for (int c=0;c<(planar ? 2 : 1);c++) for (int i=0;i<480;i++) pcm.floatChannelData[c][i] = 0.4f*sin(2*M_PI*440*i/rate);
    CMAudioFormatDescriptionRef description = NULL;
    check(CMAudioFormatDescriptionCreate(NULL,format.streamDescription,0,NULL,0,NULL,NULL,&description)==noErr,@"create audio format");
    CMSampleBufferRef sample = NULL;
    CMSampleTimingInfo timing={CMTimeMake(1,(int32_t)rate),kCMTimeZero,kCMTimeInvalid};
    check(CMSampleBufferCreate(NULL,NULL,false,NULL,NULL,description,480,1,&timing,0,NULL,&sample)==noErr,@"create sample");
    check(CMSampleBufferSetDataBufferFromAudioBufferList(sample,NULL,NULL,0,pcm.audioBufferList)==noErr,@"attach samples");
    check(CMSampleBufferSetDataReady(sample)==noErr,@"sample ready");
    CFRelease(description);return sample;
}
int main(void) { @autoreleasepool {
    for (int planar=0;planar<2;planar++) {
        int sockets[2];check(socketpair(AF_UNIX,SOCK_STREAM,0,sockets)==0,@"local capture transport");
        SampleHandler *handler=[SampleHandler new];
        handler.connection=sockets[0];handler.lastSessionRead=CFAbsoluteTimeGetCurrent()+60;handler.hostForeground=NO;
        handler.outputFormat=[[AVAudioFormat alloc] initWithCommonFormat:AVAudioPCMFormatInt16 sampleRate:48000 channels:2 interleaved:YES];
        CMSampleBufferRef sample=makeSample(planar ? 44100 : 24000,planar);
        [handler sendAudio:sample];
        struct pollfd ready={sockets[1],POLLIN,0};check(poll(&ready,1,1000)>0,@"converted audio delivered");
        uint32_t prefix;check(recv(sockets[1],&prefix,4,MSG_WAITALL)==4,@"PCM framing");uint32_t bytes=ntohl(prefix);
        check(bytes>100&&bytes<65536&&bytes%4==0,@"stereo PCM bounds");
        int16_t *pcm=malloc(bytes);check(recv(sockets[1],pcm,bytes,MSG_WAITALL)==bytes,@"complete audio frame");
        int peak=0;for(int i=0;i<bytes/2;i++)peak=MAX(peak,abs(pcm[i]));check(peak>3000,@"audio samples are nonzero");
        for(int i=0;i<bytes/2;i+=2)check(abs(pcm[i]-pcm[i+1])<8,@"mono/planar stereo channels preserved");free(pcm);
        handler.hostForeground=YES;[handler sendAudio:sample];ready.revents=0;check(poll(&ready,1,20)==0,@"MusicTalk playback excluded to avoid echo");
        [handler processSampleBuffer:sample withType:RPSampleBufferTypeVideo];
        [handler processSampleBuffer:sample withType:RPSampleBufferTypeAudioMic];
        ready.revents=0;check(poll(&ready,1,20)==0,@"video and duplicate microphone discarded");
        CFRelease(sample);close(sockets[0]);close(sockets[1]);handler.connection=-1;
        printf("PASS iPhone capture converts %s to framed stereo 48 kHz PCM and excludes video, mic, and app echo\n",planar ? "44.1 kHz planar stereo" : "24 kHz mono");
    }
 }return 0;}
