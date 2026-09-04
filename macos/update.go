package main

import (
	"crypto/ed25519"
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"
)

const (
	techReleaseAPI       = "https://api.github.com/repos/Eric-Hou1997/IMDb-Tech-Manager/releases/latest"
	techReleasePage      = "https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases/latest"
	techUpdatePublicKey  = "UibeR9KwHBQM0x91qLxz64G0InjT2p0r99a83hLzwo0="
	techUpdateMaxArchive = 512 << 20
	techUpdateCacheTTL   = 6 * time.Hour
	techUpdateStaleLimit = 30 * 24 * time.Hour
)

var versionPattern = regexp.MustCompile(`^v?(\d+)\.(\d+)\.(\d+)$`)

type githubReleaseAsset struct {
	Name               string `json:"name"`
	BrowserDownloadURL string `json:"browser_download_url"`
}

type githubRelease struct {
	TagName     string               `json:"tag_name"`
	HTMLURL     string               `json:"html_url"`
	Draft       bool                 `json:"draft"`
	Prerelease  bool                 `json:"prerelease"`
	PublishedAt string               `json:"published_at"`
	Assets      []githubReleaseAsset `json:"assets"`
}

type techUpdateCache struct {
	SchemaVersion int           `json:"schema_version"`
	SnapshotID    string        `json:"snapshot_id"`
	ETag          string        `json:"etag,omitempty"`
	CheckedAt     time.Time     `json:"checked_at"`
	Release       githubRelease `json:"release"`
}

type updateFailure struct {
	Code      string
	Status    int
	RetryAt   time.Time
	RequestID string
	Chinese   string
	English   string
}

func (failure *updateFailure) Error() string {
	if failure == nil {
		return ""
	}
	return currentLocalized(failure.Chinese, failure.English)
}

type techUpdateResult struct {
	Cache   techUpdateCache
	Source  string
	Warning *updateFailure
}

type techUpdateCoordinator struct {
	mu       sync.Mutex
	loaded   bool
	cache    *techUpdateCache
	inFlight chan struct{}
	retryAt  time.Time
	retryErr *updateFailure
	now      func() time.Time
	path     func() string
	apiURL   string
	pageURL  string
}

var techUpdates = newTechUpdateCoordinator()

func newTechUpdateCoordinator() *techUpdateCoordinator {
	return &techUpdateCoordinator{
		now:     time.Now,
		path:    func() string { return filepath.Join(baseDir(), "updates", "update-state.json") },
		apiURL:  techReleaseAPI,
		pageURL: techReleasePage,
	}
}

func githubUpdateClient(timeout time.Duration) *http.Client {
	return &http.Client{
		Timeout: timeout,
		CheckRedirect: func(req *http.Request, via []*http.Request) error {
			if len(via) >= 10 {
				return errors.New("GitHub 更新重定向次数过多")
			}
			host := strings.ToLower(req.URL.Hostname())
			if host != "github.com" && host != "api.github.com" && !strings.HasSuffix(host, ".githubusercontent.com") {
				return fmt.Errorf("GitHub 更新重定向到非官方主机 %s，已拒绝", host)
			}
			return nil
		},
	}
}

