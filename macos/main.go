package main

import (
	"archive/zip"
	"context"
	"crypto/rand"
	"crypto/sha256"
	"embed"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"log"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

const appVersion = "4.0.2"

const (
	uiLayoutSchema        = 1
	uiLayoutMaxBytes      = 64 << 10
	uiLayoutPlaceholder   = "__IMDB_UI_LAYOUT_STATE__"
	defaultIMDbCacheMaxMB = 2048
	minIMDbCacheMaxMB     = 64
	maxIMDbCacheMaxMB     = 65536
)

type uiLayoutEnvelope struct {
	SchemaVersion int    `json:"schema_version"`
	Revision      uint64 `json:"revision"`
}

const defaultAIPrompt = `你是一名熟悉电影摄影器材、镜头厂商与产品系列、声音制式、胶片/数字电影格式和 IMDb Technical Specifications 写法的专业元数据编辑。你的任务是把 IMDb Technical Specifications 转换为适合 Emby 搜索和筛选的结构化 Tag。

核心原则：
- 可以充分使用你已有的影视器材和行业知识来理解原文，包括识别厂商名称、摄影机型号、镜头品牌、产品系列、技术制式、常见缩写，以及判断省略、并列和共享前后缀的边界。
- 行业知识用于“理解和恢复原文已经表达或明确省略的关系”，不是用于扩写 IMDb 数据。不要因为你知道某产品的真实规格、标准拼写、别名、所属厂商或常见搭配，就加入原始 Technical Specifications 没有表达的新技术事实。
- 当输入与已有知识冲突时，以输入为准。保留 IMDb 原始拼写、大小写、重音符号和型号写法；可以在 warnings 中提示疑似 typo，但不要自行纠正。
- 下面的例子只是行为示例，不是厂商、型号或语法枚举表。应泛化到你认识的其他厂商、产品系列和专业术语，不要只处理示例里出现过的内容。

规则：
1. 只从 Sound mix、Aspect ratio、Camera、Negative Format、Cinematographic Process、Printed Film Format 生成 Tag，生成完检查每一项中有没有遗漏未生成的 Tag，除非这一项下面确实没有内容。Runtime、Color、Laboratory、Film Length 永远不生成 Tag。
2. 允许利用行业知识帮助判断切分边界和明确省略。例如你知道某个词组是完整厂商名、镜头系列或摄影机型号时，可以据此避免错误断句；当一个并列列表明显省略了前面的厂商/系列前缀或末尾设备类型时，可以恢复这些省略成分。不得添加与原文无语义依据的规格、版本、品牌别名、产品特征或设备。
3. Sound mix：识别声音制式本体，去掉发行版本、拷贝、适用范围、声道数等说明性括号。括号若明确是在给出制式本体的规范名称，可用于规范化，例如 DTS (DTS: X) → DTS:X。DTS:X (7.1) → DTS:X；Auro 11.1 (Auro Max) 默认仍为 Auro 11.1。完全相同值去重。
4. Aspect ratio：只输出比例本体，规范为 1.43:1 形式，括号中的影院、版本、场景等说明不进入 Tag；相同比例去重。
5. Camera：把每个 bullet 当作一条摄影配置语句理解。优先利用你对厂商、产品系列和器材类别的知识判断“机身”和“Lens Expression”的边界，再结合语法处理并列、省略和共享前后缀。不要机械按逗号、and 或 & 分割。& 可能是厂商名称的一部分；and 也只有在语义上确实连接多个设备/镜头时才是分隔。
6. Camera 中末尾 Lenses/Lens 等设备类型可以向语义上属于同一镜头并列组的项目传播；厂商或系列前缀只传播到语义上确实属于该厂商/系列的项目。Series 只传播给真正属于 Series 省略组的项目。可以使用你对产品系列的认识来判断传播边界，不要求必须在本 Prompt 中见过该品牌。
7. Camera 中如果一个项目是明显的产品系列省略，可结合语法、同一标题其他 bullet 以及你已有的产品知识恢复。例如 “Angénieux Optimo Ultra 12x, Type EZ” 中，如果语义明确，可将 Type EZ 理解为同一 Angénieux 镜头组的省略项。这里恢复的是原句省略关系，不是新增一件原文没有提到的设备。
8. Camera bullet 末尾括号若语义上是整条拍摄配置的使用范围/季/版本/场景 qualifier（如 some scenes、one shot、aerial shots、Season 5、Rialto version、某特定场景名称等），通常不进入设备 Tag；但不要用固定关键词表机械判断，需结合设备名称与句子结构理解。原始 qualifier 仍由 <technicalspecs> 保留。
9. Negative Format、Cinematographic Process、Printed Film Format：IMDb 每个 bullet 默认视为一个完整 Tag；保留括号、逗号、季/场景/版本限定，不拆括号内部列表。只有原文本身存在明确的多项结构时才考虑拆分。例外——胶片规格括号内逗号分隔的多张胶卷必须逐项拆分为独立 Tag，并保留前面的规格前缀：35 mm (Kodak Vision2 500T 5218, Vision 500T 5263, Fuji Reala 500D 8592)括号里面是三种胶卷，应该生成三个标签：35mm（Kodak Vision2 500T 5218）、35mm（Vision 500T 5263）、35mm（Fuji Reala 500D 8592）；16 mm (Kodak Vision3 50D 7203, Vision3 250D 7207, Vision3 200T 7213, Vision3 500T 7219)这里面好几张胶卷也应该分出来：16mm（Kodak Vision3 50D 7203）、16mm（Kodak Vision3 250D 7207）、16mm（Kodak Vision3 200T 7213）、16mm（Kodak Vision3 500T 7219）。
10. 每个 Tag 必须给 field 和 source_indexes（对应 field 数组的 0 起始索引）。完全相同 Tag 大小写不敏感去重。
11. 每个 Tag 给 confidence：high / medium / low。使用了行业知识并不自动降低 confidence；当原文、语法和已知产品命名高度一致、解析基本唯一时可以是 high。只有确实存在多种合理解析或需要明显猜测时才用 medium/low，并写入 warnings。
12. operation 用简短英文描述，例如 preserve、split-camera-lens、shared-suffix、shared-prefix、series-expansion、knowledge-assisted-boundary、normalize。
13. 如果无法可靠判断共享边界，宁可保留原短语为一个 Tag 并写 warning，也不要为了多切 Tag 而编造关系。
14. 输入可能包含 existing_tags：该标题当前已拥有的生成标签清单（source 标明 ai 或 rules）。它与结果语义重复时保持相同写法，由系统负责用新结果替换旧值；不要因为 existing_tags 存在而输出重复条目，也不要照抄其中已被新信息取代的旧值。
15. 输入包含 output_language，仅用于 warnings（复核说明/警告）的自然语言。zh-CN 使用简体中文，en-US 使用美国英语。此字段绝不改变 tags.value、field、source_indexes、confidence、operation、JSON 键名或 Technical Specifications；这些结构化事实和标签必须保持原始内容、既定英文枚举与原有拼写，禁止因界面语言而翻译。
16. 只返回 JSON，不要 Markdown，不要自然语言解释。

行为示例：
- Panavision C-, D-, E- and H-Series Lenses → Panavision C-Series Lenses / Panavision D-Series Lenses / Panavision E-Series Lenses / Panavision H-Series Lenses。
- Zeiss Standard Speed and Super Speed Lenses → Zeiss Standard Speed Lenses / Zeiss Super Speed Lenses。
- Panavision C-, E-Series and Super High Speed Lenses → Panavision C-Series Lenses / Panavision E-Series Lenses / Panavision Super High Speed Lenses；不要生成 Super High Speed Series Lenses。
- Bausch & Lomb Super Baltar and Angénieux Lenses → Bausch & Lomb Super Baltar Lenses / Angénieux Lenses；Bausch & Lomb 是完整名称，& 不应被拆。
- Hawk V-Lite, V-Plus, V-Series, Zeiss Master Prime and Angenieux Optimo Lenses → Hawk V-Lite Lenses / Hawk V-Plus Lenses / Hawk V-Series Lenses / Zeiss Master Prime Lenses / Angenieux Optimo Lenses；Hawk 前缀不传播到独立的 Zeiss/Angenieux 品牌。
- Arri Alexa 65, Viltrox Epic 1.33x, Luna Zoom, Arri Heroes T.One and IronGlass Helios Lenses → Arri Alexa 65 独立为机身，其余四项按语义共享 Lenses，且镜头和机身要拆开分别生成 Tag。
- Sony CineAlta Venice 2, Angénieux Optimo Ultra 12x, Type EZ, Fujinon Duvo HZK, Cooke 7/i Lenses → 可利用产品知识识别 Type EZ 与前一 Angénieux 项的省略关系，同时不能把 Angénieux 前缀传播给 Fujinon 或 Cooke。
- P+S Technik Skate Scope Masterbuilt Portrait and Soft Flare Lenses：如果你无法可靠确认 Portrait / Soft Flare 的共享前缀边界，保留整个镜头短语并 warning，而不是强拆。

返回结构：
{"tags":[{"value":"Panavision C-Series Lenses","field":"Camera","source_indexes":[0],"confidence":"high","operation":"series-expansion"}],"warnings":[]}`

const legacyDefaultAIPromptInitial = `你是 IMDb Technical Specifications → Emby Tag 的保守型结构化解析器。只做“语义切分、明确省略恢复和格式规范”，不要补充外部知识。

规则：
1. 只从 Sound mix、Aspect ratio、Camera、Negative Format、Cinematographic Process、Printed Film Format 生成 Tag。Runtime、Color、Laboratory、Film Length 永远不生成 Tag。
2. 不得添油加醋：不得补充输入未明确表达的厂商、型号、规格、版本、品牌别名或拼写纠正。只允许恢复句法中明确省略的共享前缀/后缀。保留 IMDb 原始拼写、大小写和重音符号。
3. Sound mix：输出制式本体，去掉发行版本、拷贝、适用范围、声道数等说明性括号。DTS (DTS: X) 可规范为 DTS:X；DTS:X (7.1) 仍为 DTS:X；Auro 11.1 (Auro Max) 仍为 Auro 11.1；完全相同值去重。
4. Aspect ratio：只输出比例本体，规范为 1.43:1 形式，括号说明不进入 Tag。
5. Camera：先识别“机身 + Lens Expression”，再从外层到内层处理镜头并列。末尾 Lenses/Lens 可作为明确公共后缀向左传播；品牌/系列前缀只在连续且句法明确的省略组中传播。Series 只传播给真正属于该 Series 省略组的项目。& 默认是名称内部，不是分隔符。Camera bullet 末尾的拍摄范围/季/版本 qualifier（some scenes、one shot、Season 5、Rialto version 等）通常不进入设备 Tag。遇到共享边界有歧义时保留原短语，不要强拆，并写 warning。
6. Negative Format、Cinematographic Process、Printed Film Format：IMDb 每个 bullet 默认是一个完整 Tag；保留括号、逗号、季/场景/版本限定，不拆括号内部列表。
7. 同一标题内可以用另一个 Camera bullet 的完整写法佐证明显省略项，但不能调用外部产品知识。
8. 每个 Tag 必须给 field 和 source_indexes（该 field 数组的 0 起始索引）。完全相同 Tag 大小写不敏感去重。
9. 每个 Tag 给 confidence：high / medium / low。只有无歧义结果使用 high。operation 用简短英文描述，例如 preserve、split-camera-lens、shared-suffix、shared-prefix、series-expansion、normalize。任何 medium/low 或无法无歧义拆分的情况都应写入 warnings。
10. 只返回 JSON，不要 Markdown，不要解释。

典型恢复：
- Panavision C-, D-, E- and H-Series Lenses → Panavision C-Series Lenses / D-Series / E-Series / H-Series（均补 Panavision 和 Lenses）。
- Zeiss Standard Speed and Super Speed Lenses → Zeiss Standard Speed Lenses / Zeiss Super Speed Lenses。
- Panavision C-, E-Series and Super High Speed Lenses → C-Series / E-Series / Super High Speed Lenses；禁止生成 Super High Speed Series Lenses。
- Hawk V-Lite, V-Plus, V-Series, Zeiss Master Prime and Angenieux Optimo Lenses → Hawk 前缀只传播到连续 V-* 组，不能传播到 Zeiss/Angenieux。
- Arri Alexa 65, Viltrox Epic 1.33x, Luna Zoom, Arri Heroes T.One and IronGlass Helios Lenses → 机身独立，其余四项共享 Lenses。

返回结构：
{"tags":[{"value":"Panavision C-Series Lenses","field":"Camera","source_indexes":[0],"confidence":"high","operation":"series-expansion"}],"warnings":[]}`

func knownStockAIPrompt(prompt string) bool {
	prompt = strings.TrimSpace(prompt)
	if prompt == "" || prompt == strings.TrimSpace(legacyDefaultAIPromptInitial) || prompt == strings.TrimSpace(defaultAIPrompt) {
		return true
	}
	previous371 := strings.Replace(defaultAIPrompt,
		"15. 输入包含 output_language，仅用于 warnings（复核说明/警告）的自然语言。zh-CN 使用简体中文，en-US 使用美国英语。此字段绝不改变 tags.value、field、source_indexes、confidence、operation、JSON 键名或 Technical Specifications；这些结构化事实和标签必须保持原始内容、既定英文枚举与原有拼写，禁止因界面语言而翻译。\n16. 只返回 JSON，不要 Markdown，不要自然语言解释。",
		"15. 只返回 JSON，不要 Markdown，不要自然语言解释。", 1)
	if prompt == strings.TrimSpace(previous371) {
		return true // legacy stock prompt
	}
	previous := strings.Replace(previous371,
		"1. 只从 Sound mix、Aspect ratio、Camera、Negative Format、Cinematographic Process、Printed Film Format 生成 Tag，生成完检查每一项中有没有遗漏未生成的 Tag，除非这一项下面确实没有内容。Runtime、Color、Laboratory、Film Length 永远不生成 Tag。",
		"1. 只从 Sound mix、Aspect ratio、Camera、Negative Format、Cinematographic Process、Printed Film Format 生成 Tag。Runtime、Color、Laboratory、Film Length 永远不生成 Tag。", 1)
	previous = strings.Replace(previous,
		"Arri Alexa 65 独立为机身，其余四项按语义共享 Lenses，且镜头和机身要拆开分别生成 Tag。",
		"Arri Alexa 65 独立为机身，其余四项按语义共享 Lenses。", 1)
	if prompt == strings.TrimSpace(previous) {
		return true // Python legacy stock prompt
	}
	previous = strings.Replace(previous,
		"14. 输入可能包含 existing_tags：该标题当前已拥有的生成标签清单（source 标明 ai 或 rules）。它与结果语义重复时保持相同写法，由系统负责用新结果替换旧值；不要因为 existing_tags 存在而输出重复条目，也不要照抄其中已被新信息取代的旧值。\n", "", 1)
	previous = strings.Replace(previous, "15. 只返回 JSON", "14. 只返回 JSON", 1)
	return prompt == strings.TrimSpace(previous) // Go legacy / legacy stock prompt
}

//go:embed web/index.html engine/mac-engine.py assets/ITM_logo_letter_only.png assets/ITM_logo_tiny.png
var assets embed.FS

type Settings struct {
	IntervalSeconds int `json:"interval_seconds"`
	// AutoStart is retained only to migrate the former Agent LaunchAgent
	// preference. New builds must use AutoModeOnAppStart instead.
	AutoStart                    bool   `json:"auto_start"`
	AutoStartConfigured          bool   `json:"auto_start_configured"`
	AutoModeOnAppStart           bool   `json:"auto_mode_on_app_start"`
	AutoModeOnAppStartConfigured bool   `json:"auto_mode_on_app_start_configured"`
	AppAutoStart                 bool   `json:"app_auto_start"`
	AppAutoStartConfigured       bool   `json:"app_auto_start_configured"`
	Language                     string `json:"language"`
}

type AIConfig struct {
	Enabled               bool    `json:"enabled"`
	APIProtocol           string  `json:"api_protocol"`
	Provider              string  `json:"provider"`
	BaseURL               string  `json:"base_url"`
	Model                 string  `json:"model"`
	Prompt                string  `json:"prompt"`
	Temperature           float64 `json:"temperature"`
	TopP                  float64 `json:"top_p"`
	MaxTokens             int     `json:"max_tokens"`
	OutputTokenCap        int     `json:"output_token_cap"`
	ThinkingMode          string  `json:"thinking_mode"`
	PromptCacheMode       string  `json:"prompt_cache_mode"`
	TimeoutSeconds        int     `json:"timeout_seconds"`
	JSONMode              string  `json:"json_mode"`
	ExtraBody             string  `json:"extra_body"`
	FallbackMode          string  `json:"fallback_mode"`
	LegacyCleanupMode     string  `json:"legacy_cleanup_mode"`
	WarningPolicy         string  `json:"warning_policy"`
	RetryCount            int     `json:"retry_count"`
	InputPricePerMillion  float64 `json:"input_price_per_million"`
	OutputPricePerMillion float64 `json:"output_price_per_million"`
	RunRequestLimit       int     `json:"run_request_limit"`
	RunTokenLimit         int     `json:"run_token_limit"`
	RunCostLimit          float64 `json:"run_cost_limit"`
}

type AIConfigResponse struct {
	AIConfig
	HasAPIKey     bool   `json:"has_api_key"`
	DefaultPrompt string `json:"default_prompt"`
}

type aiConfigRequest struct {
	AIConfig
	APIKey      string `json:"api_key,omitempty"`
	ClearAPIKey bool   `json:"clear_api_key,omitempty"`
}

type LibraryInfo struct {
	Name          string `json:"name,omitempty"`
	Path          string `json:"path"`
	Space         string `json:"space,omitempty"`
	Kind          string `json:"kind,omitempty"`
	Online        bool   `json:"online"`
	State         string `json:"state,omitempty"`
	Evidence      string `json:"evidence,omitempty"`
	AccessError   string `json:"access_error,omitempty"`
	LastScan      string `json:"last_scan,omitempty"`
	MismatchCount int    `json:"mismatch_count,omitempty"`
	NFOCount      int    `json:"nfo_count,omitempty"`
}

type LibraryRoots struct {
	Movies []string `json:"movies"`
	TV     []string `json:"tv"`
}

type RootCandidate struct {
	Path           string `json:"path"`
	SuggestedSpace string `json:"suggested_space,omitempty"`
	Source         string `json:"source,omitempty"`
	Online         bool   `json:"online"`
}

type JobState struct {
	ID          string `json:"job_id,omitempty"`
	Running     bool   `json:"running"`
	Action      string `json:"action,omitempty"`
	StartedAt   string `json:"started_at,omitempty"`
	EndedAt     string `json:"ended_at,omitempty"`
	ExitCode    int    `json:"exit_code,omitempty"`
	Message     string `json:"message,omitempty"`
	MessageCode string `json:"message_code,omitempty"`
	Language    string `json:"language,omitempty"`
	Log         string `json:"log,omitempty"`
}

type AgentCycleState struct {
	Running    bool   `json:"running"`
	StartedAt  string `json:"started_at,omitempty"`
	EndedAt    string `json:"ended_at,omitempty"`
	DurationMs int64  `json:"duration_ms,omitempty"`
	Error      string `json:"error,omitempty"`
}

type XmlErrorInfo struct {
	Path  string `json:"path"`
	Stamp string `json:"stamp,omitempty"`
	Error string `json:"error"`
}

type Status struct {
	AppVersion         string `json:"app_version"`
	Platform           string `json:"platform"`
	PlatformLabel      string `json:"platform_label"`
	Installed          bool   `json:"installed"`
	AgentRunning       bool   `json:"agent_running"`
	AutoModeOnAppStart bool   `json:"auto_mode_on_app_start"`
	// Deprecated compatibility alias for older local Web UI assets.
	AutoStart       bool                   `json:"auto_start"`
	AppAutoStart    bool                   `json:"app_auto_start"`
	Language        string                 `json:"language"`
	Languages       []LanguageOption       `json:"languages"`
	LanguageSync    LanguageSyncStatus     `json:"language_sync"`
	AgentPID        int                    `json:"agent_pid,omitempty"`
	LastHeartbeat   string                 `json:"last_heartbeat,omitempty"`
	LastCycle       AgentCycleState        `json:"last_cycle"`
	IntervalSeconds int                    `json:"interval_seconds"`
	EngineReady     bool                   `json:"engine_ready"`
	Python          string                 `json:"python,omitempty"`
	Chrome          string                 `json:"chrome,omitempty"`
	IndexedTitles   int                    `json:"indexed_titles,omitempty"`
	NFOTotal        int                    `json:"nfo_total,omitempty"`
	CacheCount      int                    `json:"cache_count,omitempty"`
	IMDbCache       IMDbCacheStatus        `json:"imdb_cache"`
	XmlErrors       int                    `json:"xml_errors,omitempty"`
	XmlErrorDetails []XmlErrorInfo         `json:"xml_error_details,omitempty"`
	WebPatch        bool                   `json:"web_patch,omitempty"`
	WebVersion      string                 `json:"web_version,omitempty"`
	Libraries       []LibraryInfo          `json:"libraries,omitempty"`
	Counts          map[string]int         `json:"counts,omitempty"`
	Job             JobState               `json:"job"`
	Capabilities    map[string]bool        `json:"capabilities"`
	Notes           []string               `json:"notes,omitempty"`
	Paths           map[string]string      `json:"paths,omitempty"`
	CleanupRemoved  []string               `json:"cleanup_removed,omitempty"`
	Extra           map[string]interface{} `json:"extra,omitempty"`
}

type IMDbCacheStatus struct {
	State        string `json:"state"`
	LimitMB      int    `json:"limit_mb"`
	LimitBytes   int64  `json:"limit_bytes"`
	UsedBytes    int64  `json:"used_bytes"`
	ParsedCount  int    `json:"parsed_count"`
	RawCount     int    `json:"raw_count"`
	EntryCount   int    `json:"entry_count"`
	RemovedCount int    `json:"removed_count,omitempty"`
	LastCleanup  string `json:"last_cleanup,omitempty"`
	Error        string `json:"error,omitempty"`
}

type actionRequest struct {
	Action string   `json:"action"`
	IMDb   string   `json:"imdb,omitempty"`
	Path   string   `json:"path,omitempty"`
	Paths  []string `json:"paths,omitempty"`
}

type PreviewCandidate struct {
	Path      string `json:"path"`
	Title     string `json:"title"`
	Year      string `json:"year,omitempty"`
	IMDb      string `json:"imdb,omitempty"`
	MediaType string `json:"media_type,omitempty"`
	TagState  string `json:"tag_state,omitempty"`
}

type settingsRequest struct {
	IntervalSeconds *int     `json:"interval_seconds,omitempty"`
	AutoStart       *bool    `json:"auto_start,omitempty"`
	Roots           []string `json:"roots,omitempty"`
	HasRoots        bool     `json:"-"`
}

type jobManager struct {
	mu    sync.Mutex
	st    JobState
	byID  map[string]JobState
	order []string
}

var jobs jobManager
var jobSequence atomic.Uint64
var lastHeartbeatUnix atomic.Int64
var reconcileScheduled atomic.Bool
var uiToken string
var uiLayoutMu sync.Mutex
var uiLayoutPathOverride string
var settingsRequestMu sync.Mutex
var autoModeStartupState struct {
	sync.RWMutex
	Requested bool
	Attempted bool
	Error     string
}

func setAutoModeStartupState(requested, attempted bool, err error) {
	autoModeStartupState.Lock()
	defer autoModeStartupState.Unlock()
	autoModeStartupState.Requested = requested
	autoModeStartupState.Attempted = attempted
	if err != nil {
		autoModeStartupState.Error = err.Error()
	} else {
		autoModeStartupState.Error = ""
	}
}

func autoModeStartupStatus() map[string]interface{} {
	autoModeStartupState.RLock()
	defer autoModeStartupState.RUnlock()
	return map[string]interface{}{
		"enabled":   autoModeStartupState.Requested,
		"attempted": autoModeStartupState.Attempted,
		"error":     autoModeStartupState.Error,
	}
}

func resolvedAutoModeOnAppStart(set Settings, legacyPlatformEnabled bool) bool {
	if set.AutoModeOnAppStartConfigured {
		return set.AutoModeOnAppStart
	}
	if set.AutoStartConfigured {
		return set.AutoStart
	}
	return legacyPlatformEnabled
}

// migrateAutoModeOnAppStartPreference retires the obsolete Agent
// LaunchAgent setting. Its selected value becomes an application-start
// preference; AppAutoStart remains the sole macOS login-item setting.
func migrateAutoModeOnAppStartPreference() error {
	if runtime.GOOS != "darwin" {
		return nil
	}
	set := loadSettings()
	legacyConfigured := set.AutoStartConfigured || platformAutoStartEnabled()
	if set.AutoModeOnAppStartConfigured && !legacyConfigured {
		return nil
	}
	if !set.AutoModeOnAppStartConfigured {
		set.AutoModeOnAppStart = resolvedAutoModeOnAppStart(set, platformAutoStartEnabled())
		set.AutoModeOnAppStartConfigured = true
	}
	if legacyConfigured {
		// This removes only future login ownership. A currently running Agent is
		// intentionally left alone until its normal stop/quit lifecycle.
		if err := platformSetAutoStart(false, io.Discard); err != nil {
			return fmt.Errorf("迁移旧后台登录启动设置失败：%w", err)
		}
		set.AutoStart = false
		set.AutoStartConfigured = true
	}
	return saveSettings(set)
}

func startConfiguredAutoMode() {
	set := loadSettings()
	if !set.AutoModeOnAppStartConfigured || !set.AutoModeOnAppStart {
		setAutoModeStartupState(false, false, nil)
		return
	}
	if runtime.GOOS == "darwin" && !platformLibraryRootsConfirmed() {
		err := errors.New("尚未确认电影和电视剧资料库，未自动开启后台模式")
		setAutoModeStartupState(true, false, err)
		appendManagerLog("auto mode startup: " + err.Error())
		return
	}
	if err := platformStartAgent(io.Discard); err != nil {
		setAutoModeStartupState(true, true, err)
		appendManagerLog("auto mode startup: " + err.Error())
		return
	}
	// Starting the child process is not evidence that it has completed a
	// background cycle. The status endpoint still reports AgentRunning from
	// its PID/heartbeat, while this state records the requested startup.
	setAutoModeStartupState(true, true, nil)
}

func nextJobID() string {
	return fmt.Sprintf("%d-%d", time.Now().UnixNano(), jobSequence.Add(1))
}

func (m *jobManager) rememberLocked(st JobState) {
	if m.byID == nil {
		m.byID = make(map[string]JobState)
	}
	if _, exists := m.byID[st.ID]; !exists {
		m.order = append(m.order, st.ID)
	}
	m.byID[st.ID] = st
	for len(m.order) > 128 {
		delete(m.byID, m.order[0])
		m.order = m.order[1:]
	}
}

func (m *jobManager) begin(action string) (JobState, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.st.Running {
		return JobState{}, fmt.Errorf("已有任务正在运行：%s", m.st.Action)
	}
	st := JobState{
		ID:          nextJobID(),
		Running:     true,
		Action:      action,
		StartedAt:   time.Now().Format(time.RFC3339Nano),
		Message:     "运行中",
		MessageCode: "job.running",
		Language:    normalizedLanguage(loadSettings().Language),
	}
	m.st = st
	m.rememberLocked(st)
	return st, nil
}

func (m *jobManager) complete(id, action string, code int, msg, log string) JobState {
	m.mu.Lock()
	defer m.mu.Unlock()
	st, ok := m.byID[id]
	if !ok {
		st = JobState{ID: id, Action: action}
	}
	st.Running = false
	st.Action = action
	st.EndedAt = time.Now().Format(time.RFC3339Nano)
	st.ExitCode = code
	st.Message = msg
	if code == 0 {
		st.MessageCode = "job.completed"
	} else {
		st.MessageCode = "job.failed"
	}
	st.Log = tailString(log, 40000)
	m.rememberLocked(st)
	if m.st.ID == id {
		m.st = st
	}
	return st
}

func (m *jobManager) snapshot(id string) (JobState, bool) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if id == "" {
		return m.st, true
	}
	st, ok := m.byID[id]
	return st, ok
}

