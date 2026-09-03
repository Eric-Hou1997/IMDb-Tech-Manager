//go:build darwin

package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"syscall"
	"time"
)

const macLabel = "com.local.imdb-tech-manager"
const macAppLabel = "com.local.imdb-tech-manager.app"
const oldMacLabel = "com.local.tmm-imdb-tech"

func baseDir() string {
	return filepath.Join(os.Getenv("HOME"), "Library", "Application Support", "IMDb Tech Manager")
}
func enginePath() string { return filepath.Join(engineDir(), "mac-engine.py") }
func installedExePath() string {
	// Keep the Agent inside the real .app bundle whenever possible. This gives
	// the GUI and LaunchAgent the same bundle identity,
	// so macOS Files & Folders permissions for Network/Removable Volumes apply
	// consistently.  The old bare binary path remains only as a fallback for
	// legacy launches outside an app bundle.
	if p, err := os.Executable(); err == nil {
		p, _ = filepath.EvalSymlinks(p)
		if strings.Contains(p, ".app/Contents/MacOS/") {
			return p
		}
	}
	return filepath.Join(baseDir(), "bin", "imdb-tech-manager")
}
func platformDataPath() string {
	return filepath.Join(os.Getenv("HOME"), "Library", "Application Support", "tmm-imdb-tech")
}
func macConfigPath() string         { return filepath.Join(platformDataPath(), "config.json") }
func macStatusPath() string         { return filepath.Join(platformDataPath(), "manager-status.json") }
func macCacheDir() string           { return filepath.Join(platformDataPath(), "cache") }
func macCacheStatusPath() string    { return filepath.Join(platformDataPath(), "cache-status.json") }
func macAICacheDir() string         { return filepath.Join(platformDataPath(), "ai-cache") }
func macAIStatusPath() string       { return filepath.Join(platformDataPath(), "ai-status.json") }
func macAIRuntimePath() string      { return filepath.Join(platformDataPath(), "ai-runtime.json") }
func macPipelineStatusPath() string { return filepath.Join(platformDataPath(), "pipeline-status.json") }
func macAIBatchStatePath() string   { return filepath.Join(platformDataPath(), "ai-batch-state.json") }
func macAIBatchQueuePath() string   { return filepath.Join(platformDataPath(), "ai-batch-queue.json") }
func macAIBatchPausePath() string   { return filepath.Join(platformDataPath(), "ai-batch-pause.flag") }
func macAIFailureQueuePath() string {
	return filepath.Join(platformDataPath(), "ai-failure-queue.json")
}
func macAgentPlist() string {
	return filepath.Join(os.Getenv("HOME"), "Library", "LaunchAgents", macLabel+".plist")
}
func macAppPlist() string {
	return filepath.Join(os.Getenv("HOME"), "Library", "LaunchAgents", macAppLabel+".plist")
}

func currentAppBundlePath() (string, error) {
	exe := installedExePath()
	marker := ".app/Contents/MacOS/"
	index := strings.Index(exe, marker)
	if index < 0 {
		return "", fmt.Errorf("当前程序不在 macOS .app 包内，无法启用应用登录自启动")
	}
	return exe[:index+len(".app")], nil
}

func platformAppAutoStartEnabled() bool {
	appPath, err := currentAppBundlePath()
	if err != nil {
		return false
	}
	b, err := os.ReadFile(macAppPlist())
	if err != nil {
		return false
	}
	text := string(b)
	return strings.Contains(text, "<string>"+xmlEscape(appPath)+"</string>") &&
		strings.Contains(text, "<string>--login-startup</string>") &&
		strings.Contains(text, "<string>/usr/bin/open</string>")
}

func platformSetAppAutoStart(enabled bool, w io.Writer) error {
	if !enabled {
		_, _ = commandOutput("launchctl", "disable", fmt.Sprintf("gui/%d/%s", os.Getuid(), macAppLabel))
		if err := os.Remove(macAppPlist()); err != nil && !errors.Is(err, os.ErrNotExist) {
			return fmt.Errorf("关闭应用登录自启动失败：%w", err)
		}
		if platformAppAutoStartEnabled() {
			return fmt.Errorf("关闭应用登录自启动后校验失败")
		}
		return nil
	}
	appPath, err := currentAppBundlePath()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(macAppPlist()), 0755); err != nil {
		return err
	}
	plist := fmt.Sprintf(`<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>%s</string>
<key>ProgramArguments</key><array><string>/usr/bin/open</string><string>-gj</string><string>%s</string><string>--args</string><string>--login-startup</string></array>
<key>RunAtLoad</key><true/>
<key>ProcessType</key><string>Interactive</string>
</dict></plist>`, macAppLabel, xmlEscape(appPath))
	if err := atomicWrite(macAppPlist(), []byte(plist), 0644); err != nil {
		return err
	}
	out, err := commandOutput("launchctl", "enable", fmt.Sprintf("gui/%d/%s", os.Getuid(), macAppLabel))
	if err != nil {
		return fmt.Errorf("启用应用登录自启动失败：%v %s", err, strings.TrimSpace(out))
	}
	if !platformAppAutoStartEnabled() {
		return fmt.Errorf("应用登录自启动写入后校验失败")
	}
	if w != nil {
		fmt.Fprintln(w, "✅ 已启用 macOS 登录后启动应用。")
	}
	return nil
}

// platformRepairAppAutoStartAfterBundleReplacement keeps an explicitly
// configured login item valid when Finder replaces or moves the .app bundle.
// User settings, caches, and task state live in Application Support outside
// the bundle, so this only reconciles the LaunchAgent's app path.
func platformRepairAppAutoStartAfterBundleReplacement() error {
	set := loadSettings()
	if !set.AppAutoStartConfigured || !set.AppAutoStart {
		return nil
	}
	// A development or legacy bare-binary launch cannot safely repair a Finder
	// login item. Preserve its preference until the installed app is launched.
	if _, err := currentAppBundlePath(); err != nil {
		return nil
	}
	if platformAppAutoStartEnabled() {
		return nil
	}
	return platformSetAppAutoStart(true, io.Discard)
}

