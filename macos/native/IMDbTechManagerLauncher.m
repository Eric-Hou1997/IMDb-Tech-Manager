#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>
#include <math.h>
#include <signal.h>

static NSString * const IMDBHandshakePrefix = @"IMDB_TECH_MANAGER_UI_URL=";
static NSString * const IMDBWindowFrameDefaultsKey = @"IMDBMainWindowFrameV1";

@interface IMDBAppDelegate : NSObject <NSApplicationDelegate, NSWindowDelegate, WKNavigationDelegate, WKUIDelegate, WKScriptMessageHandler>
@property(nonatomic, strong) NSWindow *window;
@property(nonatomic, strong) WKWebView *webView;
@property(nonatomic, strong) NSTask *core;
@property(nonatomic, strong) NSMutableData *stdoutBuffer;
@property(nonatomic, copy) NSString *allowedOrigin;
@property(nonatomic, copy) NSURL *serviceURL;
@property(nonatomic) BOOL didLoadUI;
@property(nonatomic) BOOL terminating;
@property(nonatomic) BOOL loginStartup;
@property(nonatomic) BOOL windowTransitioningFullScreen;
@property(nonatomic) BOOL windowEverVisible;
@end

@implementation IMDBAppDelegate

- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    (void)notification;
    self.stdoutBuffer = [NSMutableData data];
    self.loginStartup = [NSProcessInfo.processInfo.arguments containsObject:@"--login-startup"];
    [self installMainMenu];
    [self startCore];
}

- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender {
    (void)sender;
    return YES;
}

- (BOOL)applicationShouldHandleReopen:(NSApplication *)sender hasVisibleWindows:(BOOL)hasVisibleWindows {
    (void)hasVisibleWindows;
    self.windowEverVisible = YES;
    [self.window makeKeyAndOrderFront:nil];
    [sender activateIgnoringOtherApps:YES];
    return YES;
}

- (void)applicationWillTerminate:(NSNotification *)notification {
    (void)notification;
    self.terminating = YES;
    [self saveWindowFrameIfEligible];
    [self.webView.configuration.userContentController removeScriptMessageHandlerForName:@"clipboard"];
    BOOL graceful = [self requestGracefulCoreQuit];
    if (self.core.running && !graceful) {
        [self.core terminate];
    }
}

- (BOOL)requestGracefulCoreQuit {
    if (!self.serviceURL || !self.core.running) return NO;
    NSURLComponents *components = [NSURLComponents componentsWithURL:self.serviceURL resolvingAgainstBaseURL:NO];
    NSString *token = @"";
    for (NSURLQueryItem *item in components.queryItems) {
        if ([item.name isEqualToString:@"token"]) token = item.value ?: @"";
    }
    components.path = @"/api/quit";
    components.query = nil;
    NSMutableURLRequest *request = [NSMutableURLRequest requestWithURL:components.URL
                                                           cachePolicy:NSURLRequestReloadIgnoringLocalCacheData
                                                       timeoutInterval:1.5];
    request.HTTPMethod = @"POST";
    request.HTTPBody = [@"{}" dataUsingEncoding:NSUTF8StringEncoding];
    [request setValue:@"application/json" forHTTPHeaderField:@"Content-Type"];
    [request setValue:token forHTTPHeaderField:@"X-Manager-Token"];
    dispatch_semaphore_t done = dispatch_semaphore_create(0);
    __block BOOL accepted = NO;
    NSURLSessionDataTask *task = [NSURLSession.sharedSession dataTaskWithRequest:request
                                                               completionHandler:^(__unused NSData *data, NSURLResponse *response, __unused NSError *error) {
        if ([response isKindOfClass:NSHTTPURLResponse.class]) {
            NSInteger status = ((NSHTTPURLResponse *)response).statusCode;
            accepted = status >= 200 && status < 300;
        }
        dispatch_semaphore_signal(done);
    }];
    [task resume];
    dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(1.6 * NSEC_PER_SEC)));
    return accepted;
}

