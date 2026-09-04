package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"strings"
	"sync"
)

const defaultLanguage = "zh-CN"

type LanguageOption struct {
	Code           string `json:"code"`
	NativeName     string `json:"native_name"`
	EnglishName    string `json:"english_name"`
	ReviewLanguage string `json:"review_language"`
}

var languageOptions = []LanguageOption{
	{Code: "zh-CN", NativeName: "简体中文", EnglishName: "Simplified Chinese", ReviewLanguage: "zh-CN"},
	{Code: "en-US", NativeName: "English (United States)", EnglishName: "English (United States)", ReviewLanguage: "en-US"},
}

var languageByCode = func() map[string]LanguageOption {
	result := make(map[string]LanguageOption, len(languageOptions))
	for _, option := range languageOptions {
		result[option.Code] = option
	}
	return result
}()

var readPlatformOutputLanguage = platformReadOutputLanguage
var writePlatformOutputLanguage = platformSetOutputLanguage

func supportedLanguage(language string) bool {
	_, ok := languageByCode[language]
	return ok
}

func normalizedLanguage(language string) string {
	if supportedLanguage(language) {
		return language
	}
	return defaultLanguage
}

func supportedLanguages() []LanguageOption {
	result := make([]LanguageOption, len(languageOptions))
	copy(result, languageOptions)
	return result
}

func localized(language, chinese, english string) string {
	if normalizedLanguage(language) == "en-US" {
		return english
	}
	return chinese
}

func currentLocalized(chinese, english string) string {
	return localized(loadSettings().Language, chinese, english)
}

