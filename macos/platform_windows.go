//go:build windows

package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"syscall"
	"time"
	"unsafe"
)

const oldWinTaskName = "Emby Technical Specs Web Card"
const winRunValueName = "IMDb Tech Manager Agent"

func baseDir() string {
	p := os.Getenv("LOCALAPPDATA")
	if p == "" {
		p = os.Getenv("APPDATA")
	}
	return filepath.Join(p, "IMDb Tech Manager")
}

func enginePath() string       { return filepath.Join(engineDir(), "windows-engine.ps1") }
func installedExePath() string { return filepath.Join(baseDir(), "bin", "IMDbTechManager.exe") }
func platformDataPath() string {
	return filepath.Join(os.Getenv("APPDATA"), "Emby-Server", "programdata", "custom-tech-specs")
}
func embyRoot() string { return filepath.Join(os.Getenv("APPDATA"), "Emby-Server") }
func techDataFile() string {
	return filepath.Join(embyRoot(), "system", "dashboard-ui", "technical-specs-data.json")
}
func xmlErrorFile() string { return filepath.Join(platformDataPath(), "manager-xml-errors.json") }
func indexHTML() string    { return filepath.Join(embyRoot(), "system", "dashboard-ui", "index.html") }

func hiddenCommand(name string, args ...string) *exec.Cmd {
	cmd := exec.Command(name, args...)
	cmd.SysProcAttr = &syscall.SysProcAttr{HideWindow: true, CreationFlags: 0x08000000}
	return cmd
}

func platformInstall(w io.Writer) error {
	if err := ensureAssets(); err != nil {
		return err
	}

	// Replace an older background binary before copying the new manager.
	_ = platformStopAgentProcessOnly(w)
	time.Sleep(350 * time.Millisecond)
	if err := copySelf(installedExePath()); err != nil {
		return fmt.Errorf("更新 Manager 运行文件失败：%w", err)
	}

	if _, err := platformCleanupLegacy(w); err != nil {
		fmt.Fprintln(w, "旧版残留清理有非致命错误：", err)
	}

	fmt.Fprintln(w, "安装/修复 Windows Emby 技术规格引擎…")
	if err := runCommandToWriter(
		w,
		"powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass",
		"-File", enginePath(), "-ManagedInstall",
	); err != nil {
		return fmt.Errorf("Emby 引擎安装失败：%w", err)
	}

	if err := platformSetAutoStart(true, w); err != nil {
		return err
	}
	if err := platformStartAgentProcessOnly(); err != nil {
		return err
	}
	fmt.Fprintln(w, "✅ 已启用无控制台常驻 Agent；后台检查不会弹 PowerShell 黑框。")
	return nil
}

func platformAutoStartEnabled() bool {
	out, err := commandOutput(
		"reg.exe", "query",
		`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`,
		"/v", winRunValueName,
	)
	return err == nil && strings.Contains(out, winRunValueName)
}

// The Mac product's application-login setting is not used by this stale
// compatibility build target. Keep explicit no-op boundaries so cross-builds
// remain honest and cannot silently install a second Windows startup path.
func platformAppAutoStartEnabled() bool { return false }
func platformSetAppAutoStart(enabled bool, w io.Writer) error {
	if enabled {
		return fmt.Errorf("应用登录自启动仅在 macOS 产品中支持")
	}
	return nil
}
func platformSetOutputLanguage(language string) error   { return nil }
func platformReadOutputLanguage() (string, bool, error) { return "", false, nil }
func platformIMDbCacheMaxMB() int                       { return defaultIMDbCacheMaxMB }
func platformSetIMDbCacheMaxMB(value int) error {
	return fmt.Errorf("IMDb 抓取缓存设置仅在 macOS 产品中支持")
}
func platformIMDbCacheStatus() IMDbCacheStatus {
	return IMDbCacheStatus{State: "unsupported", LimitMB: defaultIMDbCacheMaxMB, LimitBytes: int64(defaultIMDbCacheMaxMB) * 1024 * 1024}
}