- (void)installMainMenu {
    NSMenu *main = [[NSMenu alloc] initWithTitle:@""];
    NSMenuItem *appItem = [[NSMenuItem alloc] initWithTitle:@"" action:nil keyEquivalent:@""];
    [main addItem:appItem];
    NSMenu *appMenu = [[NSMenu alloc] initWithTitle:@""];
    [appMenu addItemWithTitle:@"关于 IMDb Tech Manager" action:@selector(orderFrontStandardAboutPanel:) keyEquivalent:@""];
    [appMenu addItem:NSMenuItem.separatorItem];
    [appMenu addItemWithTitle:@"退出 IMDb Tech Manager" action:@selector(terminate:) keyEquivalent:@"q"];
    appItem.submenu = appMenu;
    NSApp.mainMenu = main;
}

- (void)startCore {
    NSURL *executable = NSBundle.mainBundle.executableURL;
    NSURL *coreURL = [[executable URLByDeletingLastPathComponent] URLByAppendingPathComponent:@"IMDbTechManagerCore"];
    if (!coreURL || ![NSFileManager.defaultManager isExecutableFileAtPath:coreURL.path]) {
        [self failStartup:@"App 内缺少 IMDbTechManagerCore。"];
        return;
    }

    NSPipe *output = [NSPipe pipe];
    NSPipe *errors = [NSPipe pipe];
    NSTask *task = [[NSTask alloc] init];
    task.executableURL = coreURL;
    task.arguments = @[@"--native-ui"];
    task.standardOutput = output;
    task.standardError = errors;

    __weak typeof(self) weakSelf = self;
    output.fileHandleForReading.readabilityHandler = ^(NSFileHandle *handle) {
        NSData *data = handle.availableData;
        if (data.length == 0) return;
        dispatch_async(dispatch_get_main_queue(), ^{
            [weakSelf consumeCoreOutput:data];
        });
    };
    errors.fileHandleForReading.readabilityHandler = ^(NSFileHandle *handle) {
        NSData *data = handle.availableData;
        if (data.length == 0) return;
        NSString *message = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
        if (message.length) NSLog(@"IMDbTechManagerCore: %@", message);
    };
    task.terminationHandler = ^(NSTask *finished) {
        dispatch_async(dispatch_get_main_queue(), ^{
            typeof(self) strongSelf = weakSelf;
            if (!strongSelf || strongSelf.terminating) return;
            if (!strongSelf.didLoadUI) {
                [strongSelf failStartup:[NSString stringWithFormat:@"本地服务启动失败（退出码 %d）。", finished.terminationStatus]];
            }
            [NSApp terminate:nil];
        });
    };

    NSError *error = nil;
    if (![task launchAndReturnError:&error]) {
        [self failStartup:[NSString stringWithFormat:@"无法启动本地服务：%@", error.localizedDescription]];
        return;
    }
    self.core = task;
}