var englishBackendPhrases = []struct{ zh, en string }{
	{"版本号必须为 vX.Y.Z", "The version must use the vX.Y.Z format"},
	{"无法连接 GitHub", "Could not connect to GitHub"},
	{"GitHub 尚未发布正式版本", "No official GitHub release is available"},
	{"GitHub 更新检查失败", "GitHub update check failed"},
	{"GitHub 更新信息无效", "The GitHub update response is invalid"},
	{"GitHub 最新发布不是可用的正式 vX.Y.Z 版本", "The latest GitHub release is not a valid stable vX.Y.Z release"},
	{"当前已是最新版本", "The current version is already up to date"},
	{"该正式发布缺少指定更新包", "The official release is missing the required update archive"},
	{"该正式发布缺少签名文件", "The official release is missing the signature file"},
	{"已拒绝更新", "the update was rejected"},
	{"自动更新仅支持已安装的 IMDb Tech Manager.app", "Automatic updates require an installed IMDb Tech Manager.app"},
	{"下载失败", "Download failed"},
	{"更新包过大，已拒绝下载", "The update archive is too large and was rejected"},
	{"无法下载更新签名", "Could not download the update signature"},
	{"更新签名格式无效", "The update signature format is invalid"},
	{"无法下载更新包", "Could not download the update archive"},
	{"内置更新公钥无效", "The embedded update public key is invalid"},
	{"更新包签名验证失败，已拒绝替换当前应用", "Update signature verification failed; the current app was not replaced"},
	{"仅支持 GET 或 POST", "Only GET or POST is supported"},
	{"仅支持 GET", "Only GET is supported"},
	{"仅支持 POST", "Only POST is supported"},
	{"任务不存在或已过期", "The task does not exist or has expired"},
	{"界面布局数据过大", "The interface layout data is too large"},
	{"界面布局数据为空或过大", "The interface layout data is empty or too large"},
	{"界面布局包含多余内容", "The interface layout contains unexpected content"},
	{"界面布局修订号必须大于 0", "The interface layout revision must be greater than 0"},
	{"语言只能选择简体中文或 English (United States)", "Language must be Simplified Chinese or English (United States)"},
	{"IMDb 抓取缓存上限必须是 64 到 65536 之间的整数 MB", "The IMDb fetch-cache limit must be an integer from 64 to 65536 MB"},
	{"后台检查周期必须在 30 秒到 86400 秒之间", "The background interval must be between 30 and 86400 seconds"},
	{"当前平台不支持手工资料库配置", "Manual library configuration is not supported on this platform"},
	{"仅 macOS 支持首次运行引导", "First-run setup is supported only on macOS"},
	{"IMDb 抓取缓存设置仅在 macOS 启用", "IMDb fetch-cache settings are available only on macOS"},
	{"指定 NFO AI 预演仅在 macOS Manager 中启用", "AI preview for selected NFOs is available only in the macOS Manager"},
	{"NFO Inspector 当前仅在 macOS 启用", "NFO Inspector is currently available only on macOS"},
	{"资料库版本号无效", "The library revision is invalid"},
	{"AI Tag Parser 当前仅在 macOS Manager 中启用", "AI Tag Parser is available only in the macOS Manager"},
	{"API 格式必须选择 OpenAI Chat Completions 或 Anthropic Messages", "API format must be OpenAI Chat Completions or Anthropic Messages"},
	{"temperature 必须在 [0, 2)", "temperature must be in [0, 2)"},
	{"top_p 必须在 (0, 1]", "top_p must be in (0, 1]"},
	{"max_tokens 必须在 128 到 32768 之间", "max_tokens must be between 128 and 32768"},
	{"截断恢复上限必须在 4096 到 32768 之间，且不能小于初始输出上限", "The truncation-recovery limit must be between 4096 and 32768 and cannot be below the initial output limit"},
	{"timeout 必须在 10 到 600 秒之间", "timeout must be between 10 and 600 seconds"},
	{"json_mode 无效", "json_mode is invalid"},
	{"thinking_mode 无效", "thinking_mode is invalid"},
	{"prompt_cache_mode 无效", "prompt_cache_mode is invalid"},
	{"fallback_mode 无效", "fallback_mode is invalid"},
	{"legacy_cleanup_mode 无效", "legacy_cleanup_mode is invalid"},
	{"warning_policy 无效", "warning_policy is invalid"},
	{"retry_count 必须在 0 到 5 之间", "retry_count must be between 0 and 5"},
	{"价格和预算限制不能为负数", "Prices and budget limits cannot be negative"},
	{"额外请求参数必须是 JSON 对象", "Extra request parameters must be a JSON object"},
	{"启用 AI 时 Base URL、模型名和提示词不能为空", "Base URL, model name, and prompt are required when AI is enabled"},
	{"资料库访问测试仅在 macOS 启用", "Library access testing is available only on macOS"},
	{"资料库路径为空", "The library path is empty"},
	{"AI 批量任务暂停仅在 macOS 启用", "Pausing AI batch tasks is available only on macOS"},
	{"NFO 路径为空", "The NFO path is empty"},
	{"请求 JSON 无效", "The request JSON is invalid"},
	{"路径为空", "The path is empty"},
	{"请选择至少一个 NFO", "Select at least one NFO"},
	{"一次最多", "A single operation can process at most"},
	{"刷新范围只能是 movies 或 tv", "The refresh scope must be movies or tv"},
	{"已有任务正在运行", "Another task is already running"},
	{"未找到 python3", "python3 was not found"},
	{"Inspector Engine 返回了无效 JSON", "Inspector Engine returned invalid JSON"},
	{"Inspector Engine 返回错误", "Inspector Engine returned an error"},
	{"常驻引擎不可达", "The resident Engine is unreachable"},
	{"常驻引擎已退出", "The resident Engine exited"},
	{"常驻引擎响应超时", "The resident Engine response timed out"},
}

func localizeBackendText(language, value string) string {
	if normalizedLanguage(language) != "en-US" {
		return value
	}
	for _, phrase := range englishBackendPhrases {
		value = strings.ReplaceAll(value, phrase.zh, phrase.en)
	}
	return value
}

type LanguageSyncStatus struct {
	State    string `json:"state"`
	Language string `json:"language"`
	Source   string `json:"source,omitempty"`
	Migrated bool   `json:"migrated,omitempty"`
	Error    string `json:"error,omitempty"`
}

var languageSyncState = struct {
	sync.RWMutex
	Status LanguageSyncStatus
}{Status: LanguageSyncStatus{State: "unverified", Language: defaultLanguage}}

func recordLanguageSync(status LanguageSyncStatus) {
	status.Language = normalizedLanguage(status.Language)
	languageSyncState.Lock()
	languageSyncState.Status = status
	languageSyncState.Unlock()
}

