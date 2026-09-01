#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>

@interface IMDBWebKitFetchDelegate : NSObject <NSApplicationDelegate, WKNavigationDelegate>
@property(nonatomic, strong) WKWebView *webView;
@property(nonatomic, strong) NSWindow *window;
@property(nonatomic, copy) NSURL *url;
@property(nonatomic, copy) NSString *outputPath;
@property(nonatomic) NSTimeInterval timeout;
@property(nonatomic, strong) NSDate *deadline;
@property(nonatomic, copy) NSString *bestHTML;
@property(nonatomic) BOOL finished;
@end

@implementation IMDBWebKitFetchDelegate

- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    (void)notification;
    WKWebViewConfiguration *configuration = [[WKWebViewConfiguration alloc] init];
    // The default store is intentional: IMDb cookies survive helper launches
    // and are shared by this app identity instead of requiring Chrome state.
    configuration.websiteDataStore = WKWebsiteDataStore.defaultDataStore;
    configuration.applicationNameForUserAgent = @"IMDbTechManagerFetcher/4.0.0";

    self.webView = [[WKWebView alloc] initWithFrame:NSMakeRect(0, 0, 1280, 2200)
                                      configuration:configuration];
    self.webView.navigationDelegate = self;
    self.window = [[NSWindow alloc] initWithContentRect:NSMakeRect(-20000, -20000, 1280, 2200)
                                               styleMask:NSWindowStyleMaskBorderless
                                                 backing:NSBackingStoreBuffered
                                                   defer:NO];
    self.window.contentView = self.webView;
    self.window.ignoresMouseEvents = YES;
    self.deadline = [NSDate dateWithTimeIntervalSinceNow:self.timeout];

    NSMutableURLRequest *request = [NSMutableURLRequest requestWithURL:self.url
                                                            cachePolicy:NSURLRequestReloadIgnoringLocalCacheData
                                                        timeoutInterval:self.timeout];
    [request setValue:@"en-US,en;q=0.9" forHTTPHeaderField:@"Accept-Language"];
    [request setValue:@"text/html,application/xhtml+xml" forHTTPHeaderField:@"Accept"];
    [self.webView loadRequest:request];
    [self scheduleCapture:0.75];
    [self scheduleDeadline];
}

- (void)scheduleCapture:(NSTimeInterval)delay {
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(delay * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        [self captureDOM];
    });
}

- (void)scheduleDeadline {
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(self.timeout * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        if (!self.finished) [self finishWithHTML:self.bestHTML ?: @"" code:3 reason:@"webkit-timeout"];
    });
}

- (BOOL)isFullIMDbDOM:(NSString *)html {
    if (html.length == 0) return NO;
    if ([html containsString:@"__NEXT_DATA__"] &&
        ([html containsString:@"technicalSpecifications"] || [html containsString:@"Technical specifications"])) return YES;
    return html.length >= 20000 && [[html lowercaseString] containsString:@"imdb"];
}

- (BOOL)isChallenge:(NSString *)html {
    NSString *lower = html.lowercaseString;
    for (NSString *marker in @[@"awswafcookiedomainlist", @"awswafintegration", @"challenge-container",
                               @"verify that you're not a robot", @"token.awswaf.com"]) {
        if ([lower containsString:marker]) return YES;
    }
    return NO;
}

- (void)captureDOM {
    if (self.finished) return;
    [self.webView evaluateJavaScript:@"document.documentElement ? document.documentElement.outerHTML : ''"
                  completionHandler:^(id value, NSError *error) {
        if (self.finished) return;
        NSString *html = [value isKindOfClass:NSString.class] ? value : @"";
        if (html.length > self.bestHTML.length) self.bestHTML = html;
        if (!error && [self isFullIMDbDOM:html] && ![self isChallenge:html]) {
            [self finishWithHTML:html code:0 reason:@"webkit-dom"];
            return;
        }
        if ([self.deadline timeIntervalSinceNow] > 0) [self scheduleCapture:0.75];
    }];
}

- (void)finishWithHTML:(NSString *)html code:(int)code reason:(NSString *)reason {
    if (self.finished) return;
    self.finished = YES;
    NSError *error = nil;
    BOOL wrote = [html writeToFile:self.outputPath atomically:YES encoding:NSUTF8StringEncoding error:&error];
    if (!wrote) {
        fprintf(stderr, "webkit-write-error: %s\n", error.localizedDescription.UTF8String);
        code = 4;
    } else {
        fprintf(stderr, "%s:%lu\n", reason.UTF8String, (unsigned long)html.length);
    }
    [NSApp terminate:nil];
    exit(code);
}

- (void)webView:(WKWebView *)webView didFinishNavigation:(WKNavigation *)navigation {
    (void)webView;
    (void)navigation;
    [self captureDOM];
}

- (void)webView:(WKWebView *)webView
didFailProvisionalNavigation:(WKNavigation *)navigation
       withError:(NSError *)error {
    (void)webView;
    (void)navigation;
    fprintf(stderr, "webkit-navigation-error: %s\n", error.localizedDescription.UTF8String);
    [self finishWithHTML:self.bestHTML ?: @"" code:2 reason:@"webkit-navigation-error"];
}

@end

static void printUsage(void) {
    fprintf(stderr, "usage: IMDbWebKitFetcher --url https://www.imdb.com/... --output PATH [--timeout SECONDS]\n");
}

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        NSString *urlString = nil;
        NSString *output = nil;
        NSTimeInterval timeout = 35;
        for (int i = 1; i < argc; i++) {
            NSString *arg = [NSString stringWithUTF8String:argv[i]];
            if ([arg isEqualToString:@"--url"] && i + 1 < argc) urlString = [NSString stringWithUTF8String:argv[++i]];
            else if ([arg isEqualToString:@"--output"] && i + 1 < argc) output = [NSString stringWithUTF8String:argv[++i]];
            else if ([arg isEqualToString:@"--timeout"] && i + 1 < argc) timeout = MAX(5, atof(argv[++i]));
            else { printUsage(); return 64; }
        }
        NSURL *url = [NSURL URLWithString:urlString ?: @""];
        NSString *host = url.host.lowercaseString;
        BOOL hostOK = [host isEqualToString:@"www.imdb.com"] || [host isEqualToString:@"m.imdb.com"];
        if (!url || ![url.scheme.lowercaseString isEqualToString:@"https"] || !hostOK || output.length == 0) {
            printUsage();
            return 64;
        }

        NSApplication *application = NSApplication.sharedApplication;
        IMDBWebKitFetchDelegate *delegate = [[IMDBWebKitFetchDelegate alloc] init];
        delegate.url = url;
        delegate.outputPath = output;
        delegate.timeout = timeout;
        application.delegate = delegate;
        [application setActivationPolicy:NSApplicationActivationPolicyProhibited];
        [application run];
    }
    return 0;
}