func updateRequest(method, rawURL string) (*http.Request, error) {
	req, err := http.NewRequest(method, rawURL, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Accept", "application/vnd.github+json")
	req.Header.Set("X-GitHub-Api-Version", "2022-11-28")
	req.Header.Set("User-Agent", "IMDb-Tech-Manager/"+appVersion)
	return req, nil
}

func updateRetryAt(resp *http.Response, now time.Time) time.Time {
	if reset, err := strconv.ParseInt(strings.TrimSpace(resp.Header.Get("X-RateLimit-Reset")), 10, 64); err == nil && reset > 0 {
		return time.Unix(reset, 0)
	}
	if value := strings.TrimSpace(resp.Header.Get("Retry-After")); value != "" {
		if seconds, err := strconv.Atoi(value); err == nil && seconds >= 0 {
			return now.Add(time.Duration(seconds) * time.Second)
		}
		if parsed, err := http.ParseTime(value); err == nil {
			return parsed
		}
	}
	return time.Time{}
}

func classifyUpdateHTTPFailure(resp *http.Response, body []byte, source string, now time.Time) *updateFailure {
	status := resp.StatusCode
	requestID := strings.TrimSpace(resp.Header.Get("X-GitHub-Request-Id"))
	retryAt := updateRetryAt(resp, now)
	lower := strings.ToLower(string(body))
	remaining := strings.TrimSpace(resp.Header.Get("X-RateLimit-Remaining"))
	if (status == http.StatusForbidden || status == http.StatusTooManyRequests) && remaining == "0" {
		if retryAt.IsZero() {
			retryAt = now.Add(time.Minute)
		}
		return &updateFailure{Code: "github-primary-rate-limit", Status: status, RetryAt: retryAt, RequestID: requestID,
			Chinese: "GitHub 匿名 API 的出口 IP 额度已用完", English: "The GitHub anonymous API quota for this public IP has been exhausted"}
	}
	if (status == http.StatusForbidden || status == http.StatusTooManyRequests) &&
		(resp.Header.Get("Retry-After") != "" || strings.Contains(lower, "secondary rate limit") || strings.Contains(lower, "abuse detection")) {
		if retryAt.IsZero() {
			retryAt = now.Add(time.Minute)
		}
		return &updateFailure{Code: "github-secondary-rate-limit", Status: status, RetryAt: retryAt, RequestID: requestID,
			Chinese: "GitHub 触发了次级限流，请按提示时间后再检查", English: "GitHub applied a secondary rate limit; check again after the indicated time"}
	}
	if status == http.StatusForbidden || status == http.StatusTooManyRequests {
		if requestID == "" && !strings.Contains(lower, "api.github.com") && !strings.Contains(lower, "github") {
			return &updateFailure{Code: "proxy-forbidden", Status: status,
				Chinese: "代理或中间网络拒绝了更新请求", English: "A proxy or intermediary network rejected the update request"}
		}
		return &updateFailure{Code: "github-forbidden", Status: status, RequestID: requestID,
			Chinese: "GitHub 拒绝了更新请求，但未标明为额度耗尽", English: "GitHub rejected the update request without identifying an exhausted quota"}
	}
	if status >= 500 {
		return &updateFailure{Code: "github-unavailable", Status: status, RequestID: requestID,
			Chinese: "GitHub 更新服务暂时不可用", English: "The GitHub update service is temporarily unavailable"}
	}
	return &updateFailure{Code: "github-http-error", Status: status, RequestID: requestID,
		Chinese: fmt.Sprintf("GitHub 更新请求失败（HTTP %d）", status), English: fmt.Sprintf("The GitHub update request failed (HTTP %d)", status)}
}

func classifyUpdateNetworkFailure(err error) *updateFailure {
	lower := strings.ToLower(err.Error())
	if strings.Contains(lower, "proxy") {
		return &updateFailure{Code: "proxy-connection-failed", Chinese: "无法连接代理服务器：" + err.Error(), English: "Could not connect to the proxy server: " + err.Error()}
	}
	return &updateFailure{Code: "network-unavailable", Chinese: "无法连接 GitHub：" + err.Error(), English: "Could not connect to GitHub: " + err.Error()}
}

func readUpdateErrorBody(resp *http.Response) []byte {
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 64<<10))
	return body
}

func validateTechRelease(release githubRelease) error {
	if release.Draft || release.Prerelease || versionPattern.FindStringSubmatch(release.TagName) == nil {
		return errors.New("GitHub 最新发布不是可用的正式 vX.Y.Z 版本")
	}
	archive, signature, err := selectTechUpdateAssets(release)
	if err != nil {
		return err
	}
	if !officialTechAssetURL(archive.BrowserDownloadURL, release.TagName, archive.Name) || !officialTechAssetURL(signature.BrowserDownloadURL, release.TagName, signature.Name) {
		return errors.New("正式发布返回了非官方或不匹配的更新地址")
	}
	return nil
}

func officialTechAssetURL(rawURL, tag, name string) bool {
	parsed, err := url.Parse(rawURL)
	if err != nil || parsed.Scheme != "https" || !strings.EqualFold(parsed.Hostname(), "github.com") || parsed.RawQuery != "" || parsed.Fragment != "" {
		return false
	}
	want := "/Eric-Hou1997/IMDb-Tech-Manager/releases/download/" + url.PathEscape(tag) + "/" + url.PathEscape(name)
	return parsed.EscapedPath() == want
}

