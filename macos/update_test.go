package main

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

type techUpdateRoundTripFunc func(*http.Request) (*http.Response, error)

func (fn techUpdateRoundTripFunc) RoundTrip(request *http.Request) (*http.Response, error) {
	return fn(request)
}

func TestCompareVersion(t *testing.T) {
	cases := []struct {
		left, right string
		want        int
	}{
		{"4.0.2", "v4.0.2", 0},
		{"4.0.2", "v4.0.3", -1},
		{"4.0.2", "v4.0.0", 1},
		{"4.1.0", "v4.0.9", 1},
	}
	for _, tc := range cases {
		got, err := compareVersion(tc.left, tc.right)
		if err != nil || got != tc.want {
			t.Fatalf("compareVersion(%q, %q) = %d, %v; want %d", tc.left, tc.right, got, err, tc.want)
		}
	}
	if _, err := compareVersion("4.0", "v4.0.2"); err == nil {
		t.Fatal("short version must be rejected")
	}
}

func TestSelectTechUpdateAssetsRequiresCanonicalName(t *testing.T) {
	release := githubRelease{
		TagName: "v4.0.4",
		Assets: []githubReleaseAsset{
			{Name: "ITM-v4.0.4-MacOS-AArch64-APP.zip", BrowserDownloadURL: "archive"},
			{Name: "ITM-v4.0.4-MacOS-AArch64-APP.zip.sig", BrowserDownloadURL: "signature"},
			{Name: "ITM-v4.0.4-Windows-x64-EXE.zip", BrowserDownloadURL: "wrong-platform"},
		},
	}
	archive, signature, err := selectTechUpdateAssets(release)
	if err != nil {
		t.Fatal(err)
	}
	if archive.BrowserDownloadURL != "archive" || signature.BrowserDownloadURL != "signature" {
		t.Fatalf("selected wrong assets: %#v %#v", archive, signature)
	}

	release.Assets = []githubReleaseAsset{
		{Name: "IMDb-Tech-Manager-macOS-v4.0.4-AppleSilicon-App.zip"},
		{Name: "IMDb-Tech-Manager-macOS-v4.0.4-AppleSilicon-App.zip.sig"},
	}
	if _, _, err := selectTechUpdateAssets(release); err == nil {
		t.Fatal("legacy or ambiguous archive names must not satisfy the OTA selector")
	}
}

func techUpdateTestRelease(tag string) githubRelease {
	archive, _ := techUpdateArchiveName(tag)
	base := "https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases/download/" + tag + "/"
	return githubRelease{
		TagName: tag,
		HTMLURL: "https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases/tag/" + tag,
		Assets: []githubReleaseAsset{
			{Name: archive, BrowserDownloadURL: base + archive},
			{Name: archive + ".sig", BrowserDownloadURL: base + archive + ".sig"},
		},
	}
}

func TestClassifyTechUpdate403Kinds(t *testing.T) {
	now := time.Date(2026, 9, 4, 12, 0, 0, 0, time.UTC)
	response := func(headers map[string]string, body string) *http.Response {
		header := make(http.Header)
		for key, value := range headers {
			header.Set(key, value)
		}
		return &http.Response{StatusCode: http.StatusForbidden, Header: header, Body: io.NopCloser(strings.NewReader(body))}
	}
	cases := []struct {
		name    string
		headers map[string]string
		body    string
		want    string
	}{
		{name: "primary", headers: map[string]string{"X-RateLimit-Remaining": "0", "X-RateLimit-Reset": fmt.Sprint(now.Add(time.Hour).Unix())}, body: `{"message":"rate limit exceeded"}`, want: "github-primary-rate-limit"},
		{name: "secondary", headers: map[string]string{"Retry-After": "90", "X-GitHub-Request-Id": "request"}, body: `{"message":"secondary rate limit"}`, want: "github-secondary-rate-limit"},
		{name: "proxy", headers: map[string]string{}, body: `<html>access denied</html>`, want: "proxy-forbidden"},
		{name: "github forbidden", headers: map[string]string{"X-GitHub-Request-Id": "request"}, body: `{"message":"forbidden"}`, want: "github-forbidden"},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			failure := classifyUpdateHTTPFailure(response(test.headers, test.body), []byte(test.body), "api", now)
			if failure.Code != test.want {
				t.Fatalf("code = %q, want %q", failure.Code, test.want)
			}
		})
	}
}

func TestTechUpdateCoordinatorCachesSuccessfulCheck(t *testing.T) {
	var calls atomic.Int32
	release := techUpdateTestRelease("v4.0.5")
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		calls.Add(1)
		w.Header().Set("ETag", `"release-405"`)
		_ = json.NewEncoder(w).Encode(release)
	}))
	defer server.Close()

	cacheDir := t.TempDir()
	coordinator := newTechUpdateCoordinator()
	coordinator.apiURL = server.URL
	coordinator.pageURL = server.URL + "/latest"
	coordinator.path = func() string { return cacheDir + "/update-state.json" }
	first, failure := coordinator.result(false)
	if failure != nil || first.Cache.Release.TagName != "v4.0.5" {
		t.Fatalf("first result = %#v, failure=%v", first, failure)
	}
	second, failure := coordinator.result(false)
	if failure != nil || second.Source != "cache" {
		t.Fatalf("second result = %#v, failure=%v", second, failure)
	}
	restarted := newTechUpdateCoordinator()
	restarted.apiURL = server.URL
	restarted.pageURL = server.URL + "/latest"
	restarted.path = coordinator.path
	third, failure := restarted.result(false)
	if failure != nil || third.Source != "cache" {
		t.Fatalf("restart result = %#v, failure=%v", third, failure)
	}
	if got := calls.Load(); got != 1 {
		t.Fatalf("upstream calls = %d, want 1", got)
	}
}