func platformSetAutoStart(enabled bool, w io.Writer) error {
	if enabled {
		value := `"` + installedExePath() + `" --agent`
		out, err := commandOutput(
			"reg.exe", "add",
			`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`,
			"/v", winRunValueName,
			"/t", "REG_SZ",
			"/d", value,
			"/f",
		)
		if err != nil {
			return fmt.Errorf("写入登录自启动失败：%v %s", err, out)
		}
		if w != nil {
			fmt.Fprintln(w, "✅ 已启用 Windows 登录后自动启动后台 Agent。")
		}
		return nil
	}

	_, _ = commandOutput(
		"reg.exe", "delete",
		`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`,
		"/v", winRunValueName, "/f",
	)
	if w != nil {
		fmt.Fprintln(w, "✅ 已关闭 Windows 登录自启动；当前 Agent 状态不受影响。")
	}
	return nil
}

func platformStartAgentProcessOnly() error {
	if platformAgentAlreadyRunning() {
		return nil
	}
	cmd := hiddenCommand(installedExePath(), "--agent")
	return cmd.Start()
}

func platformStopAgentProcessOnly(w io.Writer) error {
	pid := readAgentPID()
	if pid > 0 {
		_, _ = commandOutput("taskkill.exe", "/PID", fmt.Sprintf("%d", pid), "/F")
	}
	_ = os.Remove(agentHeartbeatPath())
	_ = os.Remove(agentPIDPath())
	if w != nil {
		fmt.Fprintln(w, "✅ 当前后台 Agent 已停止；登录自启动设置保持不变。")
	}
	return nil
}