func newUpdateSnapshotID() (string, error) {
	raw := make([]byte, 18)
	if _, err := rand.Read(raw); err != nil {
		return "", err
	}
	return base64.RawURLEncoding.EncodeToString(raw), nil
}

func (coordinator *techUpdateCoordinator) loadCacheLocked() {
	if coordinator.loaded {
		return
	}
	coordinator.loaded = true
	body, err := os.ReadFile(coordinator.path())
	if err != nil {
		return
	}
	var cached techUpdateCache
	if json.Unmarshal(body, &cached) != nil || cached.SchemaVersion != 1 || cached.SnapshotID == "" || validateTechRelease(cached.Release) != nil {
		return
	}
	now := coordinator.now()
	if cached.CheckedAt.IsZero() || cached.CheckedAt.After(now.Add(5*time.Minute)) {
		return
	}
	coordinator.cache = &cached
}

func (coordinator *techUpdateCoordinator) saveCache(cache techUpdateCache) error {
	body, err := json.MarshalIndent(cache, "", "  ")
	if err != nil {
		return err
	}
	return atomicWrite(coordinator.path(), body, 0600)
}

func fetchTechReleaseAPI(client *http.Client, rawURL, etag string, now time.Time) (githubRelease, string, bool, *updateFailure) {
	req, err := updateRequest(http.MethodGet, rawURL)
	if err != nil {
		return githubRelease{}, "", false, classifyUpdateNetworkFailure(err)
	}
	if etag != "" {
		req.Header.Set("If-None-Match", etag)
	}
	resp, err := client.Do(req)
	if err != nil {
		return githubRelease{}, "", false, classifyUpdateNetworkFailure(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode == http.StatusNotModified {
		return githubRelease{}, etag, true, nil
	}
	if resp.StatusCode != http.StatusOK {
		return githubRelease{}, "", false, classifyUpdateHTTPFailure(resp, readUpdateErrorBody(resp), "api", now)
	}
	var release githubRelease
	if err := json.NewDecoder(io.LimitReader(resp.Body, 2<<20)).Decode(&release); err != nil {
		return githubRelease{}, "", false, &updateFailure{Code: "invalid-release-response", Chinese: "GitHub 更新信息无效：" + err.Error(), English: "The GitHub update response is invalid: " + err.Error()}
	}
	if err := validateTechRelease(release); err != nil {
		return githubRelease{}, "", false, &updateFailure{Code: "invalid-release", Chinese: err.Error(), English: localizeBackendText("en-US", err.Error())}
	}
	return release, strings.TrimSpace(resp.Header.Get("ETag")), false, nil
}

func githubReleaseTagFromURL(rawURL, repository string) (string, bool) {
	parsed, err := url.Parse(rawURL)
	if err != nil || !strings.EqualFold(parsed.Hostname(), "github.com") {
		return "", false
	}
	prefix := "/" + repository + "/releases/tag/"
	if !strings.HasPrefix(parsed.EscapedPath(), prefix) {
		return "", false
	}
	tag, err := url.PathUnescape(strings.TrimPrefix(parsed.EscapedPath(), prefix))
	return tag, err == nil && versionPattern.FindStringSubmatch(tag) != nil
}

func probeUpdateAsset(client *http.Client, rawURL string, now time.Time) *updateFailure {
	req, err := updateRequest(http.MethodHead, rawURL)
	if err != nil {
		return classifyUpdateNetworkFailure(err)
	}
	resp, err := client.Do(req)
	if err != nil {
		return classifyUpdateNetworkFailure(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode == http.StatusOK || resp.StatusCode == http.StatusPartialContent {
		return nil
	}
	return classifyUpdateHTTPFailure(resp, readUpdateErrorBody(resp), "asset", now)
}

func discoverTechReleaseViaPage(client *http.Client, pageURL string, now time.Time) (githubRelease, *updateFailure) {
	req, err := updateRequest(http.MethodGet, pageURL)
	if err != nil {
		return githubRelease{}, classifyUpdateNetworkFailure(err)
	}
	req.Header.Set("Accept", "text/html")
	resp, err := client.Do(req)
	if err != nil {
		return githubRelease{}, classifyUpdateNetworkFailure(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return githubRelease{}, classifyUpdateHTTPFailure(resp, readUpdateErrorBody(resp), "release-page", now)
	}
	_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 2<<20))
	tag, ok := githubReleaseTagFromURL(resp.Request.URL.String(), "Eric-Hou1997/IMDb-Tech-Manager")
	if !ok {
		return githubRelease{}, &updateFailure{Code: "invalid-release-redirect", Chinese: "GitHub 最新发布页面没有返回有效的正式版本", English: "The GitHub latest-release page did not return a valid stable version"}
	}
	archiveName, _ := techUpdateArchiveName(tag)
	base := "https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases/download/" + url.PathEscape(tag) + "/"
	release := githubRelease{TagName: tag, HTMLURL: resp.Request.URL.String(), Assets: []githubReleaseAsset{
		{Name: archiveName, BrowserDownloadURL: base + url.PathEscape(archiveName)},
		{Name: archiveName + ".sig", BrowserDownloadURL: base + url.PathEscape(archiveName+".sig")},
	}}
	for _, asset := range release.Assets {
		if failure := probeUpdateAsset(client, asset.BrowserDownloadURL, now); failure != nil {
			return githubRelease{}, failure
		}
	}
	return release, nil
}

func (coordinator *techUpdateCoordinator) result(force bool) (techUpdateResult, *updateFailure) {
	for {
		coordinator.mu.Lock()
		coordinator.loadCacheLocked()
		now := coordinator.now()
		if !force && coordinator.cache != nil && now.Sub(coordinator.cache.CheckedAt) < techUpdateCacheTTL {
			result := techUpdateResult{Cache: *coordinator.cache, Source: "cache"}
			coordinator.mu.Unlock()
			return result, nil
		}
		if coordinator.retryErr != nil && now.Before(coordinator.retryAt) {
			failure := *coordinator.retryErr
			if coordinator.cache != nil && now.Sub(coordinator.cache.CheckedAt) <= techUpdateStaleLimit {
				result := techUpdateResult{Cache: *coordinator.cache, Source: "stale-cache", Warning: &failure}
				coordinator.mu.Unlock()
				return result, nil
			}
			coordinator.mu.Unlock()
			return techUpdateResult{}, &failure
		}
		if coordinator.inFlight != nil {
			wait := coordinator.inFlight
			coordinator.mu.Unlock()
			<-wait
			force = false
			continue
		}
		coordinator.inFlight = make(chan struct{})
		wait := coordinator.inFlight
		var stale *techUpdateCache
		etag := ""
		if coordinator.cache != nil {
			copyCache := *coordinator.cache
			stale = &copyCache
			etag = copyCache.ETag
		}
		coordinator.mu.Unlock()

		client := githubUpdateClient(20 * time.Second)
		release, nextETag, notModified, failure := fetchTechReleaseAPI(client, coordinator.apiURL, etag, now)
		source := "github-api"
		if failure != nil {
			if fallback, fallbackFailure := discoverTechReleaseViaPage(client, coordinator.pageURL, now); fallbackFailure == nil {
				release, nextETag, notModified, source = fallback, "", false, "github-release-page"
				failure = nil
			} else if failure.Code == "network-unavailable" || failure.Code == "github-unavailable" || failure.Code == "github-http-error" {
				failure = fallbackFailure
			}
		}
		var result techUpdateResult
		if failure == nil {
			if notModified {
				if stale == nil {
					failure = &updateFailure{Code: "invalid-not-modified", Chinese: "GitHub 返回了无法使用的未修改状态", English: "GitHub returned an unusable not-modified response"}
				} else {
					release = stale.Release
					nextETag = stale.ETag
				}
			}
		}
		if failure == nil {
			snapshotID, err := newUpdateSnapshotID()
			if err != nil {
				failure = classifyUpdateNetworkFailure(err)
			} else {
				cache := techUpdateCache{SchemaVersion: 1, SnapshotID: snapshotID, ETag: nextETag, CheckedAt: now, Release: release}
				coordinator.mu.Lock()
				coordinator.cache = &cache
				coordinator.retryAt = time.Time{}
				coordinator.retryErr = nil
				coordinator.mu.Unlock()
				result = techUpdateResult{Cache: cache, Source: source}
				if err := coordinator.saveCache(cache); err != nil {
					result.Warning = &updateFailure{Code: "update-cache-write-failed", Chinese: "已检查到更新，但无法保存更新状态：" + err.Error(), English: "The update was checked, but its state could not be saved: " + err.Error()}
				}
			}
		}
		if failure != nil && stale != nil && now.Sub(stale.CheckedAt) <= techUpdateStaleLimit {
			result = techUpdateResult{Cache: *stale, Source: "stale-cache", Warning: failure}
			failure = nil
		}

		coordinator.mu.Lock()
		cooldownFailure := failure
		if cooldownFailure == nil {
			cooldownFailure = result.Warning
		}
		if cooldownFailure != nil && !cooldownFailure.RetryAt.IsZero() {
			copyFailure := *cooldownFailure
			coordinator.retryAt = cooldownFailure.RetryAt
			coordinator.retryErr = &copyFailure
		}
		if coordinator.inFlight == wait {
			coordinator.inFlight = nil
			close(wait)
		}
		coordinator.mu.Unlock()
		return result, failure
	}
}

func compareVersion(left, right string) (int, error) {
	lm, rm := versionPattern.FindStringSubmatch(strings.TrimSpace(left)), versionPattern.FindStringSubmatch(strings.TrimSpace(right))
	if lm == nil || rm == nil {
		return 0, errors.New("版本号必须为 vX.Y.Z")
	}
	for i := 1; i <= 3; i++ {
		if lm[i] == rm[i] {
			continue
		}
		var l, r int
		_, _ = fmt.Sscanf(lm[i], "%d", &l)
		_, _ = fmt.Sscanf(rm[i], "%d", &r)
		if l < r {
			return -1, nil
		}
		return 1, nil
	}
	return 0, nil
}

func techUpdateStatus(force bool) (map[string]interface{}, techUpdateResult, *updateFailure) {
	result, failure := techUpdates.result(force)
	if failure != nil {
		return nil, techUpdateResult{}, failure
	}
	release := result.Cache.Release
	cmp, err := compareVersion(appVersion, release.TagName)
	if err != nil {
		return nil, techUpdateResult{}, &updateFailure{Code: "invalid-version", Chinese: err.Error(), English: localizeBackendText("en-US", err.Error())}
	}
	releaseURL := strings.TrimSpace(release.HTMLURL)
	if releaseURL == "" {
		releaseURL = techReleasePage
	}
	status := map[string]interface{}{
		"current_version": "v" + appVersion,
		"latest_version":  release.TagName,
		"available":       cmp < 0,
		"release_url":     releaseURL,
		"published_at":    release.PublishedAt,
		"update_id":       result.Cache.SnapshotID,
		"source":          result.Source,
		"checked_at":      result.Cache.CheckedAt.Format(time.RFC3339),
	}
	if result.Warning != nil {
		status["warning_code"] = result.Warning.Code
		status["warning"] = result.Warning.Error()
		if !result.Warning.RetryAt.IsZero() {
			status["retry_at"] = result.Warning.RetryAt.Format(time.RFC3339)
		}
	}
	return status, result, nil
}

func writeTechUpdateFailure(w http.ResponseWriter, failure *updateFailure) {
	status := http.StatusBadGateway
	if failure.Code == "github-primary-rate-limit" || failure.Code == "github-secondary-rate-limit" {
		status = http.StatusTooManyRequests
	}
	payload := map[string]interface{}{
		"error":      failure.Error(),
		"error_code": failure.Code,
	}
	if failure.Status != 0 {
		payload["upstream_status"] = failure.Status
	}
	if !failure.RetryAt.IsZero() {
		payload["retry_at"] = failure.RetryAt.Format(time.RFC3339)
	}
	if failure.RequestID != "" {
		payload["github_request_id"] = failure.RequestID
	}
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(payload)
}

func (coordinator *techUpdateCoordinator) snapshot(snapshotID string) (githubRelease, *updateFailure) {
	coordinator.mu.Lock()
	defer coordinator.mu.Unlock()
	coordinator.loadCacheLocked()
	if coordinator.cache == nil || snapshotID == "" || snapshotID != coordinator.cache.SnapshotID {
		return githubRelease{}, &updateFailure{Code: "update-snapshot-expired", Chinese: "更新信息已失效，请重新检查一次", English: "The update information has expired; check once more"}
	}
	if err := validateTechRelease(coordinator.cache.Release); err != nil {
		return githubRelease{}, &updateFailure{Code: "invalid-update-cache", Chinese: "缓存的更新信息无效，已拒绝安装", English: "The cached update information is invalid; installation was rejected"}
	}
	return coordinator.cache.Release, nil
}

func handleTechUpdate(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		force := r.URL.Query().Get("force") == "1"
		status, _, failure := techUpdateStatus(force)
		if failure != nil {
			writeTechUpdateFailure(w, failure)
			return
		}
		writeJSON(w, status)
	case http.MethodPost:
		var request struct {
			UpdateID string `json:"update_id"`
		}
		if err := json.NewDecoder(io.LimitReader(r.Body, 16<<10)).Decode(&request); err != nil {
			writeTechUpdateFailure(w, &updateFailure{Code: "invalid-update-request", Chinese: "更新安装请求无效，请重新检查一次", English: "The update installation request is invalid; check once more"})
			return
		}
		release, failure := techUpdates.snapshot(request.UpdateID)
		if failure != nil {
			writeTechUpdateFailure(w, failure)
			return
		}
		cmp, err := compareVersion(appVersion, release.TagName)
		if err != nil || cmp >= 0 {
			writeJSONStatus(w, http.StatusConflict, map[string]string{"error": "当前已是最新版本"})
			return
		}
		if err := stageTechUpdate(release); err != nil {
			var typedFailure *updateFailure
			if errors.As(err, &typedFailure) {
				writeTechUpdateFailure(w, typedFailure)
				return
			}
			writeJSONStatus(w, http.StatusBadGateway, map[string]string{"error": err.Error()})
			return
		}
		writeJSON(w, map[string]interface{}{"ok": true, "message": currentLocalized("新版已验证，应用即将退出并自动替换。", "The new version has been verified. The app will now quit and replace itself.")})
		go func() {
			time.Sleep(500 * time.Millisecond)
			platformQuitApp()
		}()
	default:
		writeJSONStatus(w, http.StatusMethodNotAllowed, map[string]string{"error": "仅支持 GET 或 POST"})
	}
}

func techUpdateArchiveName(tag string) (string, error) {
	match := versionPattern.FindStringSubmatch(strings.TrimSpace(tag))
	if match == nil {
		return "", errors.New("版本号必须为 vX.Y.Z")
	}
	return fmt.Sprintf("ITM-v%s.%s.%s-MacOS-AArch64-APP.zip", match[1], match[2], match[3]), nil
}

func selectTechUpdateAssets(release githubRelease) (githubReleaseAsset, githubReleaseAsset, error) {
	expectedArchive, err := techUpdateArchiveName(release.TagName)
	if err != nil {
		return githubReleaseAsset{}, githubReleaseAsset{}, err
	}
	archive, signature := githubReleaseAsset{}, githubReleaseAsset{}
	for _, asset := range release.Assets {
		if asset.Name == expectedArchive {
			archive = asset
			break
		}
	}
	if archive.Name == "" {
		return githubReleaseAsset{}, githubReleaseAsset{}, fmt.Errorf("该正式发布缺少指定更新包 %s", expectedArchive)
	}
	for _, asset := range release.Assets {
		if asset.Name == expectedArchive+".sig" {
			signature = asset
			break
		}
	}
	if signature.Name == "" {
		return githubReleaseAsset{}, githubReleaseAsset{}, fmt.Errorf("该正式发布缺少签名文件 %s.sig，已拒绝更新", expectedArchive)
	}
	return archive, signature, nil
}

func stageTechUpdate(release githubRelease) error {
	appPath, err := currentAppBundlePath()
	if err != nil {
		return fmt.Errorf("自动更新仅支持已安装的 IMDb Tech Manager.app：%w", err)
	}
	archive, signature, err := selectTechUpdateAssets(release)
	if err != nil {
		return err
	}
	updates := filepath.Join(baseDir(), "updates")
	if err := os.MkdirAll(updates, 0700); err != nil {
		return err
	}
	archivePath := filepath.Join(updates, archive.Name)
	if err := downloadAndVerifyTechArchive(archive.BrowserDownloadURL, signature.BrowserDownloadURL, archivePath); err != nil {
		return err
	}
	return launchTechReplacementHelper(os.Getppid(), appPath, archivePath)
}

func downloadAndVerifyTechArchive(archiveURL, signatureURL, destination string) error {
	client := githubUpdateClient(15 * time.Minute)
	download := func(rawURL string) ([]byte, error) {
		req, err := updateRequest(http.MethodGet, rawURL)
		if err != nil {
			return nil, classifyUpdateNetworkFailure(err)
		}
		resp, err := client.Do(req)
		if err != nil {
			return nil, classifyUpdateNetworkFailure(err)
		}
		defer resp.Body.Close()
		if resp.StatusCode != http.StatusOK {
			return nil, classifyUpdateHTTPFailure(resp, readUpdateErrorBody(resp), "download", time.Now())
		}
		if resp.ContentLength > techUpdateMaxArchive {
			return nil, errors.New("更新包过大，已拒绝下载")
		}
		return io.ReadAll(io.LimitReader(resp.Body, techUpdateMaxArchive+1))
	}
	sigText, err := download(signatureURL)
	if err != nil {
		return fmt.Errorf("无法下载更新签名：%w", err)
	}
	signature, err := base64.StdEncoding.DecodeString(strings.TrimSpace(string(sigText)))
	if err != nil || len(signature) != ed25519.SignatureSize {
		return errors.New("更新签名格式无效")
	}
	archive, err := download(archiveURL)
	if err != nil {
		return fmt.Errorf("无法下载更新包：%w", err)
	}
	if len(archive) > techUpdateMaxArchive {
		return errors.New("更新包过大，已拒绝下载")
	}
	publicKey, err := base64.StdEncoding.DecodeString(techUpdatePublicKey)
	if err != nil || len(publicKey) != ed25519.PublicKeySize {
		return errors.New("内置更新公钥无效")
	}
	if !ed25519.Verify(ed25519.PublicKey(publicKey), archive, signature) {
		return errors.New("更新包签名验证失败，已拒绝替换当前应用")
	}
	return atomicWrite(destination, archive, 0600)
}

func launchTechReplacementHelper(parentPID int, appPath, archivePath string) error {
	helperPath := filepath.Join(baseDir(), "updates", "replace-app.sh")
	script := `#!/bin/sh
set -eu
parent_pid="$1"
target_app="$2"
archive="$3"
updates_dir="$4"
while kill -0 "$parent_pid" 2>/dev/null; do sleep 1; done
stage="$updates_dir/stage-$$"
new_app="$updates_dir/new-$$.app"
backup="$updates_dir/previous-$$.app"
mkdir -p "$stage"
/usr/bin/ditto -x -k "$archive" "$stage"
candidate="$(/usr/bin/find "$stage" -maxdepth 1 -type d -name 'IMDb Tech Manager.app' -print -quit)"
test -n "$candidate"
test -x "$candidate/Contents/MacOS/IMDbTechManagerLauncher"
/usr/bin/ditto "$candidate" "$new_app"
test -x "$new_app/Contents/MacOS/IMDbTechManagerLauncher"
mv "$target_app" "$backup"
if mv "$new_app" "$target_app"; then
  /usr/bin/open -n "$target_app" || true
  rm -rf "$stage" "$archive"
else
  mv "$backup" "$target_app"
  exit 1
fi
`
	if err := atomicWrite(helperPath, []byte(script), 0700); err != nil {
		return err
	}
	cmd := exec.Command("/bin/sh", helperPath, fmt.Sprint(parentPID), appPath, archivePath, filepath.Join(baseDir(), "updates"))
	cmd.Stdout = io.Discard
	cmd.Stderr = io.Discard
	return cmd.Start()
}
