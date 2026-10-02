#import <UIKit/UIKit.h>
#import <WebKit/WebKit.h>
#import <ReplayKit/ReplayKit.h>
#import <AVFoundation/AVFoundation.h>
#import <TargetConditionals.h>
#import <sys/socket.h>
#import <netinet/in.h>
#import <unistd.h>
#import "CaptureSession.h"

static BOOL MTReadAll(int fd, void *buffer, size_t size) {
    uint8_t *bytes = buffer;
    while (size) {
        ssize_t count = recv(fd, bytes, size, 0);
        if (count <= 0) return NO;
        bytes += count; size -= count;
    }
    return YES;
}

@interface MTAudioBridge : NSObject <WKScriptMessageHandler>
@property(nonatomic, weak) WKWebView *webview;
@property(nonatomic) int listener;
@property(nonatomic) int connection;
@property(nonatomic) NSUInteger generation;
@property(nonatomic) NSUInteger pendingFrames;
@property(nonatomic, strong) RPSystemBroadcastPickerView *picker;
@property(nonatomic) BOOL callActive;
@end

@implementation MTAudioBridge
- (instancetype)init {
    if ((self = [super init])) {
        _listener = -1; _connection = -1;
        [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(foregroundChanged:) name:UIApplicationDidEnterBackgroundNotification object:nil];
        [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(foregroundChanged:) name:UIApplicationDidBecomeActiveNotification object:nil];
        [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(routeChanged:) name:AVAudioSessionRouteChangeNotification object:nil];
    }
    return self;
}
- (void)foregroundChanged:(NSNotification *)notification {
    if (self.listener < 0) return;
    NSMutableDictionary *session = [MTReadSession() mutableCopy];
    if (!session) return;
    session[@"hostForeground"] = @([UIApplication sharedApplication].applicationState != UIApplicationStateBackground);
    MTWriteSession(session);
}
- (void)dealloc { [[NSNotificationCenter defaultCenter] removeObserver:self]; }
- (void)emit:(NSString *)script generation:(NSUInteger)generation {
    dispatch_async(dispatch_get_main_queue(), ^{
        if (self.generation == generation) [self.webview evaluateJavaScript:script completionHandler:nil];
    });
}
- (void)error:(NSString *)message generation:(NSUInteger)generation {
    NSData *json = [NSJSONSerialization dataWithJSONObject:@[message] options:0 error:nil];
    NSString *argument = [[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding];
    [self emit:[NSString stringWithFormat:@"window.musictalkNative?.error(%@[0])", argument] generation:generation];
}
- (void)stop {
    self.generation++;
    @synchronized(self) {
        if (self.connection >= 0) { shutdown(self.connection, SHUT_RDWR); self.connection = -1; }
        if (self.listener >= 0) { shutdown(self.listener, SHUT_RDWR); close(self.listener); self.listener = -1; }
    }
    MTDeleteSession();
    [self.picker removeFromSuperview]; self.picker = nil;
}
- (void)callAudio {
    self.callActive = YES;
    AVAudioSession *session = [AVAudioSession sharedInstance];
    NSError *error = nil;
    [session setCategory:AVAudioSessionCategoryPlayAndRecord
                   mode:AVAudioSessionModeVoiceChat
                options:AVAudioSessionCategoryOptionMixWithOthers | AVAudioSessionCategoryOptionAllowBluetooth
                  error:&error];
    [session setActive:YES error:&error];
    if (error) [self error:error.localizedDescription generation:self.generation];
    [self publishRoutes];
}
- (void)routeChanged:(NSNotification *)notification {
    dispatch_async(dispatch_get_main_queue(), ^{
        if (!self.callActive) return;
        // WebKit may replace the session category when it starts its microphone.
        // Restore mixing so that opening the call does not silence the music app.
        if (!([AVAudioSession sharedInstance].categoryOptions & AVAudioSessionCategoryOptionMixWithOthers)) [self callAudio];
        else [self publishRoutes];
    });
}
- (void)publishRoutes {
    AVAudioSession *session = [AVAudioSession sharedInstance];
    NSMutableArray *routes = [@[@{@"id":@"auto",@"label":@"Automatic"},@{@"id":@"receiver",@"label":@"iPhone"},@{@"id":@"speaker",@"label":@"Speaker"}] mutableCopy];
    NSString *selected = @"auto";
    AVAudioSessionPortDescription *output = session.currentRoute.outputs.firstObject;
    if ([output.portType isEqualToString:AVAudioSessionPortBuiltInSpeaker]) selected = @"speaker";
    else if ([output.portType isEqualToString:AVAudioSessionPortBuiltInReceiver]) selected = @"receiver";
    for (AVAudioSessionPortDescription *input in session.availableInputs) {
        if ([input.portType isEqualToString:AVAudioSessionPortBuiltInMic]) continue;
        [routes addObject:@{@"id":input.UID,@"label":input.portName}];
        if ([session.currentRoute.inputs.firstObject.UID isEqualToString:input.UID]) selected = input.UID;
    }
    NSData *json = [NSJSONSerialization dataWithJSONObject:@{@"routes":routes,@"selected":selected} options:0 error:nil];
    [self emit:[NSString stringWithFormat:@"window.musictalk?.audioRoutes(%@)",[[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding]] generation:self.generation];
}
- (void)selectRoute:(NSString *)identifier {
    AVAudioSession *session = [AVAudioSession sharedInstance];
    NSError *error = nil;
    AVAudioSessionPortDescription *input = nil;
    for (AVAudioSessionPortDescription *candidate in session.availableInputs) {
        if ([candidate.UID isEqualToString:identifier] || ([identifier isEqualToString:@"receiver"] && [candidate.portType isEqualToString:AVAudioSessionPortBuiltInMic])) { input = candidate; break; }
    }
    if (!input && ![@[@"auto",@"receiver",@"speaker"] containsObject:identifier]) {
        [self error:@"That audio device is no longer connected." generation:self.generation]; return;
    }
    [session overrideOutputAudioPort:AVAudioSessionPortOverrideNone error:&error];
    if (!error) [session setPreferredInput:input error:&error];
    if (!error && [identifier isEqualToString:@"speaker"]) [session overrideOutputAudioPort:AVAudioSessionPortOverrideSpeaker error:&error];
    if (error) [self error:error.localizedDescription generation:self.generation];
    [self publishRoutes];
}
- (void)endCall {
    self.callActive = NO;
    AVAudioSession *session = [AVAudioSession sharedInstance];
    [session overrideOutputAudioPort:AVAudioSessionPortOverrideNone error:nil];
    [session setPreferredInput:nil error:nil];
    [session setActive:NO withOptions:AVAudioSessionSetActiveOptionNotifyOthersOnDeactivation error:nil];
}
- (void)start {
    [self stop]; [self callAudio];
    NSUInteger generation = self.generation;
    int listener = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in address = {0};
    address.sin_len = sizeof(address); address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK); address.sin_port = 0;
    socklen_t length = sizeof(address);
    if (listener < 0 || bind(listener, (struct sockaddr *)&address, sizeof(address)) || listen(listener, 1) || getsockname(listener, (struct sockaddr *)&address, &length)) {
        if (listener >= 0) close(listener);
        [self error:@"Could not prepare iPhone audio capture." generation:generation]; return;
    }
    self.listener = listener;
    uint8_t secret[32];
    if (SecRandomCopyBytes(kSecRandomDefault, sizeof(secret), secret) != errSecSuccess) {
        [self stop]; [self error:@"Could not prepare the capture session." generation:self.generation]; return;
    }
    NSData *token = [NSData dataWithBytes:secret length:sizeof(secret)];
    if (MTWriteSession(@{@"port":@(ntohs(address.sin_port)), @"token":[token base64EncodedStringWithOptions:0], @"hostForeground":@YES}) != errSecSuccess) {
        [self stop]; [self error:@"The iPhone capture extension needs Keychain Sharing signing enabled." generation:self.generation]; return;
    }
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        int fd = -1;
        // Authenticate before any captured sound crosses the local process boundary.
        while (self.generation == generation) {
            fd = accept(listener, NULL, NULL);
            if (fd < 0) break;
            struct timeval timeout = {3, 0};
            setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
            uint8_t received[32];
            if (MTReadAll(fd, received, sizeof(received)) && [token isEqualToData:[NSData dataWithBytes:received length:sizeof(received)]]) break;
            close(fd); fd = -1;
        }
        if (fd < 0) return;
        @synchronized(self) {
            if (self.generation != generation) { close(fd); return; }
            self.connection = fd;
        }
        struct timeval timeout = {0, 0};
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
        [self emit:@"window.musictalkNative?.started()" generation:generation];
        while (self.generation == generation) {
            uint32_t size = 0;
            if (!MTReadAll(fd, &size, sizeof(size))) break;
            size = ntohl(size);
            if (!size || size > 65536 || size % 4) break;
            NSMutableData *pcm = [NSMutableData dataWithLength:size];
            if (!MTReadAll(fd, pcm.mutableBytes, size)) break;
            NSString *encoded = [pcm base64EncodedStringWithOptions:0];
            dispatch_async(dispatch_get_main_queue(), ^{
                if (self.generation != generation || self.pendingFrames >= 8) return;
                self.pendingFrames++;
                [self.webview evaluateJavaScript:[NSString stringWithFormat:@"window.musictalkNative?.push('%@')", encoded] completionHandler:^(id result, NSError *error) { if (self.pendingFrames) self.pendingFrames--; }];
            });
        }
        @synchronized(self) { if (self.connection == fd) self.connection = -1; }
        close(fd);
        [self emit:@"window.musictalkNative?.stopped()" generation:generation];
    });
    // Apple owns the confirmation UI. No recording begins until Start Broadcast is confirmed.
    self.picker = [[RPSystemBroadcastPickerView alloc] initWithFrame:CGRectMake(0, 0, 44, 44)];
    self.picker.preferredExtension = [[[NSBundle mainBundle] bundleIdentifier] stringByAppendingString:@".broadcast"];
    self.picker.showsMicrophoneButton = NO; self.picker.hidden = YES;
    [self.webview addSubview:self.picker];
    for (UIView *view in self.picker.subviews) {
        if ([view isKindOfClass:[UIButton class]]) [(UIButton *)view sendActionsForControlEvents:UIControlEventTouchUpInside];
    }
}
- (void)userContentController:(WKUserContentController *)controller didReceiveScriptMessage:(WKScriptMessage *)message {
    if (![message.body isKindOfClass:[NSDictionary class]] || !message.frameInfo.isMainFrame) return;
    NSString *action = message.body[@"action"];
    if ([action isEqualToString:@"start"]) [self start];
    else if ([action isEqualToString:@"stop"]) [self stop];
    else if ([action isEqualToString:@"callAudio"]) [self callAudio];
    else if ([action isEqualToString:@"endCall"]) [self endCall];
    else if ([action isEqualToString:@"routes"]) [self publishRoutes];
    else if ([action isEqualToString:@"route"]) [self selectRoute:message.body[@"id"]];
    else if ([action isEqualToString:@"copy"]) [UIPasteboard generalPasteboard].string = message.body[@"text"];
}
@end