func main() {
	configureBundledHelpers()
	nativeHosted := false
	if len(os.Args) > 1 {
		switch os.Args[1] {
		case "--version":
			fmt.Println(appVersion)
			return
		case "--self-check":
			if err := bundleSelfCheck(); err != nil {
				fmt.Fprintln(os.Stderr, "SELF-CHECK FAILED:", err)
				os.Exit(2)
			}
			fmt.Printf("IMDb Tech Manager %s self-check OK (%s/%s)\n", appVersion, runtime.GOOS, runtime.GOARCH)
			return
		case "--agent":
			if err := ensureAssets(); err != nil {
				appendManagerLog("agent ensureAssets: " + err.Error())
				os.Exit(2)
			}
			_, _ = cleanupLegacyArtifacts(io.Discard)
			if err := agentLoop(); err != nil {
				appendManagerLog("agent: " + err.Error())
				os.Exit(2)
			}
			return
		case "--native-ui":
			nativeHosted = true
		case "--headless-action":
			if len(os.Args) < 3 {
				os.Exit(2)
			}
			if err := ensureAssets(); err != nil {
				writeElevatedResult(false, err.Error())
				os.Exit(2)
			}
			err := performImmediateAction(os.Args[2], "")
			if err != nil {
				writeElevatedResult(false, err.Error())
				os.Exit(1)
			}
			writeElevatedResult(true, "完成")
			return
		}
	}

	if err := ensureAssets(); err != nil {
		appendManagerLog("ensureAssets: " + err.Error())
	}
	if err := migrateLanguagePreference(); err != nil {
		appendManagerLog("language preference migration: " + err.Error())
	}
	if err := upgradeInstalledRuntime(); err != nil {
		appendManagerLog("runtime upgrade: " + err.Error())
	}
	if err := platformRepairAppAutoStartAfterBundleReplacement(); err != nil {
		appendManagerLog("app bundle upgrade login item repair: " + err.Error())
	}
	if _, err := cleanupLegacyArtifacts(io.Discard); err != nil {
		appendManagerLog("legacy cleanup: " + err.Error())
	}
	if err := migrateAutoModeOnAppStartPreference(); err != nil {
		appendManagerLog("startup preference migration: " + err.Error())
	}
	if runtime.GOOS == "darwin" {
		go func() {
			if err := platformRunEngine("cache-maintain", "", io.Discard); err != nil {
				appendManagerLog("startup cache maintain: " + err.Error())
			}
		}()
	}

	runUI(nativeHosted)
}