- (void)consumeCoreOutput:(NSData *)data {
    [self.stdoutBuffer appendData:data];
    const uint8_t newline = '\n';
    while (YES) {
        NSRange range = [self.stdoutBuffer rangeOfData:[NSData dataWithBytes:&newline length:1]
                                               options:0
                                                 range:NSMakeRange(0, self.stdoutBuffer.length)];
        if (range.location == NSNotFound) break;
        NSData *lineData = [self.stdoutBuffer subdataWithRange:NSMakeRange(0, range.location)];
        [self.stdoutBuffer replaceBytesInRange:NSMakeRange(0, NSMaxRange(range)) withBytes:NULL length:0];
        NSString *line = [[NSString alloc] initWithData:lineData encoding:NSUTF8StringEncoding];
        if (![line hasPrefix:IMDBHandshakePrefix]) continue;
        NSString *raw = [[line substringFromIndex:IMDBHandshakePrefix.length]
                         stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
        NSURL *url = [NSURL URLWithString:raw];
        if (![url.scheme isEqualToString:@"http"] || ![url.host isEqualToString:@"127.0.0.1"]) {
            [self failStartup:@"本地服务返回了无效地址。"];
            return;
        }
        [self showWindow:url];
    }
}

- (NSScreen *)screenForStoredFrame:(NSRect)frame {
    NSScreen *best = nil;
    CGFloat bestArea = 0;
    for (NSScreen *screen in NSScreen.screens) {
        NSRect intersection = NSIntersectionRect(frame, screen.visibleFrame);
        CGFloat area = NSIsEmptyRect(intersection) ? 0 : NSWidth(intersection) * NSHeight(intersection);
        if (area > bestArea) {
            best = screen;
            bestArea = area;
        }
    }
    // A sliver left on a disconnected or rearranged display is not useful.
    if (bestArea < 4096) return NSScreen.mainScreen ?: NSScreen.screens.firstObject;
    return best;
}

- (NSRect)constrainedStoredFrame:(NSRect)frame onScreen:(NSScreen *)screen {
    if (!screen) return frame;
    NSRect visible = screen.visibleFrame;
    CGFloat minimumWidth = MIN(900, NSWidth(visible));
    CGFloat minimumHeight = MIN(620, NSHeight(visible));
    frame.size.width = MIN(MAX(frame.size.width, minimumWidth), NSWidth(visible));
    frame.size.height = MIN(MAX(frame.size.height, minimumHeight), NSHeight(visible));
    frame.origin.x = MIN(MAX(frame.origin.x, NSMinX(visible)), NSMaxX(visible) - frame.size.width);
    frame.origin.y = MIN(MAX(frame.origin.y, NSMinY(visible)), NSMaxY(visible) - frame.size.height);
    return frame;
}

- (BOOL)loadStoredWindowFrame:(NSRect *)result {
    NSString *stored = [NSUserDefaults.standardUserDefaults stringForKey:IMDBWindowFrameDefaultsKey];
    if (stored.length == 0) return NO;
    NSRect frame = NSRectFromString(stored);
    if (!isfinite(frame.origin.x) || !isfinite(frame.origin.y) ||
        !isfinite(frame.size.width) || !isfinite(frame.size.height) ||
        frame.size.width <= 0 || frame.size.height <= 0) {
        [NSUserDefaults.standardUserDefaults removeObjectForKey:IMDBWindowFrameDefaultsKey];
        return NO;
    }
    NSScreen *screen = [self screenForStoredFrame:frame];
    if (!screen) return NO;
    if (result) *result = [self constrainedStoredFrame:frame onScreen:screen];
    return YES;
}

- (void)saveWindowFrameIfEligible {
    NSWindow *window = self.window;
    if (!window || (self.loginStartup && !self.windowEverVisible) || window.miniaturized || self.windowTransitioningFullScreen ||
        (window.styleMask & NSWindowStyleMaskFullScreen) != 0) return;
    NSRect frame = window.frame;
    if (!isfinite(frame.origin.x) || !isfinite(frame.origin.y) ||
        !isfinite(frame.size.width) || !isfinite(frame.size.height) ||
        frame.size.width <= 0 || frame.size.height <= 0) return;
    [NSUserDefaults.standardUserDefaults setObject:NSStringFromRect(frame)
                                            forKey:IMDBWindowFrameDefaultsKey];
}

- (void)showWindow:(NSURL *)url {
    if (self.didLoadUI) return;
    self.didLoadUI = YES;
    self.serviceURL = url;
    self.allowedOrigin = [self originForURL:url];

    WKUserContentController *controller = [[WKUserContentController alloc] init];
    [controller addScriptMessageHandler:self name:@"clipboard"];
    NSString *bridge = @"window.__imdbNativeCopy=function(value){window.webkit.messageHandlers.clipboard.postMessage(String(value==null?'':value));return Promise.resolve();};";
    [controller addUserScript:[[WKUserScript alloc] initWithSource:bridge
                                                   injectionTime:WKUserScriptInjectionTimeAtDocumentStart
                                                forMainFrameOnly:NO]];

    WKWebViewConfiguration *configuration = [[WKWebViewConfiguration alloc] init];
    configuration.websiteDataStore = WKWebsiteDataStore.defaultDataStore;
    configuration.userContentController = controller;
    configuration.applicationNameForUserAgent = @"IMDbTechManager/4.0.1";

    WKWebView *web = [[WKWebView alloc] initWithFrame:NSZeroRect configuration:configuration];
    web.navigationDelegate = self;
    web.UIDelegate = self;
    web.allowsBackForwardNavigationGestures = NO;

    NSWindow *window = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 1320, 860)
                                                  styleMask:(NSWindowStyleMaskTitled |
                                                             NSWindowStyleMaskClosable |
                                                             NSWindowStyleMaskMiniaturizable |
                                                             NSWindowStyleMaskResizable)
                                                    backing:NSBackingStoreBuffered
                                                      defer:NO];
    window.title = @"";
    window.titleVisibility = NSWindowTitleHidden;
    window.releasedWhenClosed = NO;
    window.minSize = NSMakeSize(900, 620);
    window.contentView = web;
    NSRect storedFrame = NSZeroRect;
    if ([self loadStoredWindowFrame:&storedFrame]) {
        [window setFrame:storedFrame display:NO];
    } else {
        [window center];
    }
    window.delegate = self;
    self.window = window;
    self.webView = web;
    if (self.loginStartup) {
        [window orderOut:nil];
    } else {
        self.windowEverVisible = YES;
        [window makeKeyAndOrderFront:nil];
    }

    NSURLRequest *request = [NSURLRequest requestWithURL:url
                                            cachePolicy:NSURLRequestReloadIgnoringLocalCacheData
                                        timeoutInterval:30];
    [web loadRequest:request];
    if (!self.loginStartup) [NSApp activateIgnoringOtherApps:YES];
}