void musictalk_ios_install(void *rawWebview) {
    WKWebView *webview = (__bridge WKWebView *)rawWebview;
    static MTAudioBridge *bridge;
    if (bridge.webview == webview) return;
    [bridge stop];
    bridge = [MTAudioBridge new]; bridge.webview = webview;
    WKUserContentController *manager = webview.configuration.userContentController;
    [manager removeScriptMessageHandlerForName:@"MusicTalkAudio"];
    [manager addScriptMessageHandler:bridge name:@"MusicTalkAudio"];
    BOOL available = !TARGET_OS_SIMULATOR && [[NSFileManager defaultManager] fileExistsAtPath:[[[NSBundle mainBundle] builtInPlugInsPath] stringByAppendingPathComponent:@"MusicTalkBroadcast.appex"]];
    NSString *script = [NSString stringWithFormat:
        @"window.MusicTalkAudio={available:%@,start(){webkit.messageHandlers.MusicTalkAudio.postMessage({action:'start'})},stop(){webkit.messageHandlers.MusicTalkAudio.postMessage({action:'stop'})},callAudio(){webkit.messageHandlers.MusicTalkAudio.postMessage({action:'callAudio'})},endCall(){webkit.messageHandlers.MusicTalkAudio.postMessage({action:'endCall'})},routes(){webkit.messageHandlers.MusicTalkAudio.postMessage({action:'routes'})},route(id){webkit.messageHandlers.MusicTalkAudio.postMessage({action:'route',id})},copy(text){webkit.messageHandlers.MusicTalkAudio.postMessage({action:'copy',text})}};", available ? @"true" : @"false"];
    [webview evaluateJavaScript:script completionHandler:nil];
}