func configureBundledHelpers() {
	if runtime.GOOS != "darwin" {
		return
	}
	executable, err := os.Executable()
	if err != nil {
		return
	}
	executable, _ = filepath.EvalSymlinks(executable)
	helper := filepath.Join(filepath.Dir(executable), "IMDbWebKitFetcher")
	if info, err := os.Stat(helper); err == nil && info.Mode().IsRegular() && info.Mode()&0111 != 0 {
		_ = os.Setenv("IMDB_TECH_WEBKIT_HELPER", helper)
	}
}

func runUI(nativeHosted bool) {
	uiToken = newSessionToken()
	mux := http.NewServeMux()
	mux.HandleFunc("/", serveIndex)
	mux.HandleFunc("/assets/ITM_logo_letter_only.png", serveBrandIcon("assets/ITM_logo_letter_only.png"))
	mux.HandleFunc("/assets/ITM_logo_tiny.png", serveBrandIcon("assets/ITM_logo_tiny.png"))
	mux.HandleFunc("/api/status", requireToken(handleStatus))
	mux.HandleFunc("/api/onboarding", requireToken(handleOnboarding))
	mux.HandleFunc("/api/action", requireToken(handleAction))
	mux.HandleFunc("/api/settings", requireToken(handleSettings))
	mux.HandleFunc("/api/ui-layout", requireToken(handleUILayout))
	mux.HandleFunc("/api/ai-config", requireToken(handleAIConfig))
	mux.HandleFunc("/api/preview-candidates", requireToken(handlePreviewCandidates))
	mux.HandleFunc("/api/library", requireToken(handleLibrary))
	mux.HandleFunc("/api/library/changes", requireToken(handleLibraryChanges))
	mux.HandleFunc("/api/inspector", requireToken(handleInspector))
	mux.HandleFunc("/api/inspector/edit", requireToken(handleInspectorEdit))
	mux.HandleFunc("/api/inspector/undo", requireToken(handleInspectorUndo))
	mux.HandleFunc("/api/inspector/issues", requireToken(handleInspectorIssues))
	mux.HandleFunc("/api/inspector/status", requireToken(handleInspectorStatus))
	mux.HandleFunc("/api/inspector/reload", requireToken(handleInspectorReload))
	mux.HandleFunc("/api/scope/preflight", requireToken(handleScopePreflight))
	mux.HandleFunc("/api/preview/results", requireToken(handlePreviewResults))
	mux.HandleFunc("/api/job", requireToken(handleJob))
	mux.HandleFunc("/api/task-history", requireToken(handleTaskHistory))
	mux.HandleFunc("/api/quit", requireToken(handleQuit))
	mux.HandleFunc("/api/update", requireToken(handleTechUpdate))
	mux.HandleFunc("/api/heartbeat", requireToken(func(w http.ResponseWriter, r *http.Request) {
		lastHeartbeatUnix.Store(time.Now().Unix())
		writeJSON(w, map[string]bool{"ok": true})
	}))

	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		appendManagerLog("listen: " + err.Error())
		return
	}
	srv := &http.Server{Handler: mux}
	url := "http://" + ln.Addr().String() + "/?token=" + uiToken
	lastHeartbeatUnix.Store(time.Now().Unix())
	nativeParentPID := os.Getppid()

	go func() {
		if err := srv.Serve(ln); err != nil && !errors.Is(err, http.ErrServerClosed) {
			appendManagerLog("http: " + err.Error())
		}
	}()
	startConfiguredAutoMode()

	// Background library reconcile once per launch so the catalog catches
	// external/NAS changes; its progress shows in the task center and the
	// library list spinner.
	scheduleReconcile(1500 * time.Millisecond)

	if nativeHosted {
		// The small AppKit launcher reads this one-line handshake and loads the
		// URL in WKWebView. Keep stdout otherwise quiet so no parser is needed.
		fmt.Println("IMDB_TECH_MANAGER_UI_URL=" + url)
	} else if err := openAppWindow(url); err != nil {
		appendManagerLog("open UI: " + err.Error())
	}

	// Exit the GUI helper after the app window/tab has been gone for a while.
	defer residentShutdown()
	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()
	for range ticker.C {
		if nativeHosted {
			// If AppKit exits unexpectedly, do not leave the local server or its
			// resident Python child orphaned. Normal Cmd-Q also terminates us.
			if os.Getppid() != nativeParentPID || os.Getppid() <= 1 {
				ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
				_ = srv.Shutdown(ctx)
				cancel()
				return
			}
			continue
		}
		last := time.Unix(lastHeartbeatUnix.Load(), 0)
		if time.Since(last) > 15*time.Second {
			ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
			_ = srv.Shutdown(ctx)
			cancel()
			return
		}
	}
}

