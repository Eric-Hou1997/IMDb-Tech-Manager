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
	Flag           string `json:"flag"`
	BuiltIn        bool   `json:"built_in"`
	Installed      bool   `json:"installed"`
	Downloadable   bool   `json:"downloadable"`
	State          string `json:"state"`
	Revision       int    `json:"revision,omitempty"`
	ReleasedWith   string `json:"released_with,omitempty"`
	Error          string `json:"error,omitempty"`
}

var languageOptions = []LanguageOption{
	{Code: "zh-CN", NativeName: "简体中文", EnglishName: "Simplified Chinese", ReviewLanguage: "zh-CN", Flag: "cn", BuiltIn: true, Installed: true, State: "built-in"},
	{Code: "zh-Hant", NativeName: "繁體中文", EnglishName: "Traditional Chinese", ReviewLanguage: "zh-CN", Flag: "cn", BuiltIn: true, Installed: true, State: "built-in"},
	{Code: "en-US", NativeName: "English (United States)", EnglishName: "English (United States)", ReviewLanguage: "en-US", Flag: "us", BuiltIn: true, Installed: true, State: "built-in"},
	{Code: "fr-FR", NativeName: "Français", EnglishName: "French", ReviewLanguage: "en-US", Flag: "fr", Downloadable: true, State: "not-installed"},
	{Code: "ru-RU", NativeName: "Русский", EnglishName: "Russian", ReviewLanguage: "en-US", Flag: "ru", Downloadable: true, State: "not-installed"},
	{Code: "ja-JP", NativeName: "日本語", EnglishName: "Japanese", ReviewLanguage: "en-US", Flag: "jp", Downloadable: true, State: "not-installed"},
	{Code: "es-ES", NativeName: "Español", EnglishName: "Spanish", ReviewLanguage: "en-US", Flag: "es", Downloadable: true, State: "not-installed"},
	{Code: "th-TH", NativeName: "ไทย", EnglishName: "Thai", ReviewLanguage: "en-US", Flag: "th", Downloadable: true, State: "not-installed"},
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
	option, ok := languageByCode[strings.TrimSpace(language)]
	return ok && (option.BuiltIn || (languageCatalogActive() && languagePackInstalled(option.Code)))
}

func normalizedLanguageAlias(language string) string {
	language = strings.TrimSpace(language)
	if language == "zh-TW" || language == "zh-HK" || language == "zh-MO" {
		return "zh-Hant"
	}
	return language
}

func normalizedLanguage(language string) string {
	language = configuredLanguage(language)
	if supportedLanguage(language) {
		return language
	}
	return defaultLanguage
}

func configuredLanguage(language string) string {
	language = normalizedLanguageAlias(language)
	if _, ok := languageByCode[language]; ok {
		return language
	}
	return defaultLanguage
}

func supportedLanguages() []LanguageOption {
	result := make([]LanguageOption, len(languageOptions))
	copy(result, languageOptions)
	decorateLanguageOptions(result)
	return result
}

func localized(language, chinese, english string) string {
	language = normalizedLanguage(language)
	if language == "en-US" {
		return english
	}
	if language == "zh-Hant" {
		return traditionalChinese(chinese)
	}
	if value, ok := languagePackMessage(language, "core", stableMessageID(english)); ok {
		return value
	}
	if language != "zh-CN" {
		return english
	}
	return chinese
}

func currentLocalized(chinese, english string) string {
	return localized(loadSettings().Language, chinese, english)
}