func platformSetOutputLanguage(language string) error {
	language = normalizedLanguage(language)
	root := map[string]interface{}{}
	if b, err := os.ReadFile(macConfigPath()); err == nil {
		if err := json.Unmarshal(b, &root); err != nil {
			return fmt.Errorf("AI 配置损坏，未修改语言：%w", err)
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	root["output_language"] = language
	b, err := json.MarshalIndent(root, "", "  ")
	if err != nil {
		return err
	}
	return atomicWrite(macConfigPath(), b, 0600)
}

func platformReadOutputLanguage() (string, bool, error) {
	b, err := os.ReadFile(macConfigPath())
	if errors.Is(err, os.ErrNotExist) {
		return "", false, nil
	}
	if err != nil {
		return "", false, err
	}
	var root map[string]json.RawMessage
	if err := json.Unmarshal(b, &root); err != nil || root == nil {
		if err == nil {
			err = errors.New("engine config root is not an object")
		}
		return "", false, fmt.Errorf("读取 Engine 输出语言失败：%w", err)
	}
	value, ok := root["output_language"]
	if !ok {
		return "", false, nil
	}
	var language string
	if err := json.Unmarshal(value, &language); err != nil || !supportedLanguage(language) {
		return "", false, nil
	}
	return language, true, nil
}

func platformIMDbCacheMaxMB() int {
	var cfg struct {
		MaxMB int `json:"imdb_cache_max_mb"`
	}
	if b, err := os.ReadFile(macConfigPath()); err == nil {
		_ = json.Unmarshal(b, &cfg)
	}
	if cfg.MaxMB < minIMDbCacheMaxMB || cfg.MaxMB > maxIMDbCacheMaxMB {
		return defaultIMDbCacheMaxMB
	}
	return cfg.MaxMB
}

func platformSetIMDbCacheMaxMB(value int) error {
	if value < minIMDbCacheMaxMB || value > maxIMDbCacheMaxMB {
		return fmt.Errorf("IMDb 抓取缓存上限必须是 %d 到 %d 之间的整数 MB", minIMDbCacheMaxMB, maxIMDbCacheMaxMB)
	}
	root := map[string]interface{}{}
	if b, err := os.ReadFile(macConfigPath()); err == nil {
		if err := json.Unmarshal(b, &root); err != nil {
			return fmt.Errorf("Engine 配置损坏，未修改缓存上限：%w", err)
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	root["imdb_cache_max_mb"] = value
	b, err := json.MarshalIndent(root, "", "  ")
	if err != nil {
		return err
	}
	return atomicWrite(macConfigPath(), b, 0600)
}

func platformIMDbCacheStatus() IMDbCacheStatus {
	limit := platformIMDbCacheMaxMB()
	status := IMDbCacheStatus{
		State: "unverified", LimitMB: limit, LimitBytes: int64(limit) * 1024 * 1024,
	}
	if b, err := os.ReadFile(macCacheStatusPath()); err == nil {
		var stored IMDbCacheStatus
		if err := json.Unmarshal(b, &stored); err != nil {
			status.State = "failed"
			status.Error = "缓存状态文件损坏，等待下次整理重建：" + err.Error()
			return status
		}
		storedLimit := stored.LimitMB
		status = stored
		status.LimitMB = limit
		status.LimitBytes = int64(limit) * 1024 * 1024
		if storedLimit != 0 && storedLimit != limit {
			status.State = "requested"
		} else if status.State == "" {
			status.State = "unverified"
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		status.Error = "无法读取缓存状态：" + err.Error()
	}
	return status
}
func oldMacAgentPlist() string {
	return filepath.Join(os.Getenv("HOME"), "Library", "LaunchAgents", oldMacLabel+".plist")
}

func hiddenCommand(name string, args ...string) *exec.Cmd { return exec.Command(name, args...) }

func python3() string {
	if p, err := exec.LookPath("python3"); err == nil {
		return p
	}
	for _, p := range []string{"/opt/homebrew/bin/python3", "/usr/local/bin/python3"} {
		if _, err := os.Stat(p); err == nil {
			return p
		}
	}
	return ""
}

func checkPythonRuntime(w io.Writer) error {
	py := python3()
	if py == "" {
		return fmt.Errorf("未找到 python3；当前 IMDb 抓取引擎需要 Python 3.8+")
	}
	versionCmd := exec.Command(py, "-c", "import sys; print('.'.join(map(str, sys.version_info[:3]))); raise SystemExit(0 if sys.version_info >= (3, 8) else 3)")
	versionOut, versionErr := versionCmd.CombinedOutput()
	version := strings.TrimSpace(string(versionOut))
	if version == "" {
		version = "unknown"
	}
	fmt.Fprintf(w, "Python: %s (%s)\n", version, py)
	if versionErr != nil {
		return fmt.Errorf("Python 版本过旧；需要 Python 3.8+，当前：%s", version)
	}
	compileCmd := exec.Command(py, "-m", "py_compile", enginePath())
	compileOut, compileErr := compileCmd.CombinedOutput()
	if compileErr != nil {
		msg := strings.TrimSpace(string(compileOut))
		if msg == "" {
			msg = compileErr.Error()
		}
		return fmt.Errorf("Mac Engine 与当前 Python %s 语法不兼容：%s", version, msg)
	}
	fmt.Fprintln(w, "✅ Mac Engine Python 语法预检通过。")
	return nil
}

func chromePath() string {
	for _, p := range []string{
		"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
		"/Applications/Google Chrome Beta.app/Contents/MacOS/Google Chrome Beta",
		"/Applications/Chromium.app/Contents/MacOS/Chromium",
	} {
		if _, err := os.Stat(p); err == nil {
			return p
		}
	}
	return ""
}

func platformInstall(w io.Writer) error {
	wasInstalled := false
	if _, err := os.Stat(installedExePath()); err == nil {
		wasInstalled = true
	}
	wasRunning := platformAgentAlreadyRunning()
	if err := ensureAssets(); err != nil {
		return err
	}
	if err := checkPythonRuntime(w); err != nil {
		return err
	}

	fmt.Fprintln(w, "正在执行 Mac Engine 离线自检…")
	if err := platformRunEngine("self-test", "", w); err != nil {
		return fmt.Errorf("Mac Engine 自检失败：%w", err)
	}

	_ = platformStopAgentProcessOnly(w)
	time.Sleep(250 * time.Millisecond)
	if err := copySelf(installedExePath()); err != nil {
		return err
	}
	if _, err := platformCleanupLegacy(w); err != nil {
		fmt.Fprintln(w, "旧版残留清理有非致命错误：", err)
	}

	if err := migrateAutoModeOnAppStartPreference(); err != nil {
		return err
	}
	// Never start an Agent before the first-run library confirmation. Existing
	// installations keep their explicit running state during repair.
	if platformLibraryRootsConfirmed() && (!wasInstalled || wasRunning) {
		if err := platformStartAgentProcessOnly(); err != nil {
			return err
		}
		fmt.Fprintln(w, "✅ 自动模式已运行；资料库配置由 Manager 显式管理。")
	} else if !platformLibraryRootsConfirmed() {
		fmt.Fprintln(w, "✅ 安装完成；请先确认电影和电视剧资料库，后台模式保持关闭。")
	} else {
		fmt.Fprintln(w, "✅ 修复完成；自动模式保持关闭。")
	}
	return nil
}

func writeMacPlist() error {
	_ = os.MkdirAll(filepath.Dir(macAgentPlist()), 0755)
	plist := fmt.Sprintf(`<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>%s</string>
<key>ProgramArguments</key><array><string>%s</string><string>--agent</string></array>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><false/>
<key>ThrottleInterval</key><integer>10</integer>
<key>StandardOutPath</key><string>%s</string>
<key>StandardErrorPath</key><string>%s</string>
</dict></plist>`, macLabel, xmlEscape(installedExePath()), xmlEscape(agentLogPath()), xmlEscape(filepath.Join(logDir(), "agent-error.log")))
	return os.WriteFile(macAgentPlist(), []byte(plist), 0644)
}

func xmlEscape(s string) string {
	r := strings.NewReplacer("&", "&amp;", "<", "&lt;", ">", "&gt;", "\"", "&quot;", "'", "&apos;")
	return r.Replace(s)
}

func platformAutoStartEnabled() bool {
	// The plist itself is the durable source of truth for "start at login".
	// When the user disables auto-start the current migration removes the plist, so the UI
	// cannot become re-checked merely because launchctl's override database
	// is formatted differently across macOS releases.
	if _, err := os.Stat(macAgentPlist()); err != nil {
		return false
	}

	domain := fmt.Sprintf("gui/%d", os.Getuid())
	out, err := commandOutput("launchctl", "print-disabled", domain)
	if err != nil {
		// A present plist is still a valid login item even when print-disabled
		// is unavailable, so prefer the safe/expected enabled interpretation.
		return true
	}

	// launchctl output has changed whitespace/quoting over macOS releases.
	// Parse the label line instead of looking for one exact string.
	for _, line := range strings.Split(out, "\n") {
		if !strings.Contains(line, macLabel) {
			continue
		}
		normalized := strings.ToLower(strings.ReplaceAll(strings.ReplaceAll(line, "\"", ""), " ", ""))
		if strings.Contains(normalized, "=>true") {
			return false
		}
		if strings.Contains(normalized, "=>false") {
			return true
		}
	}
	return true
}

func platformSetAutoStart(enabled bool, w io.Writer) error {
	domain := fmt.Sprintf("gui/%d", os.Getuid())
	service := domain + "/" + macLabel

	if enabled {
		// Recreate/update the plist so it always points at the current app
		// bundle, then clear any disabled override left by older releases.
		if err := writeMacPlist(); err != nil {
			return err
		}
		out, err := commandOutput("launchctl", "enable", service)
		if err != nil {
			return fmt.Errorf("启用 macOS 登录自启动失败：%v %s", err, out)
		}
		if !platformAutoStartEnabled() {
			return fmt.Errorf("macOS 登录自启动状态校验失败：launchctl 仍将 %s 标记为 disabled", macLabel)
		}
		if w != nil {
			fmt.Fprintln(w, "✅ 已启用 macOS 登录后自动启动后台 Agent。")
		}
		return nil
	}

	// Disable in launchctl's override database and remove the LaunchAgent
	// plist. Removing the plist is deliberate: it gives the checkbox a stable
	// durable OFF state while leaving the currently running Agent untouched.
	// A later "启动后台服务" runs a session-only Agent until the user enables
	// login auto-start again.
	_, _ = commandOutput("launchctl", "disable", service)
	if err := os.Remove(macAgentPlist()); err != nil && !errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("关闭登录自启动失败：%w", err)
	}
	if platformAutoStartEnabled() {
		return fmt.Errorf("关闭登录自启动后的状态校验失败")
	}
	if w != nil {
		fmt.Fprintln(w, "✅ 已关闭 macOS 登录自启动；当前 Agent 状态不受影响。")
	}
	return nil
}

func platformStartAgentProcessOnly() error {
	if platformAgentAlreadyRunning() {
		return nil
	}
	if _, err := os.Stat(installedExePath()); err != nil {
		return err
	}

	// The Agent is application-owned. Login ownership belongs solely to the
	// App LaunchAgent configured by platformSetAppAutoStart.
	cmd := exec.Command(installedExePath(), "--agent")
	cmd.Stdout = nil
	cmd.Stderr = nil
	return cmd.Start()
}

func platformStopAgentProcessOnly(w io.Writer) error {
	domain := fmt.Sprintf("gui/%d", os.Getuid())
	_, _ = commandOutput("launchctl", "bootout", domain, macAgentPlist())
	pid := readAgentPID()
	if pid > 0 {
		_ = exec.Command("/bin/kill", "-TERM", fmt.Sprintf("%d", pid)).Run()
	}
	_ = os.Remove(agentHeartbeatPath())
	_ = os.Remove(agentPIDPath())
	if w != nil {
		fmt.Fprintln(w, "✅ 当前后台 Agent 已停止；登录自启动设置保持不变。")
	}
	return nil
}

func platformStartAgent(w io.Writer) error {
	if !platformLibraryRootsConfirmed() {
		return fmt.Errorf("请先在首次运行引导中确认电影和电视剧资料库")
	}
	if _, err := os.Stat(installedExePath()); err != nil {
		if err := copySelf(installedExePath()); err != nil {
			return err
		}
	}
	if err := platformStartAgentProcessOnly(); err != nil {
		return err
	}
	fmt.Fprintln(w, "✅ 后台服务已启动；自动启动开关保持原设置。")
	return nil
}

func platformStopAgent(w io.Writer) error { return platformStopAgentProcessOnly(w) }

func platformLibraryRootsConfirmed() bool {
	b, err := os.ReadFile(macConfigPath())
	if err != nil {
		return false
	}
	var cfg struct {
		Confirmed    bool         `json:"library_roots_confirmed"`
		LibraryRoots LibraryRoots `json:"library_roots"`
		Roots        []string     `json:"roots"`
	}
	if json.Unmarshal(b, &cfg) != nil {
		return false
	}
	if cfg.Confirmed {
		return true
	}
	return len(cfg.LibraryRoots.Movies) > 0 || len(cfg.LibraryRoots.TV) > 0 || len(cfg.Roots) > 0
}

func platformDiscoverRootCandidates() ([]RootCandidate, error) {
	py := python3()
	if py == "" {
		return nil, fmt.Errorf("未找到 python3")
	}
	out, err := exec.Command(py, enginePath(), "--discover-root-candidates").CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("读取资料库候选失败：%v %s", err, strings.TrimSpace(string(out)))
	}
	var payload struct {
		Candidates []RootCandidate `json:"candidates"`
	}
	if err := json.Unmarshal(out, &payload); err != nil {
		return nil, fmt.Errorf("资料库候选 JSON 无效：%w", err)
	}
	return payload.Candidates, nil
}

func platformTestLibraryRoot(path string) (map[string]interface{}, error) {
	clean := filepath.Clean(path)
	if !platformLibraryRootsConfirmed() {
		return nil, fmt.Errorf("请先保存分类资料库")
	}
	b, err := os.ReadFile(macConfigPath())
	if err != nil {
		return nil, fmt.Errorf("读取资料库配置失败：%w", err)
	}
	var cfg struct {
		LibraryRoots LibraryRoots `json:"library_roots"`
	}
	if err := json.Unmarshal(b, &cfg); err != nil {
		return nil, fmt.Errorf("资料库配置无效：%w", err)
	}
	allowed := false
	for _, root := range append(append([]string{}, cfg.LibraryRoots.Movies...), cfg.LibraryRoots.TV...) {
		if filepath.Clean(root) == clean {
			allowed = true
			break
		}
	}
	if !allowed {
		return nil, fmt.Errorf("只能测试已确认的资料库根目录")
	}
	info, err := os.Stat(clean)
	if err != nil {
		return map[string]interface{}{"path": clean, "online": false, "state": "offline", "access_error": err.Error()}, nil
	}
	if !info.IsDir() {
		return nil, fmt.Errorf("资料库路径不是文件夹：%s", clean)
	}
	entries, err := os.ReadDir(clean)
	if err != nil {
		return map[string]interface{}{"path": clean, "online": false, "state": "permission-denied", "access_error": err.Error()}, nil
	}
	return map[string]interface{}{"path": clean, "online": true, "state": "online", "entry_count": len(entries), "checked_at": time.Now().Format(time.RFC3339)}, nil
}

func platformAgentAlreadyRunning() bool {
	pid := readAgentPID()
	if pid <= 0 {
		return false
	}
	return exec.Command("/bin/kill", "-0", fmt.Sprintf("%d", pid)).Run() == nil
}

func platformCleanupLegacy(w io.Writer) ([]string, error) {
	removed := []string{}
	domain := fmt.Sprintf("gui/%d", os.Getuid())

	// If a legacy LaunchAgent still points at the old bare Application Support
	// binary, migrate it immediately to the executable
	// inside the current .app bundle.  This happens on normal GUI startup, so
	// the new version cleans up its own runtime residue without waiting for a
	// manual repair click.
	if current := installedExePath(); strings.Contains(current, ".app/Contents/MacOS/") {
		if b, err := os.ReadFile(macAgentPlist()); err == nil && !strings.Contains(string(b), current) {
			wasEnabled := platformAutoStartEnabled()
			_ = platformStopAgentProcessOnly(nil)
			if err := writeMacPlist(); err == nil {
				removed = append(removed, "migrated LaunchAgent -> app bundle runtime")
				if wasEnabled {
					_, _ = commandOutput("launchctl", "enable", domain+"/"+macLabel)
					_ = platformStartAgentProcessOnly()
				} else {
					_, _ = commandOutput("launchctl", "disable", domain+"/"+macLabel)
				}
			}
		}
	}

	// Remove the pre-Manager LaunchAgent and clear the disabled override that
	// earlier versions wrote into launchctl's per-user override database.
	_, _ = commandOutput("launchctl", "bootout", domain, oldMacAgentPlist())
	if err := os.Remove(oldMacAgentPlist()); err == nil {
		removed = append(removed, oldMacAgentPlist())
	}
	_, _ = commandOutput("launchctl", "enable", domain+"/"+oldMacLabel)

	// A legacy Manager copied a bare Go binary outside the .app bundle.
	// That loses the app bundle identity used by macOS Files & Folders privacy
	// controls.  When the current runtime is a real .app executable, remove the
	// legacy bare copy after the Agent has been stopped/migrated.
	legacyBare := filepath.Join(baseDir(), "bin", "imdb-tech-manager")
	if current, err := os.Executable(); err == nil {
		current, _ = filepath.EvalSymlinks(current)
		if strings.Contains(current, ".app/Contents/MacOS/") && filepath.Clean(current) != filepath.Clean(legacyBare) {
			if err := os.Remove(legacyBare); err == nil {
				removed = append(removed, legacyBare)
			}
		}
	}

	// The old workflow exposed a collection of helper .command files. They are
	// no longer user-facing; config/cache are deliberately kept.
	scriptsDir := filepath.Join(os.Getenv("HOME"), "Scripts")
	matches, _ := filepath.Glob(filepath.Join(scriptsDir, "TMM-IMDb-Tech-*.command"))
	for _, p := range matches {
		if err := os.Remove(p); err == nil {
			removed = append(removed, p)
		}
	}

	if w != nil {
		if len(removed) == 0 {
			fmt.Fprintln(w, "✅ 未发现需要清理的 macOS 旧版残留。")
		} else {
			fmt.Fprintf(w, "✅ 已清理 %d 项 macOS 旧版残留；config/cache 保留。\n", len(removed))
		}
	}
	return removed, nil
}

func platformRunEngine(action, arg string, w io.Writer) error {
	py := python3()
	if py == "" {
		return fmt.Errorf("未找到 python3")
	}
	args := []string{enginePath()}
	switch action {
	case "auto":
		args = append(args, "--auto")
	case "run":
		args = append(args, "--watch-once")
	case "backfill":
		args = append(args, "--backfill")
	case "reconcile":
		args = append(args, "--rebuild-all")
	case "refresh":
		args = append(args, "--refresh-all")
	case "test-imdb":
		if !strings.HasPrefix(strings.ToLower(arg), "tt") {
			return fmt.Errorf("请输入 IMDb ID，例如 tt0064757")
		}
		args = append(args, "--test-imdb", arg)
	case "diagnose":
		args = append(args, "--doctor")
	case "self-test":
		args = append(args, "--self-test")
	case "ai-test":
		args = append(args, "--ai-test")
	case "ai-recover":
		args = append(args, "--ai-recover")
	case "ai-scan":
		args = append(args, "--ai-scan")
	case "ai-preview":
		args = append(args, "--ai-preview")
	case "ai-preview-selected":
		if strings.TrimSpace(arg) == "" {
			return fmt.Errorf("指定 NFO 列表为空")
		}
		args = append(args, "--ai-preview-paths-json", arg)
	case "reconcile-index":
		args = append(args, "--reconcile-index")
		if arg == "movies" || arg == "tv" {
			args = append(args, "--reconcile-index-space", arg)
		}
	case "cache-maintain":
		args = append(args, "--cache-maintain")
	case "cache-clear":
		args = append(args, "--cache-clear")
	case "ai-preview-write-selected":
		if strings.TrimSpace(arg) == "" {
			return fmt.Errorf("指定 NFO 列表为空")
		}
		args = append(args, "--ai-preview-write-json", arg)
	case "ai-approve-selected":
		if strings.TrimSpace(arg) == "" {
			return fmt.Errorf("指定 NFO 列表为空")
		}
		args = append(args, "--ai-approve-json", arg)
	case "local-preview-write-selected":
		if strings.TrimSpace(arg) == "" {
			return fmt.Errorf("指定 NFO 列表为空")
		}
		args = append(args, "--local-preview-write-json", arg)
	case "local-approve-selected":
		if strings.TrimSpace(arg) == "" {
			return fmt.Errorf("指定 NFO 列表为空")
		}
		args = append(args, "--local-approve-json", arg)
	case "refresh-selected":
		if strings.TrimSpace(arg) == "" {
			return fmt.Errorf("指定 NFO 列表为空")
		}
		args = append(args, "--refresh-paths-json", arg)
	case "ai-generate-selected":
		if strings.TrimSpace(arg) == "" {
			return fmt.Errorf("指定 NFO 列表为空")
		}
		args = append(args, "--ai-generate-paths-json", arg)
	case "local-generate-selected":
		if strings.TrimSpace(arg) == "" {
			return fmt.Errorf("指定 NFO 列表为空")
		}
		args = append(args, "--local-generate-paths-json", arg)
	case "ai-migrate":
		args = append(args, "--ai-migrate")
	case "pipeline-scan":
		args = append(args, "--pipeline-scan")
	case "local-generate":
		args = append(args, "--local-generate")
	case "local-rebuild":
		args = append(args, "--local-rebuild")
	case "ai-generate":
		args = append(args, "--ai-generate")
	case "ai-rebuild":
		args = append(args, "--ai-rebuild")
	case "ai-resume":
		args = append(args, "--ai-resume")
	case "ai-resume-task":
		args = append(args, "--ai-resume-task")
	case "ai-retry-failed":
		args = append(args, "--ai-retry-failed")
	default:
		return fmt.Errorf("macOS 不支持操作：%s", action)
	}
	return runCommandToWriter(w, py, args...)
}

func platformPreviewCandidates() ([]PreviewCandidate, error) {
	py := python3()
	if py == "" {
		return nil, fmt.Errorf("未找到 python3")
	}
	cmd := exec.Command(py, enginePath(), "--list-ai-preview-candidates")
	out, err := cmd.CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("读取 NFO 列表失败：%v %s", err, strings.TrimSpace(string(out)))
	}
	var payload struct {
		Items []PreviewCandidate `json:"items"`
		Error string             `json:"error"`
	}
	if err := json.Unmarshal(out, &payload); err != nil {
		return nil, fmt.Errorf("NFO 列表 JSON 无效：%w", err)
	}
	if payload.Error != "" {
		return nil, errors.New(payload.Error)
	}
	return payload.Items, nil
}

// platformQuitApp stops everything this app started: the session LaunchAgent
// (so the agent does not respawn via KeepAlive this session), the agent
// process, the resident engine child, then the GUI helper itself. Login
// autostart still follows the user's setting on next sign-in.
func platformQuitApp() {
	uid := os.Getuid()
	_ = runCommandToWriter(io.Discard, "/bin/launchctl", "bootout", fmt.Sprintf("gui/%d/%s", uid, macLabel))
	_ = runCommandToWriter(io.Discard, "/bin/launchctl", "remove", macLabel)
	if pid := readAgentPID(); pid > 0 {
		_ = syscall.Kill(pid, syscall.SIGTERM)
	}
	time.Sleep(500 * time.Millisecond)
	residentShutdown()
	appendManagerLog("quit: agent unloaded, resident engine stopped, exiting")
	os.Exit(0)
}

func platformInspectorJSON(args ...string) (json.RawMessage, error) {
	if payload, err, handled := residentInspectorJSON(args...); handled {
		return payload, err
	}
	py := python3()
	if py == "" {
		payload, _ := json.Marshal(map[string]string{"error": "未找到 python3"})
		return payload, errors.New("未找到 python3")
	}
	commandArgs := append([]string{enginePath()}, args...)
	cmd := exec.Command(py, commandArgs...)
	out, runErr := cmd.CombinedOutput()
	trimmed := strings.TrimSpace(string(out))
	if !json.Valid([]byte(trimmed)) {
		message := "Inspector Engine 返回了无效 JSON"
		if trimmed != "" {
			message += "：" + trimmed
		}
		payload, _ := json.Marshal(map[string]string{"error": message})
		return payload, errors.New(message)
	}
	if runErr != nil {
		return json.RawMessage(trimmed), runErr
	}
	return json.RawMessage(trimmed), nil
}

func collectStatus() (Status, error) {
	set := loadSettings()
	st := Status{
		AppVersion: appVersion, Platform: "darwin", PlatformLabel: "macOS / tinyMediaManager",
		IntervalSeconds: set.IntervalSeconds,
		Capabilities: map[string]bool{
			"install": true, "start": true, "stop": true, "run": true,
			"backfill": true, "reconcile": true, "refresh": true,
			"test_imdb": true, "diagnose": true, "export": true,
			"edit_roots": true, "discover_roots": true, "cleanup_legacy": true,
			"settings": true, "ai": true, "pipeline": true, "local_tags": true,
			"system_webkit_fetch": strings.TrimSpace(os.Getenv("IMDB_TECH_WEBKIT_HELPER")) != "",
		},
		Notes: []string{
			"第一阶段只准备 IMDb Technical Specs；后台 Agent 24h 运行，但绝不自动生成或删除 Tag。",
			"第二阶段由用户选择本地规则或 AI 生成。AI 额度/网络失败不会影响 Spec Agent，也不会先删除旧 Tag。",
			"Tag 更新只删除 IMDb Tech Manager 明确拥有的值；Legacy 严格模式优先使用原始 .imdbtech.bak 保护 TMM Tag。",
			"资料库路径离线时保持配置并等待卷重新挂载，不把离线视为媒体删除。",
			"主界面和 IMDb 动态页面回退使用 macOS 系统 WebKit；Chrome/Chromium 仅为可选兼容回退。",
		},
		Paths: map[string]string{
			"config": macConfigPath(), "cache": macCacheDir(), "status": macStatusPath(),
			"ai_cache": macAICacheDir(), "ai_status": macAIStatusPath(), "ai_runtime": macAIRuntimePath(),
			"pipeline_status": macPipelineStatusPath(), "ai_batch_state": macAIBatchStatePath(),
			"ai_failure_queue": macAIFailureQueuePath(), "launch_agent": macAgentPlist(),
		},
		Extra: map[string]interface{}{},
	}
	st.Extra["library_roots_confirmed"] = platformLibraryRootsConfirmed()
	st.Extra["onboarding_required"] = !platformLibraryRootsConfirmed()
	_, st.Installed = statOKDarwin(installedExePath())
	_, st.EngineReady = statOKDarwin(enginePath())
	alive := platformAgentAlreadyRunning()
	hbAlive, hb := heartbeatStatusDarwin()
	st.AgentRunning = alive && hbAlive
	st.LastHeartbeat = hb
	st.Python = python3()
	st.Chrome = chromePath()
	st.IMDbCache = platformIMDbCacheStatus()
	st.Extra["webkit_helper"] = os.Getenv("IMDB_TECH_WEBKIT_HELPER")
	st.Extra["chromium_optional"] = true

	if b, err := os.ReadFile(macConfigPath()); err == nil {
		var cfg struct {
			Roots        []string     `json:"roots"`
			LibraryRoots LibraryRoots `json:"library_roots"`
		}
		if json.Unmarshal(b, &cfg) == nil {
			rootSpaces := map[string]string{}
			for _, p := range cfg.LibraryRoots.Movies {
				rootSpaces[p] = "movies"
			}
			for _, p := range cfg.LibraryRoots.TV {
				rootSpaces[p] = "tv"
			}
			for _, p := range cfg.Roots {
				if _, ok := rootSpaces[p]; !ok {
					rootSpaces[p] = "unassigned"
				}
			}
			for p, space := range rootSpaces {
				// Status polling deliberately does not stat or enumerate configured
				// network volumes. Only explicit scans and the background agent access them.
				kind := map[string]string{"movies": "电影", "tv": "电视剧", "unassigned": "待分类"}[space]
				lib := LibraryInfo{Path: p, Space: space, Kind: kind, State: "configured"}
				st.Libraries = append(st.Libraries, lib)
			}
		}
	}
	if b, err := os.ReadFile(filepath.Join(platformDataPath(), "root-health.json")); err == nil {
		var health map[string]struct {
			LastScan      string `json:"last_scan"`
			MismatchCount int    `json:"mismatch_count"`
			NFOCount      int    `json:"nfo_count"`
		}
		if json.Unmarshal(b, &health) == nil {
			for i := range st.Libraries {
				if h, ok := health[st.Libraries[i].Path]; ok {
					st.Libraries[i].LastScan, st.Libraries[i].MismatchCount, st.Libraries[i].NFOCount = h.LastScan, h.MismatchCount, h.NFOCount
				}
			}
		}
	}
	if b, err := os.ReadFile(macStatusPath()); err == nil {
		var x struct {
			UpdatedAt   string         `json:"updated_at"`
			Mode        string         `json:"mode"`
			NFOTotal    int            `json:"nfo_total"`
			Counts      map[string]int `json:"counts"`
			ActiveRoots []string       `json:"active_roots"`
		}
		if json.Unmarshal(b, &x) == nil {
			st.NFOTotal = x.NFOTotal
			st.Counts = x.Counts
			st.Extra["last_engine_update"] = x.UpdatedAt
			st.Extra["last_engine_mode"] = x.Mode
			st.Extra["active_roots"] = x.ActiveRoots
		}
	}
	if ents, err := os.ReadDir(macCacheDir()); err == nil {
		for _, e := range ents {
			if !e.IsDir() && strings.HasPrefix(e.Name(), "tt") && strings.HasSuffix(e.Name(), ".json") {
				st.CacheCount++
			}
		}
	}
	if cfg, err := platformLoadAIConfig(); err == nil {
		st.Extra["ai_enabled"] = cfg.Enabled
		st.Extra["ai_api_protocol"] = cfg.APIProtocol
		st.Extra["ai_model"] = cfg.Model
		st.Extra["ai_provider"] = cfg.Provider
		st.Extra["ai_thinking_mode"] = cfg.ThinkingMode
		st.Extra["ai_prompt_cache_mode"] = cfg.PromptCacheMode
		st.Extra["ai_has_key"] = platformAISecretExists()
	}
	if ents, err := os.ReadDir(macAICacheDir()); err == nil {
		n := 0
		for _, e := range ents {
			if !e.IsDir() && strings.HasSuffix(e.Name(), ".json") {
				n++
			}
		}
		st.Extra["ai_cache_count"] = n
	}
	if b, err := os.ReadFile(macAIStatusPath()); err == nil {
		var x map[string]interface{}
		if json.Unmarshal(b, &x) == nil {
			st.Extra["ai_status"] = x
		}
	}
	if b, err := os.ReadFile(macAIRuntimePath()); err == nil {
		var x map[string]interface{}
		if json.Unmarshal(b, &x) == nil {
			st.Extra["ai_runtime"] = x
		}
	}
	if b, err := os.ReadFile(macPipelineStatusPath()); err == nil {
		var x map[string]interface{}
		if json.Unmarshal(b, &x) == nil {
			st.Extra["pipeline_status"] = x
		}
	}
	if b, err := os.ReadFile(macAIBatchStatePath()); err == nil {
		var x map[string]interface{}
		if json.Unmarshal(b, &x) == nil {
			st.Extra["ai_batch_state"] = x
		}
	}
	if b, err := os.ReadFile(macAIFailureQueuePath()); err == nil {
		var x map[string]interface{}
		if json.Unmarshal(b, &x) == nil {
			st.Extra["ai_failure_queue"] = x
		}
	}
	_, pauseErr := os.Stat(macAIBatchPausePath())
	st.Extra["ai_task_pause_requested"] = pauseErr == nil
	return st, nil
}

func platformRequestAITaskPause() error {
	if err := os.MkdirAll(filepath.Dir(macAIBatchPausePath()), 0755); err != nil {
		return err
	}
	return os.WriteFile(macAIBatchPausePath(), []byte(time.Now().Format(time.RFC3339)+"\n"), 0644)
}

func statOKDarwin(p string) (os.FileInfo, bool) { i, e := os.Stat(p); return i, e == nil }

func heartbeatStatusDarwin() (bool, string) {
	b, e := os.ReadFile(agentHeartbeatPath())
	if e != nil {
		return false, ""
	}
	t, e := time.Parse(time.RFC3339, strings.TrimSpace(string(b)))
	if e != nil {
		return false, ""
	}
	return time.Since(t) < 150*time.Second, t.Local().Format("2006-01-02 15:04:05")
}

func openAppWindow(url string) error {
	if c := chromePath(); c != "" {
		return exec.Command(
			c,
			"--app="+url,
			"--user-data-dir="+filepath.Join(baseDir(), "ui-profile"),
			"--no-first-run",
		).Start()
	}
	return exec.Command("/usr/bin/open", url).Start()
}

func openPath(p string) error { return exec.Command("/usr/bin/open", p).Start() }

func platformDiagnosticPaths() []string {
	paths := []string{
		macConfigPath(), macStatusPath(),
		filepath.Join(os.Getenv("HOME"), "Library", "Logs", "tmm-imdb-tech.log"),
		filepath.Join(os.Getenv("HOME"), "Library", "Logs", "tmm-imdb-tech-error.log"),
		macAgentPlist(),
	}
	debugDir := filepath.Join(platformDataPath(), "debug")
	for _, pattern := range []string{"*.html", "*.stderr.txt"} {
		if matches, err := filepath.Glob(filepath.Join(debugDir, pattern)); err == nil {
			paths = append(paths, matches...)
		}
	}
	return paths
}

func requestElevatedAction(string) error { return fmt.Errorf("not needed") }
func writeElevatedResult(bool, string)   {}

func saveMacRoots(roots []string) error {
	clean := []string{}
	seen := map[string]bool{}
	for _, p := range roots {
		p = strings.TrimSpace(p)
		if p == "" || seen[p] {
			continue
		}
		seen[p] = true
		clean = append(clean, p)
	}

	cfg := map[string]interface{}{}
	if b, err := os.ReadFile(macConfigPath()); err == nil {
		_ = json.Unmarshal(b, &cfg)
	}
	cfg["roots"] = clean
	cfg["roots_managed_by"] = "IMDb Tech Manager"
	cfg["roots_updated_at"] = time.Now().Format(time.RFC3339)
	_ = os.MkdirAll(filepath.Dir(macConfigPath()), 0755)
	b, _ := json.MarshalIndent(cfg, "", "  ")
	return atomicWrite(macConfigPath(), b, 0644)
}

func saveMacLibraryRoots(roots LibraryRoots) error {
	clean := func(values []string) []string {
		out := []string{}
		seen := map[string]bool{}
		for _, value := range values {
			value = strings.TrimSpace(value)
			if value == "" || seen[value] {
				continue
			}
			seen[value] = true
			out = append(out, value)
		}
		return out
	}
	roots.Movies, roots.TV = clean(roots.Movies), clean(roots.TV)
	seen := map[string]bool{}
	for _, value := range roots.Movies {
		seen[value] = true
	}
	for _, value := range roots.TV {
		if seen[value] {
			return fmt.Errorf("同一目录不能同时属于电影和电视剧：%s", value)
		}
	}
	cfg := map[string]interface{}{}
	if b, err := os.ReadFile(macConfigPath()); err == nil {
		_ = json.Unmarshal(b, &cfg)
	}
	cfg["library_roots"] = roots
	cfg["roots"] = append(append([]string{}, roots.Movies...), roots.TV...)
	cfg["library_roots_confirmed"] = true
	cfg["roots_managed_by"] = "IMDb Tech Manager"
	cfg["roots_updated_at"] = time.Now().Format(time.RFC3339)
	_ = os.MkdirAll(filepath.Dir(macConfigPath()), 0755)
	b, _ := json.MarshalIndent(cfg, "", "  ")
	return atomicWrite(macConfigPath(), b, 0644)
}

const aiKeychainService = "local.imdb-tech-manager.ai"
const aiKeychainAccount = "api-key"

func defaultAIConfig() AIConfig {
	return AIConfig{
		Enabled: false, APIProtocol: "openai", Provider: "openai-compatible", BaseURL: "", Model: "",
		Prompt: defaultAIPrompt, Temperature: 0, TopP: 1, MaxTokens: 2000, OutputTokenCap: 10000,
		ThinkingMode: "off", PromptCacheMode: "auto", TimeoutSeconds: 90, JSONMode: "auto", ExtraBody: "{}",
		FallbackMode: "abort", LegacyCleanupMode: "strict", WarningPolicy: "review", RetryCount: 2,
		InputPricePerMillion: 0, OutputPricePerMillion: 0, RunRequestLimit: 0, RunTokenLimit: 0, RunCostLimit: 0,
	}
}

func inferAIProtocol(explicit, provider, baseURL string) string {
	explicit = strings.ToLower(strings.TrimSpace(explicit))
	if explicit == "openai" || explicit == "anthropic" {
		return explicit
	}
	provider = strings.ToLower(strings.TrimSpace(provider))
	baseURL = strings.ToLower(strings.TrimSpace(baseURL))
	if strings.Contains(provider, "anthropic") || strings.Contains(baseURL, "/apps/anthropic") || strings.HasSuffix(strings.TrimRight(baseURL, "/"), "/v1/messages") {
		return "anthropic"
	}
	if provider == "" || provider == "openai" || provider == "openai-compatible" || provider == "bailian" || provider == "dashscope" || provider == "aliyun" || provider == "qwen" || strings.Contains(baseURL, "/compatible-mode/v1") || strings.HasSuffix(strings.TrimRight(baseURL, "/"), "/chat/completions") {
		return "openai"
	}
	// Preserve an unknown historical provider instead of silently changing its
	// wire protocol. The settings UI asks the user to choose before saving.
	return ""
}

func platformLoadAIConfig() (AIConfig, error) {
	cfg := defaultAIConfig()
	b, err := os.ReadFile(macConfigPath())
	if err != nil {
		if os.IsNotExist(err) {
			return cfg, nil
		}
		return cfg, err
	}
	var root map[string]json.RawMessage
	if err := json.Unmarshal(b, &root); err != nil {
		return cfg, err
	}
	if raw, ok := root["ai"]; ok {
		var saved map[string]json.RawMessage
		_ = json.Unmarshal(raw, &saved)
		_ = json.Unmarshal(raw, &cfg)
		// defaultAIConfig supplies the fresh-install default, but an older file
		// without this field must be inferred from its historical selection/URL.
		// Otherwise every legacy configuration would be silently forced to OpenAI.
		if _, exists := saved["api_protocol"]; !exists {
			cfg.APIProtocol = ""
		}
	}
	if strings.TrimSpace(cfg.Provider) == "" {
		cfg.Provider = "openai-compatible"
	}
	cfg.APIProtocol = inferAIProtocol(cfg.APIProtocol, cfg.Provider, cfg.BaseURL)
	if knownStockAIPrompt(cfg.Prompt) {
		cfg.Prompt = defaultAIPrompt
	}
	if cfg.TopP <= 0 || cfg.TopP > 1 {
		cfg.TopP = 1
	}
	if cfg.MaxTokens == 1800 && cfg.OutputTokenCap == 8192 {
		cfg.MaxTokens, cfg.OutputTokenCap = 2000, 10000
	}
	if cfg.MaxTokens < 128 {
		cfg.MaxTokens = 2000
	}
	if cfg.OutputTokenCap < 4096 {
		cfg.OutputTokenCap = 10000
	}
	if cfg.TimeoutSeconds < 10 {
		cfg.TimeoutSeconds = 90
	}
	if cfg.JSONMode == "" {
		cfg.JSONMode = "auto"
	}
	if cfg.ThinkingMode == "" {
		cfg.ThinkingMode = "off"
	}
	if cfg.PromptCacheMode == "" {
		cfg.PromptCacheMode = "auto"
	}
	if cfg.ExtraBody == "" {
		cfg.ExtraBody = "{}"
	}
	if cfg.FallbackMode == "" {
		cfg.FallbackMode = "abort"
	}
	if cfg.LegacyCleanupMode == "" {
		cfg.LegacyCleanupMode = "strict"
	}
	if cfg.WarningPolicy == "" {
		cfg.WarningPolicy = "review"
	}
	if cfg.RetryCount < 0 || cfg.RetryCount > 5 {
		cfg.RetryCount = 2
	}
	if cfg.InputPricePerMillion < 0 {
		cfg.InputPricePerMillion = 0
	}
	if cfg.OutputPricePerMillion < 0 {
		cfg.OutputPricePerMillion = 0
	}
	if cfg.RunRequestLimit < 0 {
		cfg.RunRequestLimit = 0
	}
	if cfg.RunTokenLimit < 0 {
		cfg.RunTokenLimit = 0
	}
	if cfg.RunCostLimit < 0 {
		cfg.RunCostLimit = 0
	}
	return cfg, nil
}

func platformSaveAIConfig(cfg AIConfig) error {
	root := map[string]interface{}{}
	if b, err := os.ReadFile(macConfigPath()); err == nil {
		_ = json.Unmarshal(b, &root)
	}
	root["ai"] = cfg
	root["ai_updated_at"] = time.Now().Format(time.RFC3339)
	b, err := json.MarshalIndent(root, "", "  ")
	if err != nil {
		return err
	}
	return atomicWrite(macConfigPath(), b, 0600)
}

func platformSaveAISecret(secret string) error {
	secret = strings.TrimSpace(secret)
	if secret == "" {
		return nil
	}
	cmd := exec.Command("/usr/bin/security", "add-generic-password", "-U", "-s", aiKeychainService, "-a", aiKeychainAccount, "-w", secret)
	if out, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("保存 API Key 到 macOS Keychain 失败：%v %s", err, strings.TrimSpace(string(out)))
	}
	return nil
}

func platformClearAISecret() error {
	cmd := exec.Command("/usr/bin/security", "delete-generic-password", "-s", aiKeychainService, "-a", aiKeychainAccount)
	if out, err := cmd.CombinedOutput(); err != nil {
		msg := strings.ToLower(string(out))
		if !strings.Contains(msg, "could not be found") && !strings.Contains(msg, "not found") {
			return fmt.Errorf("清除 API Key 失败：%v %s", err, strings.TrimSpace(string(out)))
		}
	}
	return nil
}

func platformAISecretExists() bool {
	cmd := exec.Command("/usr/bin/security", "find-generic-password", "-s", aiKeychainService, "-a", aiKeychainAccount, "-w")
	return cmd.Run() == nil
}

func platformChooseLibraryFolder(label string) (string, error) {
	script := `POSIX path of (choose folder with prompt "请选择` + strings.ReplaceAll(label, `"`, ``) + `")`
	out, err := exec.Command("/usr/bin/osascript", "-e", script).CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("未选择文件夹")
	}
	path := strings.TrimSpace(string(out))
	if path == "" {
		return "", fmt.Errorf("未选择文件夹")
	}
	return filepath.Clean(path), nil
}