func bundleSelfCheck() error {
	checks := []string{
		"web/index.html",
		"engine/mac-engine.py",
		"assets/ITM_logo_letter_only.png",
		"assets/ITM_logo_tiny.png",
	}
	for _, name := range checks {
		b, err := assets.ReadFile(name)
		if err != nil {
			return fmt.Errorf("embedded asset %s: %w", name, err)
		}
		if len(b) < 32 {
			return fmt.Errorf("embedded asset %s is unexpectedly small", name)
		}
	}
	web, _ := assets.ReadFile("web/index.html")
	if !strings.Contains(string(web), "v"+appVersion) {
		return fmt.Errorf("embedded UI version does not match manager %s", appVersion)
	}
	if runtime.GOOS == "darwin" {
		if executable, err := os.Executable(); err == nil {
			executable, _ = filepath.EvalSymlinks(executable)
			if strings.Contains(executable, ".app/Contents/MacOS/") {
				for _, name := range []string{"IMDbTechManagerLauncher", "IMDbWebKitFetcher"} {
					path := filepath.Join(filepath.Dir(executable), name)
					info, statErr := os.Stat(path)
					if statErr != nil || !info.Mode().IsRegular() || info.Mode()&0111 == 0 {
						return fmt.Errorf("bundle helper %s is missing or not executable", name)
					}
				}
			}
		}
	}
	return nil
}

func newSessionToken() string {
	b := make([]byte, 24)
	if _, err := rand.Read(b); err != nil {
		return strconv.FormatInt(time.Now().UnixNano(), 36)
	}
	return base64.RawURLEncoding.EncodeToString(b)
}

func requireToken(next http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		t := r.Header.Get("X-Manager-Token")
		if t == "" {
			t = r.URL.Query().Get("token")
		}
		if t == "" || t != uiToken {
			http.Error(w, "forbidden", http.StatusForbidden)
			return
		}
		next(w, r)
	}
}

func serveIndex(w http.ResponseWriter, r *http.Request) {
	b, err := assets.ReadFile("web/index.html")
	if err != nil {
		http.Error(w, err.Error(), 500)
		return
	}
	encodedLayout := base64.RawURLEncoding.EncodeToString(loadUILayoutBytes())
	b = []byte(strings.Replace(string(b), uiLayoutPlaceholder, encodedLayout, 1))
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	_, _ = w.Write(b)
}

func handleUILayout(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		w.Header().Set("Content-Type", "application/json; charset=utf-8")
		w.Header().Set("Cache-Control", "no-store")
		_, _ = w.Write(loadUILayoutBytes())
	case http.MethodPost:
		body, err := io.ReadAll(http.MaxBytesReader(w, r.Body, uiLayoutMaxBytes+1))
		if err != nil || len(body) > uiLayoutMaxBytes {
			writeJSONStatus(w, http.StatusRequestEntityTooLarge, map[string]string{"error": "界面布局数据过大"})
			return
		}
		if err := saveUILayoutBytes(body); err != nil {
			status := http.StatusBadRequest
			if errors.Is(err, errStaleUILayout) {
				status = http.StatusConflict
			}
			writeJSONStatus(w, status, map[string]string{"error": err.Error()})
			return
		}
		var envelope uiLayoutEnvelope
		_ = json.Unmarshal(body, &envelope)
		writeJSON(w, map[string]interface{}{"ok": true, "revision": envelope.Revision})
	default:
		writeJSONStatus(w, http.StatusMethodNotAllowed, map[string]string{"error": "仅支持 GET 或 POST"})
	}
}

func serveBrandIcon(name string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		b, err := assets.ReadFile(name)
		if err != nil {
			http.Error(w, err.Error(), 500)
			return
		}
		w.Header().Set("Content-Type", "image/png")
		w.Header().Set("Cache-Control", "no-store")
		_, _ = w.Write(b)
	}
}

func handleStatus(w http.ResponseWriter, r *http.Request) {
	st, err := collectStatus()
	if err != nil {
		writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
		return
	}
	decorateCommonStatus(&st)
	jobs.mu.Lock()
	st.Job = jobs.st
	jobs.mu.Unlock()
	writeJSON(w, st)
}

func handleOnboarding(w http.ResponseWriter, r *http.Request) {
	if runtime.GOOS != "darwin" {
		writeJSONStatus(w, http.StatusNotFound, map[string]string{"error": "仅 macOS 支持首次运行引导"})
		return
	}
	if r.Method != http.MethodGet {
		writeJSONStatus(w, http.StatusMethodNotAllowed, map[string]string{"error": "仅支持 GET"})
		return
	}
	// Returning users already have an explicit, confirmed classification.
	// Candidate discovery is only an onboarding aid; running it before this
	// check made every configured startup depend on an optional CLI path.
	confirmed := platformLibraryRootsConfirmed()
	if confirmed {
		writeJSON(w, map[string]interface{}{"onboarding_required": false, "library_roots_confirmed": true, "candidates": []RootCandidate{}})
		return
	}
	candidates, err := platformDiscoverRootCandidates()
	if err != nil {
		writeJSONStatus(w, http.StatusBadGateway, map[string]string{"error": err.Error()})
		return
	}
	writeJSON(w, map[string]interface{}{"onboarding_required": true, "library_roots_confirmed": false, "candidates": candidates})
}

func decorateCommonStatus(st *Status) {
	set := loadSettings()
	// The legacy-platform argument is used only before the one-time migration.
	autoModeOnAppStart := resolvedAutoModeOnAppStart(set, platformAutoStartEnabled())
	st.AutoModeOnAppStart = autoModeOnAppStart
	st.AutoStart = autoModeOnAppStart // compatibility alias; not a login-item state
	// Report the effective LaunchAgent state, not merely the saved preference.
	// A deleted/stale plist must never be shown as a working login item.
	st.AppAutoStart = platformAppAutoStartEnabled()
	st.Language = normalizedLanguage(set.Language)
	st.Languages = supportedLanguages()
	st.LanguageSync = currentLanguageSyncStatus()
	st.AgentPID = readAgentPID()
	st.LastCycle = loadAgentCycle()
	if st.Paths == nil {
		st.Paths = map[string]string{}
	}
	if st.Extra == nil {
		st.Extra = map[string]interface{}{}
	}
	st.Extra["auto_mode_on_app_start"] = autoModeStartupStatus()
	st.Paths["manager_data"] = baseDir()
	st.Paths["manager_executable"] = installedExePath()
	st.Paths["engine"] = enginePath()
	st.Paths["logs"] = logDir()
	if b, err := os.ReadFile(cleanupReportPath()); err == nil {
		var report struct {
			Removed []string `json:"removed"`
		}
		if json.Unmarshal(b, &report) == nil {
			st.CleanupRemoved = report.Removed
		}
	}
}

type JobProgress struct {
	Schema    int            `json:"schema"`
	Action    string         `json:"action,omitempty"`
	Done      int            `json:"done,omitempty"`
	Total     int            `json:"total,omitempty"`
	Current   string         `json:"current,omitempty"`
	UpdatedAt string         `json:"updated_at,omitempty"`
	Results   []ProgressItem `json:"results,omitempty"`
}

type ProgressItem struct {
	Path      string `json:"path"`
	Title     string `json:"title,omitempty"`
	Status    string `json:"status,omitempty"`
	TagStatus string `json:"tag_status,omitempty"`
}

func loadJobProgress(action string) JobProgress {
	var p JobProgress
	b, err := os.ReadFile(engineStatePath("job-progress.json"))
	if err != nil {
		return p
	}
	if json.Unmarshal(b, &p) != nil {
		return JobProgress{}
	}
	// Only report progress that belongs to the job the UI is watching. The
	// engine writes generic progress actions (ai-generate/local-generate);
	// every ai-*/local-* job variant maps onto them.
	if p.Action != "" && action != "" && p.Action != action {
		matched := false
		if strings.HasPrefix(action, "ai-") && p.Action == "ai-generate" {
			matched = true
		}
		if strings.HasPrefix(action, "local-") && p.Action == "local-generate" {
			matched = true
		}
		if action == "refresh-selected" || action == "reconcile-index" {
			matched = true
		}
		if !matched {
			return JobProgress{}
		}
	}
	return p
}