- (void)windowDidMove:(NSNotification *)notification {
    if (notification.object == self.window) [self saveWindowFrameIfEligible];
}

- (void)windowDidEndLiveResize:(NSNotification *)notification {
    if (notification.object == self.window) [self saveWindowFrameIfEligible];
}

- (void)windowWillClose:(NSNotification *)notification {
    if (notification.object == self.window) [self saveWindowFrameIfEligible];
}

- (void)windowWillEnterFullScreen:(NSNotification *)notification {
    if (notification.object == self.window) {
        [self saveWindowFrameIfEligible];
        self.windowTransitioningFullScreen = YES;
    }
}

- (void)windowDidExitFullScreen:(NSNotification *)notification {
    if (notification.object == self.window) {
        self.windowTransitioningFullScreen = NO;
        [self saveWindowFrameIfEligible];
    }
}

- (void)windowDidFailToEnterFullScreen:(NSWindow *)window {
    if (window == self.window) self.windowTransitioningFullScreen = NO;
}

- (void)windowDidFailToExitFullScreen:(NSWindow *)window {
    if (window == self.window) self.windowTransitioningFullScreen = NO;
}

- (NSString *)originForURL:(NSURL *)url {
    if (!url.scheme || !url.host) return nil;
    NSString *port = url.port ? [NSString stringWithFormat:@":%@", url.port] : @"";
    return [NSString stringWithFormat:@"%@://%@%@", url.scheme.lowercaseString, url.host.lowercaseString, port];
}

- (BOOL)isAllowedURL:(NSURL *)url {
    return self.allowedOrigin && [[self originForURL:url] isEqualToString:self.allowedOrigin];
}

- (void)failStartup:(NSString *)message {
    if (self.terminating) return;
    NSAlert *alert = [[NSAlert alloc] init];
    alert.alertStyle = NSAlertStyleCritical;
    alert.messageText = @"IMDb Tech Manager 无法启动";
    alert.informativeText = message;
    [alert addButtonWithTitle:@"退出"];
    [alert runModal];
    [NSApp terminate:nil];
}

- (void)userContentController:(WKUserContentController *)userContentController
      didReceiveScriptMessage:(WKScriptMessage *)message {
    (void)userContentController;
    if (![message.name isEqualToString:@"clipboard"]) return;
    [NSPasteboard.generalPasteboard clearContents];
    [NSPasteboard.generalPasteboard setString:[message.body description] forType:NSPasteboardTypeString];
}

