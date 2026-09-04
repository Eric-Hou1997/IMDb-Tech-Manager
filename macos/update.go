package main

import (
	"crypto/ed25519"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"time"
)

const (
	techReleaseAPI       = "https://api.github.com/repos/Eric-Hou1997/IMDb-Tech-Manager/releases/latest"
	techReleasePage      = "https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases/latest"
	techUpdatePublicKey  = "UibeR9KwHBQM0x91qLxz64G0InjT2p0r99a83hLzwo0="
	techUpdateMaxArchive = 512 << 20
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

func fetchTechRelease() (githubRelease, error) {
	client := &http.Client{Timeout: 12 * time.Second}
	req, err := http.NewRequest(http.MethodGet, techReleaseAPI, nil)
	if err != nil {
		return githubRelease{}, err
	}
	req.Header.Set("Accept", "application/vnd.github+json")
	req.Header.Set("User-Agent", "IMDb-Tech-Manager/"+appVersion)
	resp, err := client.Do(req)
	if err != nil {
		return githubRelease{}, fmt.Errorf("无法连接 GitHub：%w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode == http.StatusNotFound {
		return githubRelease{}, errors.New("GitHub 尚未发布正式版本")
	}
	if resp.StatusCode != http.StatusOK {
		return githubRelease{}, fmt.Errorf("GitHub 更新检查失败（HTTP %d）", resp.StatusCode)
	}
	var release githubRelease
	if err := json.NewDecoder(io.LimitReader(resp.Body, 2<<20)).Decode(&release); err != nil {
		return githubRelease{}, fmt.Errorf("GitHub 更新信息无效：%w", err)
	}
	if release.Draft || release.Prerelease || versionPattern.FindStringSubmatch(release.TagName) == nil {
		return githubRelease{}, errors.New("GitHub 最新发布不是可用的正式 vX.Y.Z 版本")
	}
	return release, nil
}

func techUpdateStatus() (map[string]interface{}, githubRelease, error) {
	release, err := fetchTechRelease()
	if err != nil {
		return nil, githubRelease{}, err
	}
	cmp, err := compareVersion(appVersion, release.TagName)
	if err != nil {
		return nil, githubRelease{}, err
	}
	releaseURL := strings.TrimSpace(release.HTMLURL)
	if releaseURL == "" {
		releaseURL = techReleasePage
	}
	return map[string]interface{}{
		"current_version": "v" + appVersion,
		"latest_version":  release.TagName,
		"available":       cmp < 0,
		"release_url":     releaseURL,
		"published_at":    release.PublishedAt,
	}, release, nil
}

func handleTechUpdate(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		status, _, err := techUpdateStatus()
		if err != nil {
			writeJSONStatus(w, http.StatusBadGateway, map[string]string{"error": err.Error()})
			return
		}
		writeJSON(w, status)
	case http.MethodPost:
		status, release, err := techUpdateStatus()
		if err != nil {
			writeJSONStatus(w, http.StatusBadGateway, map[string]string{"error": err.Error()})
			return
		}
		if status["available"] != true {
			writeJSONStatus(w, http.StatusConflict, map[string]string{"error": "当前已是最新版本"})
			return
		}
		if err := stageTechUpdate(release); err != nil {
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
	client := &http.Client{Timeout: 90 * time.Second}
	download := func(rawURL string) ([]byte, error) {
		req, err := http.NewRequest(http.MethodGet, rawURL, nil)
		if err != nil {
			return nil, err
		}
		req.Header.Set("User-Agent", "IMDb-Tech-Manager/"+appVersion)
		resp, err := client.Do(req)
		if err != nil {
			return nil, err
		}
		defer resp.Body.Close()
		if resp.StatusCode != http.StatusOK {
			return nil, fmt.Errorf("下载失败（HTTP %d）", resp.StatusCode)
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