func handleQuit(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		writeJSONStatus(w, 405, map[string]string{"error": "POST only"})
		return
	}
	writeJSON(w, map[string]bool{"ok": true})
	go func() {
		time.Sleep(300 * time.Millisecond) // let the response flush
		platformQuitApp()
	}()
}

func handleJob(w http.ResponseWriter, r *http.Request) {
	jobID := strings.TrimSpace(r.URL.Query().Get("id"))
	st, ok := jobs.snapshot(jobID)
	if jobID != "" && !ok {
		writeJSONStatus(w, http.StatusNotFound, map[string]string{"error": "任务不存在或已过期"})
		return
	}
	if st.Log == "" {
		if b, err := os.ReadFile(jobLogPath()); err == nil {
			st.Log = tailString(string(b), 40000)
		}
	} else {
		st.Log = tailString(st.Log, 40000)
	}
	resp := map[string]interface{}{}
	raw, _ := json.Marshal(st)
	_ = json.Unmarshal(raw, &resp)
	if st.Running {
		resp["progress"] = loadJobProgress(st.Action)
	}
	writeJSON(w, resp)
}

func handleTaskHistory(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		writeJSONStatus(w, http.StatusMethodNotAllowed, map[string]string{"error": "仅支持 GET"})
		return
	}
	writeJSON(w, map[string]interface{}{"items": loadTaskHistory()})
}

func handleSettings(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		writeJSONStatus(w, 405, map[string]string{"error": "POST only"})
		return
	}
	settingsRequestMu.Lock()
	defer settingsRequestMu.Unlock()
	var raw map[string]json.RawMessage
	if err := json.NewDecoder(r.Body).Decode(&raw); err != nil {
		writeJSONStatus(w, 400, map[string]string{"error": err.Error()})
		return
	}

	set := loadSettings()
	previousSettings := set
	requestedLanguage := ""
	cacheLimitMB := 0
	hasCacheLimit := false
	if value, ok := raw["imdb_cache_max_mb"]; ok {
		if runtime.GOOS != "darwin" {
			writeJSONStatus(w, 400, map[string]string{"error": "IMDb 抓取缓存设置仅在 macOS 启用"})
			return
		}
		if err := json.Unmarshal(value, &cacheLimitMB); err != nil || cacheLimitMB < minIMDbCacheMaxMB || cacheLimitMB > maxIMDbCacheMaxMB {
			writeJSONStatus(w, 400, map[string]string{"error": "IMDb 抓取缓存上限必须是 64 到 65536 之间的整数 MB"})
			return
		}
		hasCacheLimit = true
	}
	if v, ok := raw["interval_seconds"]; ok {
		var n int
		if err := json.Unmarshal(v, &n); err != nil || n < 30 || n > 86400 {
			writeJSONStatus(w, 400, map[string]string{"error": "后台检查周期必须在 30 秒到 86400 秒之间"})
			return
		}
		set.IntervalSeconds = n
	}

	// auto_start remains an input alias for older cached Web UIs. It no longer
	// owns a macOS LaunchAgent; the preference is applied after this app starts.
	autoModeValue, hasAutoModeValue := raw["auto_mode_on_app_start"]
	if !hasAutoModeValue {
		autoModeValue, hasAutoModeValue = raw["auto_start"]
	}
	if hasAutoModeValue {
		var enabled bool
		if err := json.Unmarshal(autoModeValue, &enabled); err != nil {
			writeJSONStatus(w, 400, map[string]string{"error": err.Error()})
			return
		}
		set.AutoModeOnAppStart = enabled
		set.AutoModeOnAppStartConfigured = true
	}

	if v, ok := raw["app_auto_start"]; ok {
		var enabled bool
		if err := json.Unmarshal(v, &enabled); err != nil {
			writeJSONStatus(w, 400, map[string]string{"error": err.Error()})
			return
		}
		if err := platformSetAppAutoStart(enabled, io.Discard); err != nil {
			writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
			return
		}
		set.AppAutoStart = enabled
		set.AppAutoStartConfigured = true
	}

	if v, ok := raw["language"]; ok {
		var language string
		if err := json.Unmarshal(v, &language); err != nil || !supportedLanguage(language) {
			writeJSONStatus(w, 400, map[string]string{"error": "语言只能选择简体中文或 English (United States)"})
			return
		}
		set.Language = language
		requestedLanguage = language
	}

	var settingsErr error
	if requestedLanguage != "" {
		settingsErr = saveLanguagePreference(previousSettings, set, requestedLanguage)
	} else {
		settingsErr = saveSettings(set)
	}
	if settingsErr != nil {
		writeJSONStatus(w, 500, map[string]string{"error": settingsErr.Error()})
		return
	}
	if hasCacheLimit {
		if err := platformSetIMDbCacheMaxMB(cacheLimitMB); err != nil {
			writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
			return
		}
		go func() {
			if err := platformRunEngine("cache-maintain", "", io.Discard); err != nil {
				appendManagerLog("cache maintain after settings: " + err.Error())
			}
		}()
	}

	if v, ok := raw["roots"]; ok {
		if runtime.GOOS != "darwin" {
			writeJSONStatus(w, 400, map[string]string{"error": "当前平台不支持手工资料库配置"})
			return
		}
		var roots []string
		if err := json.Unmarshal(v, &roots); err != nil {
			writeJSONStatus(w, 400, map[string]string{"error": err.Error()})
			return
		}
		if err := saveMacRoots(roots); err != nil {
			writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
			return
		}
	}
	if v, ok := raw["library_roots"]; ok {
		if runtime.GOOS != "darwin" {
			writeJSONStatus(w, 400, map[string]string{"error": "当前平台不支持手工资料库配置"})
			return
		}
		var roots LibraryRoots
		if err := json.Unmarshal(v, &roots); err != nil {
			writeJSONStatus(w, 400, map[string]string{"error": err.Error()})
			return
		}
		if err := saveMacLibraryRoots(roots); err != nil {
			writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
			return
		}
	}

	writeJSON(w, map[string]bool{"ok": true})
}

func handlePreviewCandidates(w http.ResponseWriter, r *http.Request) {
	if runtime.GOOS != "darwin" {
		writeJSONStatus(w, 400, map[string]string{"error": "指定 NFO AI 预演仅在 macOS Manager 中启用"})
		return
	}
	if r.Method != http.MethodGet {
		writeJSONStatus(w, 405, map[string]string{"error": "GET only"})
		return
	}
	items, err := platformPreviewCandidates()
	if err != nil {
		writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
		return
	}
	writeJSON(w, map[string]interface{}{"items": items})
}

func writeInspectorPayload(w http.ResponseWriter, payload json.RawMessage, err error) {
	status := http.StatusOK
	if err != nil {
		status = http.StatusBadRequest
		var problem map[string]interface{}
		if json.Unmarshal(payload, &problem) == nil && problem["kind"] == "EditConflictError" {
			status = http.StatusConflict
		}
	}
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	w.WriteHeader(status)
	_, _ = w.Write(payload)
}

func handleLibrary(w http.ResponseWriter, r *http.Request) {
	if runtime.GOOS != "darwin" {
		writeJSONStatus(w, 400, map[string]string{"error": "NFO Inspector 当前仅在 macOS 启用"})
		return
	}
	if r.Method != http.MethodGet {
		writeJSONStatus(w, 405, map[string]string{"error": "仅支持 GET"})
		return
	}
	if !platformLibraryRootsConfirmed() {
		writeJSON(w, map[string]interface{}{"schema": 1, "items": []interface{}{}, "onboarding_required": true})
		return
	}
	payload, err := platformInspectorJSON("--library-index")
	writeInspectorPayload(w, payload, err)
}

// handleLibraryChanges exposes a revisioned catalog delta.  During a long
// reconcile the Web UI polls this small payload instead of repeatedly loading
// and rendering the entire library snapshot.
func handleLibraryChanges(w http.ResponseWriter, r *http.Request) {
	if runtime.GOOS != "darwin" {
		writeJSONStatus(w, 400, map[string]string{"error": "NFO Inspector 当前仅在 macOS 启用"})
		return
	}
	if r.Method != http.MethodGet {
		writeJSONStatus(w, 405, map[string]string{"error": "仅支持 GET"})
		return
	}
	if !platformLibraryRootsConfirmed() {
		writeJSON(w, map[string]interface{}{"schema": 1, "reset": true, "revision": 0, "items": []interface{}{}, "changes": []interface{}{}, "onboarding_required": true})
		return
	}
	since := strings.TrimSpace(r.URL.Query().Get("since"))
	if since == "" {
		since = "0"
	}
	if value, err := strconv.ParseInt(since, 10, 64); err != nil || value < 0 {
		writeJSONStatus(w, 400, map[string]string{"error": "资料库版本号无效"})
		return
	}
	payload, err := platformInspectorJSON("--library-changes-since", since)
	writeInspectorPayload(w, payload, err)
}

func handleInspector(w http.ResponseWriter, r *http.Request) {
	if runtime.GOOS != "darwin" {
		writeJSONStatus(w, 400, map[string]string{"error": "NFO Inspector 当前仅在 macOS 启用"})
		return
	}
	if r.Method != http.MethodGet {
		writeJSONStatus(w, 405, map[string]string{"error": "仅支持 GET"})
		return
	}
	path := strings.TrimSpace(r.URL.Query().Get("path"))
	if path == "" {
		writeJSONStatus(w, 400, map[string]string{"error": "NFO 路径为空"})
		return
	}
	payload, err := platformInspectorJSON("--inspector-path", path)
	writeInspectorPayload(w, payload, err)
}

func handleInspectorMutation(w http.ResponseWriter, r *http.Request, flag string) {
	if runtime.GOOS != "darwin" {
		writeJSONStatus(w, 400, map[string]string{"error": "NFO Inspector 当前仅在 macOS 启用"})
		return
	}
	if r.Method != http.MethodPost {
		writeJSONStatus(w, 405, map[string]string{"error": "仅支持 POST"})
		return
	}
	var request map[string]interface{}
	decoder := json.NewDecoder(io.LimitReader(r.Body, 1<<20))
	if err := decoder.Decode(&request); err != nil {
		writeJSONStatus(w, 400, map[string]string{"error": "请求 JSON 无效：" + err.Error()})
		return
	}
	body, err := json.Marshal(request)
	if err != nil {
		writeJSONStatus(w, 400, map[string]string{"error": err.Error()})
		return
	}
	payload, runErr := platformInspectorJSON(flag, string(body))
	writeInspectorPayload(w, payload, runErr)
}

func handleInspectorEdit(w http.ResponseWriter, r *http.Request) {
	handleInspectorMutation(w, r, "--inspector-edit-json")
}

func handleInspectorUndo(w http.ResponseWriter, r *http.Request) {
	handleInspectorMutation(w, r, "--inspector-undo-json")
}

func handleInspectorIssues(w http.ResponseWriter, r *http.Request) {
	handleInspectorMutation(w, r, "--inspector-issues-json")
}