- (void)webView:(WKWebView *)webView
decidePolicyForNavigationAction:(WKNavigationAction *)navigationAction
decisionHandler:(void (^)(WKNavigationActionPolicy))decisionHandler {
    NSURL *url = navigationAction.request.URL;
    if ([self isAllowedURL:url]) {
        decisionHandler(WKNavigationActionPolicyAllow);
        return;
    }
    if ([url.scheme.lowercaseString isEqualToString:@"http"] || [url.scheme.lowercaseString isEqualToString:@"https"]) {
        [NSWorkspace.sharedWorkspace openURL:url];
    }
    decisionHandler(WKNavigationActionPolicyCancel);
}

- (nullable WKWebView *)webView:(WKWebView *)webView
createWebViewWithConfiguration:(WKWebViewConfiguration *)configuration
   forNavigationAction:(WKNavigationAction *)navigationAction
        windowFeatures:(WKWindowFeatures *)windowFeatures {
    (void)configuration;
    (void)windowFeatures;
    NSURL *url = navigationAction.request.URL;
    if ([self isAllowedURL:url]) {
        [webView loadRequest:navigationAction.request];
    } else if (url) {
        [NSWorkspace.sharedWorkspace openURL:url];
    }
    return nil;
}

- (void)webView:(WKWebView *)webView
didFailProvisionalNavigation:(WKNavigation *)navigation
       withError:(NSError *)error {
    (void)navigation;
    if ([self isAllowedURL:webView.URL ?: self.serviceURL]) {
        [self failStartup:[NSString stringWithFormat:@"本地界面载入失败：%@", error.localizedDescription]];
    }
}

- (void)webViewWebContentProcessDidTerminate:(WKWebView *)webView {
    // WebKit content-process termination is recoverable and must not strand
    // the user on a blank window while the Go service is still healthy.
    [webView reload];
}

- (void)webView:(WKWebView *)webView
runJavaScriptAlertPanelWithMessage:(NSString *)message
initiatedByFrame:(WKFrameInfo *)frame
completionHandler:(void (^)(void))completionHandler {
    (void)webView;
    (void)frame;
    NSAlert *alert = [[NSAlert alloc] init];
    alert.messageText = message;
    [alert addButtonWithTitle:@"好"];
    [alert beginSheetModalForWindow:self.window completionHandler:^(__unused NSModalResponse response) {
        completionHandler();
    }];
}

- (void)webView:(WKWebView *)webView
runJavaScriptConfirmPanelWithMessage:(NSString *)message
initiatedByFrame:(WKFrameInfo *)frame
completionHandler:(void (^)(BOOL result))completionHandler {
    (void)webView;
    (void)frame;
    NSAlert *alert = [[NSAlert alloc] init];
    alert.messageText = message;
    [alert addButtonWithTitle:@"确认"];
    [alert addButtonWithTitle:@"取消"];
    [alert beginSheetModalForWindow:self.window completionHandler:^(NSModalResponse response) {
        completionHandler(response == NSAlertFirstButtonReturn);
    }];
}

- (void)webView:(WKWebView *)webView
runJavaScriptTextInputPanelWithPrompt:(NSString *)prompt
defaultText:(nullable NSString *)defaultText
initiatedByFrame:(WKFrameInfo *)frame
completionHandler:(void (^)(NSString * _Nullable result))completionHandler {
    (void)webView;
    (void)frame;
    NSAlert *alert = [[NSAlert alloc] init];
    alert.messageText = prompt;
    NSTextField *field = [NSTextField textFieldWithString:defaultText ?: @""];
    field.frame = NSMakeRect(0, 0, 360, 24);
    alert.accessoryView = field;
    [alert addButtonWithTitle:@"确认"];
    [alert addButtonWithTitle:@"取消"];
    [alert beginSheetModalForWindow:self.window completionHandler:^(NSModalResponse response) {
        completionHandler(response == NSAlertFirstButtonReturn ? field.stringValue : nil);
    }];
}

@end

int main(int argc, const char *argv[]) {
    (void)argc;
    (void)argv;
    @autoreleasepool {
        NSApplication *application = NSApplication.sharedApplication;
        IMDBAppDelegate *delegate = [[IMDBAppDelegate alloc] init];
        application.delegate = delegate;
        [application setActivationPolicy:NSApplicationActivationPolicyRegular];
        [application run];
    }
    return 0;
}