func TestTechUpdateCoordinatorUsesStaleResultAndCooldown(t *testing.T) {
	var calls atomic.Int32
	now := time.Date(2026, 9, 4, 12, 0, 0, 0, time.UTC)
	release := techUpdateTestRelease("v4.0.5")
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		call := calls.Add(1)
		if call == 1 {
			_ = json.NewEncoder(w).Encode(release)
			return
		}
		w.Header().Set("X-RateLimit-Remaining", "0")
		w.Header().Set("X-RateLimit-Reset", fmt.Sprint(now.Add(time.Hour).Unix()))
		w.WriteHeader(http.StatusForbidden)
		_, _ = w.Write([]byte(`{"message":"rate limit exceeded"}`))
	}))
	defer server.Close()

	cacheDir := t.TempDir()
	coordinator := newTechUpdateCoordinator()
	coordinator.apiURL = server.URL + "/api"
	coordinator.pageURL = server.URL + "/latest"
	coordinator.path = func() string { return cacheDir + "/update-state.json" }
	coordinator.now = func() time.Time { return now }
	if _, failure := coordinator.result(false); failure != nil {
		t.Fatal(failure)
	}
	now = now.Add(7 * time.Hour)
	result, failure := coordinator.result(true)
	if failure != nil || result.Source != "stale-cache" || result.Warning == nil || result.Warning.Code != "github-primary-rate-limit" {
		t.Fatalf("stale result = %#v, failure=%v", result, failure)
	}
	callsAfterFailure := calls.Load()
	result, failure = coordinator.result(true)
	if failure != nil || result.Warning == nil || result.Warning.Code != "github-primary-rate-limit" {
		t.Fatalf("cooldown result = %#v, failure=%v", result, failure)
	}
	if got := calls.Load(); got != callsAfterFailure {
		t.Fatalf("cooldown made another upstream request: before=%d after=%d", callsAfterFailure, got)
	}
}

func TestTechUpdateInstallUsesCheckedSnapshotWithoutSecondAPIRequest(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	var calls atomic.Int32
	release := techUpdateTestRelease("v4.0.5")
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		calls.Add(1)
		_ = json.NewEncoder(w).Encode(release)
	}))
	defer server.Close()

	previous := techUpdates
	coordinator := newTechUpdateCoordinator()
	coordinator.apiURL = server.URL
	coordinator.pageURL = server.URL + "/latest"
	coordinator.path = func() string { return t.TempDir() + "/update-state.json" }
	techUpdates = coordinator
	defer func() { techUpdates = previous }()

	check := httptest.NewRecorder()
	handleTechUpdate(check, httptest.NewRequest(http.MethodGet, "/api/update", nil))
	if check.Code != http.StatusOK {
		t.Fatalf("check status=%d body=%s", check.Code, check.Body.String())
	}
	var payload map[string]interface{}
	if err := json.Unmarshal(check.Body.Bytes(), &payload); err != nil {
		t.Fatal(err)
	}
	updateID, _ := payload["update_id"].(string)
	install := httptest.NewRecorder()
	handleTechUpdate(install, httptest.NewRequest(http.MethodPost, "/api/update", strings.NewReader(`{"update_id":"`+updateID+`"}`)))
	if got := calls.Load(); got != 1 {
		t.Fatalf("install repeated the GitHub API request: calls=%d", got)
	}
}

func TestTechUpdateCacheRejectsUntrustedAssetURL(t *testing.T) {
	release := techUpdateTestRelease("v4.0.5")
	release.Assets[0].BrowserDownloadURL = "https://example.com/update.zip"
	if err := validateTechRelease(release); err == nil {
		t.Fatal("untrusted asset URL was accepted")
	}
}

func TestDiscoverTechReleaseViaOfficialPage(t *testing.T) {
	var calls atomic.Int32
	client := &http.Client{Transport: techUpdateRoundTripFunc(func(request *http.Request) (*http.Response, error) {
		calls.Add(1)
		if request.Method == http.MethodGet {
			request.URL, _ = request.URL.Parse("https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases/tag/v4.0.5")
		}
		return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader("ok")), Request: request}, nil
	})}
	release, failure := discoverTechReleaseViaPage(client, techReleasePage, time.Now())
	if failure != nil || release.TagName != "v4.0.5" {
		t.Fatalf("release=%#v failure=%v", release, failure)
	}
	if got := calls.Load(); got != 3 {
		t.Fatalf("requests=%d, want page plus two asset probes", got)
	}
}