func platformStartAgent(w io.Writer) error {
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

func platformStopAgent(w io.Writer) error {
	return platformStopAgentProcessOnly(w)
}

func platformAgentAlreadyRunning() bool {
	pid := readAgentPID()
	if pid <= 0 {
		return false
	}
	out, err := commandOutput("tasklist.exe", "/FI", fmt.Sprintf("PID eq %d", pid), "/FO", "CSV", "/NH")
	return err == nil && strings.Contains(out, fmt.Sprintf(`"%d"`, pid))
}

func platformCleanupLegacy(w io.Writer) ([]string, error) {
	removed := []string{}

	// Legacy scheduled task: best-effort because deleting a task created with
	// Highest privileges can require elevation. ManagedInstall repeats this
	// cleanup when elevated.
	_, _ = commandOutput("schtasks.exe", "/End", "/TN", oldWinTaskName)
	if out, err := commandOutput("schtasks.exe", "/Delete", "/TN", oldWinTaskName, "/F"); err == nil {
		removed = append(removed, "计划任务: "+oldWinTaskName)
	} else if w != nil && out != "" && !strings.Contains(strings.ToLower(out), "cannot find") && !strings.Contains(out, "找不到") {
		fmt.Fprintln(w, "旧计划任务清理提示：", out)
	}

	patterns := []string{
		"technical-specs-worker-v*.ps1",
		"state-v*.json",
		"items-cache-v*.json",
		"root-discovery-v*.json",
		"root-state-v*.json",
		"xml-errors-v*.json",
		"library-roots-v*.json",
		"tech-specs-indexer.ps1",
	}
	for _, pattern := range patterns {
		matches, _ := filepath.Glob(filepath.Join(platformDataPath(), pattern))
		for _, p := range matches {
			if err := os.Remove(p); err == nil {
				removed = append(removed, p)
			}
		}
	}

	for _, name := range []string{"state.json", "items-cache.json", "root-discovery.json", "root-state.json", "xml-errors.json"} {
		p := filepath.Join(platformDataPath(), name)
		if err := os.Remove(p); err == nil {
			removed = append(removed, p)
		}
	}

	if w != nil {
		if len(removed) == 0 {
			fmt.Fprintln(w, "✅ 未发现需要清理的 Windows 旧版残留。")
		} else {
			fmt.Fprintf(w, "✅ 已清理 %d 项 Windows 旧版残留。\n", len(removed))
		}
	}
	return removed, nil
}

func platformRunEngine(action, arg string, w io.Writer) error {
	if _, err := os.Stat(enginePath()); err != nil {
		return fmt.Errorf("Manager 引擎文件不存在，请重新启动应用或点击“安装 / 修复”")
	}

	switch action {
	case "auto", "run", "repair-web":
		return runCommandToWriter(
			w,
			"powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass",
			"-File", enginePath(), "-IndexOnly",
		)
	case "diagnose":
		return runCommandToWriter(
			w,
			"powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass",
			"-File", enginePath(), "-CheckOnly",
		)
	case "rebuild-index":
		return runCommandToWriter(
			w,
			"powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass",
			"-File", enginePath(), "-ManagedInstall",
		)
	default:
		return fmt.Errorf("Windows 不支持操作：%s", action)
	}
}

func collectStatus() (Status, error) {
	s := loadSettings()
	st := Status{
		AppVersion: appVersion, Platform: "windows", PlatformLabel: "Windows / Emby Server",
		IntervalSeconds: s.IntervalSeconds,
		Capabilities: map[string]bool{
			"install": true, "start": true, "stop": true, "run": true,
			"repair_web": true, "rebuild_index": true, "diagnose": true,
			"export": true, "cleanup_legacy": true, "settings": true,
		},
		Notes: []string{
			"后台调度由无控制台的 Manager Agent 承担；关闭管理窗口不会停止 Agent。",
			"“停止后台服务”只停止当前 Agent；是否随 Windows 登录启动由独立开关控制。",
			"iOS Emby 原生 App 不加载服务器 dashboard-ui；标准 Tag 仍可跨客户端使用。",
		},
		Paths: map[string]string{
			"emby_root":       embyRoot(),
			"tech_data":       techDataFile(),
			"xml_errors":      xmlErrorFile(),
			"dashboard_index": indexHTML(),
		},
		Extra: map[string]interface{}{},
	}
	st.IMDbCache = platformIMDbCacheStatus()
	_, st.Installed = statOK(installedExePath())
	_, st.EngineReady = statOK(enginePath())
	alive := platformAgentAlreadyRunning()
	hbAlive, hb := heartbeatStatus()
	st.AgentRunning = alive && hbAlive
	st.LastHeartbeat = hb

	if b, err := os.ReadFile(techDataFile()); err == nil {
		var data struct {
			GeneratedAt   string `json:"generatedAt"`
			IndexedTitles int    `json:"indexedTitles"`
			Libraries     []struct {
				Name, Path, Kind, Evidence string
				Online                     bool
			} `json:"libraries"`
			ScanStats struct {
				OnlineRootsScanned  int `json:"onlineRootsScanned"`
				NFOSeen             int `json:"nfoSeen"`
				NFOReparsed         int `json:"nfoReparsed"`
				TechnicalSpecsFound int `json:"technicalSpecsFound"`
				XmlReadErrors       int `json:"xmlReadErrors"`
			} `json:"scanStats"`
		}
		if json.Unmarshal(b, &data) == nil {
			st.IndexedTitles = data.IndexedTitles
			st.NFOTotal = data.ScanStats.NFOSeen
			st.XmlErrors = data.ScanStats.XmlReadErrors
			for _, l := range data.Libraries {
				st.Libraries = append(st.Libraries, LibraryInfo{
					Name: l.Name, Path: l.Path, Kind: l.Kind, Online: l.Online, Evidence: l.Evidence,
				})
			}
			st.Extra["generated_at"] = data.GeneratedAt
			st.Extra["scan_stats"] = map[string]int{
				"online_roots":          data.ScanStats.OnlineRootsScanned,
				"nfo_seen":              data.ScanStats.NFOSeen,
				"nfo_reparsed":          data.ScanStats.NFOReparsed,
				"technical_specs_found": data.ScanStats.TechnicalSpecsFound,
				"xml_read_errors":       data.ScanStats.XmlReadErrors,
			}
		}
	}

	if b, err := os.ReadFile(xmlErrorFile()); err == nil {
		var v struct {
			GeneratedAt string         `json:"generatedAt"`
			Count       int            `json:"count"`
			Errors      []XmlErrorInfo `json:"errors"`
		}
		if json.Unmarshal(b, &v) == nil {
			st.XmlErrors = v.Count
			if len(v.Errors) > 200 {
				st.XmlErrorDetails = v.Errors[:200]
			} else {
				st.XmlErrorDetails = v.Errors
			}
			st.Extra["xml_errors_generated_at"] = v.GeneratedAt
		}
	}

	if b, err := os.ReadFile(indexHTML()); err == nil {
		html := string(b)
		st.WebPatch = strings.Contains(html, "technical-specs-card.js")
		re := regexp.MustCompile(`technical-specs-card\.js\?v=([0-9.]+)`)
		if m := re.FindStringSubmatch(html); len(m) == 2 {
			st.WebVersion = m[1]
		}
	}
	return st, nil
}

func statOK(p string) (os.FileInfo, bool) { i, e := os.Stat(p); return i, e == nil }

func heartbeatStatus() (bool, string) {
	b, err := os.ReadFile(agentHeartbeatPath())
	if err != nil {
		return false, ""
	}
	t, err := time.Parse(time.RFC3339, strings.TrimSpace(string(b)))
	if err != nil {
		return false, ""
	}
	return time.Since(t) < 150*time.Second, t.Local().Format("2006-01-02 15:04:05")
}

func openAppWindow(url string) error {
	candidates := []string{
		filepath.Join(os.Getenv("ProgramFiles(x86)"), "Microsoft", "Edge", "Application", "msedge.exe"),
		filepath.Join(os.Getenv("ProgramFiles"), "Microsoft", "Edge", "Application", "msedge.exe"),
		filepath.Join(os.Getenv("LOCALAPPDATA"), "Microsoft", "Edge", "Application", "msedge.exe"),
	}
	for _, p := range candidates {
		if _, err := os.Stat(p); err == nil {
			return hiddenCommand(p, "--app="+url, "--start-maximized").Start()
		}
	}
	return hiddenCommand("rundll32.exe", "url.dll,FileProtocolHandler", url).Start()
}

func openPath(p string) error { return hiddenCommand("explorer.exe", p).Start() }

func platformDiagnosticPaths() []string {
	return []string{
		techDataFile(), xmlErrorFile(), indexHTML(),
		filepath.Join(platformDataPath(), "manager-root-discovery.json"),
		filepath.Join(platformDataPath(), "manager-root-state.json"),
		filepath.Join(platformDataPath(), "manager-state.json"),
	}
}

func requestElevatedAction(action string) error {
	exe, err := os.Executable()
	if err != nil {
		return err
	}
	shell32 := syscall.NewLazyDLL("shell32.dll")
	proc := shell32.NewProc("ShellExecuteW")
	verb, _ := syscall.UTF16PtrFromString("runas")
	file, _ := syscall.UTF16PtrFromString(exe)
	params, _ := syscall.UTF16PtrFromString("--headless-action " + action)
	ret, _, callErr := proc.Call(
		0,
		uintptr(unsafe.Pointer(verb)),
		uintptr(unsafe.Pointer(file)),
		uintptr(unsafe.Pointer(params)),
		0,
		0,
	)
	if ret <= 32 {
		return fmt.Errorf("UAC 启动失败：%v", callErr)
	}
	return nil
}

func writeElevatedResult(ok bool, msg string) {
	_ = os.MkdirAll(baseDir(), 0755)
	b, _ := json.Marshal(map[string]interface{}{
		"ok": ok, "message": msg, "time": time.Now().Format(time.RFC3339),
	})
	_ = os.WriteFile(filepath.Join(baseDir(), "elevated-result.json"), b, 0644)
}

func saveMacRoots([]string) error            { return fmt.Errorf("not supported") }
func saveMacLibraryRoots(LibraryRoots) error { return fmt.Errorf("not supported") }

func platformLoadAIConfig() (AIConfig, error) {
	return AIConfig{}, fmt.Errorf("AI config not supported on Windows")
}
func platformSaveAIConfig(AIConfig) error { return fmt.Errorf("AI config not supported on Windows") }
func platformSaveAISecret(string) error   { return fmt.Errorf("AI config not supported on Windows") }
func platformClearAISecret() error        { return fmt.Errorf("AI config not supported on Windows") }
func platformAISecretExists() bool        { return false }

func platformPreviewCandidates() ([]PreviewCandidate, error) {
	return nil, fmt.Errorf("Windows 不提供 AI NFO 预演列表")
}

func platformInspectorJSON(args ...string) (json.RawMessage, error) {
	payload, _ := json.Marshal(map[string]string{"error": "Windows 不提供 NFO 编辑功能"})
	return payload, fmt.Errorf("Windows 不提供 NFO 编辑功能")
}

func platformRequestAITaskPause() error { return fmt.Errorf("Windows 不提供 AI 批量任务") }

func platformChooseLibraryFolder(string) (string, error) { return "", fmt.Errorf("not supported") }

func platformLibraryRootsConfirmed() bool { return false }
func platformDiscoverRootCandidates() ([]RootCandidate, error) {
	return nil, fmt.Errorf("not supported")
}
func platformTestLibraryRoot(string) (map[string]interface{}, error) {
	return nil, fmt.Errorf("not supported")
}