func handleInspectorStatus(w http.ResponseWriter, r *http.Request) {
	handleInspectorMutation(w, r, "--status-override-json")
}

func handleInspectorReload(w http.ResponseWriter, r *http.Request) {
	handleInspectorMutation(w, r, "--inspector-reload-json")
}

func engineStatePath(name string) string {
	return filepath.Join(platformDataPath(), name)
}

func handlePreviewResults(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		writeJSONStatus(w, 405, map[string]string{"error": "GET only"})
		return
	}
	b, err := os.ReadFile(engineStatePath("preview-results.json"))
	if err != nil {
		writeJSON(w, map[string]interface{}{"records": []interface{}{}, "skipped": []interface{}{}})
		return
	}
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	_, _ = w.Write(b)
}

func handleScopePreflight(w http.ResponseWriter, r *http.Request) {
	handleInspectorMutation(w, r, "--scope-preflight-json")
}

func handleAIConfig(w http.ResponseWriter, r *http.Request) {
	if runtime.GOOS != "darwin" {
		writeJSONStatus(w, 400, map[string]string{"error": "AI Tag Parser 当前仅在 macOS Manager 中启用"})
		return
	}
	if r.Method == http.MethodGet {
		cfg, err := platformLoadAIConfig()
		if err != nil {
			writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
			return
		}
		writeJSON(w, AIConfigResponse{AIConfig: cfg, HasAPIKey: platformAISecretExists(), DefaultPrompt: defaultAIPrompt})
		return
	}
	if r.Method != http.MethodPost {
		writeJSONStatus(w, 405, map[string]string{"error": "GET/POST only"})
		return
	}
	var req aiConfigRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeJSONStatus(w, 400, map[string]string{"error": err.Error()})
		return
	}
	cfg := req.AIConfig
	cfg.APIProtocol = strings.ToLower(strings.TrimSpace(cfg.APIProtocol))
	cfg.Provider = strings.TrimSpace(cfg.Provider)
	cfg.BaseURL = strings.TrimSpace(cfg.BaseURL)
	cfg.Model = strings.TrimSpace(cfg.Model)
	cfg.Prompt = strings.TrimSpace(cfg.Prompt)
	cfg.JSONMode = strings.TrimSpace(cfg.JSONMode)
	cfg.ThinkingMode = strings.TrimSpace(cfg.ThinkingMode)
	cfg.PromptCacheMode = strings.TrimSpace(cfg.PromptCacheMode)
	cfg.ExtraBody = strings.TrimSpace(cfg.ExtraBody)
	cfg.FallbackMode = strings.TrimSpace(cfg.FallbackMode)
	cfg.LegacyCleanupMode = strings.TrimSpace(cfg.LegacyCleanupMode)
	cfg.WarningPolicy = strings.TrimSpace(cfg.WarningPolicy)
	if cfg.Provider == "" {
		cfg.Provider = "openai-compatible"
	}
	if cfg.APIProtocol != "openai" && cfg.APIProtocol != "anthropic" {
		writeJSONStatus(w, 400, map[string]string{"error": "API 格式必须选择 OpenAI Chat Completions 或 Anthropic Messages"})
		return
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
	if cfg.FallbackMode == "" {
		cfg.FallbackMode = "abort"
	}
	if cfg.LegacyCleanupMode == "" {
		cfg.LegacyCleanupMode = "strict"
	}
	if cfg.WarningPolicy == "" {
		cfg.WarningPolicy = "review"
	}
	if cfg.Temperature < 0 || cfg.Temperature >= 2 {
		writeJSONStatus(w, 400, map[string]string{"error": "temperature 必须在 [0, 2)"})
		return
	}
	if cfg.TopP <= 0 || cfg.TopP > 1 {
		writeJSONStatus(w, 400, map[string]string{"error": "top_p 必须在 (0, 1]"})
		return
	}
	if cfg.MaxTokens < 128 || cfg.MaxTokens > 32768 {
		writeJSONStatus(w, 400, map[string]string{"error": "max_tokens 必须在 128 到 32768 之间"})
		return
	}
	if cfg.OutputTokenCap < 4096 || cfg.OutputTokenCap > 32768 || cfg.OutputTokenCap < cfg.MaxTokens {
		writeJSONStatus(w, 400, map[string]string{"error": "截断恢复上限必须在 4096 到 32768 之间，且不能小于初始输出上限"})
		return
	}
	if cfg.TimeoutSeconds < 10 || cfg.TimeoutSeconds > 600 {
		writeJSONStatus(w, 400, map[string]string{"error": "timeout 必须在 10 到 600 秒之间"})
		return
	}
	if cfg.JSONMode != "auto" && cfg.JSONMode != "on" && cfg.JSONMode != "off" {
		writeJSONStatus(w, 400, map[string]string{"error": "json_mode 无效"})
		return
	}
	if cfg.ThinkingMode != "off" && cfg.ThinkingMode != "auto" && cfg.ThinkingMode != "on" {
		writeJSONStatus(w, 400, map[string]string{"error": "thinking_mode 无效"})
		return
	}
	if cfg.PromptCacheMode != "off" && cfg.PromptCacheMode != "auto" && cfg.PromptCacheMode != "on" {
		writeJSONStatus(w, 400, map[string]string{"error": "prompt_cache_mode 无效"})
		return
	}
	if cfg.FallbackMode != "abort" && cfg.FallbackMode != "local-rules" {
		writeJSONStatus(w, 400, map[string]string{"error": "fallback_mode 无效"})
		return
	}
	if cfg.LegacyCleanupMode != "strict" && cfg.LegacyCleanupMode != "inferred" {
		writeJSONStatus(w, 400, map[string]string{"error": "legacy_cleanup_mode 无效"})
		return
	}
	if cfg.WarningPolicy != "review" && cfg.WarningPolicy != "accept" {
		writeJSONStatus(w, 400, map[string]string{"error": "warning_policy 无效"})
		return
	}
	if cfg.RetryCount < 0 || cfg.RetryCount > 5 {
		writeJSONStatus(w, 400, map[string]string{"error": "retry_count 必须在 0 到 5 之间"})
		return
	}
	if cfg.InputPricePerMillion < 0 || cfg.OutputPricePerMillion < 0 || cfg.RunCostLimit < 0 || cfg.RunRequestLimit < 0 || cfg.RunTokenLimit < 0 {
		writeJSONStatus(w, 400, map[string]string{"error": "价格和预算限制不能为负数"})
		return
	}
	if cfg.ExtraBody == "" {
		cfg.ExtraBody = "{}"
	}
	var extra map[string]interface{}
	if err := json.Unmarshal([]byte(cfg.ExtraBody), &extra); err != nil {
		writeJSONStatus(w, 400, map[string]string{"error": "额外请求参数必须是 JSON 对象：" + err.Error()})
		return
	}
	if cfg.Enabled && (cfg.BaseURL == "" || cfg.Model == "" || cfg.Prompt == "") {
		writeJSONStatus(w, 400, map[string]string{"error": "启用 AI 时 Base URL、模型名和提示词不能为空"})
		return
	}
	if err := platformSaveAIConfig(cfg); err != nil {
		writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
		return
	}
	if req.ClearAPIKey {
		if err := platformClearAISecret(); err != nil {
			writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
			return
		}
	} else if strings.TrimSpace(req.APIKey) != "" {
		if err := platformSaveAISecret(strings.TrimSpace(req.APIKey)); err != nil {
			writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
			return
		}
	}
	writeJSON(w, AIConfigResponse{AIConfig: cfg, HasAPIKey: platformAISecretExists(), DefaultPrompt: defaultAIPrompt})
}

func handleAction(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		writeJSONStatus(w, 405, map[string]string{"error": "POST only"})
		return
	}
	var req actionRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeJSONStatus(w, 400, map[string]string{"error": err.Error()})
		return
	}
	req.Action = strings.TrimSpace(req.Action)
	req.IMDb = strings.TrimSpace(req.IMDb)
	req.Path = strings.TrimSpace(req.Path)

	switch req.Action {
	case "choose-library-folder":
		label := "资料库"
		if req.Path == "movies" {
			label = "电影资料库"
		} else if req.Path == "tv" {
			label = "电视剧资料库"
		}
		path, err := platformChooseLibraryFolder(label)
		if err != nil {
			writeJSONStatus(w, 400, map[string]string{"error": err.Error()})
			return
		}
		writeJSON(w, map[string]string{"ok": "true", "path": path})
		return
	case "test-library-root":
		if runtime.GOOS != "darwin" {
			writeJSONStatus(w, http.StatusBadRequest, map[string]string{"error": "资料库访问测试仅在 macOS 启用"})
			return
		}
		if req.Path == "" {
			writeJSONStatus(w, http.StatusBadRequest, map[string]string{"error": "资料库路径为空"})
			return
		}
		result, err := platformTestLibraryRoot(req.Path)
		if err != nil {
			writeJSONStatus(w, http.StatusBadRequest, map[string]string{"error": err.Error()})
			return
		}
		writeJSON(w, result)
		return
	case "open-data":
		if err := openPath(platformDataPath()); err != nil {
			writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
			return
		}
		writeJSON(w, map[string]bool{"ok": true})
		return
	case "open-logs":
		_ = os.MkdirAll(logDir(), 0755)
		if err := openPath(logDir()); err != nil {
			writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
			return
		}
		writeJSON(w, map[string]bool{"ok": true})
		return
	case "open-path":
		if req.Path == "" {
			writeJSONStatus(w, 400, map[string]string{"error": "路径为空"})
			return
		}
		p := req.Path
		if info, err := os.Stat(p); err == nil && !info.IsDir() {
			p = filepath.Dir(p)
		} else if err != nil {
			p = filepath.Dir(p)
		}
		if err := openPath(p); err != nil {
			writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
			return
		}
		writeJSON(w, map[string]bool{"ok": true})
		return
	case "export-diagnostics":
		p, err := exportDiagnostics()
		if err != nil {
			writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
			return
		}
		_ = openPath(filepath.Dir(p))
		writeJSON(w, map[string]string{"ok": "true", "path": p})
		return
	}

	if req.Action == "ai-task-pause" {
		if runtime.GOOS != "darwin" {
			writeJSONStatus(w, 400, map[string]string{"error": "AI 批量任务暂停仅在 macOS 启用"})
			return
		}
		if err := platformRequestAITaskPause(); err != nil {
			writeJSONStatus(w, 500, map[string]string{"error": err.Error()})
			return
		}
		writeJSON(w, map[string]string{"ok": "true", "message": "已请求暂停；当前 NFO 完成后停止取下一项"})
		return
	}

	if req.Action == "install" && runtime.GOOS == "windows" {
		if err := requestElevatedAction("install"); err == nil {
			writeJSON(w, map[string]string{"ok": "true", "message": "已请求管理员权限，请确认 UAC。"})
			return
		}
	}

	arg := req.IMDb
	if req.Action == "reconcile-index" {
		arg = strings.TrimSpace(req.Path)
		if arg != "" && arg != "movies" && arg != "tv" {
			writeJSONStatus(w, 400, map[string]string{"error": "刷新范围只能是 movies 或 tv"})
			return
		}
	}
	previewActions := map[string]bool{"ai-preview-selected": true, "ai-preview-write-selected": true, "local-preview-write-selected": true}
	if previewActions[req.Action] || req.Action == "ai-generate-selected" || req.Action == "ai-approve-selected" || req.Action == "local-generate-selected" || req.Action == "local-approve-selected" || req.Action == "refresh-selected" {
		if len(req.Paths) == 0 {
			writeJSONStatus(w, 400, map[string]string{"error": "请选择至少一个 NFO"})
			return
		}
		limit := 10
		label := "预演"
		if req.Action == "ai-generate-selected" || req.Action == "ai-approve-selected" || req.Action == "local-generate-selected" || req.Action == "local-approve-selected" || req.Action == "refresh-selected" {
			limit = 10000
			label = "写入"
		}
		if len(req.Paths) > limit {
			writeJSONStatus(w, 400, map[string]string{"error": fmt.Sprintf("一次最多%s %d 个 NFO", label, limit)})
			return
		}
		b, err := json.Marshal(req.Paths)
		if err != nil {
			writeJSONStatus(w, 400, map[string]string{"error": err.Error()})
			return
		}
		arg = string(b)
	}
	jobID, err := startJob(req.Action, arg)
	if err != nil {
		writeJSONStatus(w, 409, map[string]string{"error": err.Error()})
		return
	}
	writeJSON(w, map[string]interface{}{"ok": true, "job_id": jobID, "action": req.Action})
}