var englishBackendPhrases = []struct{ zh, en string }{
	{"语言包目录记录不完整", "The language-pack catalog entry is incomplete"},
	{"语言包文件名无效", "The language-pack filename is invalid"},
	{"该语言不需要下载", "This language does not need to be downloaded"},
	{"当前版本没有为该语言指定语言包", "This app version does not specify a pack for that language"},
	{"该语言包尚未随正式版本发布", "This language pack has not been published with a stable release"},
	{"该语言包正在下载", "This language pack is already downloading"},
	{"语言包下载失败", "Language-pack download failed"},
	{"语言包下载重定向次数过多", "Too many language-pack download redirects"},
	{"语言包下载重定向到非官方主机", "The language-pack download redirected to an unofficial host"},
	{"语言包过大，已拒绝安装", "The language pack is too large and was rejected"},
	{"语言包摘要验证失败", "The language-pack checksum verification failed"},
	{"语言包 ZIP 无效", "The language-pack ZIP is invalid"},
	{"语言包文件集合无效", "The language-pack file set is invalid"},
	{"语言包包含不允许的文件", "The language pack contains a disallowed file"},
	{"语言包内容过大", "The language-pack content is too large"},
	{"语言包内容读取失败", "The language-pack content could not be read"},
	{"语言包清单与当前应用目录不匹配", "The language-pack manifest does not match this app catalog"},
	{"语言包目录无效", "The language-pack catalog is invalid"},
	{"语言包目录不安全", "The language-pack directory is unsafe"},
	{"语言包目录不属于当前应用版本", "The language-pack catalog does not belong to this app version"},
	{"所选语言无效", "The selected language is invalid"},
	{"所选语言尚未安装或不受当前版本支持", "The selected language is not installed or is unsupported by this app version"},
	{"版本号必须为 vX.Y.Z", "The version must use the vX.Y.Z format"},
	{"无法连接 GitHub", "Could not connect to GitHub"},
	{"GitHub 尚未发布正式版本", "No official GitHub release is available"},
	{"GitHub 更新检查失败", "GitHub update check failed"},
	{"GitHub 更新信息无效", "The GitHub update response is invalid"},
	{"GitHub 最新发布不是可用的正式 vX.Y.Z 版本", "The latest GitHub release is not a valid stable vX.Y.Z release"},
	{"GitHub 更新重定向次数过多", "Too many GitHub update redirects"},
	{"GitHub 更新重定向到非官方主机 %s，已拒绝", "The GitHub update redirected to the unofficial host %s and was rejected"},
	{"GitHub 匿名 API 的出口 IP 额度已用完", "The GitHub anonymous API quota for this public IP has been exhausted"},
	{"GitHub 触发了次级限流，请按提示时间后再检查", "GitHub applied a secondary rate limit; check again after the indicated time"},
	{"代理或中间网络拒绝了更新请求", "A proxy or intermediary network rejected the update request"},
	{"GitHub 拒绝了更新请求，但未标明为额度耗尽", "GitHub rejected the update request without identifying an exhausted quota"},
	{"GitHub 更新服务暂时不可用", "The GitHub update service is temporarily unavailable"},
	{"GitHub 更新请求失败（HTTP %d）", "The GitHub update request failed (HTTP %d)"},
	{"无法连接代理服务器", "Could not connect to the proxy server"},
	{"正式发布返回了非官方或不匹配的更新地址", "The official release returned an unofficial or mismatched update URL"},
	{"GitHub 最新发布页面没有返回有效的正式版本", "The GitHub latest-release page did not return a valid stable version"},
	{"GitHub 返回了无法使用的未修改状态", "GitHub returned an unusable not-modified response"},
	{"已检查到更新，但无法保存更新状态", "The update was checked, but its state could not be saved"},
	{"更新信息已失效，请重新检查一次", "The update information has expired; check once more"},
	{"缓存的更新信息无效，已拒绝安装", "The cached update information is invalid; installation was rejected"},
	{"更新安装请求无效，请重新检查一次", "The update installation request is invalid; check once more"},
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
	language = normalizedLanguage(language)
	if language == "zh-CN" {
		return value
	}
	for _, phrase := range englishBackendPhrases {
		translated := phrase.en
		if language == "zh-Hant" {
			translated = traditionalChinese(phrase.zh)
		} else if language != "en-US" {
			if packed, ok := languagePackMessage(language, "core", stableMessageID(phrase.en)); ok {
				translated = packed
			} else if packed, ok := languagePackMessage(language, "engine", stableMessageID(phrase.en)); ok {
				translated = packed
			}
		}
		value = strings.ReplaceAll(value, phrase.zh, translated)
		if language != "en-US" {
			value = strings.ReplaceAll(value, phrase.en, translated)
		}
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
	status.Language = configuredLanguage(status.Language)
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
	storedLanguage = normalizedLanguageAlias(storedLanguage)
	_, configured := languageByCode[storedLanguage]
	set.Language = configuredLanguage(storedLanguage)
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
		reviewLanguage := languageByCode[set.Language].ReviewLanguage
		if !engineConfigured || engineLanguage != reviewLanguage {
			if err := writePlatformOutputLanguage(reviewLanguage); err != nil {
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
	reviewLanguage := languageByCode[language].ReviewLanguage
	if err := writePlatformOutputLanguage(reviewLanguage); err != nil {
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