func currentLanguageSyncStatus() LanguageSyncStatus {
	languageSyncState.RLock()
	defer languageSyncState.RUnlock()
	return languageSyncState.Status
}

func settingsForLanguageMigration() (Settings, bool, error) {
	set := Settings{IntervalSeconds: 60, Language: defaultLanguage}
	b, err := os.ReadFile(settingsPath())
	if errors.Is(err, os.ErrNotExist) {
		return set, false, nil
	}
	if err != nil {
		return set, false, err
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(b, &raw); err != nil || raw == nil {
		if err == nil {
			err = errors.New("settings root is not an object")
		}
		return set, false, fmt.Errorf("读取应用语言设置失败：%w", err)
	}
	if err := json.Unmarshal(b, &set); err != nil {
		return set, false, fmt.Errorf("读取应用语言设置失败：%w", err)
	}
	if set.IntervalSeconds < 30 {
		set.IntervalSeconds = 60
	}
	var storedLanguage string
	if value, ok := raw["language"]; ok {
		_ = json.Unmarshal(value, &storedLanguage)
	}
	configured := supportedLanguage(storedLanguage)
	set.Language = normalizedLanguage(storedLanguage)
	return set, configured, nil
}

// migrateLanguagePreference reconciles only the two language fields used by
// the Manager and Engine. It deliberately leaves caches, task state, UI
// layout, ownership data, browser profiles, Keychain items, and NFOs alone.
func migrateLanguagePreference() error {
	set, managerConfigured, err := settingsForLanguageMigration()
	if err != nil {
		recordLanguageSync(LanguageSyncStatus{State: "failed", Language: defaultLanguage, Error: err.Error()})
		return err
	}
	engineLanguage, engineConfigured, err := readPlatformOutputLanguage()
	if err != nil {
		recordLanguageSync(LanguageSyncStatus{State: "failed", Language: set.Language, Error: err.Error()})
		return err
	}

	status := LanguageSyncStatus{State: "ready", Language: defaultLanguage, Source: "default"}
	switch {
	case managerConfigured:
		status.Language = set.Language
		status.Source = "manager-settings"
		if !engineConfigured || engineLanguage != set.Language {
			if err := writePlatformOutputLanguage(set.Language); err != nil {
				status.State, status.Error = "failed", err.Error()
				recordLanguageSync(status)
				return err
			}
			status.Migrated = true
		}
	case engineConfigured:
		set.Language = engineLanguage
		status.Language = engineLanguage
		status.Source = "engine-config"
		if err := saveSettings(set); err != nil {
			status.State, status.Error = "failed", err.Error()
			recordLanguageSync(status)
			return err
		}
		status.Migrated = true
	default:
		set.Language = defaultLanguage
		if err := saveSettings(set); err != nil {
			status.State, status.Error = "failed", err.Error()
			recordLanguageSync(status)
			return err
		}
		status.Migrated = true
	}
	recordLanguageSync(status)
	return nil
}

// saveLanguagePreference makes the Manager setting authoritative. The Engine
// is inspected before any write, and a failed Engine write rolls the Manager
// setting back so a rejected UI request cannot leave a silent split-brain.
func saveLanguagePreference(previous, next Settings, language string) error {
	if !supportedLanguage(language) {
		return fmt.Errorf("unsupported language: %s", language)
	}
	if _, _, err := readPlatformOutputLanguage(); err != nil {
		return err
	}
	next.Language = language
	if err := saveSettings(next); err != nil {
		return err
	}
	if err := writePlatformOutputLanguage(language); err != nil {
		rollbackErr := saveSettings(previous)
		if rollbackErr != nil {
			combined := fmt.Errorf("%v；应用语言回滚失败：%w", err, rollbackErr)
			recordLanguageSync(LanguageSyncStatus{State: "failed", Language: language, Error: combined.Error()})
			return combined
		}
		recordLanguageSync(LanguageSyncStatus{State: "failed", Language: previous.Language, Error: err.Error()})
		return err
	}
	recordLanguageSync(LanguageSyncStatus{State: "ready", Language: language, Source: "manager-settings"})
	return nil
}