func startJob(action, arg string) (string, error) {
	if runtime.GOOS == "darwin" && actionNeedsLibrary(action) && !platformLibraryRootsConfirmed() {
		return "", fmt.Errorf("请先在首次运行引导中确认电影和电视剧资料库")
	}
	st, err := jobs.begin(action)
	if err != nil {
		return "", err
	}

	go func() {
		_ = os.MkdirAll(logDir(), 0755)
		f, err := os.Create(jobLogPath())
		if err != nil {
			finishJob(st.ID, action, 1, err.Error())
			return
		}
		defer f.Close()
		fmt.Fprintf(f, "IMDb Tech Manager %s\nJob: %s\nAction: %s\nStarted: %s\n\n", appVersion, st.ID, action, st.StartedAt)
		err = performActionWithWriter(action, arg, f)
		code := 0
		msg := "完成"
		if err != nil {
			code = 1
			msg = err.Error()
			fmt.Fprintf(f, "\nERROR: %v\n", err)
		}
		_ = f.Sync()
		finishJob(st.ID, action, code, msg)
	}()
	return st.ID, nil
}

func finishJob(jobID, action string, code int, msg string) {
	b, _ := os.ReadFile(jobLogPath())
	history := jobs.complete(jobID, action, code, msg, string(b))
	appendTaskHistory(history)
	// After generation/spec tasks the library may have new files or changed
	// NFOs: run a background reconcile so the catalog (and the UI list)
	// catches up without a manual refresh.
	switch action {
	case "ai-generate-selected", "local-generate-selected", "ai-generate", "ai-rebuild",
		"local-generate", "local-rebuild", "ai-resume-task", "ai-retry-failed",
		"refresh-selected", "ai-approve-selected", "local-approve-selected", "refresh", "backfill", "reconcile-run":
		scheduleReconcile(800 * time.Millisecond)
	}
}

// scheduleReconcile debounces startup and post-task refreshes.  The old
// implementation created a new reconcile goroutine after every write task;
// when a task completed while startup reconcile was still running, the next
// refresh either failed with "已有任务" or ran again immediately.  One
// scheduled worker waits for the current job to finish and retries briefly.
func scheduleReconcile(delay time.Duration) {
	if !reconcileScheduled.CompareAndSwap(false, true) {
		return
	}
	go func() {
		defer reconcileScheduled.Store(false)
		time.Sleep(delay)
		for attempt := 0; attempt < 60; attempt++ {
			if runtime.GOOS != "darwin" || platformLibraryRootsConfirmed() {
				jobs.mu.Lock()
				running := jobs.st.Running
				jobs.mu.Unlock()
				if !running {
					if _, err := startJob("reconcile-index", ""); err == nil {
						return
					}
				}
			}
			time.Sleep(500 * time.Millisecond)
		}
	}()
}

func actionNeedsLibrary(action string) bool {
	switch action {
	case "auto", "run", "backfill", "reconcile", "reconcile-index", "refresh", "refresh-selected", "ai-scan", "ai-preview", "ai-preview-selected", "ai-preview-write-selected", "local-preview-write-selected", "ai-generate-selected", "ai-approve-selected", "local-generate-selected", "local-approve-selected", "ai-migrate", "pipeline-scan", "local-generate", "local-rebuild", "ai-generate", "ai-rebuild", "ai-resume", "ai-resume-task", "ai-retry-failed":
		return true
	default:
		return false
	}
}

func jobHistoryPath() string { return filepath.Join(baseDir(), "task-history.json") }

func loadTaskHistory() []JobState {
	b, err := os.ReadFile(jobHistoryPath())
	if err != nil {
		return []JobState{}
	}
	var history []JobState
	if json.Unmarshal(b, &history) != nil || history == nil {
		return []JobState{}
	}
	return history
}

func appendTaskHistory(item JobState) {
	history := loadTaskHistory()
	item.Log = ""
	history = append([]JobState{item}, history...)
	if len(history) > 100 {
		history = history[:100]
	}
	b, err := json.MarshalIndent(history, "", "  ")
	if err == nil {
		_ = atomicWrite(jobHistoryPath(), b, 0644)
	}
}

func agentLoop() error {
	if platformAgentAlreadyRunning() {
		return nil
	}
	_ = os.MkdirAll(baseDir(), 0755)
	_ = os.WriteFile(agentPIDPath(), []byte(strconv.Itoa(os.Getpid())), 0644)
	defer os.Remove(agentPIDPath())

	for {
		cycleStart := time.Now()
		now := cycleStart.UTC().Format(time.RFC3339)
		_ = os.WriteFile(agentHeartbeatPath(), []byte(now), 0644)
		_ = saveAgentCycle(AgentCycleState{Running: true, StartedAt: cycleStart.Format(time.RFC3339)})

		var cycleErr error
		f, err := os.OpenFile(agentLogPath(), os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0644)
		if err == nil {
			fmt.Fprintf(f, "\n[%s] background cycle\n", now)
			cycleErr = performActionWithWriter("auto", "", f)
			if cycleErr != nil {
				fmt.Fprintf(f, "ERROR: %v\n", cycleErr)
			}
			f.Close()
		} else {
			cycleErr = err
		}

		cycleEnd := time.Now()
		cs := AgentCycleState{
			Running:    false,
			StartedAt:  cycleStart.Format(time.RFC3339),
			EndedAt:    cycleEnd.Format(time.RFC3339),
			DurationMs: cycleEnd.Sub(cycleStart).Milliseconds(),
		}
		if cycleErr != nil {
			cs.Error = cycleErr.Error()
		}
		_ = saveAgentCycle(cs)
		_ = os.WriteFile(agentHeartbeatPath(), []byte(cycleEnd.UTC().Format(time.RFC3339)), 0644)

		interval := loadSettings().IntervalSeconds
		if interval < 30 {
			interval = 60
		}
		time.Sleep(time.Duration(interval) * time.Second)
	}
}

func performImmediateAction(action, arg string) error {
	_ = os.MkdirAll(logDir(), 0755)
	f, err := os.OpenFile(managerLogPath(), os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0644)
	if err != nil {
		return err
	}
	defer f.Close()
	return performActionWithWriter(action, arg, f)
}

func performActionWithWriter(action, arg string, w io.Writer) error {
	switch action {
	case "install":
		return platformInstall(w)
	case "start":
		return platformStartAgent(w)
	case "stop":
		return platformStopAgent(w)
	case "cleanup-legacy":
		_, err := cleanupLegacyArtifacts(w)
		return err
	case "auto":
		return platformRunEngine("auto", arg, w)
	case "run":
		return platformRunEngine("run", arg, w)
	case "reconcile-index":
		return platformRunEngine("reconcile-index", arg, w)
	case "backfill", "reconcile", "refresh", "test-imdb", "diagnose", "repair-web", "rebuild-index", "cache-maintain", "cache-clear", "ai-test", "ai-recover", "ai-scan", "ai-preview", "ai-preview-selected", "ai-preview-write-selected", "local-preview-write-selected", "ai-generate-selected", "ai-approve-selected", "local-generate-selected", "local-approve-selected", "refresh-selected", "ai-migrate", "pipeline-scan", "local-generate", "local-rebuild", "ai-generate", "ai-rebuild", "ai-resume", "ai-resume-task", "ai-retry-failed":
		return platformRunEngine(action, arg, w)
	default:
		return fmt.Errorf("未知操作：%s", action)
	}
}

func ensureAssets() error {
	if err := os.MkdirAll(engineDir(), 0755); err != nil {
		return err
	}
	if runtime.GOOS != "darwin" {
		return fmt.Errorf("不支持的平台：%s", runtime.GOOS)
	}
	name := "engine/mac-engine.py"
	b, err := assets.ReadFile(name)
	if err != nil {
		return err
	}
	path := enginePath()
	old, _ := os.ReadFile(path)
	if string(old) != string(b) {
		if err := os.WriteFile(path, b, 0755); err != nil {
			return err
		}
	}
	_ = os.MkdirAll(logDir(), 0755)
	return nil
}

func loadSettings() Settings {
	s := Settings{IntervalSeconds: 60, Language: "zh-CN"}
	b, err := os.ReadFile(settingsPath())
	if err == nil {
		_ = json.Unmarshal(b, &s)
	}
	if s.IntervalSeconds < 30 {
		s.IntervalSeconds = 60
	}
	s.Language = normalizedLanguage(s.Language)
	return s
}

func saveSettings(s Settings) error {
	if s.IntervalSeconds < 30 {
		s.IntervalSeconds = 60
	}
	s.Language = normalizedLanguage(s.Language)
	root := map[string]json.RawMessage{}
	if existing, err := os.ReadFile(settingsPath()); err == nil {
		if err := json.Unmarshal(existing, &root); err != nil || root == nil {
			if err == nil {
				err = errors.New("settings root is not an object")
			}
			return fmt.Errorf("应用设置损坏，未覆盖原文件：%w", err)
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	knownBytes, err := json.Marshal(s)
	if err != nil {
		return err
	}
	var known map[string]json.RawMessage
	if err := json.Unmarshal(knownBytes, &known); err != nil {
		return err
	}
	for key, value := range known {
		root[key] = value
	}
	b, err := json.MarshalIndent(root, "", "  ")
	if err != nil {
		return err
	}
	return atomicWrite(settingsPath(), b, 0644)
}

var errStaleUILayout = errors.New("界面布局版本已过期")

func defaultUILayoutBytes() []byte {
	return []byte(`{"schema_version":1,"revision":0}`)
}

func parseUILayout(raw []byte) (uiLayoutEnvelope, error) {
	if len(raw) == 0 || len(raw) > uiLayoutMaxBytes {
		return uiLayoutEnvelope{}, errors.New("界面布局数据为空或过大")
	}
	var envelope uiLayoutEnvelope
	decoder := json.NewDecoder(strings.NewReader(string(raw)))
	if err := decoder.Decode(&envelope); err != nil {
		return uiLayoutEnvelope{}, fmt.Errorf("界面布局不是有效 JSON：%w", err)
	}
	if envelope.SchemaVersion != uiLayoutSchema {
		return uiLayoutEnvelope{}, fmt.Errorf("不支持的界面布局版本：%d", envelope.SchemaVersion)
	}
	var trailing interface{}
	if err := decoder.Decode(&trailing); !errors.Is(err, io.EOF) {
		return uiLayoutEnvelope{}, errors.New("界面布局包含多余内容")
	}
	return envelope, nil
}

func readUILayoutLocked() ([]byte, uiLayoutEnvelope, error) {
	raw, err := os.ReadFile(uiLayoutPath())
	if errors.Is(err, os.ErrNotExist) {
		raw = defaultUILayoutBytes()
		var envelope uiLayoutEnvelope
		_ = json.Unmarshal(raw, &envelope)
		return raw, envelope, nil
	}
	if err != nil {
		return nil, uiLayoutEnvelope{}, err
	}
	envelope, err := parseUILayout(raw)
	return raw, envelope, err
}

func loadUILayoutBytes() []byte {
	uiLayoutMu.Lock()
	defer uiLayoutMu.Unlock()
	raw, _, err := readUILayoutLocked()
	if err != nil {
		appendManagerLog("ui layout load: " + err.Error())
		return defaultUILayoutBytes()
	}
	return raw
}

func saveUILayoutBytes(raw []byte) error {
	incoming, err := parseUILayout(raw)
	if err != nil {
		return err
	}
	if incoming.Revision == 0 {
		return errors.New("界面布局修订号必须大于 0")
	}

	uiLayoutMu.Lock()
	defer uiLayoutMu.Unlock()
	currentRaw, current, readErr := readUILayoutLocked()
	if readErr == nil {
		if incoming.Revision < current.Revision {
			return errStaleUILayout
		}
		if incoming.Revision == current.Revision {
			if string(raw) == string(currentRaw) {
				return nil
			}
			return errStaleUILayout
		}
	} else {
		appendManagerLog("ui layout replace invalid state: " + readErr.Error())
	}
	return atomicWriteUILayout(raw)
}

func atomicWriteUILayout(raw []byte) error {
	path := uiLayoutPath()
	if err := os.MkdirAll(filepath.Dir(path), 0755); err != nil {
		return err
	}
	tmp, err := os.CreateTemp(filepath.Dir(path), ".ui-layout-*.tmp")
	if err != nil {
		return err
	}
	tmpPath := tmp.Name()
	committed := false
	defer func() {
		_ = tmp.Close()
		if !committed {
			_ = os.Remove(tmpPath)
		}
	}()
	if err := tmp.Chmod(0600); err != nil {
		return err
	}
	if _, err := tmp.Write(raw); err != nil {
		return err
	}
	if err := tmp.Sync(); err != nil {
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	if runtime.GOOS == "windows" {
		_ = os.Remove(path)
	}
	if err := os.Rename(tmpPath, path); err != nil {
		return err
	}
	committed = true
	if dir, err := os.Open(filepath.Dir(path)); err == nil {
		_ = dir.Sync()
		_ = dir.Close()
	}
	return nil
}

func atomicWrite(path string, b []byte, mode fs.FileMode) error {
	if err := os.MkdirAll(filepath.Dir(path), 0755); err != nil {
		return err
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, b, mode); err != nil {
		return err
	}
	if runtime.GOOS == "windows" {
		_ = os.Remove(path)
	}
	return os.Rename(tmp, path)
}

func writeJSON(w http.ResponseWriter, v interface{}) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	_ = json.NewEncoder(w).Encode(v)
}

func writeJSONStatus(w http.ResponseWriter, code int, v interface{}) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	w.WriteHeader(code)
	_ = json.NewEncoder(w).Encode(v)
}

func tailString(s string, max int) string {
	if len(s) <= max {
		return s
	}
	return "…\n" + s[len(s)-max:]
}

func appendManagerLog(s string) {
	_ = os.MkdirAll(logDir(), 0755)
	f, err := os.OpenFile(managerLogPath(), os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0644)
	if err == nil {
		fmt.Fprintf(f, "[%s] %s\n", time.Now().Format(time.RFC3339), s)
		f.Close()
	}
}

func cleanupLegacyArtifacts(w io.Writer) ([]string, error) {
	removed, err := platformCleanupLegacy(w)
	report := map[string]interface{}{
		"version": appVersion,
		"time":    time.Now().Format(time.RFC3339),
		"removed": removed,
	}
	b, _ := json.MarshalIndent(report, "", "  ")
	_ = atomicWrite(cleanupReportPath(), b, 0644)
	return removed, err
}

func upgradeInstalledRuntime() error {
	installed := installedExePath()
	if _, err := os.Stat(installed); err != nil {
		return nil
	}
	current, err := os.Executable()
	if err != nil {
		return err
	}
	current, _ = filepath.EvalSymlinks(current)
	if filepath.Clean(current) == filepath.Clean(installed) {
		return nil
	}
	same, err := sameFileContent(current, installed)
	if err == nil && same {
		return nil
	}

	wasRunning := platformAgentAlreadyRunning()
	var w io.Writer = io.Discard
	if f, err := os.OpenFile(managerLogPath(), os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0644); err == nil {
		defer f.Close()
		w = f
	}
	if wasRunning {
		_ = platformStopAgentProcessOnly(w)
		time.Sleep(500 * time.Millisecond)
	}
	if err := copySelf(installed); err != nil {
		return err
	}
	if wasRunning {
		if err := platformStartAgentProcessOnly(); err != nil {
			return err
		}
	}
	appendManagerLog("upgraded installed runtime to " + appVersion)
	return nil
}

func sameFileContent(a, b string) (bool, error) {
	ha, err := fileSHA256(a)
	if err != nil {
		return false, err
	}
	hb, err := fileSHA256(b)
	if err != nil {
		return false, err
	}
	return ha == hb, nil
}

func fileSHA256(path string) ([32]byte, error) {
	var zero [32]byte
	f, err := os.Open(path)
	if err != nil {
		return zero, err
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return zero, err
	}
	var out [32]byte
	copy(out[:], h.Sum(nil))
	return out, nil
}

func exportDiagnostics() (string, error) {
	_ = os.MkdirAll(logDir(), 0755)
	out := filepath.Join(logDir(), "IMDb-Tech-Diagnostics-"+time.Now().Format("20060102-150405")+".zip")
	f, err := os.Create(out)
	if err != nil {
		return "", err
	}
	zw := zip.NewWriter(f)
	paths := []string{
		settingsPath(), managerLogPath(), agentLogPath(), jobLogPath(),
		agentHeartbeatPath(), agentCyclePath(), cleanupReportPath(), enginePath(),
	}
	paths = append(paths, platformDiagnosticPaths()...)
	seen := map[string]bool{}
	for _, p := range paths {
		if p == "" || seen[p] {
			continue
		}
		seen[p] = true
		info, err := os.Stat(p)
		if err != nil || info.IsDir() {
			continue
		}
		r, err := os.Open(p)
		if err != nil {
			continue
		}
		h := &zip.FileHeader{Name: filepath.Base(p), Method: zip.Deflate}
		h.SetModTime(info.ModTime())
		zf, err := zw.CreateHeader(h)
		if err == nil {
			_, _ = io.Copy(zf, io.LimitReader(r, 12<<20))
		}
		r.Close()
	}
	if st, err := collectStatus(); err == nil {
		decorateCommonStatus(&st)
		zf, _ := zw.Create("status.json")
		enc := json.NewEncoder(zf)
		enc.SetIndent("", "  ")
		_ = enc.Encode(st)
	}
	_ = zw.Close()
	_ = f.Close()
	return out, nil
}

func runCommandToWriter(w io.Writer, name string, args ...string) error {
	cmd := hiddenCommand(name, args...)
	cmd.Stdout = w
	cmd.Stderr = w
	return cmd.Run()
}

func commandOutput(name string, args ...string) (string, error) {
	cmd := hiddenCommand(name, args...)
	b, err := cmd.CombinedOutput()
	return strings.TrimSpace(string(b)), err
}

func copySelf(dst string) error {
	src, err := os.Executable()
	if err != nil {
		return err
	}
	src, _ = filepath.EvalSymlinks(src)
	if filepath.Clean(src) == filepath.Clean(dst) {
		return nil
	}
	if err := os.MkdirAll(filepath.Dir(dst), 0755); err != nil {
		return err
	}
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	tmp := dst + ".tmp"
	_ = os.Remove(tmp)
	out, err := os.Create(tmp)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return err
	}
	if err := out.Close(); err != nil {
		return err
	}
	_ = os.Chmod(tmp, 0755)
	if runtime.GOOS == "windows" {
		_ = os.Remove(dst)
	}
	return os.Rename(tmp, dst)
}

func readAgentPID() int {
	b, err := os.ReadFile(agentPIDPath())
	if err != nil {
		return 0
	}
	n, _ := strconv.Atoi(strings.TrimSpace(string(b)))
	return n
}

func saveAgentCycle(s AgentCycleState) error {
	b, _ := json.MarshalIndent(s, "", "  ")
	return atomicWrite(agentCyclePath(), b, 0644)
}

func loadAgentCycle() AgentCycleState {
	var s AgentCycleState
	b, err := os.ReadFile(agentCyclePath())
	if err == nil {
		_ = json.Unmarshal(b, &s)
	}
	return s
}

func engineDir() string    { return filepath.Join(baseDir(), "engine") }
func logDir() string       { return filepath.Join(baseDir(), "logs") }
func settingsPath() string { return filepath.Join(baseDir(), "settings.json") }
func uiLayoutPath() string {
	if uiLayoutPathOverride != "" {
		return uiLayoutPathOverride
	}
	return filepath.Join(baseDir(), "ui-layout.json")
}
func managerLogPath() string     { return filepath.Join(logDir(), "manager.log") }
func agentLogPath() string       { return filepath.Join(logDir(), "agent.log") }
func jobLogPath() string         { return filepath.Join(logDir(), "job.log") }
func agentPIDPath() string       { return filepath.Join(baseDir(), "agent.pid") }
func agentHeartbeatPath() string { return filepath.Join(baseDir(), "agent-heartbeat.txt") }
func agentCyclePath() string     { return filepath.Join(baseDir(), "agent-cycle.json") }
func cleanupReportPath() string  { return filepath.Join(baseDir(), "cleanup-report.json") }

func init() {
	log.SetOutput(io.Discard)
}
