#!/usr/bin/env python3
import argparse, base64, contextlib, datetime as dt, hashlib, html, json, os, random, re, stat
import shutil, signal, subprocess, sys, tempfile, time, urllib.request, urllib.error
import xml.etree.ElementTree as ET
from html.parser import HTMLParser
from pathlib import Path
from xml.dom import minidom

APP = Path.home() / "Library/Application Support/tmm-imdb-tech"
CFG = APP / "config.json"
CACHE = APP / "cache"
CACHE_STATUS = APP / "cache-status.json"
CACHE_MAINTENANCE_LOCK = APP / "cache-maintenance.lock"
LOCK = APP / "run.lock"
PROFILE = APP / "chrome-profile"
STATUS = APP / "manager-status.json"
AI_CACHE = APP / "ai-cache"
AI_STATUS = APP / "ai-status.json"
AI_RUNTIME = APP / "ai-runtime.json"
PIPELINE_STATUS = APP / "pipeline-status.json"
OWNERSHIP_DIR = APP / "ownership"
UNDO_DIR = APP / "undo"
AI_BATCH_STATE = APP / "ai-batch-state.json"
AI_BATCH_QUEUE = APP / "ai-batch-queue.json"
AI_BATCH_PAUSE = APP / "ai-batch-pause.flag"
AI_FAILURE_QUEUE = APP / "ai-failure-queue.json"
ISSUE_ACKS = APP / "issue-acknowledgements.json"
ROOT_HEALTH = APP / "root-health.json"
INDEX_CACHE = APP / "index-cache.json"
STATUS_OVERRIDES = APP / "status-overrides.json"
JOB_PROGRESS = APP / "job-progress.json"
PREVIEW_RESULTS = APP / "preview-results.json"
MANUAL_TASK_FLAG = APP / "manual-task.flag"
AI_BATCH_SCHEMA = 2
INDEX_CACHE_SCHEMA = 3
STATUS_OVERRIDE_VALUES = (
    "ai-complete", "local-complete", "spec-ready", "spec-missing", "spec-empty",
    "no-tags", "stale", "review", "tag-missing", "legacy", "manual-spec",
)

FORMAT_VERSION = 21
CACHE_VERSION = 8
# Parsed Technical Specs have their own version.  A parser upgrade reuses the
# retained raw page, instead of turning every title into a network refresh.
PARSER_VERSION = 1
AI_CACHE_SCHEMA = 2
AI_RESULT_SCHEMA = 2
LOCAL_RULES_VERSION = "4.0.0"
WATCH_WINDOW = 900
STABLE_SECONDS = 90
CACHE_DAYS = 30
NO_TECH_CACHE_DAYS = 7
FETCH_ERROR_CACHE_HOURS = 1
IMDB_CACHE_DEFAULT_MAX_MB = 2048
IMDB_CACHE_MIN_MAX_MB = 64
IMDB_CACHE_MAX_MAX_MB = 65536
IMDB_CACHE_LOW_WATERMARK = 0.90
BACKFILL_BATCH = 1
CHROME_TIMEOUT_SECONDS = 20
CHROME_VIRTUAL_TIME_MS = 7000
CHROME_COMPAT_TIMEOUT_SECONDS = 35
CHROME_COMPAT_VIRTUAL_TIME_MS = 9000

SECTIONS = [
    "Runtime",
    "Sound mix",
    "Color",
    "Aspect ratio",
    "Camera",
    "Laboratory",
    "Film Length",
    "Negative Format",
    "Cinematographic Process",
    "Printed Film Format",
]
HEADINGS = set(SECTIONS)

# Only these fields become normal Emby <tag> values.
# All ten IMDb Technical Specifications fields remain in <technicalspecs>.
TAG_SECTIONS = [
    "Sound mix",
    "Camera",
    "Aspect ratio",
    "Negative Format",
    "Cinematographic Process",
    "Printed Film Format",
]
OLD_PREFIXES = SECTIONS + ["摄影器材", "后期实验室", "底片格式", "摄影工艺", "放映拷贝格式"]

UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
      "AppleWebKit/537.36 (KHTML, like Gecko) "
      "Chrome/151.0.0.0 Safari/537.36")

LAST_CHROME_STDERR = ""
LAST_CHROME_COMMAND = []
LAST_WEBKIT_STDERR = ""

# legacy performance counters are intentionally cheap call/duration counters.
# They stay per engine process, are reset at a task boundary, and are emitted
# once when the task finishes; collecting them must never add per-NFO I/O.
PERF_METRICS = {}
PERF_METRICS_STARTED = None


def metrics_incr(name, value=1):
    PERF_METRICS[name] = PERF_METRICS.get(name, 0) + int(value)


def metrics_snapshot():
    return dict(PERF_METRICS)


def metrics_reset():
    global PERF_METRICS_STARTED
    PERF_METRICS.clear()
    PERF_METRICS_STARTED = time.monotonic()


def metrics_duration(name, started):
    """Record a non-negative elapsed duration in milliseconds."""
    metrics_incr(name, int(max(0.0, time.monotonic() - started) * 1000))


def metrics_emit_summary(action, **extra):
    """Write one compact, machine-readable task summary to the task log."""
    total_ms = int(max(0.0, time.monotonic() - PERF_METRICS_STARTED) * 1000) if PERF_METRICS_STARTED else 0
    summary = dict(metrics_snapshot())
    summary.update(extra)
    summary["action"] = action
    summary["total_duration_ms"] = total_ms
    print("PERFORMANCE_SUMMARY " + json.dumps(summary, ensure_ascii=False, sort_keys=True), flush=True)
    return summary


def _xml_parse(text):
    metrics_incr("xml_parse_count")
    return ET.fromstring(text.encode("utf-8") if isinstance(text, str) else text)

def chrome_user_agent():
    """Match the known-good desktop UA shape to the installed Chrome major."""
    c = chrome()
    major = "151"
    if c:
        try:
            p = subprocess.run(
                [c, "--version"],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=5,
            )
            m = re.search(r"(\d{2,3})\.", (p.stdout or "") + " " + (p.stderr or ""))
            if m:
                major = m.group(1)
        except Exception:
            pass
    return (
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
        "AppleWebKit/537.36 (KHTML, like Gecko) "
        f"Chrome/{major}.0.0.0 Safari/537.36"
    )

def cleanup_profile_chrome_processes():
    """Kill only stale Chrome processes using the Manager headless profile."""
    marker = f"--user-data-dir={PROFILE}"
    try:
        p = subprocess.run(
            ["/bin/ps", "-axo", "pid=,command="],
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            timeout=5,
        )
        victims = []
        for line in (p.stdout or "").splitlines():
            line = line.strip()
            if not line or marker not in line:
                continue
            m = re.match(r"(\d+)\s+(.*)", line)
            if not m:
                continue
            pid = int(m.group(1))
            if pid != os.getpid():
                victims.append(pid)
        for pid in victims:
            with contextlib.suppress(Exception):
                os.kill(pid, signal.SIGTERM)
        if victims:
            time.sleep(0.4)
        for pid in victims:
            with contextlib.suppress(Exception):
                os.kill(pid, 0)
                os.kill(pid, signal.SIGKILL)
    except Exception:
        pass

last_fetch = 0.0

def save_json(path, obj):
    path.parent.mkdir(parents=True, exist_ok=True)
    # A fixed "*.tmp" name lets two engine processes overwrite one another
    # before either reaches os.replace(). Use a unique file in the target
    # directory so replace remains atomic across APFS volumes.
    fd, tmp_name = tempfile.mkstemp(prefix=path.name + ".", suffix=".tmp", dir=str(path.parent))
    tmp = Path(tmp_name)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            json.dump(obj, fh, ensure_ascii=False, indent=2)
            fh.flush()
            os.fsync(fh.fileno())
        os.replace(str(tmp), str(path))
    finally:
        with contextlib.suppress(FileNotFoundError):
            tmp.unlink()

def load_json(path, default=None):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return default

DEFAULT_AI_PROMPT = r'''你是一名熟悉电影摄影器材、镜头厂商与产品系列、声音制式、胶片/数字电影格式和 IMDb Technical Specifications 写法的专业元数据编辑。你的任务是把 IMDb Technical Specifications 转换为适合 Emby 搜索和筛选的结构化 Tag。

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
{"tags":[{"value":"Panavision C-Series Lenses","field":"Camera","source_indexes":[0],"confidence":"high","operation":"series-expansion"}],"warnings":[]}'''

DEFAULT_AI_PROMPT_LEGACY_LOCALIZED = DEFAULT_AI_PROMPT.replace(
    "15. 输入包含 output_language，仅用于 warnings（复核说明/警告）的自然语言。zh-CN 使用简体中文，en-US 使用美国英语。此字段绝不改变 tags.value、field、source_indexes、confidence、operation、JSON 键名或 Technical Specifications；这些结构化事实和标签必须保持原始内容、既定英文枚举与原有拼写，禁止因界面语言而翻译。\n16. 只返回 JSON，不要 Markdown，不要自然语言解释。",
    "15. 只返回 JSON，不要 Markdown，不要自然语言解释。",
    1,
)

# legacy is derived from the actual pre-language stock prompt. Keeping these
# historical values exact lets us upgrade known defaults without ever
# overwriting a user's customized prompt.
DEFAULT_AI_PROMPT_LEGACY_STRUCTURED = DEFAULT_AI_PROMPT_LEGACY_LOCALIZED.replace(
    "1. 只从 Sound mix、Aspect ratio、Camera、Negative Format、Cinematographic Process、Printed Film Format 生成 Tag，生成完检查每一项中有没有遗漏未生成的 Tag，除非这一项下面确实没有内容。Runtime、Color、Laboratory、Film Length 永远不生成 Tag。",
    "1. 只从 Sound mix、Aspect ratio、Camera、Negative Format、Cinematographic Process、Printed Film Format 生成 Tag。Runtime、Color、Laboratory、Film Length 永远不生成 Tag。",
    1,
).replace(
    "Arri Alexa 65 独立为机身，其余四项按语义共享 Lenses，且镜头和机身要拆开分别生成 Tag。",
    "Arri Alexa 65 独立为机身，其余四项按语义共享 Lenses。",
    1,
)

DEFAULT_AI_PROMPT_LEGACY_STRUCTURED = DEFAULT_AI_PROMPT.replace(
    "1. 只从 Sound mix、Aspect ratio、Camera、Negative Format、Cinematographic Process、Printed Film Format 生成 Tag，生成完检查每一项中有没有遗漏未生成的 Tag，除非这一项下面确实没有内容。Runtime、Color、Laboratory、Film Length 永远不生成 Tag。",
    "1. 只从 Sound mix、Aspect ratio、Camera、Negative Format、Cinematographic Process、Printed Film Format 生成 Tag。Runtime、Color、Laboratory、Film Length 永远不生成 Tag。",
    1,
).replace(
    "Arri Alexa 65 独立为机身，其余四项按语义共享 Lenses，且镜头和机身要拆开分别生成 Tag。",
    "Arri Alexa 65 独立为机身，其余四项按语义共享 Lenses。",
    1,
)

DEFAULT_AI_PROMPT_LEGACY_EXISTING_TAGS = r'''你是一名熟悉电影摄影器材、镜头厂商与产品系列、声音制式、胶片/数字电影格式和 IMDb Technical Specifications 写法的专业元数据编辑。你的任务是把 IMDb Technical Specifications 转换为适合 Emby 搜索和筛选的结构化 Tag。

核心原则：
- 可以充分使用你已有的影视器材和行业知识来理解原文，包括识别厂商名称、摄影机型号、镜头品牌、产品系列、技术制式、常见缩写，以及判断省略、并列和共享前后缀的边界。
- 行业知识用于“理解和恢复原文已经表达或明确省略的关系”，不是用于扩写 IMDb 数据。不要因为你知道某产品的真实规格、标准拼写、别名、所属厂商或常见搭配，就加入原始 Technical Specifications 没有表达的新技术事实。
- 当输入与已有知识冲突时，以输入为准。保留 IMDb 原始拼写、大小写、重音符号和型号写法；可以在 warnings 中提示疑似 typo，但不要自行纠正。
- 下面的例子只是行为示例，不是厂商、型号或语法枚举表。应泛化到你认识的其他厂商、产品系列和专业术语，不要只处理示例里出现过的内容。

规则：
1. 只从 Sound mix、Aspect ratio、Camera、Negative Format、Cinematographic Process、Printed Film Format 生成 Tag。Runtime、Color、Laboratory、Film Length 永远不生成 Tag。
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
14. 只返回 JSON，不要 Markdown，不要自然语言解释。

行为示例：
- Panavision C-, D-, E- and H-Series Lenses → Panavision C-Series Lenses / Panavision D-Series Lenses / Panavision E-Series Lenses / Panavision H-Series Lenses。
- Zeiss Standard Speed and Super Speed Lenses → Zeiss Standard Speed Lenses / Zeiss Super Speed Lenses。
- Panavision C-, E-Series and Super High Speed Lenses → Panavision C-Series Lenses / Panavision E-Series Lenses / Panavision Super High Speed Lenses；不要生成 Super High Speed Series Lenses。
- Bausch & Lomb Super Baltar and Angénieux Lenses → Bausch & Lomb Super Baltar Lenses / Angénieux Lenses；Bausch & Lomb 是完整名称，& 不应被拆。
- Hawk V-Lite, V-Plus, V-Series, Zeiss Master Prime and Angenieux Optimo Lenses → Hawk V-Lite Lenses / Hawk V-Plus Lenses / Hawk V-Series Lenses / Zeiss Master Prime Lenses / Angenieux Optimo Lenses；Hawk 前缀不传播到独立的 Zeiss/Angenieux 品牌。
- Arri Alexa 65, Viltrox Epic 1.33x, Luna Zoom, Arri Heroes T.One and IronGlass Helios Lenses → Arri Alexa 65 独立为机身，其余四项按语义共享 Lenses。
- Sony CineAlta Venice 2, Angénieux Optimo Ultra 12x, Type EZ, Fujinon Duvo HZK, Cooke 7/i Lenses → 可利用产品知识识别 Type EZ 与前一 Angénieux 项的省略关系，同时不能把 Angénieux 前缀传播给 Fujinon 或 Cooke。
- P+S Technik Skate Scope Masterbuilt Portrait and Soft Flare Lenses：如果你无法可靠确认 Portrait / Soft Flare 的共享前缀边界，保留整个镜头短语并 warning，而不是强拆。

返回结构：
{"tags":[{"value":"Panavision C-Series Lenses","field":"Camera","source_indexes":[0],"confidence":"high","operation":"series-expansion"}],"warnings":[]}'''

DEFAULT_AI_PROMPT_LEGACY_SPLIT = r'''你是一名熟悉电影摄影器材、镜头厂商与产品系列、声音制式、胶片/数字电影格式和 IMDb Technical Specifications 写法的专业元数据编辑。你的任务是把 IMDb Technical Specifications 转换为适合 Emby 搜索和筛选的结构化 Tag。

核心原则：
- 可以充分使用你已有的影视器材和行业知识来理解原文，包括识别厂商名称、摄影机型号、镜头品牌、产品系列、技术制式、常见缩写，以及判断省略、并列和共享前后缀的边界。
- 行业知识用于“理解和恢复原文已经表达或明确省略的关系”，不是用于扩写 IMDb 数据。不要因为你知道某产品的真实规格、标准拼写、别名、所属厂商或常见搭配，就加入原始 Technical Specifications 没有表达的新技术事实。
- 当输入与已有知识冲突时，以输入为准。保留 IMDb 原始拼写、大小写、重音符号和型号写法；可以在 warnings 中提示疑似 typo，但不要自行纠正。
- 下面的例子只是行为示例，不是厂商、型号或语法枚举表。应泛化到你认识的其他厂商、产品系列和专业术语，不要只处理示例里出现过的内容。

规则：
1. 只从 Sound mix、Aspect ratio、Camera、Negative Format、Cinematographic Process、Printed Film Format 生成 Tag。Runtime、Color、Laboratory、Film Length 永远不生成 Tag。
2. 允许利用行业知识帮助判断切分边界和明确省略。例如你知道某个词组是完整厂商名、镜头系列或摄影机型号时，可以据此避免错误断句；当一个并列列表明显省略了前面的厂商/系列前缀或末尾设备类型时，可以恢复这些省略成分。不得添加与原文无语义依据的规格、版本、品牌别名、产品特征或设备。
3. Sound mix：识别声音制式本体，去掉发行版本、拷贝、适用范围、声道数等说明性括号。括号若明确是在给出制式本体的规范名称，可用于规范化，例如 DTS (DTS: X) → DTS:X。DTS:X (7.1) → DTS:X；Auro 11.1 (Auro Max) 默认仍为 Auro 11.1。完全相同值去重。
4. Aspect ratio：只输出比例本体，规范为 1.43:1 形式，括号中的影院、版本、场景等说明不进入 Tag；相同比例去重。
5. Camera：把每个 bullet 当作一条摄影配置语句理解。优先利用你对厂商、产品系列和器材类别的知识判断“机身”和“Lens Expression”的边界，再结合语法处理并列、省略和共享前后缀。不要机械按逗号、and 或 & 分割。& 可能是厂商名称的一部分；and 也只有在语义上确实连接多个设备/镜头时才是分隔。
6. Camera 中末尾 Lenses/Lens 等设备类型可以向语义上属于同一镜头并列组的项目传播；厂商或系列前缀只传播到语义上确实属于该厂商/系列的项目。Series 只传播给真正属于 Series 省略组的项目。可以使用你对产品系列的认识来判断传播边界，不要求必须在本 Prompt 中见过该品牌。
7. Camera 中如果一个项目是明显的产品系列省略，可结合语法、同一标题其他 bullet 以及你已有的产品知识恢复。例如 “Angénieux Optimo Ultra 12x, Type EZ” 中，如果语义明确，可将 Type EZ 理解为同一 Angénieux 镜头组的省略项。这里恢复的是原句省略关系，不是新增一件原文没有提到的设备。
8. Camera bullet 末尾括号若语义上是整条拍摄配置的使用范围/季/版本/场景 qualifier（如 some scenes、one shot、aerial shots、Season 5、Rialto version、某特定场景名称等），通常不进入设备 Tag；但不要用固定关键词表机械判断，需结合设备名称与句子结构理解。原始 qualifier 仍由 <technicalspecs> 保留。
9. Negative Format、Cinematographic Process、Printed Film Format：IMDb 每个 bullet 默认视为一个完整 Tag；保留括号、逗号、季/场景/版本限定，不拆括号内部列表。只有原文本身存在明确的多项结构时才考虑拆分。
10. 每个 Tag 必须给 field 和 source_indexes（对应 field 数组的 0 起始索引）。完全相同 Tag 大小写不敏感去重。
11. 每个 Tag 给 confidence：high / medium / low。使用了行业知识并不自动降低 confidence；当原文、语法和已知产品命名高度一致、解析基本唯一时可以是 high。只有确实存在多种合理解析或需要明显猜测时才用 medium/low，并写入 warnings。
12. operation 用简短英文描述，例如 preserve、split-camera-lens、shared-suffix、shared-prefix、series-expansion、knowledge-assisted-boundary、normalize。
13. 如果无法可靠判断共享边界，宁可保留原短语为一个 Tag 并写 warning，也不要为了多切 Tag 而编造关系。
14. 只返回 JSON，不要 Markdown，不要自然语言解释。

行为示例：
- Panavision C-, D-, E- and H-Series Lenses → Panavision C-Series Lenses / Panavision D-Series Lenses / Panavision E-Series Lenses / Panavision H-Series Lenses。
- Zeiss Standard Speed and Super Speed Lenses → Zeiss Standard Speed Lenses / Zeiss Super Speed Lenses。
- Panavision C-, E-Series and Super High Speed Lenses → Panavision C-Series Lenses / Panavision E-Series Lenses / Panavision Super High Speed Lenses；不要生成 Super High Speed Series Lenses。
- Bausch & Lomb Super Baltar and Angénieux Lenses → Bausch & Lomb Super Baltar Lenses / Angénieux Lenses；Bausch & Lomb 是完整名称，& 不应被拆。
- Hawk V-Lite, V-Plus, V-Series, Zeiss Master Prime and Angenieux Optimo Lenses → Hawk V-Lite Lenses / Hawk V-Plus Lenses / Hawk V-Series Lenses / Zeiss Master Prime Lenses / Angenieux Optimo Lenses；Hawk 前缀不传播到独立的 Zeiss/Angenieux 品牌。
- Arri Alexa 65, Viltrox Epic 1.33x, Luna Zoom, Arri Heroes T.One and IronGlass Helios Lenses → Arri Alexa 65 独立为机身，其余四项按语义共享 Lenses。
- Sony CineAlta Venice 2, Angénieux Optimo Ultra 12x, Type EZ, Fujinon Duvo HZK, Cooke 7/i Lenses → 可利用产品知识识别 Type EZ 与前一 Angénieux 项的省略关系，同时不能把 Angénieux 前缀传播给 Fujinon 或 Cooke。
- P+S Technik Skate Scope Masterbuilt Portrait and Soft Flare Lenses：如果你无法可靠确认 Portrait / Soft Flare 的共享前缀边界，保留整个镜头短语并 warning，而不是强拆。

返回结构：
{"tags":[{"value":"Panavision C-Series Lenses","field":"Camera","source_indexes":[0],"confidence":"high","operation":"series-expansion"}],"warnings":[]}'''

DEFAULT_AI_PROMPT_LEGACY_INITIAL = r'''你是 IMDb Technical Specifications → Emby Tag 的保守型结构化解析器。只做“语义切分、明确省略恢复和格式规范”，不要补充外部知识。

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
{"tags":[{"value":"Panavision C-Series Lenses","field":"Camera","source_indexes":[0],"confidence":"high","operation":"series-expansion"}],"warnings":[]}'''

AI_KEYCHAIN_SERVICE = "local.imdb-tech-manager.ai"
AI_KEYCHAIN_ACCOUNT = "api-key"
AI_ALLOWED_FIELDS = set(TAG_SECTIONS)
LANGUAGE_CONTRACT_VERSION = 1
LANGUAGE_BOUNDARY_PROMPT = """\
Language boundary (mandatory, overrides any conflicting instruction): input.output_language controls only the natural language of warnings/review explanations. It must never translate or rewrite tags[].value, field, source_indexes, confidence, operation, JSON keys, or Technical Specifications. Preserve structured facts, established English enums, and source spelling exactly regardless of output_language.
""".strip()


def _effective_ai_prompt(cfg):
    prompt = str(cfg.get("prompt") or "").strip()
    return prompt + "\n\n" + LANGUAGE_BOUNDARY_PROMPT


def _accepted_ai_prompt_hashes(cfg):
    """Return current and narrowly compatible pre-boundary prompt hashes."""
    prompt = str(cfg.get("prompt") or "").strip()
    values = {_effective_ai_prompt(cfg), prompt}
    if prompt == DEFAULT_AI_PROMPT.strip():
        values.add(DEFAULT_AI_PROMPT_LEGACY_LOCALIZED.strip())
    return {
        hashlib.sha256(value.encode("utf-8")).hexdigest()[:16]
        for value in values if value
    }


def ai_config():
    cfg = load_json(CFG, {}) or {}
    raw = cfg.get("ai") if isinstance(cfg, dict) else None
    raw = raw if isinstance(raw, dict) else {}
    thinking_mode = clean(str(raw.get("thinking_mode") or "off")).lower()
    if thinking_mode not in ("off", "auto", "on"):
        thinking_mode = "off"
    prompt_cache_mode = clean(str(raw.get("prompt_cache_mode") or "auto")).lower()
    if prompt_cache_mode not in ("off", "auto", "on"):
        prompt_cache_mode = "auto"
    known_prompts = (DEFAULT_AI_PROMPT_LEGACY_INITIAL.strip(), DEFAULT_AI_PROMPT_LEGACY_SPLIT.strip(), DEFAULT_AI_PROMPT_LEGACY_EXISTING_TAGS.strip(), DEFAULT_AI_PROMPT_LEGACY_STRUCTURED.strip(), DEFAULT_AI_PROMPT_LEGACY_LOCALIZED.strip())
    old_token_defaults = int(raw.get("max_tokens", 0) or 0) == 1800 and int(raw.get("output_token_cap", 0) or 0) == 8192
    provider = clean(str(raw.get("provider") or "openai-compatible"))
    base_url = clean(str(raw.get("base_url") or ""))
    explicit_protocol = clean(str(raw.get("api_protocol") or "")).lower()
    if explicit_protocol not in ("openai", "anthropic"):
        low_provider = provider.lower()
        low_base = base_url.lower().rstrip("/")
        if "anthropic" in low_provider or "/apps/anthropic" in low_base or low_base.endswith("/v1/messages"):
            explicit_protocol = "anthropic"
        elif low_provider in ("", "openai", "openai-compatible", "bailian", "dashscope", "aliyun", "qwen") or \
                "/compatible-mode/v1" in low_base or low_base.endswith("/chat/completions"):
            explicit_protocol = "openai"
        else:
            explicit_protocol = ""
    out = {
        "enabled": bool(raw.get("enabled", False)),
        "api_protocol": explicit_protocol,
        "provider": provider,
        "base_url": base_url,
        "model": clean(str(raw.get("model") or "")),
        "prompt": (DEFAULT_AI_PROMPT if str(raw.get("prompt") or "").strip() in known_prompts else str(raw.get("prompt") or DEFAULT_AI_PROMPT)).strip(),
        "temperature": float(raw.get("temperature", 0) or 0),
        "top_p": float(raw.get("top_p", 1) or 1),
        "max_tokens": 2000 if old_token_defaults else int(raw.get("max_tokens", 2000) or 2000),
        "output_token_cap": 10000 if old_token_defaults else max(4096, min(32768, int(raw.get("output_token_cap", 10000) or 10000))),
        "thinking_mode": thinking_mode,
        "prompt_cache_mode": prompt_cache_mode,
        "timeout_seconds": int(raw.get("timeout_seconds", 90) or 90),
        "json_mode": clean(str(raw.get("json_mode") or "auto")),
        "extra_body": str(raw.get("extra_body") or "{}"),
        "fallback_mode": clean(str(raw.get("fallback_mode") or "abort")),
        "legacy_cleanup_mode": clean(str(raw.get("legacy_cleanup_mode") or "strict")),
        "warning_policy": clean(str(raw.get("warning_policy") or "review")),
        "retry_count": max(0, min(5, int(raw.get("retry_count", 2) or 0))),
        "input_price_per_million": float(raw.get("input_price_per_million", 0) or 0),
        "output_price_per_million": float(raw.get("output_price_per_million", 0) or 0),
        "run_request_limit": max(0, int(raw.get("run_request_limit", 0) or 0)),
        "run_token_limit": max(0, int(raw.get("run_token_limit", 0) or 0)),
        "run_cost_limit": max(0.0, float(raw.get("run_cost_limit", 0) or 0)),
        "output_language": "en-US" if clean(str(cfg.get("output_language") or "zh-CN")) == "en-US" else "zh-CN",
    }
    return out


def ai_api_key():
    env = clean(os.environ.get("IMDB_TECH_AI_API_KEY", ""))
    if env:
        return env
    try:
        p = subprocess.run(
            ["/usr/bin/security", "find-generic-password", "-s", AI_KEYCHAIN_SERVICE,
             "-a", AI_KEYCHAIN_ACCOUNT, "-w"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, timeout=8,
        )
        if p.returncode == 0:
            return clean(p.stdout)
    except Exception:
        pass
    return ""


def ai_endpoint(base_url, protocol="openai"):
    base = clean(base_url).rstrip("/")
    if not base:
        return ""
    protocol = clean(str(protocol or "openai")).lower()
    if protocol == "anthropic":
        if base.lower().endswith("/v1/messages"):
            return base
        if base.lower().endswith("/v1"):
            return base + "/messages"
        return base + "/v1/messages"
    if base.lower().endswith("/chat/completions"):
        return base
    return base + "/chat/completions"


class AIRequestError(RuntimeError):
    def __init__(self, kind, message, http_status=0, retry_after=0, finish_reason=""):
        super().__init__(message)
        self.kind = kind
        self.http_status = int(http_status or 0)
        self.retry_after = float(retry_after or 0)
        self.finish_reason = clean(str(finish_reason or ""))


def _load_ai_runtime():
    obj = load_json(AI_RUNTIME, {}) or {}
    if not isinstance(obj, dict):
        obj = {}
    obj.setdefault("paused", False)
    obj.setdefault("reason_kind", "")
    obj.setdefault("reason", "")
    obj.setdefault("paused_at", "")
    obj.setdefault("last_success_at", "")
    obj.setdefault("last_error_at", "")
    return obj


def _save_ai_runtime(obj):
    obj = dict(obj or {})
    obj["updated_at"] = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()
    save_json(AI_RUNTIME, obj)


def ai_pause(kind, reason):
    obj = _load_ai_runtime()
    obj.update({
        "paused": True,
        "reason_kind": clean(kind),
        "reason": clean(reason)[:1000],
        "paused_at": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
        "last_error_at": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
    })
    _save_ai_runtime(obj)


def ai_resume():
    obj = _load_ai_runtime()
    obj.update({"paused": False, "reason_kind": "", "reason": "", "paused_at": ""})
    _save_ai_runtime(obj)
    return obj


def _record_ai_success():
    obj = _load_ai_runtime()
    obj["last_success_at"] = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()
    if obj.get("reason_kind") in ("transient", "rate-limit"):
        obj.update({"paused": False, "reason_kind": "", "reason": "", "paused_at": ""})
    _save_ai_runtime(obj)


def ai_ready(require_enabled=True, allow_paused=False):
    cfg = ai_config()
    if require_enabled and not cfg["enabled"]:
        return False, "AI Tag Parser 尚未启用"
    if not cfg["base_url"]:
        return False, "AI Base URL 为空"
    if cfg.get("api_protocol") not in ("openai", "anthropic"):
        return False, "请在设置中选择 OpenAI Chat Completions 或 Anthropic Messages API 格式"
    if not cfg["model"]:
        return False, "AI 模型名为空"
    if not cfg["prompt"]:
        return False, "AI 提示词为空"
    if not ai_api_key():
        return False, "macOS Keychain 中没有 API Key"
    rt = _load_ai_runtime()
    if rt.get("paused") and not allow_paused:
        reason = clean(rt.get("reason") or rt.get("reason_kind") or "AI Runtime 已暂停")
        kind = clean(rt.get("reason_kind") or "")
        if kind in ("quota", "auth"):
            return False, "AI Runtime 已暂停：%s。请先点击“恢复 AI”或在设置中点击“保存并测试”；测试成功后会自动解除暂停。" % reason
        return False, "AI Runtime 已暂停：" + reason
    return True, ""


def _ai_input_specs(specs):
    # 2.0 only sends fields that can actually become searchable tags. Runtime,
    # Color, Laboratory and Film Length stay local and never consume tokens.
    return {k: list(specs.get(k, []) or []) for k in TAG_SECTIONS if specs.get(k)}


def _specs_hash(specs):
    raw = json.dumps({k: list(specs.get(k, []) or []) for k in SECTIONS}, ensure_ascii=False,
                     sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(raw).hexdigest()


def _ai_existing_tags(old_obj):
    """Current owned generated tags (value + source engine) for one title."""
    out = []
    if not isinstance(old_obj, dict):
        return out
    block_engine = clean(str(old_obj.get("tag_engine") or ""))
    for entry in old_obj.get("owned_entries") or []:
        value = clean(str(entry.get("value") or ""))
        if not value:
            continue
        engine = clean(str(entry.get("engine") or "")) or block_engine
        out.append({"value": value, "source": "ai" if engine == "ai" else "rules"})
    return out


def _ai_cache_key(specs, cfg, existing=None):
    stable = {
        "schema": AI_RESULT_SCHEMA, "existing": existing or [],
        "api_protocol": cfg.get("api_protocol", "openai"),
        "provider": cfg["provider"], "base_url": cfg["base_url"], "model": cfg["model"],
        "prompt": _effective_ai_prompt(cfg), "temperature": cfg["temperature"], "top_p": cfg["top_p"],
        "max_tokens": cfg["max_tokens"], "json_mode": cfg["json_mode"],
        "thinking_mode": cfg.get("thinking_mode", "off"), "prompt_cache_mode": cfg.get("prompt_cache_mode", "auto"),
        "extra_body": cfg["extra_body"],
        "output_language": cfg.get("output_language", "zh-CN"),
        "language_contract": LANGUAGE_CONTRACT_VERSION,
        "specs": _ai_input_specs(specs),
    }
    raw = json.dumps(stable, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(raw).hexdigest()


def _parse_ai_json(text):
    text = (text or "").strip()
    if text.startswith("```"):
        text = re.sub(r"^```(?:json)?\s*", "", text, flags=re.I)
        text = re.sub(r"\s*```$", "", text)
    try:
        return json.loads(text)
    except Exception:
        a, b = text.find("{"), text.rfind("}")
        if a >= 0 and b > a:
            return json.loads(text[a:b+1])
        raise


def _validate_ai_result(obj, specs, output_language="zh-CN"):
    if not isinstance(obj, dict) or not isinstance(obj.get("tags"), list):
        raise ValueError("模型返回 JSON 缺少 tags 数组")
    english_review = clean(str(output_language)) == "en-US"
    entries = []
    seen = set()
    review_reasons = []
    source_coverage = {field: set() for field in _ai_input_specs(specs)}
    for item in obj["tags"]:
        if not isinstance(item, dict):
            raise ValueError("tags 中存在非对象项")
        value = clean(str(item.get("value") or ""))
        field = clean(str(item.get("field") or ""))
        idxs = item.get("source_indexes", [])
        confidence = clean(str(item.get("confidence") or "high")).lower()
        operation = clean(str(item.get("operation") or ""))
        if not value or field not in AI_ALLOWED_FIELDS:
            raise ValueError(f"非法 AI Tag：field={field!r} value={value!r}")
        if "\n" in value or "\r" in value or len(value) > 320:
            raise ValueError(f"AI Tag 长度/换行异常：{value[:80]}")
        if not isinstance(idxs, list) or not idxs:
            raise ValueError(f"AI Tag 缺少 source_indexes：{value}")
        valid_count = len(specs.get(field, []) or [])
        norm_idxs = []
        for x in idxs:
            if not isinstance(x, int) or x < 0 or x >= valid_count:
                raise ValueError(f"AI Tag source_indexes 越界：{value}")
            norm_idxs.append(x)
        if field == "Aspect ratio":
            value = re.sub(r"\s*:\s*", ":", value)
        if confidence not in ("high", "medium", "low"):
            confidence = "medium"
        if confidence != "high":
            review_reasons.append(
                f"{value}: confidence={confidence}" if english_review
                else f"{value}：置信度={confidence}"
            )
        # Record provenance coverage before value de-duplication. Two IMDb
        # fields can legitimately produce the same Emby Tag (e.g. Negative
        # Format=35 mm and Printed Film Format=35 mm); the final Tag is stored
        # once, but both source bullets still count as represented.
        source_coverage.setdefault(field, set()).update(norm_idxs)
        key = value.casefold()
        if key in seen:
            continue
        seen.add(key)
        entries.append({
            "value": value, "field": field, "source_indexes": norm_idxs,
            "confidence": confidence, "operation": operation,
        })
    warnings = obj.get("warnings", [])
    if not isinstance(warnings, list):
        warnings = [str(warnings)]
    warnings = [clean(str(x)) for x in warnings if clean(str(x))]
    review_reasons.extend(warnings)
    relevant = _ai_input_specs(specs)
    if relevant and not entries:
        review_reasons.append(
            "The model returned 0 tags for non-empty Technical Specs."
            if english_review else "模型对非空 Technical Specs 返回了 0 个 Tag"
        )

    # Every relevant IMDb bullet must be accounted for.  Ambiguous Camera
    # expressions are allowed to stay unsplit, but they still need one output
    # entry pointing back to that source index.  This catches partial model
    # answers that look valid JSON yet silently omit whole fields/bullets.
    for field, values in relevant.items():
        missing = [idx for idx in range(len(values)) if idx not in source_coverage.get(field, set())]
        if missing:
            indexes = ", ".join(str(x) for x in missing)
            review_reasons.append(
                f"{field} is missing source_indexes coverage: {indexes}"
                if english_review else f"{field} 缺少 source_indexes 覆盖：{indexes}"
            )

    return {"tags": entries, "warnings": warnings, "review_reasons": review_reasons}


def _ai_error_kind(code, detail):
    low = (detail or "").lower()
    quota_words = (
        "insufficient_quota", "quota exceeded", "quota_exceeded", "insufficient balance",
        "balance insufficient", "余额不足", "额度不足", "欠费", "account balance", "billing",
    )
    if code == 401 or "invalid api key" in low or "unauthorized" in low:
        return "auth"
    if code == 402 or any(x in low for x in quota_words):
        return "quota"
    if code == 429:
        return "rate-limit"
    if code in (408, 409, 425) or code >= 500:
        return "transient"
    return "request"


def _usage_tokens(usage):
    if not isinstance(usage, dict):
        return 0, 0, 0
    if usage.get("prompt_tokens") is not None:
        inp = int(usage.get("prompt_tokens") or 0)
    else:
        inp = int(usage.get("input_tokens") or 0) + \
            int(usage.get("cache_creation_input_tokens") or 0) + \
            int(usage.get("cache_read_input_tokens") or 0)
    out = int(usage.get("completion_tokens") or usage.get("output_tokens") or 0)
    total = int(usage.get("total_tokens") or (inp + out) or 0)
    return inp, out, total


def _usage_cost(usage, cfg):
    inp, out, _ = _usage_tokens(usage)
    return (inp / 1_000_000.0) * max(0.0, cfg.get("input_price_per_million", 0)) + \
           (out / 1_000_000.0) * max(0.0, cfg.get("output_price_per_million", 0))


def _is_bailian_qwen(cfg):
    model = clean(str(cfg.get("model") or "")).lower()
    base = clean(str(cfg.get("base_url") or "")).lower()
    provider = clean(str(cfg.get("provider") or "")).lower()
    is_qwen = model.startswith("qwen") or "qwen" in provider
    is_bailian = (
        "dashscope" in base or "aliyuncs.com" in base or
        provider in ("bailian", "dashscope", "aliyun", "qwen")
    )
    return is_qwen and is_bailian


def _use_prompt_cache(cfg):
    mode = clean(str(cfg.get("prompt_cache_mode") or "auto")).lower()
    if mode == "off":
        return False
    if mode == "on":
        return True
    return _is_bailian_qwen(cfg)


def _apply_thinking_mode(body, cfg):
    mode = clean(str(cfg.get("thinking_mode") or "off")).lower()
    if mode == "auto":
        return
    # enable_thinking is a Bailian/Qwen extension to the OpenAI-compatible
    # request. Do not leak it to unrelated providers that may reject it.
    if _is_bailian_qwen(cfg):
        body["enable_thinking"] = (mode == "on")


def _reasoning_tokens(usage):
    if not isinstance(usage, dict):
        return 0
    details = usage.get("completion_tokens_details") or usage.get("output_tokens_details") or {}
    if not isinstance(details, dict):
        return 0
    return int(details.get("reasoning_tokens") or 0)


def _usage_add(a, b):
    a_in, a_out, a_total = _usage_tokens(a)
    b_in, b_out, b_total = _usage_tokens(b)
    return {
        "prompt_tokens": a_in + b_in,
        "completion_tokens": a_out + b_out,
        "total_tokens": a_total + b_total,
    }


# Per-process HTTP meter. Unlike the historical api_calls counter, this counts
# every real HTTP attempt, including retries and provider fallbacks. Usage from
# a 2xx response is accounted before JSON/schema validation, so malformed model
# output does not disappear from the user's Token totals.
AI_HTTP_METER = {
    "http_attempts": 0,
    "http_2xx": 0,
    "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
    "reasoning_tokens": 0,
    "cost": 0.0,
}


def _meter_reset():
    global AI_HTTP_METER
    AI_HTTP_METER = {
        "http_attempts": 0,
        "http_2xx": 0,
        "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
        "reasoning_tokens": 0,
        "cost": 0.0,
    }


def _meter_attempt():
    AI_HTTP_METER["http_attempts"] = int(AI_HTTP_METER.get("http_attempts", 0)) + 1
    metrics_incr("ai_request_count")


def _meter_response(payload, cfg):
    AI_HTTP_METER["http_2xx"] = int(AI_HTTP_METER.get("http_2xx", 0)) + 1
    usage = payload.get("usage") if isinstance(payload, dict) and isinstance(payload.get("usage"), dict) else {}
    AI_HTTP_METER["usage"] = _usage_add(AI_HTTP_METER.get("usage", {}), usage)
    AI_HTTP_METER["reasoning_tokens"] = int(AI_HTTP_METER.get("reasoning_tokens", 0)) + _reasoning_tokens(usage)
    AI_HTTP_METER["cost"] = float(AI_HTTP_METER.get("cost", 0.0) or 0.0) + _usage_cost(usage, cfg)


def _meter_snapshot():
    return {
        "http_attempts": int(AI_HTTP_METER.get("http_attempts", 0)),
        "http_2xx": int(AI_HTTP_METER.get("http_2xx", 0)),
        "usage": dict(AI_HTTP_METER.get("usage", {}) or {}),
        "reasoning_tokens": int(AI_HTTP_METER.get("reasoning_tokens", 0)),
        "cost": float(AI_HTTP_METER.get("cost", 0.0) or 0.0),
    }


AI_OUTPUT_JSON_SCHEMA = {
    "type": "object",
    "properties": {
        "tags": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "value": {"type": "string"},
                    "field": {"type": "string"},
                    "source_indexes": {"type": "array", "items": {"type": "integer"}},
                    "confidence": {"type": "string", "enum": ["high", "medium", "low"]},
                    "operation": {"type": "string"},
                },
                "required": ["value", "field", "source_indexes", "confidence", "operation"],
                "additionalProperties": False,
            },
        },
        "warnings": {"type": "array", "items": {"type": "string"}},
    },
    "required": ["tags", "warnings"],
    "additionalProperties": False,
}


def _ai_protocol(cfg):
    protocol = clean(str(cfg.get("api_protocol") or "")).lower()
    if protocol in ("openai", "anthropic"):
        return protocol
    provider = clean(str(cfg.get("provider") or "")).lower()
    base = clean(str(cfg.get("base_url") or "")).lower().rstrip("/")
    if "anthropic" in provider or "/apps/anthropic" in base or base.endswith("/v1/messages"):
        return "anthropic"
    return "openai"


def _ai_user_payload(specs, cfg, existing=None):
    user_payload = {
        "technical_specs": _ai_input_specs(specs),
        "output_language": cfg.get("output_language", "zh-CN"),
    }
    if existing:
        user_payload["existing_tags"] = existing
    recovery_instruction = clean(str(cfg.get("_recovery_instruction") or ""))
    if recovery_instruction:
        user_payload["recovery_instruction"] = recovery_instruction
    return json.dumps(user_payload, ensure_ascii=False, separators=(",", ":"))


def _apply_extra_body(body, cfg, protected):
    try:
        extra = json.loads(cfg.get("extra_body") or "{}")
    except Exception as e:
        raise ValueError(f"额外请求参数 JSON 无效：{e}")
    if isinstance(extra, dict):
        for key, value in extra.items():
            if key not in protected:
                body[key] = value


def _build_openai_request(specs, cfg, with_json_mode, with_prompt_cache, existing):
    recovery_instruction = clean(str(cfg.get("_recovery_instruction") or ""))
    system_content = _effective_ai_prompt(cfg)
    if with_prompt_cache:
        system_content = [{
            "type": "text", "text": system_content,
            "cache_control": {"type": "ephemeral"},
        }]
    body = {
        "model": cfg["model"],
        "messages": [
            {"role": "system", "content": system_content},
            {"role": "user", "content": _ai_user_payload(specs, cfg, existing)},
        ],
        "temperature": 0 if recovery_instruction else cfg["temperature"],
        "top_p": cfg["top_p"],
        "max_tokens": cfg["max_tokens"],
    }
    if with_json_mode:
        body["response_format"] = {"type": "json_object"}
    _apply_extra_body(body, cfg, {"model", "messages"})
    _apply_thinking_mode(body, cfg)
    return body


def _apply_anthropic_thinking(body, cfg):
    mode = clean(str(cfg.get("thinking_mode") or "off")).lower()
    if mode == "auto":
        return
    if _is_bailian_qwen(cfg):
        if mode == "off":
            body["thinking"] = {"type": "disabled"}
        else:
            limit = max(1024, min(4096, int(cfg.get("max_tokens", 2000) or 2000) // 2))
            body["thinking"] = {"type": "enabled", "budget_tokens": limit}
    elif mode == "on":
        # Native Anthropic supports adaptive thinking. Omitting the field is
        # the protocol-safe equivalent of the UI's default/off setting.
        body["thinking"] = {"type": "adaptive"}


def _build_anthropic_request(specs, cfg, with_json_mode, with_prompt_cache, existing):
    system_content = _effective_ai_prompt(cfg)
    if with_prompt_cache:
        system_content = [{
            "type": "text", "text": system_content,
            "cache_control": {"type": "ephemeral"},
        }]
    body = {
        "model": cfg["model"],
        "system": system_content,
        "messages": [{"role": "user", "content": _ai_user_payload(specs, cfg, existing)}],
        "temperature": 0 if cfg.get("_recovery_instruction") else cfg["temperature"],
        "top_p": cfg["top_p"],
        "max_tokens": cfg["max_tokens"],
    }
    if with_json_mode:
        body["output_config"] = {
            "format": {"type": "json_schema", "schema": AI_OUTPUT_JSON_SCHEMA}
        }
    _apply_extra_body(body, cfg, {"model", "system", "messages", "max_tokens"})
    _apply_anthropic_thinking(body, cfg)
    return body


def _standard_usage(raw):
    inp, out, total = _usage_tokens(raw)
    # Preserve provider-specific detail objects for diagnostics while adding
    # one stable set of counters used by budgets and the task UI.
    usage = dict(raw) if isinstance(raw, dict) else {}
    usage.update({"prompt_tokens": inp, "completion_tokens": out, "total_tokens": total})
    for key in ("cache_creation_input_tokens", "cache_read_input_tokens"):
        if isinstance(raw, dict) and raw.get(key) is not None:
            usage[key] = int(raw.get(key) or 0)
    return usage


def _normalize_openai_response(payload):
    try:
        choice = payload["choices"][0]
        message = choice["message"]
    except Exception:
        raise AIRequestError("provider-response", "OpenAI 响应缺少 choices[0].message")
    finish_reason = clean(str(choice.get("finish_reason") or "")).lower()
    if finish_reason == "length":
        raise AIRequestError("output-truncated", "AI 输出达到长度上限，返回内容被截断", finish_reason=finish_reason)
    if finish_reason in ("content_filter", "content-filter"):
        raise AIRequestError("content-filter", "AI 输出被内容过滤策略终止", finish_reason=finish_reason)
    refusal = clean(str(message.get("refusal") or "")) if isinstance(message, dict) else ""
    if refusal:
        raise AIRequestError("provider-refusal", "AI 服务拒绝了本次请求：" + refusal, finish_reason=finish_reason)
    content = message.get("content") if isinstance(message, dict) else None
    if not isinstance(content, str):
        raise AIRequestError("provider-response", "OpenAI 响应缺少 choices[0].message.content", finish_reason=finish_reason)
    return content, finish_reason


def _normalize_anthropic_response(payload):
    stop_reason = clean(str(payload.get("stop_reason") or "")).lower()
    if stop_reason == "max_tokens":
        raise AIRequestError("output-truncated", "AI 输出达到长度上限，返回内容被截断", finish_reason=stop_reason)
    if stop_reason == "refusal":
        raise AIRequestError("provider-refusal", "Anthropic Messages 服务拒绝了本次请求", finish_reason=stop_reason)
    if stop_reason == "model_context_window_exceeded":
        raise AIRequestError("context-length", "输入超过模型上下文窗口", finish_reason=stop_reason)
    if stop_reason in ("tool_use", "pause_turn"):
        raise AIRequestError("provider-response", "Anthropic Messages 返回了当前任务未请求的 " + stop_reason, finish_reason=stop_reason)
    blocks = payload.get("content")
    if not isinstance(blocks, list):
        raise AIRequestError("provider-response", "Anthropic Messages 响应缺少 content 数组", finish_reason=stop_reason)
    text_blocks = [str(block.get("text") or "") for block in blocks if isinstance(block, dict) and block.get("type") == "text"]
    content = "\n".join(part for part in text_blocks if part)
    if not content:
        raise AIRequestError("provider-response", "Anthropic Messages 响应没有文本内容块", finish_reason=stop_reason)
    return content, stop_reason


def _ai_http_request(specs, cfg, with_json_mode=True, with_prompt_cache=False, existing=None):
    protocol = _ai_protocol(cfg)
    endpoint = ai_endpoint(cfg["base_url"], protocol)
    if protocol == "anthropic":
        body = _build_anthropic_request(specs, cfg, with_json_mode, with_prompt_cache, existing)
        headers = {
            "x-api-key": ai_api_key(),
            "anthropic-version": "2023-06-01",
            "Content-Type": "application/json",
            "Accept": "application/json",
            "User-Agent": "IMDb-Tech-Manager/4.0.1",
        }
    else:
        body = _build_openai_request(specs, cfg, with_json_mode, with_prompt_cache, existing)
        headers = {
            "Authorization": "Bearer " + ai_api_key(),
            "Content-Type": "application/json",
            "Accept": "application/json",
            "User-Agent": "IMDb-Tech-Manager/4.0.1",
        }

    req = urllib.request.Request(
        endpoint,
        data=json.dumps(body, ensure_ascii=False).encode("utf-8"),
        headers=headers,
        method="POST",
    )
    _meter_attempt()
    try:
        with urllib.request.urlopen(req, timeout=cfg["timeout_seconds"]) as r:
            payload = json.loads(r.read().decode("utf-8", "replace"))
        _meter_response(payload, cfg)
    except urllib.error.HTTPError as e:
        detail = e.read().decode("utf-8", "replace")[:5000]
        retry_after = 0
        try:
            retry_after = float(e.headers.get("Retry-After") or 0)
        except Exception:
            pass
        kind = _ai_error_kind(e.code, detail)
        raise AIRequestError(kind, f"AI HTTP {e.code}: {detail}", e.code, retry_after)
    except (TimeoutError, urllib.error.URLError) as e:
        raise AIRequestError("transient", f"AI 网络请求失败：{e}")
    except Exception as e:
        raise AIRequestError("transient", f"AI 请求失败：{e}")

    content, finish_reason = _normalize_anthropic_response(payload) if protocol == "anthropic" else _normalize_openai_response(payload)
    try:
        parsed = _parse_ai_json(content)
    except Exception as e:
        raise AIRequestError("malformed-json", f"AI 返回内容不是完整有效的 JSON：{e}", finish_reason=finish_reason)
    try:
        result = _validate_ai_result(parsed, specs, cfg.get("output_language", "zh-CN"))
    except Exception as e:
        raise AIRequestError("schema-invalid", f"AI JSON 不符合标签结构要求：{e}", finish_reason=finish_reason)
    raw_usage = payload.get("usage") if isinstance(payload.get("usage"), dict) else {}
    usage = _standard_usage(raw_usage)
    result["usage"] = usage
    result["raw_model"] = payload.get("model") or cfg["model"]
    result["cost"] = _usage_cost(raw_usage, cfg)
    result["api_protocol"] = protocol
    result["thinking_mode"] = cfg.get("thinking_mode", "off")
    result["reasoning_tokens"] = _reasoning_tokens(usage)
    result["prompt_cache_requested"] = bool(with_prompt_cache)
    return result


def _ai_request_with_retry(specs, cfg, with_json_mode, with_prompt_cache=False, existing=None):
    request_cfg = dict(cfg)
    transient_retries = max(0, int(cfg.get("retry_count", 2)))
    recovery_used = set()
    while True:
        try:
            return _ai_http_request(
                specs, request_cfg,
                with_json_mode=with_json_mode,
                with_prompt_cache=with_prompt_cache,
                existing=existing,
            )
        except AIRequestError as e:
            if e.kind in ("quota", "auth"):
                ai_pause(e.kind, str(e))
                raise

            if e.kind == "output-truncated" and e.kind not in recovery_used:
                current = max(1, int(request_cfg.get("max_tokens", 2000) or 2000))
                cap = max(4096, int(request_cfg.get("output_token_cap", 10000) or 10000))
                # The configured recovery cap is a user-facing promise: when a
                # provider explicitly reports truncation, retry once at that
                # limit instead of stepping through intermediate output sizes.
                next_limit = cap
                if next_limit > current:
                    recovery_used.add(e.kind)
                    request_cfg = dict(request_cfg)
                    request_cfg["max_tokens"] = next_limit
                    request_cfg["_recovery_instruction"] = "上次输出被截断。请在新的输出长度内返回完整 JSON，不要省略任何字段或数组结尾。"
                    continue

            if e.kind == "malformed-json" and e.kind not in recovery_used:
                recovery_used.add(e.kind)
                request_cfg = dict(request_cfg)
                request_cfg["_recovery_instruction"] = "上次返回的 JSON 语法不完整。请重新生成完整、严格有效的 JSON；只返回 JSON 对象。"
                with_json_mode = True
                continue

            if e.kind == "schema-invalid" and e.kind not in recovery_used:
                recovery_used.add(e.kind)
                request_cfg = dict(request_cfg)
                request_cfg["_recovery_instruction"] = "上次 JSON 未通过标签结构校验：%s。请按既定 schema 完整重生。" % str(e)[:800]
                with_json_mode = True
                continue

            if e.kind not in ("rate-limit", "transient", "provider-response", "request") or transient_retries <= 0:
                raise
            retry_index = max(0, int(cfg.get("retry_count", 2)) - transient_retries)
            transient_retries -= 1
            wait = e.retry_after if e.retry_after > 0 else min(8.0, 1.2 * (2 ** retry_index))
            time.sleep(wait)


def ai_generate_tags(specs, force=False, ignore_pause=False, existing=None):
    cfg = ai_config()
    ok, msg = ai_ready(require_enabled=False, allow_paused=ignore_pause)
    if not ok:
        raise AIRequestError("paused" if "暂停" in msg else "config", msg)
    key = _ai_cache_key(specs, cfg, existing)
    cp = AI_CACHE / f"{key}.json"
    if not force:
        old = load_json(cp)
        if isinstance(old, dict) and old.get("cache_schema") == AI_CACHE_SCHEMA and isinstance(old.get("result"), dict):
            metrics_incr("ai_cache_hit")
            result = _validate_ai_result(old["result"], specs, cfg.get("output_language", "zh-CN"))
            # A cache hit must cost zero in the *current* run. Keep the original
            # accounting only as historical metadata so run budgets are not
            # consumed again when cached results are reused.
            result["usage"] = {}
            result["cost"] = 0.0
            result["cached_usage"] = old.get("usage", {})
            result["cached_cost"] = float(old.get("cost", 0) or 0)
            result["cache_hit"] = True
            result["cache_key"] = key
            result["model"] = old.get("model") or cfg["model"]
            result["prompt_hash"] = hashlib.sha256(_effective_ai_prompt(cfg).encode("utf-8")).hexdigest()[:16]
            result["spec_hash"] = _specs_hash(specs)
            result["review_required"] = bool(result.get("review_reasons")) and cfg.get("warning_policy") == "review"
            result["output_language"] = cfg.get("output_language", "zh-CN")
            return result

    json_mode = cfg.get("json_mode", "auto")
    cache_mode = cfg.get("prompt_cache_mode", "auto")
    want_cache = _use_prompt_cache(cfg)
    with_json = (json_mode != "off")
    try:
        result = _ai_request_with_retry(
            specs, cfg, with_json_mode=with_json, with_prompt_cache=want_cache, existing=existing
        )
    except AIRequestError as first:
        # Explicit cache and JSON mode are optimizations. In Auto mode a
        # provider-side 400/422 falls back without weakening tag validation.
        if want_cache and cache_mode == "auto" and first.http_status in (400, 422):
            try:
                result = _ai_request_with_retry(
                    specs, cfg, with_json_mode=with_json, with_prompt_cache=False, existing=existing
                )
            except AIRequestError as second:
                if json_mode == "auto" and second.http_status in (400, 422):
                    result = _ai_request_with_retry(
                        specs, cfg, with_json_mode=False, with_prompt_cache=False, existing=existing
                    )
                else:
                    raise
        elif json_mode == "auto" and first.http_status in (400, 422):
            result = _ai_request_with_retry(
                specs, cfg, with_json_mode=False, with_prompt_cache=False, existing=existing
            )
        else:
            raise

    result["cache_hit"] = False
    result["cache_key"] = key
    result["model"] = result.get("raw_model") or cfg["model"]
    result["prompt_hash"] = hashlib.sha256(_effective_ai_prompt(cfg).encode("utf-8")).hexdigest()[:16]
    result["spec_hash"] = _specs_hash(specs)
    result["review_required"] = bool(result.get("review_reasons")) and cfg.get("warning_policy") == "review"
    result["output_language"] = cfg.get("output_language", "zh-CN")
    save_json(cp, {
        "cache_schema": AI_CACHE_SCHEMA,
        "created_at": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
        "model": result["model"],
        "prompt_hash": result["prompt_hash"],
        "spec_hash": result["spec_hash"],
        "output_language": result["output_language"],
        "usage": result.get("usage", {}),
        "cost": result.get("cost", 0),
        "result": {"tags": result["tags"], "warnings": result.get("warnings", [])},
    })
    _record_ai_success()
    return result


def _cached_ai_review_state(specs):
    """Return review/current for the current AI configuration without making an API call."""
    try:
        cfg = ai_config()
        if not cfg.get("model") or not cfg.get("prompt") or not cfg.get("base_url"):
            return ""
        key = _ai_cache_key(specs, cfg)
        old = load_json(AI_CACHE / f"{key}.json", {}) or {}
        if not isinstance(old, dict) or old.get("cache_schema") != AI_CACHE_SCHEMA or not isinstance(old.get("result"), dict):
            return ""
        result = _validate_ai_result(old["result"], specs, cfg.get("output_language", "zh-CN"))
        if result.get("review_reasons") and cfg.get("warning_policy") == "review":
            return "review"
        return "current"
    except Exception:
        return ""

def discover_roots():
    """
    TMM is NOT required at runtime. If its v5 settings are present, use them
    only as a convenient way to discover mounted movie/TV source folders.
    Actual title/IDs/metadata always come from each NFO.
    """
    data_dir = Path.home() / "Library/Application Support/tinyMediaManager/data"
    out = []

    def add(x):
        if isinstance(x, str) and x.startswith("/") and os.path.isdir(x):
            out.append(os.path.realpath(x))
        elif isinstance(x, list):
            for v in x:
                add(v)

    def walk(x):
        if isinstance(x, dict):
            for k, v in x.items():
                nk = re.sub(r"[^a-z]", "", str(k).lower())
                if "datasource" in nk:
                    add(v)
                walk(v)
        elif isinstance(x, list):
            for v in x:
                walk(v)

    for filename in ("movies.json", "tvshows.json"):
        walk(load_json(data_dir / filename, {}))

    return list(dict.fromkeys(out))

def discover_root_candidates():
    """Return read-only TMM candidates; never modify Manager configuration."""
    candidates = []
    for path in discover_roots():
        lower = path.lower()
        if any(token in lower for token in ("tv", "series", "show", "电视剧", "电视")):
            suggested = "tv"
        elif any(token in lower for token in ("movie", "film", "电影")):
            suggested = "movies"
        else:
            suggested = "unassigned"
        candidates.append({
            "path": path,
            "suggested_space": suggested,
            "source": "tinyMediaManager",
            "online": os.path.isdir(path),
        })
    return candidates

def library_roots_confirmed(cfg=None):
    cfg = cfg if isinstance(cfg, dict) else (load_json(CFG, {}) or {})
    if bool(cfg.get("library_roots_confirmed")):
        return True
    grouped = cfg.get("library_roots") if isinstance(cfg.get("library_roots"), dict) else {}
    # A non-empty legacy roots list belongs to an existing installation;
    # only a genuinely empty/fresh configuration requires onboarding.
    return bool(grouped.get("movies") or grouped.get("tv") or cfg.get("roots"))

def save_roots_config(roots):
    clean = []
    seen = set()
    for value in roots:
        if not isinstance(value, str):
            continue
        value = value.strip()
        if not value or value in seen:
            continue
        seen.add(value)
        clean.append(value)

    cfg = load_json(CFG, {}) or {}
    if not isinstance(cfg, dict):
        cfg = {}
    cfg["roots"] = clean
    cfg["roots_managed_by"] = "IMDb Tech Manager"
    cfg["roots_updated_at"] = dt.datetime.now(dt.timezone.utc).isoformat()
    save_json(CFG, cfg)

def configured_library_roots(cfg=None):
    """Return categorized roots while retaining legacy roots as unassigned."""
    cfg = cfg if isinstance(cfg, dict) else (load_json(CFG, {}) or {})
    raw = cfg.get("library_roots") if isinstance(cfg.get("library_roots"), dict) else {}
    result = {"movies": [], "tv": [], "unassigned": []}
    seen = set()
    for space in ("movies", "tv"):
        for value in raw.get(space, []):
            if not isinstance(value, str) or not value.strip():
                continue
            path = os.path.realpath(os.path.expanduser(value.strip()))
            if path not in seen:
                seen.add(path)
                result[space].append(path)
    for value in cfg.get("roots", []):
        if not isinstance(value, str) or not value.strip():
            continue
        path = os.path.realpath(os.path.expanduser(value.strip()))
        if path not in seen:
            seen.add(path)
            result["unassigned"].append(path)
    return result

def configured_roots_flat(cfg=None):
    if not library_roots_confirmed(cfg):
        return []
    grouped = configured_library_roots(cfg)
    return grouped["movies"] + grouped["tv"] + grouped["unassigned"]

def configured_space_for_path(path_value, cfg=None):
    path = os.path.realpath(str(path_value))
    grouped = configured_library_roots(cfg)
    for space in ("movies", "tv"):
        for root in grouped[space]:
            with contextlib.suppress(ValueError):
                if os.path.commonpath([path, root]) == root and path != root:
                    return space
    return ""

def discover_root_candidates_cli():
    print(json.dumps({"candidates": discover_root_candidates()}, ensure_ascii=False))
    return 0

def configure():
    roots = discover_roots()
    if roots:
        print("识别到电影 / 电视剧数据源：")
        for r in roots:
            print("  ", r)
        ans = input("直接使用？[Y/n] ").strip().lower()
        if ans not in ("", "y", "yes"):
            roots = []
    while not roots:
        print("请输入电影 / 电视剧资料库根目录（一行一个，空行结束）：")
        while True:
            s = input("> ").strip()
            if not s:
                break
            s = os.path.expanduser(s)
            if os.path.isdir(s):
                roots.append(os.path.realpath(s))
            else:
                print("不存在：", s)
    save_roots_config(roots)
    print("配置完成。")

def throttle():
    global last_fetch
    wait = 1.4 + random.random() * 0.8
    left = wait - (time.monotonic() - last_fetch)
    if left > 0:
        time.sleep(left)
    last_fetch = time.monotonic()

def chrome():
    for p in [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Google Chrome Beta.app/Contents/MacOS/Google Chrome Beta",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ]:
        if os.path.isfile(p):
            return p
    return None


def webkit_helper():
    """Return the system-WebKit helper bundled beside the Go Core."""
    configured = clean(os.environ.get("IMDB_TECH_WEBKIT_HELPER", ""))
    if configured and os.path.isfile(configured) and os.access(configured, os.X_OK):
        return configured
    return None

WAF_MARKERS = (
    "awsWafCookieDomainList",
    "AwsWafIntegration",
    "challenge-container",
    "verify that you're not a robot",
    "token.awswaf.com",
)

class NextDataParser(HTMLParser):
    """Extract IMDb's structured __NEXT_DATA__ JSON without third-party modules."""

    def __init__(self):
        super().__init__(convert_charrefs=False)
        self.capture = False
        self.buf = []
        self.payload = None

    def handle_starttag(self, tag, attrs):
        if tag.lower() != "script":
            return
        attr = dict(attrs)
        if attr.get("id") == "__NEXT_DATA__":
            self.capture = True
            self.buf = []

    def handle_endtag(self, tag):
        if tag.lower() == "script" and self.capture:
            self.capture = False
            self.payload = "".join(self.buf)

    def handle_data(self, data):
        if self.capture:
            self.buf.append(data)


class HTMLLineExtractor(HTMLParser):
    BLOCK_TAGS = {
        "article", "aside", "blockquote", "br", "dd", "div", "dl", "dt",
        "footer", "h1", "h2", "h3", "h4", "h5", "h6", "header",
        "li", "main", "nav", "ol", "p", "section", "td", "th", "tr", "ul",
    }
    SKIP_TAGS = {"script", "style", "noscript", "svg"}

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.lines = []
        self.buf = []
        self.skip_depth = 0

    def flush(self):
        if not self.buf:
            return
        value = clean(" ".join(self.buf))
        self.buf = []
        if value:
            self.lines.append(value)

    def handle_starttag(self, tag, attrs):
        tag = tag.lower()
        if tag in self.SKIP_TAGS:
            self.skip_depth += 1
            return
        if self.skip_depth:
            return
        if tag in self.BLOCK_TAGS:
            self.flush()

    def handle_endtag(self, tag):
        tag = tag.lower()
        if tag in self.SKIP_TAGS:
            if self.skip_depth:
                self.skip_depth -= 1
            return
        if self.skip_depth:
            return
        if tag in self.BLOCK_TAGS:
            self.flush()

    def handle_data(self, data):
        if self.skip_depth:
            return
        value = clean(data)
        if value:
            self.buf.append(value)

    def close(self):
        super().close()
        self.flush()


def _decode_timeout_output(value):
    if not value:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", "replace")
    return value


def _response_is_waf_challenge(status, headers, src):
    action = ""
    try:
        action = clean(str(headers.get("x-amzn-waf-action", "")))
        if not action:
            action = next((clean(str(value)) for key, value in headers.items()
                           if str(key).casefold() == "x-amzn-waf-action"), "")
    except Exception:
        action = ""
    return (int(status or 0) == 202 and action.casefold() == "challenge") or is_waf_challenge(src)


def fetch_direct(url):
    started = time.monotonic()
    metrics_incr("imdb_http_attempt_count")
    throttle()
    req = urllib.request.Request(url, headers={
        "User-Agent": UA,
        "Accept-Language": "en-US,en;q=0.9",
        "Accept": "text/html,application/xhtml+xml",
        "Cache-Control": "no-cache",
        "Pragma": "no-cache",
        "Referer": "https://www.imdb.com/",
    })

    try:
        with urllib.request.urlopen(req, timeout=20) as r:
            src = r.read().decode("utf-8", "replace")
            status = getattr(r, "status", None)
            headers = getattr(r, "headers", {})
        metrics_incr("imdb_http_bytes", len(src.encode("utf-8", "replace")))

        if _response_is_waf_challenge(status, headers, src):
            return src, f"direct-waf-{status or 'unknown'}"

        return src, f"direct-{status or 'ok'}"

    except urllib.error.HTTPError as e:
        try:
            src = e.read().decode("utf-8", "replace")
        except Exception:
            src = ""
        metrics_incr("imdb_http_bytes", len(src.encode("utf-8", "replace")))
        if _response_is_waf_challenge(e.code, getattr(e, "headers", {}), src):
            return src, f"direct-waf-{e.code}"
        if e.code == 403:
            return src, "direct-http-403"
        if e.code == 429:
            return src, "direct-http-429"
        return src, f"direct-http-{e.code}"
    except (TimeoutError, urllib.error.URLError) as e:
        reason = getattr(e, "reason", None)
        if isinstance(reason, TimeoutError) or "timed out" in str(reason or e).lower():
            return "", "direct-timeout"
        return "", f"direct-network-error:{type(e).__name__}"
    except Exception as e:
        return "", f"direct-network-error:{type(e).__name__}"
    finally:
        metrics_duration("imdb_direct_duration_ms", started)


def fetch_webkit(url):
    """Render an IMDb page with macOS WebKit; no external browser required."""
    global LAST_WEBKIT_STDERR
    helper = webkit_helper()
    if not helper:
        LAST_WEBKIT_STDERR = "bundled WebKit helper unavailable"
        return "", "webkit-unavailable"

    throttle()
    metrics_incr("imdb_webkit_attempt_count")
    started = time.monotonic()
    fd, output_path = tempfile.mkstemp(prefix="imdb-tech-webkit-", suffix=".html")
    os.close(fd)
    try:
        process = subprocess.run(
            [helper, "--url", url, "--output", output_path, "--timeout", "35"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
            timeout=42,
        )
        LAST_WEBKIT_STDERR = clean(process.stderr or "")
        try:
            src = Path(output_path).read_text(encoding="utf-8", errors="replace")
        except Exception:
            src = ""
        metrics_incr("imdb_webkit_bytes", len(src.encode("utf-8", "replace")))
        if _looks_like_full_imdb_dom(src) and not is_waf_challenge(src):
            return src, "webkit-dom"
        if is_waf_challenge(src):
            return src, "webkit-waf"
        return src, "webkit-exit-%d" % process.returncode
    except subprocess.TimeoutExpired as e:
        LAST_WEBKIT_STDERR = "webkit-helper-timeout: %s" % e
        with contextlib.suppress(Exception):
            src = Path(output_path).read_text(encoding="utf-8", errors="replace")
            if src:
                return src, "webkit-process-timeout"
        return "", "webkit-process-timeout-empty"
    except Exception as e:
        LAST_WEBKIT_STDERR = "%s: %s" % (type(e).__name__, e)
        return "", "webkit-error-%s" % type(e).__name__
    finally:
        metrics_duration("imdb_webkit_duration_ms", started)
        with contextlib.suppress(Exception):
            os.unlink(output_path)


def _stop_chrome_process_group(proc):
    """Stop only the headless Chrome process tree started by this fetch."""
    if proc.poll() is not None:
        return

    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except Exception:
        try:
            proc.terminate()
        except Exception:
            pass

    try:
        proc.wait(timeout=2)
        return
    except Exception:
        pass

    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except Exception:
        try:
            proc.kill()
        except Exception:
            pass


def _run_chrome_dump(url, *, compatibility=False):
    """Run one isolated Chrome --dump-dom attempt and salvage timeout output."""
    c = chrome()
    if not c:
        return "", "chrome-unavailable", "", []

    if compatibility:
        timeout_seconds = CHROME_COMPAT_TIMEOUT_SECONDS
        virtual_time_ms = CHROME_COMPAT_VIRTUAL_TIME_MS
        # This is the exact minimal command shape captured in the user's
        # 2026-08-18 diagnostic ZIP when Chrome produced the full IMDb DOM.
        cmd = [
            c,
            "--headless=new",
            "--disable-gpu",
            "--no-first-run",
            "--no-default-browser-check",
            "--lang=en-US",
            f"--user-data-dir={PROFILE}",
            f"--virtual-time-budget={virtual_time_ms}",
            "--window-size=1280,2200",
            f"--user-agent={UA}",
            "--dump-dom",
            url,
        ]
        mode = "compat"
    else:
        timeout_seconds = CHROME_TIMEOUT_SECONDS
        virtual_time_ms = CHROME_VIRTUAL_TIME_MS
        # Fast path retained from the legacy single-item test that also worked
        # on this Mac. If it returns only a tiny shell page, fetch_chrome()
        # automatically retries with the diagnostic compatibility command.
        cmd = [
            c,
            "--headless=new",
            "--disable-gpu",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-sync",
            "--no-first-run",
            "--no-default-browser-check",
            "--lang=en-US",
            f"--user-data-dir={PROFILE}",
            f"--virtual-time-budget={virtual_time_ms}",
            "--window-size=1280,2400",
            f"--user-agent={UA}",
            "--dump-dom",
            url,
        ]
        mode = "fast"

    started = time.monotonic()

    try:
        p = subprocess.run(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=False,
            timeout=timeout_seconds,
        )
        src = _decode_timeout_output(p.stdout)
        err = _decode_timeout_output(p.stderr)
        elapsed = max(0, int(time.monotonic() - started))
        if src:
            return src, f"chrome-{mode}-dom-{elapsed}s-{len(src)}b", err, cmd
        return "", f"chrome-{mode}-empty-exit-{p.returncode}-{elapsed}s", err, cmd

    except subprocess.TimeoutExpired as e:
        src = _decode_timeout_output(e.stdout)
        err = _decode_timeout_output(e.stderr)
        elapsed = max(0, int(time.monotonic() - started))
        if src:
            return src, f"chrome-{mode}-timeout-dom-{elapsed}s-{len(src)}b", err, cmd
        return "", f"chrome-{mode}-timeout-empty-{elapsed}s", err, cmd

    except Exception as e:
        elapsed = max(0, int(time.monotonic() - started))
        return "", f"chrome-{mode}-error-{type(e).__name__}-{elapsed}s", f"{type(e).__name__}: {e}", cmd


def _looks_like_full_imdb_dom(src):
    if not src:
        return False
    # A real IMDb technical page from the user's diagnostic was >1 MB raw
    # HTML and included Next.js structured data. Tiny ~100 byte shell pages
    # must never suppress the compatibility retry.
    if '__NEXT_DATA__' in src and ('technicalSpecifications' in src or 'Technical specifications' in src):
        return True
    return len(src) >= 20000 and ('imdb' in src.lower())


def fetch_chrome(url):
    """
    Two-stage browser fetch.

    1) Fast mode: the command used by the successful legacy single-item test.
    2) Compatibility mode: the exact 35 s / 9 s virtual-time command captured
       in the user's original diagnostic ZIP that yielded the full IMDb DOM.

    Both modes salvage TimeoutExpired.stdout.  Debug stderr/commands from all
    attempts are retained, and cleanup is limited to the dedicated Manager
    Chrome profile.
    """
    global LAST_CHROME_STDERR, LAST_CHROME_COMMAND

    c = chrome()
    if not c:
        LAST_CHROME_STDERR = "Chrome not found"
        LAST_CHROME_COMMAND = []
        return "", "chrome-unavailable"

    throttle()
    PROFILE.mkdir(parents=True, exist_ok=True)
    cleanup_profile_chrome_processes()

    diagnostics = []
    commands = []
    best_src = ""
    best_method = "chrome-none"

    try:
        for compatibility in (False, True):
            metrics_incr("imdb_chrome_attempt_count")
            started = time.monotonic()
            src, method, err, cmd = _run_chrome_dump(
                url,
                compatibility=compatibility,
            )
            metrics_duration("imdb_chrome_duration_ms", started)
            metrics_incr("imdb_chrome_bytes", len(src.encode("utf-8", "replace")))
            commands.append(cmd)
            diagnostics.append(
                f"===== {method} =====\n"
                + (err.strip() if err.strip() else "(stderr empty)")
            )

            if len(src) > len(best_src):
                best_src = src
                best_method = method

            if _looks_like_full_imdb_dom(src):
                LAST_CHROME_STDERR = "\n\n".join(diagnostics)
                LAST_CHROME_COMMAND = commands
                return src, method

            # Ensure a timed-out/stale Chrome from the first attempt cannot
            # hold the Manager profile lock when compatibility mode starts.
            cleanup_profile_chrome_processes()
            time.sleep(0.25)

        LAST_CHROME_STDERR = "\n\n".join(diagnostics)
        LAST_CHROME_COMMAND = commands
        return best_src, best_method

    finally:
        cleanup_profile_chrome_processes()

def html_lines(src):
    if not src:
        return []

    parser = HTMLLineExtractor()
    try:
        parser.feed(src)
        parser.close()
        return parser.lines
    except Exception:
        return []


def html_text(src):
    # Secondary fallback only. Structured __NEXT_DATA__ is preferred.
    if not src:
        return ""

    f = tempfile.NamedTemporaryFile(
        "w", suffix=".html", encoding="utf-8", delete=False
    )

    try:
        f.write(src)
        f.close()
        p = subprocess.run([
            "/usr/bin/textutil",
            "-convert", "txt",
            "-encoding", "UTF-8",
            "-stdout", f.name,
        ], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
           text=True, timeout=20)
        return p.stdout or ""
    finally:
        with contextlib.suppress(Exception):
            os.unlink(f.name)


def clean(s):
    s = html.unescape(str(s)).replace("\u00a0", " ")
    return re.sub(r"\s+", " ", s).strip()


def _canon_tag_value(value):
    """Canonical comparison key for tag ownership matching.

    Legacy NFOs may carry values like "1.43 : 1 (some scene)" while the
    manifest stores the normalized "1.43:1". Aspect-ratio-shaped values are
    compared as the bare ratio (colon spacing and scene/version qualifiers
    ignored); other values only normalize colon spacing. Display text is
    never modified by this helper.
    """
    text = re.sub(r"\s*:\s*", ":", clean(value)).casefold()
    ratio = re.fullmatch(r"(\d+(?:\.\d+)?):(\d+)(?:\s*\(.*\))?", text)
    if ratio:
        return "%s:%s" % (ratio.group(1), ratio.group(2))
    return text


def is_waf_challenge(src):
    if not src:
        return False
    low = src.lower()
    return any(marker.lower() in low for marker in WAF_MARKERS)


def _find_title_payload(value):
    """
    Find the title object containing both runtimes and technicalSpecifications.
    This avoids depending on a single hard-coded Next.js object path.
    """
    if isinstance(value, dict):
        if "technicalSpecifications" in value and "runtimes" in value:
            return value

        for child in value.values():
            found = _find_title_payload(child)
            if found is not None:
                return found

    elif isinstance(value, list):
        for child in value:
            found = _find_title_payload(child)
            if found is not None:
                return found

    return None


def _attrs(item):
    values = []
    for attr in (item or {}).get("attributes") or []:
        value = clean((attr or {}).get("text", ""))
        if value:
            values.append(value)
    return values


def _with_attrs(base, item):
    base = clean(base)
    attrs = _attrs(item)

    if not base:
        return ""

    if attrs:
        return f"{base} ({', '.join(attrs)})"

    return base


def _plain_text(item):
    try:
        return clean(
            item["displayableProperty"]["value"]["plainText"]
        )
    except Exception:
        return ""


def parse_next_data_specs(src):
    """
    Preferred parser.

    IMDb embeds the complete structured Technical Specifications dataset in:
        <script id="__NEXT_DATA__" type="application/json">...</script>

    This preserves separate Camera / Printed Film Format items and avoids
    textutil collapsing adjacent <li> elements together.
    """
    empty = {k: [] for k in SECTIONS}

    if not src:
        return empty, False

    parser = NextDataParser()

    try:
        parser.feed(src)
    except Exception:
        return empty, False

    if not parser.payload:
        return empty, False

    try:
        data = json.loads(parser.payload)
    except Exception:
        return empty, False

    title = _find_title_payload(data)
    if not title:
        return empty, False

    result = {k: [] for k in SECTIONS}

    # Runtime is outside technicalSpecifications in IMDb's current Next data.
    for edge in (title.get("runtimes") or {}).get("edges") or []:
        node = (edge or {}).get("node") or {}
        base = _plain_text(node)
        if not base:
            continue

        extras = []

        seconds = node.get("seconds")
        if isinstance(seconds, (int, float)) and seconds > 0:
            minutes = int(round(seconds / 60))
            extras.append(f"{minutes} min")

        extras.extend(_attrs(node))

        country = clean((node.get("country") or {}).get("text", ""))
        if country:
            extras.append(country)

        if extras:
            # Match IMDb's visible presentation:
            # 2h 4m (124 min) (1974 Re-release) (United Kingdom)
            base += "".join(f" ({x})" for x in extras)

        result["Runtime"].append(base)

    tech = title.get("technicalSpecifications") or {}

    mapping = [
        ("Sound mix", "soundMixes", "text"),
        ("Color", "colorations", "text"),
        ("Aspect ratio", "aspectRatios", "aspectRatio"),
        ("Camera", "cameras", "camera"),
        ("Laboratory", "laboratories", "laboratory"),
        ("Negative Format", "negativeFormats", "negativeFormat"),
        ("Cinematographic Process", "processes", "process"),
        ("Printed Film Format", "printedFormats", "printedFormat"),
    ]

    for section, group_key, value_key in mapping:
        for item in (tech.get(group_key) or {}).get("items") or []:
            value = _with_attrs((item or {}).get(value_key, ""), item)
            if value:
                result[section].append(value)

    for item in (tech.get("filmLengths") or {}).get("items") or []:
        base = _plain_text(item)
        if not base:
            continue

        extras = _attrs(item)

        for country in (item or {}).get("countries") or []:
            value = clean((country or {}).get("text", ""))
            if value:
                extras.append(value)

        if extras:
            base += f" ({', '.join(extras)})"

        result["Film Length"].append(base)

    # Stable de-duplication.
    for section in result:
        seen = set()
        values = []

        for value in result[section]:
            value = clean(value)
            key = value.casefold()

            if value and key not in seen:
                seen.add(key)
                values.append(value)

        result[section] = values

    return result, True


def _append_value(result, section, value):
    value = clean(value)
    if not value:
        return

    if value.startswith("(") and result[section]:
        result[section][-1] = clean(result[section][-1] + " " + value)
        return

    for part in re.split(r"\s+[·•]\s+", value):
        part = clean(re.sub(r"^[•·-]\s*", "", part))
        if part:
            result[section].append(part)


def parse_specs(source):
    """
    Legacy visual-text fallback for unexpected IMDb layouts.
    __NEXT_DATA__ parsing is always attempted first.
    """
    result = {k: [] for k in SECTIONS}
    current = None
    started = False

    if isinstance(source, str):
        lines = source.splitlines()
    else:
        lines = source or []

    ordered_headings = sorted(HEADINGS, key=len, reverse=True)

    for raw in lines:
        line = clean(raw)
        if not line:
            continue

        # textutil can leave bullets before the first Runtime heading.
        line = re.sub(r"^(?:[•·]\s*)+", "", line)

        if line in HEADINGS:
            current = line
            started = True
            continue

        matched_heading = None

        for h in ordered_headings:
            if line.startswith(h + " "):
                matched_heading = h
                break

        if matched_heading:
            current = matched_heading
            started = True
            rest = clean(line[len(matched_heading):])

            if rest:
                _append_value(result, current, rest)

            continue

        if not started:
            continue

        if line in (
            "Edit",
            "See more",
            "See all",
            "Technical specifications",
            "Technical Specifications",
        ):
            continue

        if re.fullmatch(r"\d+\s+more", line, flags=re.I):
            continue

        if line.startswith((
            "Contribute to this page",
            "Suggest an edit",
            "More from this title",
            "Recently viewed",
        )):
            current = None
            continue

        if current:
            _append_value(result, current, line)

    for k in result:
        seen = set()
        vals = []

        for v in result[k]:
            v = clean(v)
            key = v.casefold()

            if v and key not in seen:
                seen.add(key)
                vals.append(v)

        result[k] = vals

    return result


def useful(specs):
    return any(specs.get(k) for k in SECTIONS)


def extract_specs(src):
    started = time.monotonic()
    metrics_incr("html_parse_count")
    # 1) Structured IMDb Next.js data — authoritative within the fetched page.
    try:
        specs, structured_page = parse_next_data_specs(src)

        if useful(specs):
            return specs, structured_page, "next-data"

        # 2) DOM text fallback.
        lines = html_lines(src)
        specs = parse_specs(lines)

        if useful(specs):
            return specs, structured_page, "html-lines"

        # 3) macOS textutil fallback.
        text = html_text(src)
        specs = parse_specs(text)

        if useful(specs):
            return specs, structured_page, "textutil"

        return specs, structured_page, "none"
    finally:
        metrics_duration("html_parse_duration_ms", started)


def cache_file(imdb):
    return CACHE / f"{imdb}.json"


TRANSIENT_FETCH_STATUSES = {
    "fetch-error", "imdb-waf-challenge", "http-403", "http-429",
    "network-error", "timeout", "invalid-response", "parse-error",
    "partial-or-suspicious-response",
}

_PARSED_CACHE_NAME = re.compile(r"^(tt\d{5,12})\.json$", re.I)
_RAW_META_CACHE_NAME = re.compile(r"^raw-(tt\d{5,12})\.json$", re.I)
_RAW_BODY_CACHE_NAME = re.compile(r"^raw-(tt\d{5,12})\.html\.gz$", re.I)
_CACHE_TEMP_NAME = re.compile(
    r"^(?:tt\d{5,12}\.json|raw-tt\d{5,12}\.(?:json|html\.gz))\..+\.tmp$",
    re.I,
)


def imdb_cache_max_mb():
    cfg = load_json(CFG, {}) or {}
    value = cfg.get("imdb_cache_max_mb", IMDB_CACHE_DEFAULT_MAX_MB) if isinstance(cfg, dict) else IMDB_CACHE_DEFAULT_MAX_MB
    if isinstance(value, bool):
        return IMDB_CACHE_DEFAULT_MAX_MB
    try:
        value = int(value)
    except (TypeError, ValueError):
        return IMDB_CACHE_DEFAULT_MAX_MB
    if value < IMDB_CACHE_MIN_MAX_MB or value > IMDB_CACHE_MAX_MAX_MB:
        return IMDB_CACHE_DEFAULT_MAX_MB
    return value


def _cache_status_file():
    return CACHE.parent / CACHE_STATUS.name


def _cache_maintenance_lock_file():
    return CACHE.parent / CACHE_MAINTENANCE_LOCK.name


def _cache_expired(path, kind):
    obj = load_json(path, {}) or {}
    if not isinstance(obj, dict):
        return True
    age = _cache_age(obj)
    if age is None:
        return True
    if kind == "raw":
        return age >= dt.timedelta(days=RAW_CACHE_DAYS)
    status_value = clean(str(obj.get("status") or ("ok" if obj.get("ok") else "fetch-error")))
    if obj.get("ok"):
        return age >= dt.timedelta(days=CACHE_DAYS)
    if status_value == "no-tech":
        return age >= dt.timedelta(days=NO_TECH_CACHE_DAYS)
    if status_value in TRANSIENT_FETCH_STATUSES:
        return age >= dt.timedelta(hours=FETCH_ERROR_CACHE_HOURS)
    return age >= dt.timedelta(hours=FETCH_ERROR_CACHE_HOURS)


def _scan_imdb_cache():
    """Return only cache files owned by the IMDb Specs fetcher.

    AI results, indexes, browser data, diagnostics, locks and unknown files are
    deliberately outside this budget. Symlinks and non-regular files are never
    followed or removed.
    """
    CACHE.mkdir(parents=True, exist_ok=True)
    groups = {}
    now = time.time()
    with os.scandir(str(CACHE)) as entries:
        for entry in entries:
            try:
                info = entry.stat(follow_symlinks=False)
            except OSError:
                continue
            if not stat.S_ISREG(info.st_mode) or entry.is_symlink():
                continue
            name = entry.name
            parsed = _PARSED_CACHE_NAME.fullmatch(name)
            raw_meta = _RAW_META_CACHE_NAME.fullmatch(name)
            raw_body = _RAW_BODY_CACHE_NAME.fullmatch(name)
            if parsed:
                imdb = parsed.group(1).lower()
                key = ("parsed", imdb)
            elif raw_meta:
                imdb = raw_meta.group(1).lower()
                key = ("raw", imdb)
            elif raw_body:
                imdb = raw_body.group(1).lower()
                key = ("raw", imdb)
            elif _CACHE_TEMP_NAME.fullmatch(name) and now - info.st_mtime >= 86400:
                imdb = ""
                key = ("temp", name)
            else:
                continue
            record = groups.setdefault(key, {
                "kind": key[0], "imdb": imdb, "paths": [], "bytes": 0,
                "accessed_at": 0.0, "expired": key[0] == "temp",
            })
            path = Path(entry.path)
            record["paths"].append(path)
            record["bytes"] += max(0, int(info.st_size))
            record["accessed_at"] = max(record["accessed_at"], float(info.st_mtime))

    records = list(groups.values())
    for record in records:
        if record["kind"] == "parsed":
            record["expired"] = _cache_expired(record["paths"][0], "parsed")
        elif record["kind"] == "raw":
            meta = next((p for p in record["paths"] if p.name.endswith(".json")), None)
            record["expired"] = meta is None or _cache_expired(meta, "raw")
    return records


def _cache_status_payload(records, state="ready", removed=0, error=""):
    limit_mb = imdb_cache_max_mb()
    parsed = [record for record in records if record["kind"] == "parsed"]
    raw = [record for record in records if record["kind"] == "raw"]
    return {
        "schema": 1,
        "state": state,
        "limit_mb": limit_mb,
        "limit_bytes": limit_mb * 1024 * 1024,
        "used_bytes": sum(record["bytes"] for record in records),
        "parsed_count": len(parsed),
        "raw_count": len(raw),
        "entry_count": len(parsed) + len(raw),
        "removed_count": int(removed),
        "last_cleanup": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
        "error": clean(error)[:1000],
    }


def _cache_path_is_owned(path):
    try:
        if path.parent != CACHE:
            return False
        name = path.name
        if not (_PARSED_CACHE_NAME.fullmatch(name) or _RAW_META_CACHE_NAME.fullmatch(name)
                or _RAW_BODY_CACHE_NAME.fullmatch(name) or _CACHE_TEMP_NAME.fullmatch(name)):
            return False
        info = os.lstat(str(path))
        return stat.S_ISREG(info.st_mode) and not stat.S_ISLNK(info.st_mode)
    except OSError:
        return False


def _remove_cache_record(record):
    import fcntl
    handle = None
    if record.get("imdb"):
        lock_path = CACHE / ("imdb-%s.lock" % record["imdb"])
        handle = lock_path.open("a+")
        try:
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except (BlockingIOError, OSError):
            handle.close()
            return False, ""
    errors = []
    try:
        for path in record["paths"]:
            if not path.exists():
                continue
            if not _cache_path_is_owned(path):
                errors.append("拒绝删除非本模块缓存文件：%s" % path.name)
                continue
            try:
                path.unlink()
            except OSError as exc:
                errors.append("%s: %s" % (path.name, exc))
    finally:
        if handle is not None:
            with contextlib.suppress(Exception):
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
            handle.close()
    return not errors, "; ".join(errors)


def maintain_imdb_cache(clear=False, best_effort=False):
    """Expire and size-bound the IMDb cache under one cross-process lock."""
    import fcntl
    APP.mkdir(parents=True, exist_ok=True)
    CACHE.mkdir(parents=True, exist_ok=True)
    lock = _cache_maintenance_lock_file().open("a+")
    try:
        try:
            flags = fcntl.LOCK_EX | (fcntl.LOCK_NB if best_effort else 0)
            fcntl.flock(lock.fileno(), flags)
        except (BlockingIOError, OSError):
            return load_json(_cache_status_file(), {}) or {}

        previous = load_json(_cache_status_file(), {}) or {}
        running = dict(previous) if isinstance(previous, dict) else {}
        running.update({
            "schema": 1, "state": "running", "limit_mb": imdb_cache_max_mb(),
            "limit_bytes": imdb_cache_max_mb() * 1024 * 1024, "error": "",
        })
        save_json(_cache_status_file(), running)

        records = _scan_imdb_cache()
        removed = 0
        errors = []

        # Expired and abandoned temporary records are removed even below the
        # capacity ceiling. Fresh raw pages are then preferred for eviction;
        # compact parsed results are retained until they are truly needed.
        candidates = sorted(
            records,
            key=lambda record: (
                0 if clear or record["expired"] else (1 if record["kind"] == "raw" else 2),
                record["accessed_at"],
            ),
        )
        used = sum(record["bytes"] for record in records)
        limit = imdb_cache_max_mb() * 1024 * 1024
        target = 0 if clear else int(limit * IMDB_CACHE_LOW_WATERMARK)
        trim_capacity = used > limit
        for record in candidates:
            should_remove = clear or record["expired"] or (trim_capacity and used > target)
            if not should_remove:
                continue
            ok, error = _remove_cache_record(record)
            if error:
                errors.append(error)
            if ok:
                used = max(0, used - record["bytes"])
                removed += 1

        final_records = _scan_imdb_cache()
        final_used = sum(record["bytes"] for record in final_records)
        if errors:
            state = "failed"
        elif clear and final_used:
            state = "busy"
        elif final_used > limit:
            state = "over-limit"
        else:
            state = "ready"
        payload = _cache_status_payload(final_records, state=state, removed=removed, error="; ".join(errors))
        save_json(_cache_status_file(), payload)
        return payload
    except Exception as exc:
        previous = load_json(_cache_status_file(), {}) or {}
        payload = dict(previous) if isinstance(previous, dict) else {}
        payload.update({
            "schema": 1, "state": "failed", "limit_mb": imdb_cache_max_mb(),
            "limit_bytes": imdb_cache_max_mb() * 1024 * 1024,
            "error": "%s: %s" % (type(exc).__name__, exc),
        })
        with contextlib.suppress(Exception):
            save_json(_cache_status_file(), payload)
        if best_effort:
            return payload
        raise
    finally:
        with contextlib.suppress(Exception):
            fcntl.flock(lock.fileno(), fcntl.LOCK_UN)
        lock.close()


def _touch_cache_files(*paths):
    for path in paths:
        try:
            info = os.lstat(str(path))
            if stat.S_ISREG(info.st_mode) and not stat.S_ISLNK(info.st_mode):
                os.utime(str(path), None, follow_symlinks=False)
        except OSError:
            pass


def _cache_age(old):
    try:
        t = dt.datetime.fromisoformat(old["fetched_at"])
        return dt.datetime.now(dt.timezone.utc) - t
    except Exception:
        return None


def _save_debug(imdb, src, label):
    if not src:
        return ""

    debug_dir = APP / "debug"
    debug_dir.mkdir(parents=True, exist_ok=True)
    safe = re.sub(r"[^a-zA-Z0-9._-]+", "-", label)
    path = debug_dir / f"{imdb}-{safe}.html"

    try:
        path.write_text(src, encoding="utf-8")
        return str(path)
    except Exception:
        return ""


def _save_chrome_stderr(imdb, label):
    """Persist Chrome stderr + exact commands without ever raising."""
    debug_dir = APP / "debug"
    safe = re.sub(r"[^a-zA-Z0-9._-]+", "-", label or "chrome")
    path = debug_dir / f"{imdb}-{safe}.stderr.txt"

    try:
        debug_dir.mkdir(parents=True, exist_ok=True)
        commands = LAST_CHROME_COMMAND
        if commands and isinstance(commands[0], str):
            commands = [commands]

        parts = []
        for i, cmd in enumerate(commands or [], 1):
            parts.append(f"===== command {i} =====")
            parts.append(json.dumps(cmd, ensure_ascii=False))

        parts.append("===== chrome stderr =====")
        parts.append(LAST_CHROME_STDERR or "(stderr empty)")
        path.write_text("\n".join(parts) + "\n", encoding="utf-8")
        return str(path)
    except Exception:
        return ""


RAW_CACHE_DAYS = 30


def _raw_cache_files(imdb):
    return CACHE / ("raw-%s.json" % imdb), CACHE / ("raw-%s.html.gz" % imdb)


def _specs_from_raw_cache(imdb):
    """Parse a cached raw IMDb page instead of hitting the network.

    Parser upgrades only need the raw page; they must never re-fetch IMDb for
    content that was already downloaded.
    """
    meta_path, body_path = _raw_cache_files(imdb)
    meta = load_json(meta_path)
    if not isinstance(meta, dict) or not meta.get("url"):
        return None
    age = _cache_age(meta)
    if age is None or age >= dt.timedelta(days=RAW_CACHE_DAYS):
        return None
    try:
        import gzip
        raw_body = gzip.decompress(body_path.read_bytes())
        expected_hash = clean(str(meta.get("body_hash") or ""))
        if expected_hash and hashlib.sha256(raw_body).hexdigest() != expected_hash:
            return None
        body = raw_body.decode("utf-8", "replace")
    except Exception:
        return None
    if not body or is_waf_challenge(body):
        return None
    _touch_cache_files(meta_path, body_path)
    specs, structured_page, parser_name = extract_specs(body)
    if useful(specs):
        obj = {
            "cache_version": CACHE_VERSION, "imdb": imdb,
            "parser_version": PARSER_VERSION,
            "fetched_at": clean(str(meta.get("fetched_at") or "")) or dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
            "url": clean(str(meta.get("url"))), "method": "raw-cache", "parser": parser_name,
            "attempts": ["raw-cache"], "status": "ok", "specs": specs, "ok": True,
        }
        save_json(cache_file(imdb), obj)
        metrics_incr("imdb_raw_cache_hit")
        return obj
    return None


def _save_raw_page(imdb, url, body):
    if not body:
        return
    with contextlib.suppress(Exception):
        import gzip
        meta_path, body_path = _raw_cache_files(imdb)
        packed = gzip.compress(body.encode("utf-8", "replace"))
        fd, tmp_name = tempfile.mkstemp(prefix=body_path.name + ".", suffix=".tmp", dir=str(body_path.parent))
        tmp = Path(tmp_name)
        try:
            with os.fdopen(fd, "wb") as fh:
                fh.write(packed)
                fh.flush()
                os.fsync(fh.fileno())
            os.replace(str(tmp), str(body_path))
        finally:
            with contextlib.suppress(FileNotFoundError):
                tmp.unlink()
        save_json(meta_path, {
            "url": url,
            "fetched_at": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
            "bytes": len(body),
            "body_hash": hashlib.sha256(body.encode("utf-8", "replace")).hexdigest(),
        })


def _parsed_specs_cache(imdb, retry_failed=False):
    """Return a fresh parsed result without touching raw HTML or the network."""
    old = load_json(cache_file(imdb))
    if not isinstance(old, dict) or old.get("cache_version") != CACHE_VERSION:
        return None
    # legacy cache entries predate parser_version and are parser v1.
    if int(old.get("parser_version", 1) or 1) != PARSER_VERSION:
        return None
    age = _cache_age(old)
    if age is None:
        return None
    status = old.get("status", "ok" if old.get("ok") else "fetch-error")
    if old.get("ok") and age < dt.timedelta(days=CACHE_DAYS):
        metrics_incr("imdb_parsed_cache_hit")
        _touch_cache_files(cache_file(imdb))
        return old
    if status == "no-tech" and age < dt.timedelta(days=NO_TECH_CACHE_DAYS):
        metrics_incr("imdb_parsed_cache_hit")
        _touch_cache_files(cache_file(imdb))
        return old
    if status in TRANSIENT_FETCH_STATUSES and not retry_failed and age < dt.timedelta(hours=FETCH_ERROR_CACHE_HOURS):
        metrics_incr("imdb_parsed_cache_hit")
        _touch_cache_files(cache_file(imdb))
        return old
    return None


def _save_parsed_specs(path, obj):
    obj = dict(obj or {})
    obj["parser_version"] = PARSER_VERSION
    save_json(path, obj)
    return obj


def _imdb_singleflight(imdb, producer):
    """One in-flight fetch per IMDb id across engine processes.

    Concurrent consumers (job, agent, resident) wait for the winner's parsed
    cache instead of hammering IMDb in parallel.
    """
    import fcntl
    CACHE.mkdir(parents=True, exist_ok=True)
    lock_path = CACHE / ("imdb-%s.lock" % imdb)
    handle = lock_path.open("a+")
    try:
        shared = False
        try:
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except (BlockingIOError, OSError):
            shared = True
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        try:
            if shared:
                # Even a forced refresh is coalesced with the request that
                # just completed.  If the winner failed before persisting a
                # classified result, fall through and let this caller retry.
                cached = _parsed_specs_cache(imdb, retry_failed=False)
                if cached is not None:
                    metrics_incr("imdb_singleflight_shared")
                    return cached
            result = producer()
            if shared:
                metrics_incr("imdb_singleflight_shared")
            return result
        finally:
            with contextlib.suppress(Exception):
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
    finally:
        handle.close()


def get_specs(imdb, force=False, retry_failed=False, debug=False):
    if not force:
        parsed = _parsed_specs_cache(imdb, retry_failed=retry_failed)
        if parsed is not None:
            return parsed
    result = _imdb_singleflight(imdb, lambda: _get_specs_network(imdb, force=force, retry_failed=retry_failed, debug=debug))
    maintain_imdb_cache(best_effort=True)
    return result


def _fetch_failure_status(attempts, last_src, last_label=""):
    joined = " ".join(clean(str(value)).lower() for value in (attempts or []))
    current_label = clean(str(last_label)).lower()
    if is_waf_challenge(last_src) or "waf" in current_label:
        return "imdb-waf-challenge"
    if "http-429" in current_label:
        return "http-429"
    if "http-403" in current_label:
        return "http-403"
    if last_src:
        if _looks_like_full_imdb_dom(last_src):
            return "parse-error"
        if len(last_src) >= 20000 or "__next_data__" in last_src.lower():
            return "partial-or-suspicious-response"
        return "invalid-response"
    if "waf" in joined:
        return "imdb-waf-challenge"
    if "http-429" in joined:
        return "http-429"
    if "http-403" in joined:
        return "http-403"
    if "timeout" in joined:
        return "timeout"
    if "network-error" in joined or "error" in joined or "unavailable" in joined:
        return "network-error"
    return "fetch-error"


def _get_specs_network(imdb, force=False, retry_failed=False, debug=False):
    CACHE.mkdir(parents=True, exist_ok=True)
    cp = cache_file(imdb)

    if not force:
        parsed = _parsed_specs_cache(imdb, retry_failed=retry_failed)
        if parsed is not None:
            return parsed

    if not force:
        raw_obj = _specs_from_raw_cache(imdb)
        if raw_obj is not None:
            return raw_obj

    canonical_url = f"https://www.imdb.com/title/{imdb}/technical/"
    mobile_url = f"https://m.imdb.com/title/{imdb}/technical/"

    attempts = []
    last_src = ""
    last_label = "none"

    # Plain HTTP is cheap, so try it first. On the user's current connection
    # IMDb returns AWS WAF HTTP 202; that is recognized explicitly and is
    # never mistaken for "no technical data".
    src, method = fetch_direct(canonical_url)
    attempts.append(method)

    if src:
        last_src = src
        last_label = method

    if src and not method.startswith("direct-waf-") and not is_waf_challenge(src):
        _save_raw_page(imdb, canonical_url, src)
        specs, structured_page, parser_name = extract_specs(src)

        if useful(specs):
            obj = {
                "cache_version": CACHE_VERSION,
                "imdb": imdb,
                "fetched_at": dt.datetime.now(dt.timezone.utc)
                    .replace(microsecond=0).isoformat(),
                "url": canonical_url,
                "method": method,
                "parser": parser_name,
                "attempts": attempts,
                "status": "ok",
                "specs": specs,
                "ok": True,
            }
            return _save_parsed_specs(cp, obj)

        if structured_page:
            obj = {
                "cache_version": CACHE_VERSION,
                "imdb": imdb,
                "fetched_at": dt.datetime.now(dt.timezone.utc)
                    .replace(microsecond=0).isoformat(),
                "url": canonical_url,
                "method": method,
                "parser": parser_name,
                "attempts": attempts,
                "status": "no-tech",
                "specs": specs,
                "ok": False,
            }
            return _save_parsed_specs(cp, obj)

    # System WebKit is the primary browser fallback and ships with macOS.
    # Chrome/Chromium remains an optional tertiary compatibility path, never
    # a product dependency. Each renderer tries desktop before mobile.
    browser_fetchers = [("system-webkit", fetch_webkit)]
    if chrome():
        browser_fetchers.append(("optional-chromium", fetch_chrome))
    for renderer, fetcher in browser_fetchers:
        for host_label, url in (
            ("desktop", canonical_url),
            ("mobile", mobile_url),
        ):
            src, method = fetcher(url)
            label = f"{method}:{host_label}"
            attempts.append(label)

            if src:
                last_src = src
                last_label = label

            if not src or is_waf_challenge(src):
                continue

            _save_raw_page(imdb, url, src)
            specs, structured_page, parser_name = extract_specs(src)

            if useful(specs):
                obj = {
                    "cache_version": CACHE_VERSION,
                    "imdb": imdb,
                    "fetched_at": dt.datetime.now(dt.timezone.utc)
                        .replace(microsecond=0).isoformat(),
                    "url": canonical_url,
                    "method": label,
                    "renderer": renderer,
                    "parser": parser_name,
                    "attempts": attempts,
                    "status": "ok",
                    "specs": specs,
                    "ok": True,
                }
                return _save_parsed_specs(cp, obj)

            # Only call something truly "no-tech" when a valid structured
            # IMDb title payload exists but all ten fields are empty.
            if structured_page:
                obj = {
                    "cache_version": CACHE_VERSION,
                    "imdb": imdb,
                    "fetched_at": dt.datetime.now(dt.timezone.utc)
                        .replace(microsecond=0).isoformat(),
                    "url": canonical_url,
                    "method": label,
                    "renderer": renderer,
                    "parser": parser_name,
                    "attempts": attempts,
                    "status": "no-tech",
                    "specs": specs,
                    "ok": False,
                }
                return _save_parsed_specs(cp, obj)

    obj = {
        "cache_version": CACHE_VERSION,
        "imdb": imdb,
        "fetched_at": dt.datetime.now(dt.timezone.utc)
            .replace(microsecond=0).isoformat(),
        "url": canonical_url,
        "method": last_label,
        "parser": "none",
        "attempts": attempts,
        "status": _fetch_failure_status(attempts, last_src, last_label),
        "specs": {k: [] for k in SECTIONS},
        "ok": False,
    }

    if debug:
        # Diagnostics are best-effort only. A broken debug writer must never
        # turn a normal fetch-error into a traceback and hide the real cause.
        try:
            obj["debug_file"] = _save_debug(
                imdb,
                last_src,
                last_label,
            )
        except Exception as e:
            obj["debug_file_error"] = f"{type(e).__name__}: {e}"

        try:
            obj["debug_stderr_file"] = _save_chrome_stderr(
                imdb,
                last_label,
            )
        except Exception as e:
            obj["debug_stderr_error"] = f"{type(e).__name__}: {e}"

    return _save_parsed_specs(cp, obj)


def test_imdb(iid):
    iid = clean(iid)
    m = re.search(r"\btt\d{5,12}\b", iid, re.I)

    if not m:
        print("❌ IMDb ID 格式不正确，例如：tt0064757")
        return 2

    iid = m.group(0).lower()
    APP.mkdir(parents=True, exist_ok=True)
    PROFILE.mkdir(parents=True, exist_ok=True)

    # Manual fetch tests and the background Agent use the same Chrome profile.
    # Serialize them with the same cross-process lock used by backfill/auto so
    # two headless Chrome instances can never fight over that profile.
    lock = LOCK.open("a+")
    import fcntl

    try:
        try:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            print("⏳ 后台 Agent 正在处理 IMDb/NFO，等待当前任务完成后再测试……", flush=True)
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
            print("✅ 已取得抓取锁，开始测试。", flush=True)

        print(f"🧪 强制测试 IMDb Technical Specifications：{iid}")
        print("   不修改任何 NFO，不使用旧失败缓存，也不会覆盖正式 IMDb 缓存。")
        print("   先尝试普通 HTTP，再使用 App 内置的 macOS WebKit；若安装了 Chrome/Chromium，仅作为最后兼容回退。")
        print()

        cp = cache_file(iid)
        old_cache = None
        old_exists = cp.exists()

        if old_exists:
            try:
                old_cache = cp.read_bytes()
            except Exception:
                old_cache = None

        try:
            obj = get_specs(
                iid,
                force=True,
                retry_failed=True,
                debug=True,
            )
        finally:
            # A diagnostic test must never poison a previously good cache with
            # a transient fetch-error.  Restore exactly what existed before.
            try:
                if old_exists and old_cache is not None:
                    cp.parent.mkdir(parents=True, exist_ok=True)
                    cp.write_bytes(old_cache)
                elif not old_exists and cp.exists():
                    cp.unlink()
            except Exception:
                pass

        print("状态：", obj.get("status"))
        print("方式：", obj.get("method"))
        print("解析：", obj.get("parser"))
        print("尝试：", " -> ".join(obj.get("attempts", [])))

        if obj.get("ok"):
            print()

            for section in SECTIONS:
                vals = obj["specs"].get(section, [])

                if vals:
                    print(section)

                    for v in vals:
                        print("  -", v)

            return 0

        print()

        if obj.get("status") == "no-tech":
            print("⚠️ IMDb 返回了有效的结构化标题数据，但该条目没有 Technical Specifications。")
        else:
            print("❌ IMDb 页面获取失败；不会再把这种情况写成“无数据”。")

        if obj.get("debug_file"):
            print("调试 HTML 已保存：", obj["debug_file"])
        if obj.get("debug_stderr_file"):
            print("Chrome stderr 已保存：", obj["debug_stderr_file"])

        return 1

    finally:
        try:
            fcntl.flock(lock.fileno(), fcntl.LOCK_UN)
        except Exception:
            pass
        lock.close()

def imdb_id(text):
    for pat in [
        r'<uniqueid\b[^>]*type=["\']imdb["\'][^>]*>\s*(tt\d+)\s*</uniqueid>',
        r'<imdbid>\s*(tt\d+)\s*</imdbid>',
    ]:
        m = re.search(pat, text, re.I)
        if m:
            return m.group(1)

    m = re.search(r"\btt\d{5,12}\b", text)
    return m.group(0) if m else None

def local_name(tag):
    return str(tag).rsplit("}", 1)[-1].lower()


def child_text(root, name):
    wanted = name.lower()
    for child in list(root):
        if local_name(child.tag) == wanted:
            value = "".join(child.itertext()).strip()
            if value:
                return value
    return ""


def inspect_nfo(text, root=None):
    """
    Read identity strictly from the already-scraped NFO.
    Never search IMDb/TMDb by title.

    Supported:
      <movie>          -> movie
      <tvshow>         -> TV series
      <episodedetails> -> individual episode

    season.nfo is intentionally skipped because an IMDb season is not a
    standalone title ID in the same way.
    """
    root = root if root is not None else _xml_parse(text)
    tag = local_name(root.tag)

    type_map = {
        "movie": "movie",
        "tvshow": "tvshow",
        "episodedetails": "episode",
    }
    media_type = type_map.get(tag)
    if not media_type:
        return None

    title = child_text(root, "title")
    year = child_text(root, "year")
    show_title = child_text(root, "showtitle")
    season = child_text(root, "season")
    episode = child_text(root, "episode")

    if media_type == "episode":
        prefix = show_title or path_safe_title(title) or "TV Episode"
        se = ""
        if season.isdigit() and episode.isdigit():
            se = f" S{int(season):02d}E{int(episode):02d}"
        display_title = f"{prefix}{se}"
        if title and title != show_title:
            display_title += f" {title}"
    else:
        display_title = title or ""

    return {
        "root_tag": tag,
        "media_type": media_type,
        "title": title,
        "year": year,
        "show_title": show_title,
        "season": int(season) if season.isdigit() else None,
        "episode": int(episode) if episode.isdigit() else None,
        "display_title": display_title,
    }


def path_safe_title(title):
    return (title or "").strip()


def esc(s):
    return (s.replace("&", "&amp;")
             .replace("<", "&lt;")
             .replace(">", "&gt;")
             .replace('"', "&quot;")
             .replace("'", "&apos;"))

def _xml_text(value):
    return html.unescape(re.sub(r"<[^>]+>", "", value or "")).strip()


def _all_normal_tags(text, root=None):
    """Return only root-level TMM/Emby <tag> values, never our nested ownership manifest."""
    try:
        root = root if root is not None else _xml_parse(text)
        values = []
        for child in list(root):
            if local_name(child.tag) != "tag":
                continue
            value = clean("".join(child.itertext()))
            if value:
                values.append(value)
        return values
    except Exception:
        # Fallback for a transient/partially formatted file. Normal writes still
        # require a valid XML parse before anything can be replaced.
        values = []
        for m in re.finditer(
            r"^[ \t]*<tag(?:\s[^>]*)?>([^\r\n]*?)</tag>[ \t]*\r?$",
            text,
            flags=re.M | re.I,
        ):
            value = _xml_text(m.group(1))
            if value:
                values.append(value)
        return values


def strip_generated(text, old_generated_tags=None, protected_tags=None):
    """
    Remove our previous output without depending on comments surviving TMM.

    TMM 5.3.1 may preserve <technicalspecs> but remove our BEGIN/END comments
    and move normal <tag> nodes back into its standard tag area. Therefore
    ownership is reconstructed from the previous <technicalspecs> values and
    exact matching generated tags are removed before the new set is written.
    """
    text = re.sub(
        r'\s*<!--\s*tmm-imdb-tech:BEGIN\s*-->.*?'
        r'<!--\s*tmm-imdb-tech:END\s*-->\s*',
        "\n", text, flags=re.S | re.I)

    text = re.sub(
        r'\s*<!--\s*IMDb Technical Specifications BEGIN\s*-->.*?'
        r'<!--\s*IMDb Technical Specifications END\s*-->\s*',
        "\n", text, flags=re.S | re.I)

    text = re.sub(
        r'\s*<!--\s*IMDb Technical Tags BEGIN\s*-->.*?'
        r'<!--\s*IMDb Technical Tags END\s*-->\s*',
        "\n", text, flags=re.S | re.I)

    text = re.sub(
        r"\s*<technicalspecs\b(?=[^>]*source=[\"']IMDb[\"'])"
        r'[^>]*>.*?</technicalspecs>\s*',
        "\n", text, flags=re.S | re.I)

    # Remove old prefixed tag formats from v1-v5.
    alt = "|".join(re.escape(x) for x in OLD_PREFIXES)
    text = re.sub(
        rf'<tag>\s*(?:{alt})\s*(?:·|:)\s*.*?</tag>[ \t]*(?:\r?\n)?',
        "", text, flags=re.S | re.I)

    # Remove exact values that the previous technicalspecs block proves were
    # generated by us. This is the key TMM-rewrite compatibility path.
    # Comparison is canonical (colon spacing insensitive) so legacy spaced
    # values such as "1.43 : 1" are still recognized as our own output.
    old_keys = {
        _canon_tag_value(v)
        for v in (old_generated_tags or [])
        if clean(v)
    }
    protected_keys = {
        _canon_tag_value(v)
        for v in (protected_tags or [])
        if clean(v)
    }

    if old_keys:
        tag_line = re.compile(
            r'<tag(?:\s[^>]*)?>(.*?)</tag>[ \t]*(?:\r?\n)?',
            flags=re.S | re.I,
        )

        def keep_or_remove(match):
            value = clean(_xml_text(match.group(1)))
            key = _canon_tag_value(value)
            return "" if key in old_keys and key not in protected_keys else match.group(0)

        text = tag_line.sub(keep_or_remove, text)

    return text

def split_top_level(text, separators=(",",), split_and=False):
    """Split separators only outside (), [], {} so notes stay intact."""
    parts = []
    buf = []
    depth = 0
    i = 0

    while i < len(text):
        ch = text[i]

        if ch in "([{":
            depth += 1
            buf.append(ch)
            i += 1
            continue

        if ch in ")]}":
            depth = max(0, depth - 1)
            buf.append(ch)
            i += 1
            continue

        if depth == 0 and ch in separators:
            part = "".join(buf).strip()
            if part:
                parts.append(part)
            buf = []
            i += 1
            continue

        if depth == 0 and split_and and text[i:i+5].lower() == " and ":
            part = "".join(buf).strip()
            if part:
                parts.append(part)
            buf = []
            i += 5
            continue

        buf.append(ch)
        i += 1

    part = "".join(buf).strip()
    if part:
        parts.append(part)

    return parts


def _camera_searchable_value(value):
    """Remove IMDb scene/shot notes only from searchable Camera tags."""
    return re.sub(
        r"\s*\((?=[^)]*(?:scene|scenes|shot|shots))[^)]*\)\s*$",
        "",
        value or "",
        flags=re.I,
    ).strip()


# Lens/camera manufacturers used only to disambiguate a real equipment
# conjunction from model names that legitimately contain the word "and".
_CAMERA_MAKERS = (
    "ARRI", "Arri", "Arriflex", "Panavision", "Zeiss", "Angenieux",
    "Cooke", "Leica", "Leitz", "Canon", "Fujinon", "Sony", "RED",
    "Red", "Vantage", "Hawk", "Kowa", "Lomo", "Nikon", "Sigma",
    "Tokina", "Atlas", "Schneider", "Technovision", "Blackmagic",
    "DJI", "Rodenstock", "ISCO", "Isco", "P+S Technik",
)
_CAMERA_MAKER_RE = re.compile(
    r"^(?:" + "|".join(re.escape(x) for x in sorted(
        _CAMERA_MAKERS, key=len, reverse=True
    )) + r")\b",
    flags=re.I,
)


def _dedupe_clean(values):
    out = []
    seen = set()
    for value in values:
        value = clean(value)
        key = value.casefold()
        if value and key not in seen:
            seen.add(key)
            out.append(value)
    return out


def _expand_shared_series_tag(value):
    """
    Expand IMDb's compact shared-Series lens notation while preserving brand.

    Examples:
      Panavision B- and C-Series Lenses
        -> Panavision B-Series Lenses
        -> Panavision C-Series Lenses

      Panavision C-, D-, E- and H-Series Lenses
        -> Panavision C-Series Lenses
        -> Panavision D-Series Lenses
        -> Panavision E-Series Lenses
        -> Panavision H-Series Lenses
    """
    value = clean(value)
    if not value:
        return []

    # The first token owns the manufacturer prefix; middle tokens are the
    # abbreviated series names; the final token carries "-Series" + tail.
    m = re.match(
        r"^(?P<prefix>.+?\s)"
        r"(?P<first>[A-Za-z0-9]+)-"
        r"(?P<middle>(?:\s*,\s*[A-Za-z0-9]+-)*)"
        r"\s*(?:,\s*)?(?:and|&)\s*"
        r"(?P<last>[A-Za-z0-9]+)-Series\s+"
        r"(?P<tail>.+)$",
        value,
        flags=re.I,
    )
    if not m:
        return [value]

    prefix = m.group("prefix")
    series = [m.group("first")]
    series.extend(re.findall(r"[A-Za-z0-9]+(?=-)", m.group("middle") or ""))
    series.append(m.group("last"))
    tail = m.group("tail")

    return _dedupe_clean(
        f"{prefix}{name}-Series {tail}" for name in series
    )


def _fold_shared_series_chunks(chunks):
    """
    Rejoin comma tokens that belong to one compact Series expression before
    treating commas as Camera-item separators.

    split_top_level("Panavision C-, D-, E- and H-Series Lenses, Sony ...")
    initially yields three fragments for the Panavision lenses. This pass
    folds them back into one expression so _expand_shared_series_tag() can
    expand it correctly instead of leaking tags such as "Panavision C-".
    """
    out = []
    i = 0

    while i < len(chunks):
        chunk = clean(chunks[i])
        if not re.search(r"\b[A-Za-z0-9]+-$", chunk):
            out.append(chunk)
            i += 1
            continue

        group = [chunk]
        j = i + 1
        completed = False

        while j < len(chunks):
            nxt = clean(chunks[j])
            if re.fullmatch(r"[A-Za-z0-9]+-", nxt):
                group.append(nxt)
                j += 1
                continue

            if re.match(
                r"^[A-Za-z0-9]+-\s+(?:and|&)\s+"
                r"[A-Za-z0-9]+-Series\s+.+$",
                nxt,
                flags=re.I,
            ):
                group.append(nxt)
                j += 1
                completed = True
            break

        if completed:
            out.append(", ".join(group))
            i = j
        else:
            out.append(chunk)
            i += 1

    return out


def _split_cross_maker_lenses(value):
    """
    Split a conjunction only when both halves clearly start with different
    equipment manufacturers and the right side owns the shared Lens/Lenses
    suffix. Generic "and" is intentionally NOT a delimiter.

    Zeiss Ultra Prime and Angenieux Optimo Lenses
      -> Zeiss Ultra Prime Lenses
      -> Angenieux Optimo Lenses
    """
    value = clean(value)
    if not value:
        return []

    m = re.match(
        r"^(?P<left>.+?)\s+(?:and|&)\s+"
        r"(?P<right>.+?\s+(?P<suffix>Lens|Lenses))$",
        value,
        flags=re.I,
    )
    if not m:
        return [value]

    left = clean(m.group("left"))
    right = clean(m.group("right"))
    suffix = m.group("suffix")

    if not (_CAMERA_MAKER_RE.match(left) and _CAMERA_MAKER_RE.match(right)):
        return [value]

    if not re.search(r"\b(?:Lens|Lenses)$", left, flags=re.I):
        left = clean(f"{left} {suffix}")

    return _dedupe_clean([left, right])


def camera_atomic_tags(value):
    # Preserve the complete IMDb Camera line in <technicalspecs>; only the
    # searchable Emby tags are normalized/split.
    # Split first so a scene/shot note attached to an individual comma item
    # can be removed from that searchable tag without touching the source
    # value preserved in <technicalspecs>.
    raw_chunks = split_top_level(
        clean(value),
        separators=(",", ";"),
        split_and=False,
    )
    raw_chunks = [
        _camera_searchable_value(chunk)
        for chunk in raw_chunks
        if _camera_searchable_value(chunk)
    ]
    chunks = _fold_shared_series_chunks(raw_chunks)

    out = []
    for chunk in chunks:
        # Shared-series expansion must run before cross-manufacturer "and"
        # handling because "B- and C-Series" is one lens-family shorthand.
        for expanded in _expand_shared_series_tag(chunk):
            out.extend(_split_cross_maker_lenses(expanded))

    return _dedupe_clean(out)

def _sound_mix_local_tag(value):
    value = clean(value)
    m = re.match(r"^DTS\s*\(\s*DTS\s*:\s*X\s*\)$", value, flags=re.I)
    if m:
        return "DTS:X"
    # Local fallback is deliberately conservative: remove only one trailing
    # explanatory parenthesis. AI mode handles the semantic edge cases.
    value = re.sub(r"\s*\([^)]*\)\s*$", "", value).strip()
    return value

def emby_tag_values(section, value):
    if section == "Camera":
        return camera_atomic_tags(value)
    if section == "Sound mix":
        tag = _sound_mix_local_tag(value)
        return [tag] if tag else []
    if section == "Aspect ratio":
        ratio = re.sub(r"\s*\([^)]*\)\s*$", "", clean(value))
        ratio = re.sub(r"\s*:\s*", ":", ratio)
        return [ratio] if ratio else []

    # Negative Format / Cinematographic Process / Printed Film Format are
    # already individual IMDb bullets and stay intact in local fallback.
    return [value.strip()] if value.strip() else []



def legacy_camera_atomic_tags(value):
    """Values older builds may already have written; removal only."""
    out = []

    # A legacy camera-tag writer removed a trailing scene note from the whole
    # Camera value, then split top-level commas while preserving generic "and".
    whole = _camera_searchable_value(value)
    out.extend(split_top_level(
        whole,
        separators=(",", ";"),
        split_and=False,
    ))

    # Some TMM/Emby rewrites normalize a note on an individual comma item.
    # Include those normalized legacy spellings so local migration can remove
    # fragments such as "E- and H-Series Lenses" as well as the note-bearing
    # variant that the legacy writer originally produced.
    raw_chunks = split_top_level(
        clean(value),
        separators=(",", ";"),
        split_and=False,
    )
    normalized_chunks = [
        _camera_searchable_value(chunk)
        for chunk in raw_chunks
        if _camera_searchable_value(chunk)
    ]
    out.extend(normalized_chunks)

    # Earlier builds also split every top-level "and". Cover both the original
    # and per-item-normalized forms strictly for removal.
    out.extend(split_top_level(
        whole,
        separators=(",", ";"),
        split_and=True,
    ))
    for chunk in normalized_chunks:
        out.extend(split_top_level(
            chunk,
            separators=(",", ";"),
            split_and=True,
        ))

    return _dedupe_clean(out)

def generated_tag_values(specs, include_legacy=False):
    out = []
    seen = set()

    for section in TAG_SECTIONS:
        for value in specs.get(section, []) or []:
            candidates = list(emby_tag_values(section, value))
            if include_legacy and section == "Camera":
                candidates.extend(legacy_camera_atomic_tags(value))
            elif include_legacy and section == "Aspect ratio":
                # Older 1.x builds sometimes preserved IMDb spacing around the
                # colon (for example "2.39 : 1") while newer rules normalize
                # it to "2.39:1".  Include both spellings strictly for legacy
                # ownership/removal so an upgrade cannot leave duplicate ratios.
                raw_ratio = re.sub(r"\s*\([^)]*\)\s*$", "", clean(value))
                if raw_ratio:
                    candidates.append(raw_ratio)
                    candidates.append(re.sub(r"\s*:\s*", ":", raw_ratio))

            for tag_value in candidates:
                tag_value = clean(tag_value)
                key = tag_value.casefold()
                if tag_value and key not in seen:
                    seen.add(key)
                    out.append(tag_value)

    return out


def local_tag_entries(specs):
    entries = []
    seen = set()
    for section in TAG_SECTIONS:
        for idx, value in enumerate(specs.get(section, []) or []):
            for tag_value in emby_tag_values(section, value):
                tag_value = clean(tag_value)
                key = tag_value.casefold()
                if tag_value and key not in seen:
                    seen.add(key)
                    entries.append({"value": tag_value, "field": section, "source_indexes": [idx]})
    return entries


def _ownership_path(nfo):
    key = hashlib.sha256(os.path.realpath(str(nfo)).encode("utf-8", "surrogatepass")).hexdigest()
    return OWNERSHIP_DIR / f"{key}.json"


def load_ownership_record(nfo, imdb=""):
    obj = load_json(_ownership_path(nfo), {}) or {}
    if not isinstance(obj, dict):
        return None
    if imdb and clean(obj.get("imdb", "")).casefold() != clean(imdb).casefold():
        return None
    if not obj.get("engine") or not isinstance(obj.get("entries"), list):
        return None
    return obj


def save_ownership_record(nfo, imdb, tag_result):
    if not tag_result or not tag_result.get("engine"):
        return
    obj = {
        "schema": 2, "path": os.path.realpath(str(nfo)), "imdb": clean(imdb).lower(),
        "engine": clean(tag_result.get("engine", "")), "model": clean(tag_result.get("model", "")),
        "prompt_hash": clean(tag_result.get("prompt_hash", "")),
        "spec_hash": clean(tag_result.get("spec_hash", "")), "state": clean(tag_result.get("state", "current")) or "current",
        "generated": clean(tag_result.get("generated", "")) or dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
        "entries": list(tag_result.get("entries") or []),
        "manual_entries": list(tag_result.get("manual_entries") or []),
    }
    save_json(_ownership_path(nfo), obj)


def _manifest_result_from_record(rec):
    if not rec:
        return None
    return {
        "entries": list(rec.get("entries") or []), "engine": rec.get("engine") or "preserved",
        "model": rec.get("model") or "", "prompt_hash": rec.get("prompt_hash") or "",
        "spec_hash": rec.get("spec_hash") or "", "state": rec.get("state") or "current",
        "generated": rec.get("generated") or "", "write_manifest": True,
        "manual_entries": list(rec.get("manual_entries") or []),
    }


def _manifest_values(old_obj):
    return [clean(x.get("value", "")) for x in (old_obj or {}).get("owned_entries", []) if clean(x.get("value", ""))]


def _pristine_backup(nfo, current_iid):
    candidates = [Path(str(nfo) + ".imdbtech.original.bak"), Path(str(nfo) + ".imdbtech.bak")]
    for bp in candidates:
        try:
            text = bp.read_bytes().decode("utf-8-sig")
            ET.fromstring(text.encode("utf-8"))
        except Exception:
            continue
        if existing_tech_object(text):
            continue
        bid = imdb_id(text)
        if current_iid and bid and bid.lower() != current_iid.lower():
            continue
        return bp, text
    return None, None


def protected_tmm_tags(nfo, current_iid):
    bp, text = _pristine_backup(nfo, current_iid)
    if not text:
        return [], ""
    return _dedupe_clean(_all_normal_tags(text)), str(bp)


def preserve_pristine_original(nfo, current_iid):
    original = Path(str(nfo) + ".imdbtech.original.bak")
    if original.exists():
        return str(original)
    bp, text = _pristine_backup(nfo, current_iid)
    if not bp or not text or bp == original:
        return ""
    try:
        raw = bp.read_bytes()
        tmp = Path(str(original) + ".tmp")
        tmp.write_bytes(raw)
        os.replace(tmp, original)
        return str(original)
    except Exception:
        return ""


def resolve_tag_entries(obj, mode="local", existing=None):
    specs = obj["specs"]
    cfg = ai_config()
    spec_hash = _specs_hash(specs)

    if mode == "preserve":
        preserved = _manifest_result_from_obj(obj)
        if preserved:
            return preserved
        return {"entries": [], "engine": "", "model": "", "prompt_hash": "", "warnings": [], "write_manifest": False}

    if mode == "ai":
        try:
            result = ai_generate_tags(specs, existing=existing)
            return {
                "entries": result["tags"], "engine": "ai",
                "model": result.get("model", cfg.get("model", "")),
                "prompt_hash": result.get("prompt_hash", ""), "spec_hash": spec_hash,
                "warnings": result.get("warnings", []), "review_reasons": result.get("review_reasons", []),
                "review_required": result.get("review_required", False),
                "usage": result.get("usage", {}), "cost": result.get("cost", 0),
                "cached_usage": result.get("cached_usage", {}), "cached_cost": result.get("cached_cost", 0),
                "cache_hit": result.get("cache_hit", False), "write_manifest": True, "state": "current",
            }
        except AIRequestError as e:
            # Quota/auth/pause are runtime states, not parse failures. Never
            # silently turn an AI batch into local-rule output when the account
            # runs out of credit or the key becomes invalid.
            if e.kind in ("quota", "auth", "paused", "budget"):
                raise
            if cfg.get("fallback_mode") != "local-rules":
                raise
        except Exception:
            if cfg.get("fallback_mode") != "local-rules":
                raise

    entries = local_tag_entries(specs)
    for x in entries:
        x.setdefault("confidence", "high")
        x.setdefault("operation", "local-rule")
    return {
        "entries": entries, "engine": "local-rules", "model": LOCAL_RULES_VERSION,
        "prompt_hash": "", "spec_hash": spec_hash, "warnings": [], "review_required": False,
        "write_manifest": True, "state": "current",
    }

def existing_tech_object(text, root=None):
    try:
        root = root if root is not None else _xml_parse(text)
    except ET.ParseError:
        return None

    tech = None
    for child in list(root):
        if local_name(child.tag) != "technicalspecs":
            continue
        if clean(child.attrib.get("source", "")).casefold() == "imdb":
            tech = child

    if tech is None:
        return None

    specs = {k: [] for k in SECTIONS}
    for section in list(tech):
        if local_name(section.tag) != "section":
            continue
        name = clean(section.attrib.get("name", ""))
        if name not in specs:
            continue
        for item in list(section):
            if local_name(item.tag) != "item":
                continue
            value = clean("".join(item.itertext()))
            if value and value not in specs[name]:
                specs[name].append(value)

    source_specs = {k: [] for k in SECTIONS}
    source_fetched = source_spec_hash = ""
    for snapshot in list(tech):
        if local_name(snapshot.tag) != "sourcesnapshot":
            continue
        source_fetched = clean(snapshot.attrib.get("fetched", ""))
        source_spec_hash = clean(snapshot.attrib.get("specHash", ""))
        for section in list(snapshot):
            if local_name(section.tag) != "section":
                continue
            name = clean(section.attrib.get("name", ""))
            if name not in source_specs:
                continue
            for item in list(section):
                if local_name(item.tag) != "item":
                    continue
                value = clean("".join(item.itertext()))
                if value and value not in source_specs[name]:
                    source_specs[name].append(value)
        break
    if not useful(source_specs):
        source_specs = {key: list(values) for key, values in specs.items()}

    tech_status = clean(tech.attrib.get("status", "ok")).lower() or "ok"
    if not useful(specs) and tech_status not in ("empty", "no-tech"):
        return None

    imdb = clean(tech.attrib.get("imdbid", "")) or imdb_id(text)
    if not imdb or not re.fullmatch(r"tt\d{5,12}", imdb, flags=re.I):
        return None

    url = ""
    for child in list(tech):
        if local_name(child.tag) == "url":
            url = clean("".join(child.itertext()))
            break

    try:
        format_version = int(clean(tech.attrib.get("formatVersion", "0")) or 0)
    except Exception:
        format_version = 0

    fetched = clean(tech.attrib.get("fetched", ""))
    if not fetched:
        fetched = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()

    owned_entries = []
    manual_entries = []
    tag_engine = tag_model = tag_prompt_hash = tag_spec_hash = tag_state = tag_generated = ""
    for child in list(tech):
        child_name = local_name(child.tag)
        if child_name == "manualtags":
            for tag in list(child):
                if local_name(tag.tag) != "tag":
                    continue
                value = clean("".join(tag.itertext()))
                if value:
                    manual_entries.append({
                        "id": clean(tag.attrib.get("id", "")), "value": value,
                        "origin": clean(tag.attrib.get("origin", "manual-add")) or "manual-add",
                        "replaces": clean(tag.attrib.get("replaces", "")),
                        "field": clean(tag.attrib.get("field", "")),
                        "created": clean(tag.attrib.get("created", "")),
                        "modified": clean(tag.attrib.get("modified", "")),
                    })
            continue
        if child_name != "generatedtags":
            continue
        tag_engine = clean(child.attrib.get("engine", ""))
        tag_model = clean(child.attrib.get("model", ""))
        tag_prompt_hash = clean(child.attrib.get("promptHash", ""))
        tag_spec_hash = clean(child.attrib.get("specHash", ""))
        tag_state = clean(child.attrib.get("state", "current")) or "current"
        tag_generated = clean(child.attrib.get("generated", ""))
        for tag in list(child):
            if local_name(tag.tag) != "tag":
                continue
            value = clean("".join(tag.itertext()))
            field = clean(tag.attrib.get("field", ""))
            raw_idxs = clean(tag.attrib.get("sourceIndexes", ""))
            idxs = []
            for x in raw_idxs.split(",") if raw_idxs else []:
                try:
                    idxs.append(int(x))
                except Exception:
                    pass
            if value:
                owned_entries.append({
                    "id": clean(tag.attrib.get("id", "")), "value": value,
                    "origin": clean(tag.attrib.get("origin", "generated")) or "generated",
                    "field": field, "source_indexes": idxs,
                    "confidence": clean(tag.attrib.get("confidence", "")) or "high",
                    "operation": clean(tag.attrib.get("operation", "")),
                })

    calculated_spec_hash = _specs_hash(specs)
    return {
        "cache_version": CACHE_VERSION,
        "imdb": imdb.lower(),
        "fetched_at": fetched,
        "url": url or f"https://www.imdb.com/title/{imdb.lower()}/technical/",
        "method": "existing-nfo",
        "parser": "existing-nfo",
        "attempts": [],
        "status": "empty" if tech_status in ("empty", "no-tech") else "ok",
        "specs": specs,
        "ok": True,
        "ready": True,
        "format_version": format_version,
        "spec_hash": clean(tech.attrib.get("specHash", "")) or calculated_spec_hash,
        "source_specs": source_specs,
        "source_spec_hash": clean(tech.attrib.get("sourceSpecHash", "")) or source_spec_hash or _specs_hash(source_specs),
        "source_fetched_at": source_fetched or fetched,
        "modified": clean(tech.attrib.get("modified", "")).lower() == "manual",
        "modified_at": clean(tech.attrib.get("modifiedAt", "")),
        "owned_entries": owned_entries,
        "manual_entries": manual_entries,
        "tag_engine": tag_engine,
        "tag_model": tag_model,
        "tag_prompt_hash": tag_prompt_hash,
        "tag_spec_hash": tag_spec_hash,
        "tag_state": tag_state,
        "tag_generated": tag_generated,
    }


def nfo_spec_is_ready(text, expected_imdb=""):
    existing = existing_tech_object(text)
    if not existing:
        return False
    if expected_imdb and existing.get("imdb", "").casefold() != expected_imdb.casefold():
        return False
    return bool(existing.get("ready"))


def _manifest_result_from_obj(obj):
    if not obj or not obj.get("tag_engine"):
        return None
    return {
        "entries": list(obj.get("owned_entries") or []),
        "engine": obj.get("tag_engine") or "preserved",
        "model": obj.get("tag_model") or "",
        "prompt_hash": obj.get("tag_prompt_hash") or "",
        "spec_hash": obj.get("tag_spec_hash") or obj.get("spec_hash") or _specs_hash(obj.get("specs", {})),
        "state": obj.get("tag_state") or "current",
        "generated": obj.get("tag_generated") or "",
        "manual_entries": list(obj.get("manual_entries") or []),
        "write_manifest": True,
    }


def tag_state(text, obj=None, nfo=None, static=False, root=None):
    obj = obj or existing_tech_object(text, root)
    if not obj:
        return "spec-missing"
    current_spec_hash = _specs_hash(obj.get("specs", {}))
    if not obj.get("tag_engine") and nfo is not None:
        rec = load_ownership_record(nfo, obj.get("imdb", ""))
        if rec:
            obj = dict(obj)
            obj.update({
                "owned_entries": list(rec.get("entries") or []), "tag_engine": rec.get("engine", ""),
                "tag_model": rec.get("model", ""), "tag_prompt_hash": rec.get("prompt_hash", ""),
                "tag_spec_hash": rec.get("spec_hash", ""), "tag_state": rec.get("state", "current"),
            })
    if obj.get("tag_engine"):
        manifest_values = _manifest_values(obj)
        have = {_canon_tag_value(v) for v in _all_normal_tags(text, root) if clean(v)}
        if any(_canon_tag_value(v) not in have for v in manifest_values):
            return "tag-missing"
        tag_spec_hash = obj.get("tag_spec_hash") or ""
        if tag_spec_hash and tag_spec_hash != current_spec_hash:
            return "stale"
        if obj.get("tag_state") == "review":
            return "review"
        if obj.get("tag_engine") == "ai":
            if not static:
                cfg = ai_config()
                if obj.get("tag_model") != cfg.get("model") or obj.get("tag_prompt_hash") not in _accepted_ai_prompt_hashes(cfg):
                    return "stale"
            return "ai-current"
        if obj.get("tag_engine") == "local-rules":
            return "local-current"
        return "current"
    # pre-2.0 data: specs exist but ownership is not explicit.
    legacy = {clean(v).casefold() for v in generated_tag_values(obj["specs"], include_legacy=True) if clean(v)}
    have = {clean(v).casefold() for v in _all_normal_tags(text, root) if clean(v)}
    return "legacy" if legacy & have else "none"

def nfo_is_current_and_complete(text):
    # Backward-compatible helper used by tag jobs. Step 1 no longer calls this;
    # Spec readiness and Tag freshness are independent in 2.0.
    return tag_state(text) in ("ai-current", "local-current", "current")


def _normalized_status(obj):
    status = clean((obj or {}).get("status", "ok")).lower()
    return "empty" if status in ("empty", "no-tech") else "ok"


def _tag_entry_id(prefix, entry):
    stable = {
        "value": clean(entry.get("value", "")),
        "field": clean(entry.get("field", "")),
        "source_indexes": [x for x in entry.get("source_indexes", []) if isinstance(x, int)],
        "origin": clean(entry.get("origin", "")),
        "replaces": clean(entry.get("replaces", "")),
    }
    digest = hashlib.sha256(json.dumps(stable, ensure_ascii=False, sort_keys=True).encode("utf-8")).hexdigest()[:16]
    return f"{prefix}-{digest}"


def _append_spec_sections(lines, specs, indent="    "):
    for section in SECTIONS:
        vals = specs.get(section, [])
        if not vals:
            continue
        lines.append(f'{indent}<section name="{esc(section)}">')
        for value in vals:
            lines.append(f"{indent}  <item>{esc(value)}</item>")
        lines.append(f"{indent}</section>")

def technical_block(obj, nl, media_type, local_title, tag_result=None):
    lines = ["  <!-- tmm-imdb-tech:BEGIN -->"]
    spec_hash = _specs_hash(obj.get("specs", {}))
    attrs = (
        f'source="IMDb" '
        f'imdbid="{esc(obj["imdb"])}" '
        f'mediatype="{esc(media_type)}" '
        f'fetched="{esc(obj["fetched_at"])}" '
        f'formatVersion="{FORMAT_VERSION}" '
        f'specHash="{esc(spec_hash)}" '
        f'factOrigin="imdb" '
        f'status="{esc(_normalized_status(obj))}"'
    )
    source_specs = obj.get("source_specs") or obj.get("specs", {})
    source_spec_hash = clean(obj.get("source_spec_hash", "")) or _specs_hash(source_specs)
    if source_spec_hash:
        attrs += f' sourceSpecHash="{esc(source_spec_hash)}"'
    if obj.get("modified"):
        attrs += ' modified="manual"'
        if obj.get("modified_at"):
            attrs += f' modifiedAt="{esc(obj["modified_at"])}"'
    if local_title:
        attrs += f' localtitle="{esc(local_title)}"'
    lines.append(f"  <technicalspecs {attrs}>")

    _append_spec_sections(lines, obj.get("specs", {}))

    if obj.get("modified"):
        snapshot_meta = f'factOrigin="imdb" specHash="{esc(source_spec_hash)}"'
        if obj.get("source_fetched_at"):
            snapshot_meta += f' fetched="{esc(obj["source_fetched_at"])}"'
        lines.append(f"    <sourcesnapshot {snapshot_meta}>")
        _append_spec_sections(lines, source_specs, indent="      ")
        lines.append("    </sourcesnapshot>")

    fallback_url = "https://www.imdb.com/title/{}/technical/".format(obj["imdb"])
    lines.append(f'    <url>{esc(obj.get("url") or fallback_url)}</url>')
    if tag_result and tag_result.get("write_manifest", True):
        state = clean(tag_result.get("state", "current")) or "current"
        tag_spec_hash = clean(tag_result.get("spec_hash", "")) or spec_hash
        generated = clean(tag_result.get("generated", "")) or dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()
        meta = (
            f'owner="IMDb Tech Manager" engine="{esc(tag_result.get("engine", ""))}" '
            f'generated="{esc(generated)}" specHash="{esc(tag_spec_hash)}" state="{esc(state)}"'
        )
        if tag_result.get("model"):
            meta += f' model="{esc(tag_result["model"])}"'
        if tag_result.get("prompt_hash"):
            meta += f' promptHash="{esc(tag_result["prompt_hash"])}"'
        lines.append(f'    <generatedtags schema="2" {meta}>')
        for entry in tag_result.get("entries", []):
            idxs = ",".join(str(x) for x in entry.get("source_indexes", []) if isinstance(x, int))
            entry_id = clean(entry.get("id", "")) or _tag_entry_id("generated", entry)
            attr = f' id="{esc(entry_id)}" origin="generated" field="{esc(entry.get("field", ""))}"'
            if idxs:
                attr += f' sourceIndexes="{idxs}"'
            if entry.get("confidence"):
                attr += f' confidence="{esc(entry.get("confidence", ""))}"'
            if entry.get("operation"):
                attr += f' operation="{esc(entry.get("operation", ""))}"'
            lines.append(f'      <tag{attr}>{esc(entry["value"])}</tag>')
        lines.append("    </generatedtags>")
    manual_entries = list(obj.get("manual_entries") or [])
    if manual_entries:
        lines.append('    <manualtags owner="IMDb Tech Manager" schema="1">')
        for entry in manual_entries:
            entry_id = clean(entry.get("id", "")) or _tag_entry_id("manual", entry)
            origin = clean(entry.get("origin", "manual-add")) or "manual-add"
            attr = f' id="{esc(entry_id)}" origin="{esc(origin)}"'
            for key, xml_name in (("replaces", "replaces"), ("field", "field"), ("created", "created"), ("modified", "modified")):
                if entry.get(key):
                    attr += f' {xml_name}="{esc(entry[key])}"'
            lines.append(f'      <tag{attr}>{esc(entry["value"])}</tag>')
        lines.append("    </manualtags>")
    lines.append("  </technicalspecs>")
    lines.append("  <!-- tmm-imdb-tech:END -->")
    return nl.join(lines) + nl


def strip_technical_block_only(text):
    # Step 1 must never delete or generate normal <tag> values. Remove only our
    # Technical Specs XML and marker comments, leaving every root-level tag in place.
    text = re.sub(r'^[ \t]*<!--\s*tmm-imdb-tech:(?:BEGIN|END)\s*-->[ \t]*(?:\r?\n|$)', '', text, flags=re.M | re.I)
    text = re.sub(r'^[ \t]*<!--\s*IMDb Technical Specifications (?:BEGIN|END)\s*-->[ \t]*(?:\r?\n|$)', '', text, flags=re.M | re.I)
    text = re.sub(
        r'\s*<technicalspecs\b(?=[^>]*source=["\']IMDb["\'])[^>]*>.*?</technicalspecs>\s*',
        '\n', text, flags=re.S | re.I,
    )
    return text

def insert_standard_tags(text, tag_values, nl):
    # TMM's native schema keeps <tag> before <actor>. Put our searchable tags
    # there directly so TMM no longer needs to relocate them on its next save.
    have = {clean(v).casefold() for v in _all_normal_tags(text) if clean(v)}
    add = []
    for value in tag_values:
        value = clean(value)
        key = value.casefold()
        if value and key not in have:
            have.add(key)
            add.append(value)

    if not add:
        return text

    payload = "".join(f"  <tag>{esc(v)}</tag>{nl}" for v in add)

    # Preferred: after the last existing tag.
    matches = list(re.finditer(
        r'<tag(?:\s[^>]*)?>.*?</tag>[ \t]*(?:\r?\n)?',
        text,
        flags=re.S | re.I,
    ))
    if matches:
        pos = matches[-1].end()
        return text[:pos] + payload + text[pos:]

    # Otherwise follow TMM's usual genre -> studio -> tag -> actor order.
    for pattern in (
        r'^[ \t]*<studio>[^\r\n]*?</studio>[ \t]*(?:\r?\n|$)',
        r'^[ \t]*<genre>[^\r\n]*?</genre>[ \t]*(?:\r?\n|$)',
    ):
        matches = list(re.finditer(pattern, text, flags=re.M | re.I))
        if matches:
            pos = matches[-1].end()
            return text[:pos] + payload + text[pos:]

    actor = re.search(r'^[ \t]*<actor>\s*$', text, flags=re.M | re.I)
    if actor:
        return text[:actor.start()] + payload + text[actor.start():]

    fileinfo = re.search(r'^[ \t]*<fileinfo>\s*$', text, flags=re.M | re.I)
    if fileinfo:
        return text[:fileinfo.start()] + payload + text[fileinfo.start():]

    # Compact/minified NFO fallback: insert before the actual root closing tag.
    # This keeps v2 safe even when a valid NFO does not follow TMM's usual
    # one-element-per-line formatting.
    try:
        root = ET.fromstring(text.encode("utf-8"))
        root_name = local_name(root.tag)
        closes = list(re.finditer(rf'</{re.escape(root_name)}\s*>', text, flags=re.I))
        if closes:
            pos = closes[-1].start()
            prefix = "" if pos == 0 or text[:pos].endswith(("\n", "\r")) else nl
            return text[:pos] + prefix + payload + text[pos:]
    except Exception:
        pass

    return text


def _render_object_with_preserved_edits(obj, old_obj=None, source_refresh=False):
    """Carry user-owned metadata forward and keep manual effective facts authoritative."""
    render_obj = dict(obj or {})
    old_obj = old_obj or {}
    manual_entries = list(old_obj.get("manual_entries") or render_obj.get("manual_entries") or [])
    render_obj["manual_entries"] = manual_entries

    if old_obj.get("modified"):
        render_obj["specs"] = {key: list(values) for key, values in old_obj.get("specs", {}).items()}
        render_obj["modified"] = True
        render_obj["modified_at"] = old_obj.get("modified_at", "")
        if source_refresh:
            incoming_specs = (obj or {}).get("source_specs") or (obj or {}).get("specs", {})
            render_obj["source_specs"] = {key: list(values) for key, values in incoming_specs.items()}
            render_obj["source_spec_hash"] = _specs_hash(incoming_specs)
            render_obj["source_fetched_at"] = (obj or {}).get("source_fetched_at") or (obj or {}).get("fetched_at", "")
        else:
            render_obj["source_specs"] = old_obj.get("source_specs") or old_obj.get("specs", {})
            render_obj["source_spec_hash"] = old_obj.get("source_spec_hash", "")
            render_obj["source_fetched_at"] = old_obj.get("source_fetched_at", "")
    else:
        source_specs = render_obj.get("source_specs") or render_obj.get("specs", {})
        render_obj["source_specs"] = source_specs
        render_obj["source_spec_hash"] = clean(render_obj.get("source_spec_hash", "")) or _specs_hash(source_specs)
        render_obj["source_fetched_at"] = render_obj.get("source_fetched_at") or render_obj.get("fetched_at", "")
    return render_obj

def rewrite_specs_only(nfo, obj, info, dry_run=False):
    """Persist IMDb source facts only. Never add/remove normal root-level <tag>."""
    global LAST_REWRITE_DETAIL
    raw = nfo.read_bytes()
    source_hash = hashlib.sha256(raw).digest()
    bom = raw.startswith(b"\xef\xbb\xbf")
    text = raw.decode("utf-8-sig")
    nl = "\r\n" if "\r\n" in text else "\n"
    ET.fromstring(text.encode("utf-8"))

    old_obj = existing_tech_object(text)
    render_obj = _render_object_with_preserved_edits(obj, old_obj, source_refresh=True)
    preserved = _manifest_result_from_obj(old_obj) or _manifest_result_from_record(load_ownership_record(nfo, obj.get("imdb", "")))
    new_spec_hash = _specs_hash(render_obj.get("specs", {}))
    if preserved and preserved.get("spec_hash") != new_spec_hash:
        preserved["state"] = "stale"

    cleaned = strip_technical_block_only(text)
    block = technical_block(
        render_obj, nl, info["media_type"], info.get("display_title") or info.get("title") or "",
        tag_result=preserved,
    )
    tmm_lock = re.search(r'^[ \t]*<tmm_locked\s*/>[ \t]*(?:\r?\n|$)', cleaned, flags=re.M | re.I)
    if tmm_lock:
        new = cleaned[:tmm_lock.start()] + block + cleaned[tmm_lock.start():]
    else:
        close_re = rf"</{re.escape(info['root_tag'])}\s*>"
        if not re.search(close_re, cleaned, re.I):
            return "skip"
        new = re.sub(close_re, block + f"</{info['root_tag']}>", cleaned, count=1, flags=re.I)
    new = re.sub(r"(\r?\n){3,}", nl * 2, new)
    ET.fromstring(new.encode("utf-8"))

    before_tags = _all_normal_tags(text)
    after_tags = _all_normal_tags(new)
    if [x.casefold() for x in before_tags] != [x.casefold() for x in after_tags]:
        raise RuntimeError("Spec-only 安全检查失败：根级 <tag> 在第一阶段发生变化")

    LAST_REWRITE_DETAIL = {
        "status": "preview" if dry_run else "planned", "spec_only": True,
        "old_spec_hash": (old_obj or {}).get("spec_hash", ""), "new_spec_hash": new_spec_hash,
        "tag_state_preserved": bool(preserved), "root_tags_unchanged": True,
        "manual_specs_preserved": bool(old_obj and old_obj.get("modified")),
    }
    if dry_run:
        return "preview"
    if new == text:
        metrics_incr("nfo_write_skipped_unchanged")
        LAST_REWRITE_DETAIL["status"] = "current"
        return "current"

    preserve_pristine_original(nfo, obj.get("imdb", ""))
    tmp = Path(str(nfo) + ".imdbtech.tmp")
    data = new.encode("utf-8")
    if bom:
        data = b"\xef\xbb\xbf" + data
    with open(tmp, "wb") as f:
        f.write(data); f.flush(); os.fsync(f.fileno())
    with contextlib.suppress(Exception):
        shutil.copymode(nfo, tmp)
    try:
        current = nfo.read_bytes()
    except Exception:
        with contextlib.suppress(Exception): tmp.unlink()
        return "retry"
    if hashlib.sha256(current).digest() != source_hash:
        with contextlib.suppress(Exception): tmp.unlink()
        return "retry"

    backup = Path(str(nfo) + ".imdbtech.bak")
    backup_tmp = Path(str(backup) + ".tmp")
    with open(backup_tmp, "wb") as f:
        f.write(raw); f.flush(); os.fsync(f.fileno())
    os.replace(backup_tmp, backup)
    try:
        current = nfo.read_bytes()
    except Exception:
        with contextlib.suppress(Exception): tmp.unlink()
        return "retry"
    if hashlib.sha256(current).digest() != source_hash:
        with contextlib.suppress(Exception): tmp.unlink()
        return "retry"
    os.replace(tmp, nfo)
    metrics_incr("nfo_write_count")
    LAST_REWRITE_DETAIL["status"] = "updated"
    return "updated"


LAST_REWRITE_DETAIL = {}

def rewrite(nfo, obj, info, tag_mode="local", cleanup_mode="strict", dry_run=False, precomputed=None):
    global LAST_REWRITE_DETAIL
    raw = nfo.read_bytes()
    source_hash = hashlib.sha256(raw).digest()
    bom = raw.startswith(b"\xef\xbb\xbf")
    text = raw.decode("utf-8-sig")
    nl = "\r\n" if "\r\n" in text else "\n"
    ET.fromstring(text.encode("utf-8"))

    old_obj = existing_tech_object(text)
    render_obj = _render_object_with_preserved_edits(obj, old_obj, source_refresh=False)
    old_manifest = _manifest_values(old_obj)
    sidecar = load_ownership_record(nfo, obj.get("imdb", "")) if not (old_obj and old_obj.get("tag_engine")) else None
    if sidecar:
        old_manifest = [clean(x.get("value", "")) for x in sidecar.get("entries", []) if clean(x.get("value", ""))]
        ownership = "sidecar"
        if not render_obj.get("manual_entries"):
            render_obj["manual_entries"] = list(sidecar.get("manual_entries") or [])
    else:
        ownership = "manifest" if (old_obj and old_obj.get("tag_engine")) else ("legacy-inferred" if old_obj else "none")
    old_tags = old_manifest or (generated_tag_values(old_obj["specs"], include_legacy=True) if old_obj else [])

    protected, baseline_path = protected_tmm_tags(nfo, obj.get("imdb", ""))
    current_values = _all_normal_tags(text)
    current_keys = {_canon_tag_value(v) for v in current_values if clean(v)}
    current_exact = {clean(v).casefold() for v in current_values if clean(v)}
    old_present_raw = [v for v in old_tags if _canon_tag_value(v) in current_keys]
    # Canonical matching may surface several spelling variants of the same tag
    # (e.g. "2.39:1" legacy value vs the spaced "2.39 : 1" root tag). Collapse
    # them to the variant that literally exists in the NFO so preview diffs
    # report each tag once and show the spaced legacy spelling as replaced.
    old_present = []
    seen_canon = set()
    for v in old_present_raw:
        key = _canon_tag_value(v)
        if key in seen_canon:
            continue
        seen_canon.add(key)
        if clean(v).casefold() in current_exact:
            old_present.append(v)
        else:
            old_present.append(next((c for c in current_values if _canon_tag_value(c) == key), v))
    if cleanup_mode == "strict" and ownership == "legacy-inferred" and old_present and not baseline_path:
        LAST_REWRITE_DETAIL = {
            "status": "unsafe-skip", "ownership": ownership, "old_present": old_present,
            "protected": [], "reason": "没有可验证的原始 TMM 备份，严格模式拒绝推断删除旧标准 <tag>",
        }
        return "unsafe-skip"

    if precomputed is not None and tag_mode in ("ai", "local"):
        # Approved preview result: write exactly what the user reviewed.
        # AI reuses its cached model output; local rules reuse the deterministic
        # structured entries stored with the preview.
        tag_result = dict(precomputed)
        tag_result["spec_hash"] = _specs_hash(render_obj.get("specs", {}))
        tag_result["review_required"] = False
        tag_result["review_reasons"] = []
        tag_result["cache_hit"] = tag_mode == "ai"
    else:
        existing_tags_payload = _ai_existing_tags(render_obj)
        if not existing_tags_payload and sidecar and sidecar.get("entries"):
            se_engine = clean(str(sidecar.get("engine") or ""))
            existing_tags_payload = [
                {"value": clean(x.get("value", "")), "source": "ai" if se_engine == "ai" else "rules"}
                for x in sidecar.get("entries") or [] if clean(x.get("value", ""))
            ]
        tag_result = resolve_tag_entries(render_obj, mode=tag_mode, existing=existing_tags_payload or None)
    if tag_result.get("review_required"):
        LAST_REWRITE_DETAIL = {
            "status": "review", "ownership": ownership, "baseline": baseline_path,
            "protected": protected, "old_owned": old_present, "new_tags": [x.get("value", "") for x in tag_result.get("entries", [])],
            "tag_engine": tag_result.get("engine", ""), "model": tag_result.get("model", ""),
            "warnings": tag_result.get("warnings", []), "review_reasons": tag_result.get("review_reasons", []),
            "usage": tag_result.get("usage", {}), "cost": tag_result.get("cost", 0),
            "cached_usage": tag_result.get("cached_usage", {}), "cached_cost": tag_result.get("cached_cost", 0),
            "cache_hit": tag_result.get("cache_hit", False),
        }
        return "review"
    manual_values = _dedupe_clean(x.get("value", "") for x in render_obj.get("manual_entries", []))
    manual_keys = {_canon_tag_value(v) for v in manual_values if clean(v)}
    old_owned_keys = {_canon_tag_value(v) for v in old_tags if clean(v)}
    external_values = [
        value for value in current_values
        if _canon_tag_value(value) not in old_owned_keys and _canon_tag_value(value) not in manual_keys
    ]
    external_keys = {_canon_tag_value(v) for v in external_values if clean(v)}
    generated_entries = []
    satisfied_external = []
    satisfied_manual = []
    for entry in tag_result.get("entries", []):
        key = _canon_tag_value(entry.get("value", ""))
        if not key:
            continue
        if key in manual_keys:
            satisfied_manual.append(entry.get("value", ""))
            continue
        if key in external_keys:
            satisfied_external.append(entry.get("value", ""))
            continue
        generated_entries.append(entry)
    tag_result = dict(tag_result)
    tag_result["entries"] = generated_entries
    tag_result["manual_entries"] = list(render_obj.get("manual_entries") or [])
    new_entries = generated_entries
    new_tags = _dedupe_clean(x.get("value", "") for x in new_entries)

    protected_for_cleanup = _dedupe_clean(list(protected) + manual_values)
    cleaned = strip_generated(text, old_tags, protected_tags=protected_for_cleanup)
    cleaned = insert_standard_tags(cleaned, new_tags, nl)
    block = technical_block(render_obj, nl, info["media_type"], info.get("display_title") or info.get("title") or "", tag_result=tag_result)

    tmm_lock = re.search(r'^[ \t]*<tmm_locked\s*/>[ \t]*(?:\r?\n|$)', cleaned, flags=re.M | re.I)
    if tmm_lock:
        new = cleaned[:tmm_lock.start()] + block + cleaned[tmm_lock.start():]
    else:
        root_tag = info["root_tag"]
        close_re = rf"</{re.escape(root_tag)}\s*>"
        if not re.search(close_re, cleaned, re.I):
            return "skip"
        new = re.sub(close_re, block + f"</{root_tag}>", cleaned, count=1, flags=re.I)

    new = re.sub(r"(\r?\n){3,}", nl * 2, new)
    ET.fromstring(new.encode("utf-8"))

    after_values = _all_normal_tags(new)
    after_exact = {clean(v).casefold() for v in after_values if clean(v)}
    removed = [v for v in _all_normal_tags(text) if clean(v).casefold() not in after_exact]
    added = [v for v in after_values if clean(v).casefold() not in current_exact]
    retained_owned = [v for v in old_present if clean(v).casefold() in after_exact]
    replaced_owned = [v for v in old_present if clean(v).casefold() not in after_exact]
    LAST_REWRITE_DETAIL = {
        "status": "preview" if dry_run else "planned", "ownership": ownership,
        "baseline": baseline_path, "protected": protected_for_cleanup, "old_owned": old_present,
        "retained_owned": retained_owned, "replaced_owned": replaced_owned,
        "new_tags": new_tags, "final_tags": after_values, "removed": removed, "added": added,
        "manual_tags": manual_values, "external_tags": external_values,
        "satisfied_by_manual": _dedupe_clean(satisfied_manual),
        "satisfied_by_external": _dedupe_clean(satisfied_external),
        "tag_engine": tag_result.get("engine", ""), "model": tag_result.get("model", ""),
        "warnings": tag_result.get("warnings", []), "usage": tag_result.get("usage", {}),
        "cost": tag_result.get("cost", 0), "cached_usage": tag_result.get("cached_usage", {}),
        "cached_cost": tag_result.get("cached_cost", 0), "spec_hash": tag_result.get("spec_hash", ""),
        "cache_hit": tag_result.get("cache_hit", False),
    }
    if dry_run:
        return "preview"
    if new == text:
        metrics_incr("nfo_write_skipped_unchanged")
        return "current"

    preserve_pristine_original(nfo, obj.get("imdb", ""))
    tmp = Path(str(nfo) + ".imdbtech.tmp")
    data = new.encode("utf-8")
    if bom:
        data = b"\xef\xbb\xbf" + data
    with open(tmp, "wb") as f:
        f.write(data); f.flush(); os.fsync(f.fileno())
    with contextlib.suppress(Exception):
        shutil.copymode(nfo, tmp)

    try:
        current = nfo.read_bytes()
    except Exception:
        with contextlib.suppress(Exception): tmp.unlink()
        return "retry"
    if hashlib.sha256(current).digest() != source_hash:
        with contextlib.suppress(Exception): tmp.unlink()
        return "retry"

    backup = Path(str(nfo) + ".imdbtech.bak")
    backup_tmp = Path(str(backup) + ".tmp")
    with open(backup_tmp, "wb") as f:
        f.write(raw); f.flush(); os.fsync(f.fileno())
    os.replace(backup_tmp, backup)

    try:
        current = nfo.read_bytes()
    except Exception:
        with contextlib.suppress(Exception): tmp.unlink()
        return "retry"
    if hashlib.sha256(current).digest() != source_hash:
        with contextlib.suppress(Exception): tmp.unlink()
        return "retry"
    os.replace(tmp, nfo)
    metrics_incr("nfo_write_count")
    # legacy: rebuild the sidecar from the manifest actually written into the
    # NFO. technical_block assigns entry ids at write time; saving the raw
    # in-memory entries left the sidecar id-less and every freshly generated
    # NFO reported "manifest 与本地 ownership 镜像不一致".
    saved_from_final = False
    with contextlib.suppress(Exception):
        final_obj = existing_tech_object(nfo.read_bytes().decode("utf-8-sig"))
        final_result = _manifest_result_from_obj(final_obj) if final_obj else None
        if final_result:
            final_result["manual_entries"] = list(final_obj.get("manual_entries") or [])
            save_ownership_record(nfo, final_obj.get("imdb", ""), final_result)
            saved_from_final = True
    if not saved_from_final:
        save_ownership_record(nfo, obj.get("imdb", ""), tag_result)
    LAST_REWRITE_DETAIL["status"] = "updated"
    return "updated"


class PathOutsideLibraryError(ValueError):
    pass


class EditConflictError(RuntimeError):
    pass


def _configured_roots_for_access():
    return configured_roots_flat()


def _allowed_nfo_path(value):
    path = Path(os.path.realpath(os.path.expanduser(str(value or ""))))
    if path.suffix.casefold() != ".nfo":
        raise PathOutsideLibraryError("只允许读取或编辑 .nfo 文件")
    allowed = False
    for root in _configured_roots_for_access():
        try:
            root_real = os.path.realpath(os.path.expanduser(str(root)))
            if os.path.commonpath([str(path), root_real]) == root_real and str(path) != root_real:
                allowed = True
                break
        except ValueError:
            continue
    if not allowed:
        raise PathOutsideLibraryError("NFO 路径不在已配置的资料库中")
    if not path.is_file():
        raise FileNotFoundError("NFO 文件不存在或当前离线")
    return path


def _source_hash(raw):
    return hashlib.sha256(raw).hexdigest()


def _entry_matches_sidecar(obj, rec):
    if not obj or not rec:
        return None
    # Recovery mode: TMM stripped the embedded manifest, ownership lives in
    # the sidecar and tag_state merges it back. That is the design working,
    # not an inconsistency.
    if not clean(str(obj.get("tag_engine") or "")) and rec.get("entries"):
        return None
    def rows(values):
        return sorted((clean(x.get("id", "")), clean(x.get("value", "")).casefold()) for x in values or [])
    return rows(obj.get("owned_entries")) == rows(rec.get("entries")) and rows(obj.get("manual_entries")) == rows(rec.get("manual_entries"))


def _tag_rows(text, obj, root=None):
    generated = list((obj or {}).get("owned_entries") or [])
    manual = list((obj or {}).get("manual_entries") or [])
    used_generated = set()
    used_manual = set()
    rows = []
    for root_index, value in enumerate(_all_normal_tags(text, root)):
        key = _canon_tag_value(value)
        ownership = "external"
        entry = {}
        for idx, candidate in enumerate(manual):
            if idx not in used_manual and _canon_tag_value(candidate.get("value", "")) == key:
                ownership = "manual"
                entry = candidate
                used_manual.add(idx)
                break
        if ownership == "external":
            for idx, candidate in enumerate(generated):
                if idx not in used_generated and _canon_tag_value(candidate.get("value", "")) == key:
                    ownership = "generated"
                    entry = candidate
                    used_generated.add(idx)
                    break
        row = {
            "root_index": root_index, "id": clean(entry.get("id", "")) or f"root-tag-{root_index}",
            "value": value, "ownership": ownership,
        }
        for name in ("origin", "field", "source_indexes", "confidence", "operation", "replaces", "created", "modified", "engine"):
            if entry.get(name) not in (None, "", []):
                row[name] = entry[name]
        rows.append(row)
    return rows


def _inspector_base(path_value, nfo_stat=None):
    """Static (cacheable) layer: NFO-derived facts only, no dynamic overlay."""
    path = _allowed_nfo_path(path_value)
    if nfo_stat is None:
        with contextlib.suppress(Exception):
            nfo_stat = path.stat()
    raw = path.read_bytes()
    source_hash = _source_hash(raw)
    bom = raw.startswith(b"\xef\xbb\xbf")
    text = raw.decode("utf-8-sig")
    newline = "CRLF" if "\r\n" in text else "LF"
    assigned_space = configured_space_for_path(path)
    base = {
        "path": str(path), "source_hash": source_hash, "bom": bom, "newline": newline,
        "file_mode": oct(nfo_stat.st_mode & 0o777) if nfo_stat is not None else "", "xml_valid": False,
        "media_space": assigned_space or "movies", "media_type": "unknown", "title": path.stem,
        "year": "", "imdb": "", "spec_status": "missing", "tag_status": "none",
        "tags": [], "issues": [], "counts": {"external": 0, "generated": 0, "manual": 0, "issues": 0},
        "status_override": "", "ai_preview_ready": False,
    }
    metrics_incr("nfo_read_count")
    try:
        root = _xml_parse(text)
    except Exception as exc:
        base["issues"] = [{"kind": "xml-error", "message": str(exc), "path": str(path)}]
        base["counts"]["issues"] = 1
        base["lifecycle"] = _lifecycle_state(base["xml_valid"], base["media_type"], base["spec_status"], base["tag_status"])
        return base

    base["xml_valid"] = True
    info = inspect_nfo(text, root)
    root_name = local_name(root.tag)
    if not info:
        base["media_type"] = "season" if root_name == "season" else root_name
        detected_space = "tv" if root_name in ("season", "tvshow", "episodedetails") else "movies"
        base["media_space"] = assigned_space or detected_space
        base["title"] = child_text(root, "title") or path.stem
        base["spec_status"] = "not-applicable"
        base["tag_status"] = "not-applicable"
        base["lifecycle"] = _lifecycle_state(base["xml_valid"], base["media_type"], base["spec_status"], base["tag_status"])
        return base

    iid = (imdb_id(text) or "").lower()
    obj = existing_tech_object(text, root)
    tags = _tag_rows(text, obj, root)
    counts = {
        "external": sum(1 for x in tags if x["ownership"] == "external"),
        "generated": sum(1 for x in tags if x["ownership"] == "generated"),
        "manual": sum(1 for x in tags if x["ownership"] == "manual"),
        "issues": 0,
    }
    issues = []
    if not iid:
        issues.append({"kind": "missing-imdb", "message": "NFO 缺少 IMDb ID", "path": str(path)})
    if not obj:
        spec_status = "missing"
        tag_status_value = "none"
        issues.append({"kind": "spec-missing", "message": "尚未准备 IMDb Technical Specs", "path": str(path)})
    else:
        spec_status = "manual" if obj.get("modified") else ("empty" if obj.get("status") == "empty" else "ready")
        tag_status_value = tag_state(text, obj, path, static=True, root=root)
        if tag_status_value in ("stale", "tag-missing", "review"):
            issues.append({"kind": tag_status_value, "message": {"stale": "技术标签尚未与当前规格同步", "tag-missing": "manifest 中的生成标签在根节点缺失", "review": "AI 结果等待人工复核"}[tag_status_value], "path": str(path)})
    duplicates = []
    seen = set()
    for row in tags:
        key = clean(row["value"]).casefold()
        if key in seen and key not in duplicates:
            duplicates.append(key)
        seen.add(key)
    if duplicates:
        issues.append({"kind": "duplicate-tag", "message": "发现同值重复标签；为安全起见不会自动删除", "path": str(path)})

    rec = load_ownership_record(path, iid) if obj else None
    sidecar_match = _entry_matches_sidecar(obj, rec)
    # legacy migration: older releases could persist the pre-render sidecar
    # while the NFO received stable manifest IDs. The embedded manifest is the
    # authoritative artifact. Rebuild only the local mirror, then verify it;
    # never mutate the NFO or infer ownership from tag text.
    if obj and sidecar_match is False:
        with contextlib.suppress(Exception):
            healed = _manifest_result_from_obj(obj)
            if healed:
                healed["manual_entries"] = list(obj.get("manual_entries") or [])
                save_ownership_record(path, iid, healed)
                rec = load_ownership_record(path, iid)
                sidecar_match = _entry_matches_sidecar(obj, rec)
    if sidecar_match is False:
        issues.append({"kind": "ownership-mismatch", "message": "NFO manifest 与本地 ownership 镜像不一致", "path": str(path)})
    detected_space = "movies" if info["media_type"] == "movie" else "tv"
    if assigned_space and assigned_space != detected_space:
        issues.append({"kind": "library-type-mismatch", "message": "NFO 类型与所选资料库分类不一致；已停止生成操作", "path": str(path)})
    counts["issues"] = len(issues)
    nfo_mtime = nfo_stat.st_mtime if nfo_stat is not None else 0.0
    date_added_raw = child_text(root, "dateadded") or ""
    date_added_match = re.search(r"(\d{4})-(\d{2})-(\d{2})", date_added_raw)
    added_date = date_added_match.group(0) if date_added_match else ""
    base.update({
        "media_space": assigned_space or detected_space,
        "detected_media_space": detected_space,
        "library_type_mismatch": bool(assigned_space and assigned_space != detected_space),
        "media_type": info["media_type"], "series_key": info.get("show_title", ""),
        "season": info.get("season"), "episode": info.get("episode"),
        "title": info.get("display_title") or info.get("title") or path.stem,
        "year": info.get("year", ""), "imdb": iid,
        "added_date": added_date,
        "spec_status": spec_status, "tag_status": tag_status_value,
        "status_override": "",
        "ai_preview_ready": bool(obj and _ai_input_specs(obj.get("specs", {}))),
        "cached_review": bool(obj and _cached_ai_review_state(obj.get("specs", {})) == "review"),
        "has_ai_ownership": bool(clean(str((obj or {}).get("tag_engine") or "") or clean(str((rec or {}).get("engine") or ""))) == "ai"),
        "tag_engine": clean(str((obj or {}).get("tag_engine") or "")),
        "tag_model": clean(str((obj or {}).get("tag_model") or "")),
        "tag_prompt_hash": clean(str((obj or {}).get("tag_prompt_hash") or "")),
        "nfo_mtime": nfo_mtime,
        "tags": tags, "issues": issues, "ignored_issues": [], "counts": counts,
        "manifest_sidecar_match": sidecar_match,
        "backups": {
            "latest": Path(str(path) + ".imdbtech.bak").exists(),
            "pristine": Path(str(path) + ".imdbtech.original.bak").exists(),
        },
    })
    base["lifecycle"] = _lifecycle_state(base["xml_valid"], base["media_type"], spec_status, tag_status_value)
    if obj:
        base["technical_specs"] = {
            "effective": obj.get("specs", {}), "source": obj.get("source_specs", {}),
            "modified": bool(obj.get("modified")), "modified_at": obj.get("modified_at", ""),
            "fetched_at": obj.get("fetched_at", ""), "source_fetched_at": obj.get("source_fetched_at", ""),
            "spec_hash": _specs_hash(obj.get("specs", {})), "source_spec_hash": _specs_hash(obj.get("source_specs", {})),
            "tag_fields": list(TAG_SECTIONS), "spec_only_fields": [x for x in SECTIONS if x not in TAG_SECTIONS],
        }
    return base

def _apply_summary_overlay(item, dynamic=None):
    """Dynamic layer: AI-config freshness, failures, acks, manual override.

    Applied on top of a cached base summary on every read. These inputs change
    independently of NFO content and must never invalidate the base cache.
    """
    if not isinstance(item, dict) or not item.get("path"):
        return item
    dynamic = dynamic if isinstance(dynamic, dict) else _dynamic_overlay_context()
    out = dict(item)
    path_str = str(out.get("path"))
    issues = [dict(x) for x in (out.get("issues") or [])]

    tag_status = out.get("tag_status")
    if clean(str(out.get("tag_engine") or "")) == "ai" and tag_status == "ai-current":
        cfg = dynamic.get("config") or {}
        prompt_hashes = set(dynamic.get("accepted_prompt_hashes") or [dynamic.get("prompt_hash") or ""])
        if clean(str(out.get("tag_model") or "")) != clean(str(cfg.get("model") or "")) or clean(str(out.get("tag_prompt_hash") or "")) not in prompt_hashes:
            # legacy rule: AI ownership is persistent. A prompt/model change
            # only annotates (regeneration is still offered); the bucket
            # stays AI 完成. Spec edits downgrade via the base-layer stale.
            out["prompt_stale"] = True
            issues.append({"kind": "prompt-stale", "message": "AI 提示词/模型已更新；建议重新生成（状态保持 AI 完成）", "path": path_str})

    # Cached summaries already contain canonical paths. Resolving every item
    # here made a library read touch the NAS once per NFO.
    failure = (dynamic.get("failures") or {}).get(path_str)
    if failure:
        issues.append({
            "kind": failure.get("kind") or "ai-failure", "message": failure.get("message") or "AI 处理失败",
            "time": failure.get("last_time") or failure.get("updated_at") or "", "task_id": failure.get("task_id") or "", "path": path_str,
        })

    acknowledgements = dynamic.get("acknowledgements") or {}
    ack = acknowledgements.get(path_str, {}) if isinstance(acknowledgements, dict) else {}
    ignored_kinds = set(ack.get("kinds", [])) if ack.get("source_hash") == out.get("source_hash") else set()
    ignored_issues = [item2 for item2 in issues if item2.get("kind") in ignored_kinds]
    issues = [item2 for item2 in issues if item2.get("kind") not in ignored_kinds]

    counts = dict(out.get("counts") or {})
    counts["issues"] = len(issues)
    counts["ignored_issues"] = len(ignored_issues)
    override = _status_override_from_store(path_str, out.get("source_hash"), dynamic.get("overrides") or {})
    out["tag_status"] = tag_status
    out["issues"] = issues
    out["ignored_issues"] = ignored_issues
    out["counts"] = counts
    out["status_override"] = override
    out["lifecycle"] = _lifecycle_state(out.get("xml_valid"), out.get("media_type"), out.get("spec_status"), tag_status, override)
    return out


def inspector_detail(path_value):
    return _apply_summary_overlay(_inspector_base(path_value))


def acknowledge_issue(payload):
    payload = payload or {}
    path = _allowed_nfo_path(payload.get("path"))
    raw = path.read_bytes()
    source_hash = _source_hash(raw)
    expected = clean(str(payload.get("expected_source_hash") or ""))
    if expected and expected != source_hash:
        raise EditConflictError("NFO 已变化，请刷新后重新确认问题")
    operation = clean(str(payload.get("operation") or "ignore"))
    store = load_json(ISSUE_ACKS, {}) or {}
    if not isinstance(store, dict): store = {}
    if operation == "restore":
        store.pop(str(path), None)
    else:
        kind = clean(str(payload.get("kind") or ""))
        protected = {"xml-error", "read-error", "library-type-mismatch"}
        if not kind or kind in protected:
            raise ValueError("此问题涉及文件安全或资料库类型，不能忽略")
        current = inspector_detail(str(path))
        if kind not in {item.get("kind") for item in current.get("issues", [])}:
            raise ValueError("问题已不存在，请刷新")
        old = store.get(str(path), {})
        kinds = set(old.get("kinds", [])) if old.get("source_hash") == source_hash else set()
        kinds.add(kind)
        store[str(path)] = {"source_hash": source_hash, "kinds": sorted(kinds), "updated_at": _utc_now()}
    save_json(ISSUE_ACKS, store)
    return {"ok": True, "operation": operation, "item": inspector_detail(str(path))}


INDEX_SUMMARY_KEYS = (
    "path", "source_hash", "media_space", "media_type", "series_key", "title", "year", "imdb",
    "season", "episode", "xml_valid", "spec_status", "tag_status", "lifecycle", "status_override",
    "counts", "issues", "ignored_issues", "library_type_mismatch", "ai_preview_ready", "sort_title",
    "tag_engine", "tag_model", "tag_prompt_hash", "nfo_mtime", "added_date", "cached_review", "has_ai_ownership",
	"manifest_sidecar_match",
)


def _lifecycle_state(xml_valid, media_type, spec_status, tag_status_value, override=""):
    """Single source of truth for the legacy status model.

    ai-complete is the only "best" state; everything else (including
    local-complete) is a working state. The list/overview/filters all read
    this field so they can never disagree again.
    """
    if not xml_valid:
        return "xml-error"
    if media_type not in ("movie", "tvshow", "episode"):
        return "not-applicable"
    if override:
        return override
    if spec_status == "missing":
        return "spec-missing"
    if spec_status == "empty":
        return "spec-empty"
    if tag_status_value == "ai-current":
        return "ai-complete"
    if tag_status_value == "local-current":
        return "local-complete"
    if tag_status_value in ("stale", "review", "tag-missing", "legacy", "current"):
        return "legacy" if tag_status_value == "current" else tag_status_value
    if spec_status in ("ready", "manual"):
        return "no-tags"
    return "spec-missing"


def _status_override_for(path_value, source_hash):
    store = load_json(STATUS_OVERRIDES, {}) or {}
    if not isinstance(store, dict):
        return ""
    return _status_override_from_store(path_value, source_hash, store)


def _status_override_from_store(path_value, source_hash, store):
    if not isinstance(store, dict):
        return ""
    record = store.get(str(path_value))
    if not isinstance(record, dict):
        return ""
    value = clean(str(record.get("value") or ""))
    if value not in STATUS_OVERRIDE_VALUES:
        return ""
    # Manual status is intent for one specific NFO revision; any content
    # change (edit, regeneration) invalidates it automatically.
    if record.get("source_hash") and source_hash and record.get("source_hash") != source_hash:
        return ""
    return value


def set_status_override(payload):
    payload = payload or {}
    path = _allowed_nfo_path(payload.get("path"))
    raw = path.read_bytes()
    source_hash = _source_hash(raw)
    value = clean(str(payload.get("value") or ""))
    if value and value not in STATUS_OVERRIDE_VALUES:
        raise ValueError("无效的状态值")
    store = load_json(STATUS_OVERRIDES, {}) or {}
    if not isinstance(store, dict):
        store = {}
    key = str(path)
    if value:
        store[key] = {"value": value, "source_hash": source_hash, "updated_at": _utc_now()}
    else:
        store.pop(key, None)
    save_json(STATUS_OVERRIDES, store)
    item = inspector_detail(str(path))
    with contextlib.suppress(Exception):
        refresh_path_summary(path, item=item)
    return {"ok": True, "value": value, "item": item}


def _summary_from_detail(detail):
    detail = dict(detail or {})
    detail.setdefault("lifecycle", _lifecycle_state(
        detail.get("xml_valid"), detail.get("media_type"),
        detail.get("spec_status"), detail.get("tag_status"), detail.get("status_override", ""),
    ))
    detail["sort_title"] = clean(str(detail.get("title") or "")).casefold()
    return {key: detail.get(key) for key in INDEX_SUMMARY_KEYS}


def _index_context_stamp():
    """Cache-format stamp for the base summary store.

    legacy: dynamic layers (AI config, failure queue, acknowledgements,
    overrides) moved into _apply_summary_overlay, so a local state change no
    longer invalidates every cached NFO summary. Only the schema version
    controls the base cache now.
    """
    return "v%d" % INDEX_CACHE_SCHEMA


_INDEX_CACHE_MEM = {"stamp": "", "items": {}, "revision": 0}
_INDEX_DELTA_SUFFIX = ".updates"
_INDEX_INCREMENTAL_THRESHOLD = 200
_INDEX_COMPACT_BYTES = 4 * 1024 * 1024
_INDEX_COMPACT_UPDATES = 512

# Dynamic state is shared by all summaries returned in one request.  The
# previous implementation loaded four JSON files once per NFO, which made a
# warm library listing O(items * files) even though none of those files was
# title-specific.
_DYNAMIC_OVERLAY_MEM = {"stamp": None, "value": {}}


def _index_cache_lock_path():
    return Path(str(INDEX_CACHE) + ".lock")


@contextlib.contextmanager
def _index_cache_lock(exclusive=False):
    """Serialize base-file replacement and journal append across processes."""
    import fcntl
    lock_path = _index_cache_lock_path()
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    handle = lock_path.open("a+")
    try:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX if exclusive else fcntl.LOCK_SH)
        yield
    finally:
        with contextlib.suppress(Exception):
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
        handle.close()


def _revision_value(value):
    try:
        return max(0, int(value or 0))
    except Exception:
        return 0


def _next_index_revision(current=0):
    return max(_revision_value(current) + 1, int(time.time() * 1000000))


def _index_cache_load_unlocked(stamp, force_disk=False):
    if not force_disk and _INDEX_CACHE_MEM.get("stamp") == stamp and isinstance(_INDEX_CACHE_MEM.get("items"), dict):
        return _INDEX_CACHE_MEM["items"]
    data = load_json(INDEX_CACHE, {}) or {}
    items = data.get("items") if data.get("stamp") == stamp and isinstance(data.get("items"), dict) else {}
    revision = _revision_value(data.get("revision")) if data.get("stamp") == stamp else 0
    # Large catalogs use a small append-only delta journal between compaction
    # points.  This keeps one NFO update from rewriting a multi-megabyte JSON
    # document while preserving the existing cache format for readers.
    journal = Path(str(INDEX_CACHE) + _INDEX_DELTA_SUFFIX)
    if isinstance(items, dict) and journal.exists():
        try:
            for line in journal.read_text(encoding="utf-8").splitlines():
                row = json.loads(line)
                if row.get("stamp") != stamp:
                    continue
                real = row.get("path")
                if not real:
                    continue
                if row.get("deleted"):
                    items.pop(real, None)
                elif isinstance(row.get("entry"), dict):
                    items[real] = row["entry"]
                revision = max(revision, _revision_value(row.get("revision")))
        except Exception:
            # A truncated journal must not prevent the app from opening.  The
            # next successful reconcile will compact it into the base cache.
            pass
    _INDEX_CACHE_MEM.update(stamp=stamp, items=items, revision=revision)
    return items


def _index_cache_load(stamp, force_disk=False):
    if not force_disk and _INDEX_CACHE_MEM.get("stamp") == stamp and isinstance(_INDEX_CACHE_MEM.get("items"), dict):
        return _INDEX_CACHE_MEM["items"]
    with _index_cache_lock(exclusive=False):
        return _index_cache_load_unlocked(stamp, force_disk=force_disk)


def _index_cache_save_unlocked(stamp, items, revision):
    revision = _revision_value(revision)
    _INDEX_CACHE_MEM.update(stamp=stamp, items=dict(items), revision=revision)
    save_json(INDEX_CACHE, {"schema": INDEX_CACHE_SCHEMA, "stamp": stamp, "revision": revision, "items": items})
    with contextlib.suppress(Exception):
        Path(str(INDEX_CACHE) + _INDEX_DELTA_SUFFIX).unlink()


def _index_cache_save(stamp, items, revision=None):
    with _index_cache_lock(exclusive=True):
        _index_cache_load_unlocked(stamp, force_disk=True)
        if revision is None:
            revision = _next_index_revision(_INDEX_CACHE_MEM.get("revision"))
        _index_cache_save_unlocked(stamp, items, revision)


def _index_cache_append_delta(stamp, real, entry=None, deleted=False):
    _index_cache_append_batch(stamp, [(real, entry, deleted)])


def _index_cache_append_batch(stamp, updates):
    """Atomically publish a small set of catalog changes for live readers.

    Reconcile uses this journal while it is still parsing. Readers merge the
    journal over the last reliable base snapshot; removals are never emitted
    until the complete scan succeeds.
    """
    if not updates:
        return
    journal = Path(str(INDEX_CACHE) + _INDEX_DELTA_SUFFIX)
    journal.parent.mkdir(parents=True, exist_ok=True)
    with _index_cache_lock(exclusive=True):
        # Always merge from disk while holding the writer lock. The resident
        # Inspector and a reconcile job are distinct Python processes.
        items = dict(_index_cache_load_unlocked(stamp, force_disk=True))
        revision = _next_index_revision(_INDEX_CACHE_MEM.get("revision"))
        with journal.open("a", encoding="utf-8") as fh:
            for real, entry, deleted in updates:
                row = {"stamp": stamp, "revision": revision, "path": real, "deleted": bool(deleted)}
                if deleted:
                    items.pop(real, None)
                else:
                    row["entry"] = entry
                    items[real] = entry
                fh.write(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n")
            fh.flush()
            with contextlib.suppress(Exception):
                os.fsync(fh.fileno())
        _INDEX_CACHE_MEM.update(stamp=stamp, items=items, revision=revision)
        try:
            should_compact = journal.stat().st_size >= _INDEX_COMPACT_BYTES
            if not should_compact:
                with journal.open("r", encoding="utf-8") as fh:
                    should_compact = sum(1 for _ in fh) >= _INDEX_COMPACT_UPDATES
        except Exception:
            should_compact = False
        if should_compact:
            _index_cache_save_unlocked(stamp, items, revision)


def _index_cache_changes(since):
    """Return journal deltas, or a reset snapshot when compaction intervened."""
    stamp = _index_context_stamp()
    since = _revision_value(since)
    journal = Path(str(INDEX_CACHE) + _INDEX_DELTA_SUFFIX)
    with _index_cache_lock(exclusive=False):
        data = load_json(INDEX_CACHE, {}) or {}
        base_revision = _revision_value(data.get("revision")) if data.get("stamp") == stamp else 0
        items = dict(_index_cache_load_unlocked(stamp, force_disk=True))
        current_revision = _revision_value(_INDEX_CACHE_MEM.get("revision"))
        if since < base_revision:
            return {"reset": True, "revision": current_revision, "entries": list(items.values())}
        changes = {}
        if journal.exists():
            try:
                for line in journal.read_text(encoding="utf-8").splitlines():
                    row = json.loads(line)
                    if row.get("stamp") != stamp or _revision_value(row.get("revision")) <= since:
                        continue
                    path = clean(str(row.get("path") or ""))
                    if path:
                        changes[path] = row
            except Exception:
                # A partial journal is safe for reads, but a delta client must
                # receive a complete snapshot rather than miss an update.
                return {"reset": True, "revision": current_revision, "entries": list(items.values())}
        return {"reset": False, "revision": current_revision, "entries": list(changes.values())}


def _dynamic_overlay_context():
    """Read dynamic overlay stores once and reuse them for one catalog read."""
    paths = (CFG, AI_FAILURE_QUEUE, ISSUE_ACKS, STATUS_OVERRIDES)
    stamp_parts = []
    for path in paths:
        try:
            st = path.stat()
            stamp_parts.append((str(path), st.st_mtime_ns, st.st_size))
        except Exception:
            stamp_parts.append((str(path), 0, 0))
    stamp = tuple(stamp_parts)
    if _DYNAMIC_OVERLAY_MEM.get("stamp") == stamp:
        return _DYNAMIC_OVERLAY_MEM["value"]
    cfg = ai_config()
    failures_obj = _load_failure_queue()
    failures = {}
    for entry in failures_obj.get("items", []):
        if isinstance(entry, dict):
            # Canonicalize the small failure queue once, not every catalog row.
            failures[os.path.realpath(str(entry.get("path") or ""))] = entry
    acknowledgements = load_json(ISSUE_ACKS, {}) or {}
    if not isinstance(acknowledgements, dict):
        acknowledgements = {}
    overrides = load_json(STATUS_OVERRIDES, {}) or {}
    if not isinstance(overrides, dict):
        overrides = {}
    value = {
        "config": cfg,
        "prompt_hash": hashlib.sha256(_effective_ai_prompt(cfg).encode("utf-8")).hexdigest()[:16],
        "accepted_prompt_hashes": sorted(_accepted_ai_prompt_hashes(cfg)),
        "failures": failures,
        "acknowledgements": acknowledgements,
        "overrides": overrides,
    }
    _DYNAMIC_OVERLAY_MEM.update(stamp=stamp, value=value)
    return value


def refresh_path_summary(path, item=None):
    """legacy single writer for per-path summaries.

    Re-reads the persisted cache first (a job process may have written newer
    entries since this process last looked), then updates the entry in BOTH
    the index-cache file and the resident LibraryCatalog. Every producer -
    job writes, inspector edits, reload, status overrides - goes through
    this helper so catalog and file can never diverge or lose updates.
    """
    real = os.path.realpath(str(path))
    stamp = _index_context_stamp()
    items = dict(_index_cache_load(stamp))
    if item is None:
        summary = _summary_from_detail(_inspector_base(str(path)))
    else:
        summary = _summary_from_detail(item)
    entry = {"nfo_stamp": _file_stamp(real), "sidecar_stamp": _sidecar_stamp(path), "summary": summary}
    items[real] = entry
    # Keep small installs simple and immediately durable.  Large installs use
    # the delta journal so a per-title edit is O(1) instead of rewriting the
    # complete catalog for every NFO.
    if len(items) > _INDEX_INCREMENTAL_THRESHOLD:
        _index_cache_append_delta(stamp, real, entry=entry)
    else:
        _index_cache_save(stamp, items)
    _INDEX_CACHE_MEM["stamp"] = stamp
    _INDEX_CACHE_MEM["items"] = items
    if _LIBRARY_CATALOG.get("loaded") or _LIBRARY_CATALOG.get("items"):
        _LIBRARY_CATALOG["items"] = dict(items)
        with contextlib.suppress(Exception):
            _LIBRARY_CATALOG["cache_mtime"] = INDEX_CACHE.stat().st_mtime_ns
    return summary


def _index_cache_drop(path):
    real = os.path.realpath(str(path))
    stamp = _index_context_stamp()
    items = dict(_index_cache_load(stamp))
    items.pop(real, None)
    if len(items) > _INDEX_INCREMENTAL_THRESHOLD:
        _index_cache_append_delta(stamp, real, deleted=True)
    else:
        _index_cache_save(stamp, items)
    _INDEX_CACHE_MEM["stamp"] = stamp
    _INDEX_CACHE_MEM["items"] = items
    if isinstance(_LIBRARY_CATALOG.get("items"), dict) and _LIBRARY_CATALOG["items"]:
        _LIBRARY_CATALOG["items"].pop(real, None)
        with contextlib.suppress(Exception):
            _LIBRARY_CATALOG["cache_mtime"] = INDEX_CACHE.stat().st_mtime_ns


def _file_stamp(path):
    metrics_incr("stat_count")
    try:
        st = os.stat(path)
        return _file_stamp_from_stat(st)
    except Exception:
        return ""


def _file_stamp_from_stat(st):
    return "%s:%s:%s" % (st.st_mtime_ns, st.st_size, st.st_ino)


def _cached_library_items(paths):
    """Per-file summaries with a persistent mtime cache.

    Unchanged NFOs (and unchanged ownership sidecars) reuse the stored
    summary, so library refreshes and scope preflights no longer reparse the
    whole library.
    """
    stamp = _index_context_stamp()
    items = dict(_index_cache_load(stamp))
    sidecar_stat = {}
    summaries = []
    changed = False
    for path in paths:
        real = os.path.realpath(str(path))
        nfo_stamp = _file_stamp(real)
        if real not in sidecar_stat:
            sidecar_stat[real] = _sidecar_stamp(path)
        entry = items.get(real)
        if (
            isinstance(entry, dict) and nfo_stamp and entry.get("nfo_stamp") == nfo_stamp
            and entry.get("sidecar_stamp") == sidecar_stat[real]
            and isinstance(entry.get("summary"), dict)
        ):
            summaries.append(entry["summary"])
            continue
        try:
            detail = inspector_detail(str(path))
        except Exception as exc:
            detail = {"path": str(path), "xml_valid": False, "title": Path(str(path)).stem, "media_space": configured_space_for_path(path) or "movies", "media_type": "unknown", "issues": [{"kind": "read-error", "message": str(exc), "path": str(path)}], "counts": {"external": 0, "generated": 0, "manual": 0, "issues": 1}}
        summary = _summary_from_detail(detail)
        items[real] = {"nfo_stamp": nfo_stamp, "sidecar_stamp": sidecar_stat[real], "summary": summary}
        changed = True
        summaries.append(summary)
    if changed:
        _index_cache_save(stamp, items)
    return summaries


def _sidecar_stamp(path):
    # Ownership sidecars need their own freshness stamp, but they are not
    # media-file stats and must not obscure the NFO scan I/O metric.
    metrics_incr("sidecar_stat_count")
    try:
        return _file_stamp_from_stat(os.stat(str(_ownership_path(path))))
    except Exception:
        return ""


def _validate_selected_nfo_paths(values, strict=True):
    """Validate explicitly provided paths WITHOUT enumerating the library.

    Point/selected operations must be O(selected count): they validate each
    path against the configured roots via _allowed_nfo_path and never touch
    nfos()/rglob/library_index.
    """
    out = []
    seen = set()
    for raw in values or []:
        if not raw:
            continue
        real = os.path.realpath(str(raw))
        if not real or real in seen:
            continue
        seen.add(real)
        try:
            out.append(_allowed_nfo_path(real))
        except Exception as exc:
            if strict:
                raise
            print("⚠️ 跳过无效或不在资料库配置中的 NFO：%s（%s）" % (raw, exc), flush=True)
    return out


def _cached_summary_for_path(path):
    """Per-path summary fast path: stat this NFO only, reuse cache if unchanged."""
    real = os.path.realpath(str(path))
    stamp = _index_context_stamp()
    items = dict(_index_cache_load(stamp))
    entry = items.get(real)
    nfo_stamp = _file_stamp(real)
    sidecar_stamp = _sidecar_stamp(path)
    if (
        isinstance(entry, dict) and nfo_stamp
        and entry.get("nfo_stamp") == nfo_stamp
        and entry.get("sidecar_stamp") == sidecar_stamp
        and isinstance(entry.get("summary"), dict)
    ):
        metrics_incr("index_cache_hit")
        return entry["summary"]
    metrics_incr("index_cache_miss")
    summary = _summary_from_detail(inspector_detail(str(path)))
    items[real] = {"nfo_stamp": nfo_stamp, "sidecar_stamp": sidecar_stamp, "summary": summary}
    _index_cache_save(stamp, items)
    return summary


_LIBRARY_CATALOG = {"loaded": False, "cache_mtime": None, "cache_stamp": None, "items": {}}


def _catalog_storage_stamp():
    """Cheap local stamp; never follows or stats a configured media path."""
    out = []
    for path in (INDEX_CACHE, Path(str(INDEX_CACHE) + _INDEX_DELTA_SUFFIX)):
        try:
            st = path.stat()
            out.append((st.st_mtime_ns, st.st_size))
        except Exception:
            out.append((0, 0))
    return tuple(out)


def _catalog_persist():
    stamp = _index_context_stamp()
    final_items = dict(_LIBRARY_CATALOG["items"])
    with _index_cache_lock(exclusive=True):
        disk_items = dict(_index_cache_load_unlocked(stamp, force_disk=True))
        # A user may edit an already-visible NFO while reconcile continues.
        # Every scan result has been journaled before this final commit, so a
        # differing current disk stamp represents a later point update and
        # must win over the scan snapshot.
        for real, candidate in list(final_items.items()):
            current = disk_items.get(real)
            if not isinstance(candidate, dict) or not isinstance(current, dict):
                continue
            candidate_stamp = (candidate.get("nfo_stamp"), candidate.get("sidecar_stamp"))
            current_stamp = (current.get("nfo_stamp"), current.get("sidecar_stamp"))
            if current_stamp != candidate_stamp:
                final_items[real] = current
        # The authoritative commit may remove files that were present in the
        # old base.  Always advance the base revision so delta clients receive
        # a reset rather than retaining deleted rows indefinitely.
        revision = _next_index_revision(_INDEX_CACHE_MEM.get("revision"))
        _index_cache_save_unlocked(stamp, final_items, revision)
    _LIBRARY_CATALOG["items"] = final_items
    _LIBRARY_CATALOG["cache_stamp"] = _catalog_storage_stamp()
    _LIBRARY_CATALOG["cache_mtime"] = _LIBRARY_CATALOG["cache_stamp"][0][0]


def _catalog_load():
    # Force a disk merge when the base file or its live update journal moves.
    # Retry if a reconcile batch lands while this snapshot is being read, so
    # the resident cannot remember a new stamp with an older item set.
    items = {}
    storage_stamp = None
    for _ in range(3):
        before = _catalog_storage_stamp()
        _INDEX_CACHE_MEM.update(stamp="", items={})
        items = dict(_index_cache_load(_index_context_stamp()))
        storage_stamp = _catalog_storage_stamp()
        if before == storage_stamp:
            break
    if not items:
        _LIBRARY_CATALOG.update(loaded=True, items={}, cache_stamp=storage_stamp)
        _LIBRARY_CATALOG["cache_mtime"] = _LIBRARY_CATALOG["cache_stamp"][0][0]
        return
    _LIBRARY_CATALOG.update(loaded=True, items=items, cache_stamp=storage_stamp)
    _LIBRARY_CATALOG["cache_mtime"] = _LIBRARY_CATALOG["cache_stamp"][0][0]


def _catalog_ensure(allow_cold_scan=True):
    """Load the resident catalog from the persisted index cache; cold-scan once.

    Serving the catalog never stats NAS files: freshness comes from explicit
    reconcile runs and per-path fast paths (_cached_summary_for_path).
    """
    if not _LIBRARY_CATALOG["loaded"]:
        _catalog_load()
    elif _LIBRARY_CATALOG.get("cache_stamp") is not None and _catalog_storage_stamp() != _LIBRARY_CATALOG.get("cache_stamp"):
        _catalog_load()
    if not _LIBRARY_CATALOG["items"] and allow_cold_scan:
        try:
            _configured_nfo_paths()
        except Exception:
            return
        _catalog_reconcile(reason="cold-start")


def _catalog_reconcile(reason="manual", space=""):
    """The only place allowed to rglob the library: O(library) by design."""
    metrics_reset()
    metrics_incr("reconcile_count")
    # A manual reconcile runs in a separate job process. Load the reliable
    # snapshot first so unchanged files are reused instead of reparsing all
    # NFOs on every refresh.
    if not _LIBRARY_CATALOG.get("loaded"):
        _catalog_load()
    if space not in ("", "movies", "tv"):
        raise ValueError("资料库范围只能是 movies 或 tv")
    snapshots = _configured_nfo_snapshots(space=space)
    old_items = dict(_LIBRARY_CATALOG.get("items") or {})
    new_items = {
        real: entry for real, entry in old_items.items()
        if space and isinstance(entry, dict) and isinstance(entry.get("summary"), dict)
        and entry["summary"].get("media_space") != space
    }
    total = len(snapshots)
    done = 0
    reused = 0
    publish_batch = []
    for path, real, nfo_stamp, nfo_stat in snapshots:
        done += 1
        sidecar_stamp = _sidecar_stamp(path)
        entry = old_items.get(real)
        if (
            isinstance(entry, dict) and nfo_stamp
            and entry.get("nfo_stamp") == nfo_stamp
            and entry.get("sidecar_stamp") == sidecar_stamp
            and isinstance(entry.get("summary"), dict)
        ):
            new_items[real] = entry
            reused += 1
            metrics_incr("index_cache_hit")
        else:
            metrics_incr("index_cache_miss")
            try:
                # Reuse the enumeration stat for this first parse.  On a NAS
                # this removes two extra metadata round trips per changed NFO.
                base = _inspector_base(str(path), nfo_stat=nfo_stat)
            except Exception as exc:
                base = {"path": str(path), "xml_valid": False, "title": Path(str(path)).stem, "media_space": configured_space_for_path(path) or "movies", "media_type": "unknown", "issues": [{"kind": "read-error", "message": str(exc), "path": str(path)}], "counts": {"external": 0, "generated": 0, "manual": 0, "issues": 1}}
            new_items[real] = {"nfo_stamp": nfo_stamp, "sidecar_stamp": sidecar_stamp, "summary": _summary_from_detail(base)}
            publish_batch.append((real, new_items[real], False))
        if done % 50 == 0 or done == total:
            if publish_batch:
                _index_cache_append_batch(_index_context_stamp(), publish_batch)
                publish_batch = []
            _progress_write("reconcile-index", done, total, str(path), [])
    _LIBRARY_CATALOG["items"] = new_items
    _catalog_persist()
    _catalog_save_root_health(new_items)
    counts = {"reason": reason, "space": space or "all", "total": total, "reused": reused, "reparsed": total - reused}
    metrics_incr("scan_file_count", total)
    metrics_emit_summary("reconcile-index", **counts)
    print("📦 资料库对账完成：共 %d 个 NFO，复用 %d，重新解析 %d。" % (total, reused, counts["reparsed"]), flush=True)
    return counts


def _catalog_save_root_health(items):
    """Persist root counts at reconcile time, never during a UI read."""
    generated_at = _utc_now()
    grouped = configured_library_roots()
    configured = (load_json(CFG, {}) or {}).get("library_roots", {})
    health = {}
    summaries = [entry.get("summary") for entry in items.values() if isinstance(entry, dict)]
    for space in ("movies", "tv"):
        for root in grouped[space]:
            matching = []
            for item in summaries:
                if not isinstance(item, dict):
                    continue
                path = str(item.get("path") or "")
                try:
                    if os.path.commonpath([path, root]) == root:
                        matching.append(item)
                except ValueError:
                    continue
            display = next((value for value in configured.get(space, []) if os.path.realpath(os.path.expanduser(value)) == root), root)
            health[display] = {"last_scan": generated_at, "nfo_count": len(matching), "mismatch_count": sum(1 for item in matching if item.get("library_type_mismatch"))}
    save_json(ROOT_HEALTH, health)


def _catalog_query(space=None, series_key=None, season=None):
    _catalog_ensure()
    dynamic = _dynamic_overlay_context()
    out = []
    for real, entry in (_LIBRARY_CATALOG.get("items") or {}).items():
        summary = entry.get("summary") if isinstance(entry, dict) else None
        if not isinstance(summary, dict):
            continue
        if space and summary.get("media_space") != space:
            continue
        if series_key is not None and summary.get("series_key") != series_key:
            continue
        if season is not None and summary.get("season") != season:
            continue
        out.append(_apply_summary_overlay(summary, dynamic))
    return out


def _catalog_reload_paths(paths):
    """Right-click 重新读取: unconditionally re-read the given NFOs."""
    out = []
    for raw in paths or []:
        path = _allowed_nfo_path(raw)
        base = _inspector_base(str(path))
        summary = refresh_path_summary(path, item=base)
        out.append(_apply_summary_overlay(summary))
    return out


def library_index(allow_cold_scan=True):
    metrics_incr("library_index_calls")
    _t0 = time.time()
    _catalog_ensure(allow_cold_scan=allow_cold_scan)
    dynamic = _dynamic_overlay_context()
    items = [_apply_summary_overlay(entry.get("summary"), dynamic) for entry in (_LIBRARY_CATALOG.get("items") or {}).values() if isinstance(entry, dict) and isinstance(entry.get("summary"), dict)]
    generated_at = _utc_now()
    metrics_incr("library_index_ms", int((time.time() - _t0) * 1000))
    return {
        "schema": 1,
        "generated_at": generated_at,
        "revision": _revision_value(_INDEX_CACHE_MEM.get("revision")),
        "items": items,
    }


def library_changes(since=0):
    """Serve a compact catalog delta for the Web UI scan loop."""
    _catalog_ensure(allow_cold_scan=False)
    raw = _index_cache_changes(since)
    dynamic = _dynamic_overlay_context()
    if raw.get("reset"):
        items = [
            _apply_summary_overlay(entry.get("summary"), dynamic)
            for entry in raw.get("entries", [])
            if isinstance(entry, dict) and isinstance(entry.get("summary"), dict)
        ]
        return {"schema": 1, "reset": True, "revision": raw.get("revision", 0), "items": items, "changes": []}
    changes = []
    for row in raw.get("entries", []):
        if not isinstance(row, dict):
            continue
        path = clean(str(row.get("path") or ""))
        if not path:
            continue
        if row.get("deleted"):
            changes.append({"path": path, "deleted": True})
            continue
        entry = row.get("entry")
        summary = entry.get("summary") if isinstance(entry, dict) else None
        if isinstance(summary, dict):
            changes.append({"path": path, "deleted": False, "item": _apply_summary_overlay(summary, dynamic)})
    return {"schema": 1, "reset": False, "revision": raw.get("revision", 0), "items": [], "changes": changes}


def scope_preflight(payload):
    metrics_incr("scope_preflight_calls")
    payload = payload or {}
    kind = clean(str(payload.get("kind") or "current"))
    engine = clean(str(payload.get("engine") or "local-rules"))
    if engine not in ("ai", "local-rules"):
        raise ValueError("生成方式必须是 ai 或 local-rules")
    entries = []
    label = ""
    if kind in ("current", "selection", "filter-results"):
        # Point/selected scopes never enumerate the library: validate the
        # explicit paths and read one summary per path (cached by stamp).
        raw_paths = [payload.get("path")] if kind == "current" else (
            payload.get("paths") if isinstance(payload.get("paths"), list) else []
        )
        label = "当前 NFO" if kind == "current" else ("当前选择" if kind == "selection" else "当前搜索/筛选结果")
        validated = _validate_selected_nfo_paths(raw_paths)
        for path in validated:
            real = os.path.realpath(str(path))
            entries.append((real, _cached_summary_for_path(path)))
    else:
        index = library_index()["items"]
        by_path = {os.path.realpath(str(item.get("path") or "")): item for item in index}
        requested = []
        if kind == "all-movies":
            requested = [item.get("path") for item in index if item.get("media_space") == "movies"]
            label = "全部电影"
        elif kind == "all-tv":
            requested = [item.get("path") for item in index if item.get("media_space") == "tv"]
            label = "全部电视剧"
        elif kind in ("current-series", "current-season"):
            current_path = os.path.realpath(str(payload.get("path") or ""))
            current = by_path.get(current_path, {})
            series_key = clean(str(payload.get("series_key") or current.get("series_key") or current.get("title") or ""))
            season_value = payload.get("season", current.get("season"))
            for item in index:
                item_series = clean(str(item.get("series_key") or (item.get("title") if item.get("media_type") == "tvshow" else "")))
                if item.get("media_space") != "tv" or item_series != series_key:
                    continue
                if kind == "current-season" and item.get("season") != season_value:
                    continue
                requested.append(item.get("path"))
            label = series_key or ("当前季" if kind == "current-season" else "当前剧")
            if kind == "current-season":
                label += " · 第 %s 季" % season_value
        else:
            raise ValueError("Scope 类型无效")
        for value in requested:
            if not value:
                continue
            real = os.path.realpath(str(value))
            item = by_path.get(real)
            if not item:
                raise PathOutsideLibraryError("Scope 包含资料库外或已经不存在的 NFO")
            entries.append((real, item))

    resolved = []
    for real, item in entries:
        if item.get("media_type") not in ("movie", "tvshow", "episode"):
            continue
        if item.get("library_type_mismatch"):
            continue
        if real not in resolved:
            resolved.append(real)
    if not resolved:
        raise ValueError("当前 Scope 没有可处理的 NFO")
    by_real = {real: item for real, item in entries}

    counts = {
        "total": len(resolved), "current": 0, "needs_generation": 0,
        "not_applicable": 0, "known_failure": 0, "review": 0, "unsafe": 0,
        "movies": 0, "shows": 0, "episodes": 0,
    }
    for path in resolved:
        item = by_real[path]
        status = item.get("tag_status")
        if item.get("spec_status") in ("missing", "empty", "not-applicable"):
            counts["not_applicable"] += 1
        elif status in ("ai-current", "local-current") and ((engine == "ai" and status == "ai-current") or (engine == "local-rules" and status == "local-current")):
            counts["current"] += 1
        else:
            counts["needs_generation"] += 1
        if status == "review":
            counts["review"] += 1
        if status in ("unsafe", "legacy"):
            counts["unsafe"] += 1
        if _failure_for_path(path):
            counts["known_failure"] += 1
    return {
        "schema": 1, "kind": kind, "engine": engine, "label": label,
        "media_space": clean(str(payload.get("media_space") or "")),
        "created_at": _utc_now(), "resolved_paths": resolved, "counts": counts,
    }


def _normalise_newlines(text, nl):
    return text.replace("\r\n", "\n").replace("\r", "\n").replace("\n", nl)


def _edit_root_tag(text, root_index, value):
    doc = minidom.parseString(text.encode("utf-8"))
    root = doc.documentElement
    tags = [node for node in root.childNodes if node.nodeType == node.ELEMENT_NODE and node.tagName.rsplit(":", 1)[-1].casefold() == "tag"]
    if root_index < 0 or root_index >= len(tags):
        raise ValueError("标签位置已经变化，请刷新后重试")
    node = tags[root_index]
    while node.firstChild:
        node.removeChild(node.firstChild)
    node.appendChild(doc.createTextNode(value))
    return doc.toxml()


def _delete_root_tag(text, root_index):
    doc = minidom.parseString(text.encode("utf-8"))
    root = doc.documentElement
    tags = [node for node in root.childNodes if node.nodeType == node.ELEMENT_NODE and node.tagName.rsplit(":", 1)[-1].casefold() == "tag"]
    if root_index < 0 or root_index >= len(tags):
        raise ValueError("标签位置已经变化，请刷新后重试")
    node = tags[root_index]
    if node.parentNode is not None:
        node.parentNode.removeChild(node)
    return doc.toxml()


def _replace_technical_block(text, obj, info, tag_result):
    nl = "\r\n" if "\r\n" in text else "\n"
    cleaned = strip_technical_block_only(text)
    block = technical_block(obj, nl, info["media_type"], info.get("display_title") or info.get("title") or "", tag_result)
    lock = re.search(r'^[ \t]*<tmm_locked\s*/>[ \t]*(?:\r?\n|$)', cleaned, flags=re.M | re.I)
    if lock:
        result = cleaned[:lock.start()] + block + cleaned[lock.start():]
    else:
        close_re = rf"</{re.escape(info['root_tag'])}\s*>"
        if not re.search(close_re, cleaned, re.I):
            raise ValueError("NFO 根节点结束标签缺失")
        result = re.sub(close_re, block + f"</{info['root_tag']}>", cleaned, count=1, flags=re.I)
    result = re.sub(r"(\r?\n){3,}", nl * 2, result)
    ET.fromstring(result.encode("utf-8"))
    return result


def _undo_path(nfo):
    key = hashlib.sha256(os.path.realpath(str(nfo)).encode("utf-8", "surrogatepass")).hexdigest()
    return UNDO_DIR / f"{key}.json"


def _atomic_inspector_write(nfo, raw, new_text, expected_hash, operation, record_undo=True):
    if _source_hash(raw) != clean(expected_hash):
        raise EditConflictError("NFO 已被其他程序修改，请刷新 Inspector 后重试")
    bom = raw.startswith(b"\xef\xbb\xbf")
    old_text = raw.decode("utf-8-sig")
    nl = "\r\n" if "\r\n" in old_text else "\n"
    new_text = _normalise_newlines(new_text, nl)
    ET.fromstring(new_text.encode("utf-8"))
    data = new_text.encode("utf-8")
    if bom:
        data = b"\xef\xbb\xbf" + data
    if data == raw:
        return _source_hash(raw)
    current = nfo.read_bytes()
    if _source_hash(current) != clean(expected_hash):
        raise EditConflictError("NFO 已被其他程序修改，请刷新 Inspector 后重试")
    tmp = Path(str(nfo) + ".imdbtech.tmp")
    with open(tmp, "wb") as handle:
        handle.write(data)
        handle.flush()
        os.fsync(handle.fileno())
    with contextlib.suppress(Exception):
        shutil.copymode(nfo, tmp)
    backup = Path(str(nfo) + ".imdbtech.bak")
    backup_tmp = Path(str(backup) + ".tmp")
    with open(backup_tmp, "wb") as handle:
        handle.write(raw)
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(backup_tmp, backup)
    if _source_hash(nfo.read_bytes()) != clean(expected_hash):
        with contextlib.suppress(Exception):
            tmp.unlink()
        raise EditConflictError("写入前 NFO 再次发生变化，已安全取消")
    os.replace(tmp, nfo)
    after_hash = _source_hash(data)
    if record_undo:
        save_json(_undo_path(nfo), {
            "schema": 1, "path": os.path.realpath(str(nfo)), "operation": operation,
            "created_at": _utc_now(), "expires_at": (dt.datetime.now(dt.timezone.utc) + dt.timedelta(minutes=30)).replace(microsecond=0).isoformat(),
            "before": base64.b64encode(raw).decode("ascii"), "before_hash": _source_hash(raw), "after_hash": after_hash,
        })
    return after_hash


def edit_nfo(payload):
    payload = payload or {}
    nfo = _allowed_nfo_path(payload.get("path"))
    expected = clean(str(payload.get("expected_source_hash") or ""))
    if not expected:
        raise ValueError("缺少 expected_source_hash")
    operation = clean(str(payload.get("operation") or ""))
    raw = nfo.read_bytes()
    if _source_hash(raw) != expected:
        raise EditConflictError("NFO 已被其他程序修改，请刷新 Inspector 后重试")
    text = raw.decode("utf-8-sig")
    info = inspect_nfo(text)
    if not info:
        raise ValueError("该 NFO 不支持编辑")
    obj = existing_tech_object(text)
    target = payload.get("target") if isinstance(payload.get("target"), dict) else {}
    value = clean(str(payload.get("value") or ""))
    manifest_changed = False

    if operation == "edit-tag":
        if not value:
            raise ValueError("标签不能为空；如需删除请使用删除按钮")
        root_index = int(target.get("root_index", -1))
        detail = inspector_detail(str(nfo))
        if root_index < 0 or root_index >= len(detail["tags"]):
            raise ValueError("标签位置已经变化，请刷新后重试")
        row = detail["tags"][root_index]
        text = _edit_root_tag(text, root_index, value)
        if row["ownership"] in ("generated", "manual"):
            if not obj:
                raise ValueError("ownership manifest 缺失，拒绝接管标签")
            now = _utc_now()
            generated = list(obj.get("owned_entries") or [])
            manual = list(obj.get("manual_entries") or [])
            if row["ownership"] == "generated":
                generated = [x for x in generated if clean(x.get("id", "")) != row.get("id") and clean(x.get("value", "")).casefold() != clean(row["value"]).casefold()]
                manual.append({
                    "id": _tag_entry_id("manual", {"value": value, "replaces": row.get("id", ""), "origin": "manual-edit"}),
                    "value": value, "origin": "manual-edit", "replaces": row.get("id", ""),
                    "field": row.get("field", ""), "created": now, "modified": now,
                })
            else:
                changed = False
                for entry in manual:
                    if clean(entry.get("id", "")) == row.get("id") or (not changed and clean(entry.get("value", "")).casefold() == clean(row["value"]).casefold()):
                        entry["value"] = value
                        entry["modified"] = now
                        changed = True
            obj["manual_entries"] = manual
            tag_result = _manifest_result_from_obj(obj) or {"entries": generated, "engine": "preserved", "write_manifest": True}
            tag_result["entries"] = generated
            tag_result["manual_entries"] = manual
            text = _replace_technical_block(text, obj, info, tag_result)
            manifest_changed = True
    elif operation == "delete-tag":
        root_index = int(target.get("root_index", -1))
        detail = inspector_detail(str(nfo))
        if root_index < 0 or root_index >= len(detail["tags"]):
            raise ValueError("标签位置已经变化，请刷新后重试")
        row = detail["tags"][root_index]
        if row["ownership"] == "external" and not payload.get("confirm_external"):
            # External tags belong to other apps (e.g. TMM). Deleting them is
            # allowed only as an explicit, confirmed user action.
            raise ValueError("外部标签确认：该标签不属于本软件，删除前需要确认")
        text = _delete_root_tag(text, root_index)
        if row["ownership"] in ("generated", "manual"):
            if not obj:
                raise ValueError("ownership manifest 缺失，拒绝调整标签")
            generated = [x for x in (obj.get("owned_entries") or []) if clean(x.get("id", "")) != row.get("id") and _canon_tag_value(x.get("value", "")) != _canon_tag_value(row["value"])]
            manual = [x for x in (obj.get("manual_entries") or []) if clean(x.get("id", "")) != row.get("id") and _canon_tag_value(x.get("value", "")) != _canon_tag_value(row["value"])]
            obj["owned_entries"] = generated
            obj["manual_entries"] = manual
            tag_result = _manifest_result_from_obj(obj) or {"entries": [], "engine": "preserved", "write_manifest": True}
            tag_result["entries"] = generated
            tag_result["manual_entries"] = manual
            text = _replace_technical_block(text, obj, info, tag_result)
            manifest_changed = True
    elif operation == "set-tag-ownership":
        choice = clean(str(value or ""))
        if choice not in ("external", "ai", "local-rules", "manual"):
            raise ValueError("所有权必须是 external / ai / local-rules / manual")
        root_index = int(target.get("root_index", -1))
        detail = inspector_detail(str(nfo))
        if root_index < 0 or root_index >= len(detail["tags"]):
            raise ValueError("标签位置已经变化，请刷新后重试")
        row = detail["tags"][root_index]
        current = row["ownership"]
        desired = "external" if choice == "external" else ("manual" if choice == "manual" else "generated")
        if current == desired and (desired != "generated" or clean(str(row.get("engine") or "")) == choice):
            return {"ok": True, "operation": operation, "item": detail, "unchanged": True}
        if choice != "external" and not obj:
            raise ValueError("当前 NFO 尚无 Technical Specs 块，无法记录所有权；请先获取 Tech Spec")
        now = _utc_now()
        generated = [x for x in (obj.get("owned_entries") or []) if clean(x.get("id", "")) != row.get("id") and _canon_tag_value(x.get("value", "")) != _canon_tag_value(row["value"])]
        manual = [x for x in (obj.get("manual_entries") or []) if clean(x.get("id", "")) != row.get("id") and _canon_tag_value(x.get("value", "")) != _canon_tag_value(row["value"])]
        if choice in ("ai", "local-rules"):
            entry = {
                "id": _tag_entry_id("generated", {"value": row["value"], "origin": "manual-takeover"}),
                "value": row["value"], "origin": "manual-takeover", "engine": choice,
                "field": row.get("field", ""), "confidence": "high", "operation": "takeover",
            }
            generated.append(entry)
        elif choice == "manual":
            manual.append({
                "id": _tag_entry_id("manual", {"value": row["value"], "origin": "manual-takeover"}),
                "value": row["value"], "origin": "manual-takeover",
                "field": row.get("field", ""), "created": now, "modified": now,
            })
        obj["owned_entries"] = generated
        obj["manual_entries"] = manual
        tag_result = _manifest_result_from_obj(obj) or {"entries": [], "engine": "preserved", "write_manifest": True}
        tag_result["entries"] = generated
        tag_result["manual_entries"] = manual
        text = _replace_technical_block(text, obj, info, tag_result)
        manifest_changed = True
    elif operation == "clear-ai-tags":
        if not obj:
            raise ValueError("当前 NFO 没有 ownership 信息")
        detail = inspector_detail(str(nfo))
        block_engine = clean(str(detail.get("tag_engine") or ""))
        targets = [r for r in detail["tags"] if r["ownership"] == "generated" and (clean(str(r.get("engine") or "")) or block_engine) == "ai"]
        if not targets:
            raise ValueError("当前 NFO 没有 AI 生成的标签")
        if not payload.get("confirm"):
            raise ValueError("清除 AI 标签需要确认")
        # Remove root tags from the highest index down so earlier indexes stay valid.
        for row in sorted(targets, key=lambda r: r["root_index"], reverse=True):
            text = _delete_root_tag(text, row["root_index"])
        ai_keys = {_canon_tag_value(r["value"]) for r in targets}
        generated = [x for x in (obj.get("owned_entries") or []) if _canon_tag_value(x.get("value", "")) not in ai_keys]
        obj["owned_entries"] = generated
        tag_result = _manifest_result_from_obj(obj) or {"entries": [], "engine": "preserved", "write_manifest": True}
        tag_result["entries"] = generated
        tag_result["manual_entries"] = list(obj.get("manual_entries") or [])
        text = _replace_technical_block(text, obj, info, tag_result)
        manifest_changed = True
    elif operation == "add-manual-tag":
        if not value:
            raise ValueError("Manual Tech Tag 不能为空")
        if not obj:
            raise ValueError("请先准备 Technical Specs，再添加 Manual Tech Tag")
        if value.casefold() in {clean(x).casefold() for x in _all_normal_tags(text)}:
            raise ValueError("根节点已经存在同名标签；不会反向取得 ownership")
        nl = "\r\n" if "\r\n" in text else "\n"
        text = strip_technical_block_only(text)
        text = insert_standard_tags(text, [value], nl)
        now = _utc_now()
        entry = {"value": value, "origin": "manual-add", "created": now, "modified": now}
        entry["id"] = _tag_entry_id("manual", entry)
        obj["manual_entries"] = list(obj.get("manual_entries") or []) + [entry]
        tag_result = _manifest_result_from_obj(obj) or {"entries": [], "engine": "preserved", "write_manifest": True}
        tag_result["manual_entries"] = obj["manual_entries"]
        text = _replace_technical_block(text, obj, info, tag_result)
        manifest_changed = True
    elif operation in ("edit-spec", "restore-spec"):
        if not obj:
            raise ValueError("当前 NFO 没有可编辑的 Technical Specs")
        old_hash = _specs_hash(obj.get("specs", {}))
        if operation == "restore-spec":
            obj["specs"] = {key: list(values) for key, values in obj.get("source_specs", {}).items()}
            obj["modified"] = False
            obj["modified_at"] = ""
        else:
            section = clean(str(target.get("section") or ""))
            if section not in SECTIONS:
                raise ValueError("Technical Specs 字段无效")
            values = list(obj.get("specs", {}).get(section, []))
            raw_index = target.get("index")
            if raw_index is None:
                if not value:
                    raise ValueError("新增规格值不能为空")
                values.append(value)
            else:
                index = int(raw_index)
                if index < 0 or index >= len(values):
                    raise ValueError("规格位置已经变化，请刷新后重试")
                if value:
                    values[index] = value
                else:
                    values.pop(index)
            if not obj.get("modified"):
                obj["source_specs"] = {key: list(items) for key, items in obj.get("specs", {}).items()}
                obj["source_spec_hash"] = old_hash
                obj["source_fetched_at"] = obj.get("fetched_at", "")
            obj["specs"] = dict(obj.get("specs", {}))
            obj["specs"][section] = _dedupe_clean(values)
            obj["modified"] = True
            obj["modified_at"] = _utc_now()
        tag_result = _manifest_result_from_obj(obj)
        if tag_result and tag_result.get("spec_hash") != _specs_hash(obj.get("specs", {})):
            tag_result["state"] = "stale"
        text = _replace_technical_block(text, obj, info, tag_result)
        manifest_changed = bool(tag_result)
    else:
        raise ValueError("不支持的 Inspector 编辑操作")

    _atomic_inspector_write(nfo, raw, text, expected, operation)
    if manifest_changed:
        final = existing_tech_object(nfo.read_bytes().decode("utf-8-sig"))
        result = _manifest_result_from_obj(final)
        if result:
            result["manual_entries"] = list(final.get("manual_entries") or [])
            save_ownership_record(nfo, final.get("imdb", ""), result)
    return {"ok": True, "operation": operation, "item": inspector_detail(str(nfo))}


def undo_nfo(payload):
    payload = payload or {}
    nfo = _allowed_nfo_path(payload.get("path"))
    expected = clean(str(payload.get("expected_source_hash") or ""))
    journal_path = _undo_path(nfo)
    journal = load_json(journal_path, {}) or {}
    if not journal or os.path.realpath(str(journal.get("path") or "")) != os.path.realpath(str(nfo)):
        raise ValueError("没有可撤销的 Inspector 操作")
    raw = nfo.read_bytes()
    current_hash = _source_hash(raw)
    if current_hash != expected or current_hash != journal.get("after_hash"):
        raise EditConflictError("NFO 已在编辑后发生变化，为避免覆盖外部修改，不能撤销")
    try:
        expires = dt.datetime.fromisoformat(journal.get("expires_at", ""))
        if dt.datetime.now(dt.timezone.utc) > expires:
            raise ValueError("撤销记录已过期")
        before = base64.b64decode(journal["before"].encode("ascii"), validate=True)
        before_text = before.decode("utf-8-sig")
        ET.fromstring(before_text.encode("utf-8"))
    except ValueError:
        raise
    except Exception as exc:
        raise ValueError("撤销记录无效：" + str(exc))
    bom = before.startswith(b"\xef\xbb\xbf")
    restored_text = before.decode("utf-8-sig")
    if bom and not raw.startswith(b"\xef\xbb\xbf"):
        restored_text = restored_text
    _atomic_inspector_write(nfo, raw, restored_text, expected, "undo", record_undo=False)
    with contextlib.suppress(Exception):
        journal_path.unlink()
    final = existing_tech_object(nfo.read_bytes().decode("utf-8-sig"))
    result = _manifest_result_from_obj(final)
    if final and result:
        result["manual_entries"] = list(final.get("manual_entries") or [])
        save_ownership_record(nfo, final.get("imdb", ""), result)
    return {"ok": True, "operation": "undo", "item": inspector_detail(str(nfo))}

def nfos(roots):
    for r in roots:
        p = Path(r)
        if p.is_dir():
            with contextlib.suppress(Exception):
                for path in p.rglob("*.nfo"):
                    metrics_incr("paths_enumerated")
                    yield path


def _configured_nfo_snapshots(space=""):
    """Enumerate once and retain the NFO stat used by reconcile.

    The former reconcile path built a Path list and immediately called stat
    again for every NFO. On an SMB/NAS library that second round trip is a
    material part of the cold-scan delay.
    """
    cfg = load_json(CFG, {}) or {}
    if space in ("movies", "tv"):
        roots = [r for r in configured_library_roots(cfg).get(space, []) if os.path.isdir(r)]
    else:
        roots = [r for r in configured_roots_flat(cfg) if os.path.isdir(r)]
    if not roots:
        raise RuntimeError("当前没有已挂载且可访问的资料库")
    snapshots = []
    for path in nfos(list(dict.fromkeys(roots))):
        metrics_incr("stat_count")
        try:
            st = path.stat()
            real = os.path.realpath(str(path))
            snapshots.append((path, real, _file_stamp_from_stat(st), st))
        except Exception:
            # Preserve the existing reconcile behavior: a transiently missing
            # file is represented by an empty stamp and becomes a read error
            # only if it remains present long enough to inspect.
            snapshots.append((path, os.path.realpath(str(path)), "", None))
    return snapshots

_MANUAL_TASK_OWNER = False


def _manual_task_requested():
    return (not _MANUAL_TASK_OWNER) and MANUAL_TASK_FLAG.exists()


@contextlib.contextmanager
def _manual_task_session():
    """Mark a user-initiated task so background cycles yield the write lock."""
    global _MANUAL_TASK_OWNER
    try:
        APP.mkdir(parents=True, exist_ok=True)
        MANUAL_TASK_FLAG.write_text(_utc_now(), encoding="utf-8")
    except Exception:
        pass
    _MANUAL_TASK_OWNER = True
    try:
        yield
    finally:
        _MANUAL_TASK_OWNER = False
        with contextlib.suppress(Exception):
            MANUAL_TASK_FLAG.unlink()


def _flock_exclusive(handle, wait_message="等待其他任务释放 NFO 写入锁"):
    """Acquire the exclusive lock, reporting progress instead of hanging silently.

    Background cycles check _manual_task_requested() between items, so a
    manual task normally only waits for the NFO currently being processed.
    """
    import fcntl
    waited = 0
    while True:
        try:
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            if waited:
                print("✅ 已取得写入锁（等待了 %d 秒）。" % waited, flush=True)
            return True
        except (BlockingIOError, OSError):
            if waited == 0:
                print("⏳ %s……（后台任务会在当前影片完成后让位）" % wait_message, flush=True)
            time.sleep(1)
            waited += 1
            if waited % 15 == 0:
                print("   已等待 %d 秒；后台任务仍在运行。" % waited, flush=True)


def _progress_write(action, done, total, current_path, results):
    with contextlib.suppress(Exception):
        save_json(JOB_PROGRESS, {
            "schema": 1, "action": action, "done": done, "total": total,
            "current": current_path, "updated_at": _utc_now(),
            "results": list(results)[-500:],
        })


def _progress_clear():
    with contextlib.suppress(Exception):
        JOB_PROGRESS.unlink()


def process(path, force=False, only_missing=False, verbose=False):
    """Step 1 only: make IMDb source data ready. Never generate or delete tags."""
    try:
        text = path.read_bytes().decode("utf-8-sig")
    except Exception:
        return "skip"
    try:
        info = inspect_nfo(text)
    except ET.ParseError:
        return "retry"
    if not info:
        return "unsupported-nfo"

    label_map = {"movie": "Movie", "tvshow": "TV Show", "episode": "Episode"}
    display = info.get("display_title") or info.get("title") or path.stem
    media_label = label_map[info["media_type"]]
    iid = imdb_id(text)
    if not iid:
        if verbose:
            print(f"⏭️  [{media_label}] {display}：NFO 没有 IMDb ID，跳过。", flush=True)
        return "no-imdb"
    iid = iid.lower()

    existing = existing_tech_object(text)
    if not force and existing and existing.get("imdb") == iid and existing.get("ready"):
        if verbose and only_missing:
            print(f"✓ Spec 已准备：{display}", flush=True)
        return "spec-ready"

    if verbose:
        print(f"⏳ [{media_label}] {display}  [{iid}]", flush=True)

    obj = get_specs(iid, force=force, retry_failed=only_missing)
    if not obj.get("ok"):
        status = obj.get("status", "fetch-error")
        if status == "no-tech":
            # Confirmed empty is still a completed Step-1 state. Persist a small
            # marker so the Agent does not repeatedly treat it as a failed fetch.
            obj = {
                "cache_version": CACHE_VERSION, "imdb": iid,
                "fetched_at": obj.get("fetched_at") or dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
                "url": obj.get("url") or f"https://www.imdb.com/title/{iid}/technical/",
                "method": obj.get("method") or "imdb", "parser": obj.get("parser") or "",
                "attempts": obj.get("attempts") or [], "status": "empty",
                "specs": {k: [] for k in SECTIONS}, "ok": True, "ready": True,
            }
            try:
                result = rewrite_specs_only(path, obj, info)
                if verbose and result == "updated":
                    print(f"   ✅ IMDb 明确无 Technical Specifications，已记录 Ready：{display}", flush=True)
                return "spec-empty" if result in ("updated", "current") else result
            except Exception as e:
                if verbose:
                    print(f"   ❌ {path}: {e}", flush=True)
                return "error"
        if verbose:
            print("   ↳ IMDb 获取/解析失败；不会影响已有 Tag，稍后重试。", flush=True)
        return status

    try:
        result = rewrite_specs_only(path, obj, info)
        if result == "updated":
            print(f"   ✅ Spec 已写入 [{media_label}] {display}  [{iid}]", flush=True)
        elif result == "retry":
            print(f"   ↻ NFO 正在被其他程序修改，延后重试：{display}", flush=True)
        return "spec-updated" if result == "updated" else result
    except ET.ParseError:
        print(f"   ↻ NFO 尚未稳定，延后重试：{display}", flush=True)
        return "retry"
    except Exception as e:
        print(f"   ❌ {path}: {e}", flush=True)
        return "error"

def run(mode):
    cfg = load_json(CFG, {})
    configured_roots = configured_roots_flat(cfg)

    # Configured roots are authoritative. TMM discovery is
    # intentionally NOT merged during background cycles.  A path that the
    # user removes in the GUI must stay removed.  TMM discovery now happens
    # only during first install or when the user explicitly clicks
    # "从 TMM 发现资料库".  Temporarily offline SMB/NAS roots remain in config.
    roots = list(dict.fromkeys(configured_roots))

    active_roots = [
        os.path.realpath(r)
        for r in roots
        if os.path.isdir(r)
    ]

    if not roots:
        print("未配置电影 / 电视剧资料库。请先运行 --configure。", file=sys.stderr)
        return 2

    if not active_roots:
        print("当前没有已挂载的电影 / 电视剧资料库，本轮跳过，稍后自动重试。")
        return 0

    APP.mkdir(parents=True, exist_ok=True)
    PROFILE.mkdir(parents=True, exist_ok=True)

    lock = LOCK.open("a+")
    import fcntl

    if mode in ("backfill", "rebuild", "refresh"):
        if not _flock_exclusive(lock, "后台自动任务正在处理影片，等待它释放 NFO 写入锁"):
            return 0
    else:
        try:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except Exception:
            # 自动后台任务遇到手工 Backfill 时直接跳过本轮，
            # 下一次 LaunchAgent 周期会再试。
            return 0

    if mode in ("watch", "auto"):
        # Work queue (legacy): background cycles serve from the resident
        # LibraryCatalog and never rglob the library per cycle.
        try:
            _catalog_ensure()
            paths = [Path(real) for real in (_LIBRARY_CATALOG.get("items") or {})]
        except Exception:
            paths = []
        if not paths:
            print("当前索引没有 NFO；打开管理器或手动刷新会建立索引。", flush=True)
    else:
        print("🔎 正在扫描电影 / 电视剧资料库中的 NFO……", flush=True)
        paths = list(nfos(active_roots))
        print(f"✅ 共发现 {len(paths)} 个 NFO。", flush=True)
        print("说明：只读取 NFO 已有的标题和 IMDb ID，不按片名重新匹配。", flush=True)
        print("", flush=True)
        print("说明：只读取 NFO 已有的标题和 IMDb ID，不按片名重新匹配。", flush=True)
        print("", flush=True)

    counts = {}
    seen = set()
    now = time.time()
    manual_done = 0
    manual_total = len(paths)

    def do(p, force=False, only_missing=False):
        nonlocal manual_done
        status = process(
            p,
            force=force,
            only_missing=only_missing,
            verbose=(mode not in ("watch", "auto")),
        )
        counts[status] = counts.get(status, 0) + 1
        if mode not in ("watch", "auto"):
            manual_done += 1
            if manual_done % 25 == 0 or manual_done == manual_total:
                print(
                    f"📊 进度 {manual_done}/{manual_total}  "
                    + "，".join(f"{k}={v}" for k, v in sorted(counts.items())),
                    flush=True,
                )

    # 优先处理 TMM 刚写过的 NFO（mtime 来自索引摘要，不再逐个 stat）。
    if mode in ("watch", "auto"):
        for p in paths:
            if _manual_task_requested():
                print("⏸️ 检测到手动任务，后台本轮让位。", flush=True)
                break
            entry = (_LIBRARY_CATALOG.get("items") or {}).get(os.path.realpath(str(p)))
            summary = entry.get("summary") if isinstance(entry, dict) else None
            mtime = (summary or {}).get("nfo_mtime") or 0
            age = now - mtime
            if age < STABLE_SECONDS or age > WATCH_WINDOW:
                continue

            seen.add(str(p))
            do(p, only_missing=True)

    # 自动后台分批补齐存量；候选来自索引摘要（Spec 未就绪项），不读 NFO。
    if mode == "auto":
        added = 0

        for p in paths:
            if added >= BACKFILL_BATCH:
                break
            if _manual_task_requested():
                print("⏸️ 检测到手动任务，后台本轮让位。", flush=True)
                break

            if str(p) in seen:
                continue

            entry = (_LIBRARY_CATALOG.get("items") or {}).get(os.path.realpath(str(p)))
            summary = entry.get("summary") if isinstance(entry, dict) else None
            if not isinstance(summary, dict):
                continue
            if summary.get("media_type") not in ("movie", "tvshow", "episode"):
                continue
            if summary.get("spec_status") != "missing":
                continue

            iid = clean(str(summary.get("imdb") or "")).lower()
            if not iid:
                continue

            # Respect the same retry windows as get_specs(): genuine no-tech
            # results cool down for 7 days, while transient fetch errors only
            # cool down for 1 hour.  The old code incorrectly treated both as
            # 7-day failures, so a temporary network/WAF issue could stall
            # background backfill for a week.
            old = load_json(cache_file(iid))
            if old and old.get("cache_version") == CACHE_VERSION and not old.get("ok"):
                try:
                    fetched = dt.datetime.fromisoformat(old["fetched_at"])
                    age = dt.datetime.now(dt.timezone.utc) - fetched
                    status = old.get("status")
                    if status == "no-tech" and age < dt.timedelta(days=NO_TECH_CACHE_DAYS):
                        continue
                    if status in TRANSIENT_FETCH_STATUSES and age < dt.timedelta(hours=FETCH_ERROR_CACHE_HOURS):
                        continue
                except Exception:
                    pass

            do(p, only_missing=True)
            added += 1

    elif mode in ("backfill", "rebuild", "refresh"):
        for p in paths:
            if _manual_task_requested():
                print("⏸️ 检测到手动任务，本任务让位（剩余项下次继续）。", flush=True)
                break
            do(
                p,
                force=(mode == "refresh"),
                only_missing=(mode == "backfill"),
            )

    if mode not in ("watch", "auto"):
        print()
        print("完成：", "，".join(f"{k}={v}" for k, v in sorted(counts.items())))

    try:
        save_json(STATUS, {
            "updated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
            "mode": mode,
            "configured_roots": roots,
            "active_roots": active_roots,
            "nfo_total": len(paths),
            "counts": counts,
        })
        refresh_pipeline_status(force=(mode not in ("watch", "auto")))
    except Exception:
        pass

    return 0


def _configured_nfo_paths():
    cfg = load_json(CFG, {}) or {}
    roots = [r for r in configured_roots_flat(cfg) if os.path.isdir(r)]
    if not roots:
        raise RuntimeError("当前没有已挂载且可访问的资料库")
    return list(nfos(list(dict.fromkeys(roots))))


def _cache_failure_state(iid):
    if not iid:
        return ""
    obj = load_json(cache_file(iid), {}) or {}
    if not isinstance(obj, dict) or obj.get("ok"):
        return ""
    status = clean(obj.get("status", ""))
    raw = json.dumps(obj, ensure_ascii=False).lower()
    if "http 202" in raw or "waf" in raw or "blocked" in raw:
        return "blocked"
    if status == "no-tech":
        return "empty-cached"
    if status:
        return status
    return "fetch-error"


def compute_pipeline_status(paths=None, save=True):
    metrics_incr("pipeline_status_calls")
    if paths is None:
        # legacy: derived state. Aggregate in-memory catalog summaries instead
        # of re-reading/re-parsing the whole library after every task.
        _catalog_ensure()
        dynamic = _dynamic_overlay_context()
        summaries = [_apply_summary_overlay(entry.get("summary"), dynamic) for entry in (_LIBRARY_CATALOG.get("items") or {}).values() if isinstance(entry, dict) and isinstance(entry.get("summary"), dict)]
        return _pipeline_status_from_summaries(summaries, save=save)
    summaries = []
    for p in paths:
        try:
            summaries.append(_cached_summary_for_path(p))
        except Exception:
            continue
    return _pipeline_status_from_summaries(summaries, save=save, total_override=len(paths))


def _pipeline_status_from_summaries(summaries, save=True, total_override=None):
    counts = {
        "nfo_total": 0, "supported": 0, "xml_error": 0, "no_imdb": 0,
        "spec_ready": 0, "spec_empty": 0, "spec_missing": 0, "spec_mismatch": 0,
        "spec_fetch_failed": 0, "spec_blocked": 0,
        "tag_ai_current": 0, "tag_local_current": 0, "tag_legacy": 0,
        "tag_none": 0, "tag_stale": 0, "tag_review": 0, "tag_missing": 0,
        "tag_not_applicable": 0, "legacy_safe": 0, "legacy_unsafe": 0,
    }
    counts["nfo_total"] = total_override if total_override is not None else len(summaries)
    for summary in summaries:
        path_str = str(summary.get("path") or "")
        if not summary.get("xml_valid", True):
            counts["xml_error"] += 1
            continue
        media_type = summary.get("media_type")
        if media_type not in ("movie", "tvshow", "episode"):
            continue
        counts["supported"] += 1
        iid = clean(str(summary.get("imdb") or "")).lower()
        if not iid:
            counts["no_imdb"] += 1
            continue
        spec_status = summary.get("spec_status")
        if spec_status == "missing":
            counts["spec_missing"] += 1
            fail = _cache_failure_state(iid)
            if fail == "blocked":
                counts["spec_blocked"] += 1
            elif fail and fail != "empty-cached":
                counts["spec_fetch_failed"] += 1
            continue
        if spec_status == "not-applicable":
            continue
        counts["spec_ready"] += 1
        if spec_status == "empty":
            counts["spec_empty"] += 1
            counts["tag_not_applicable"] += 1
            continue
        if not summary.get("ai_preview_ready"):
            counts["tag_not_applicable"] += 1
            continue
        st = summary.get("tag_status")
        # A reviewed AI result intentionally leaves the NFO unchanged. The
        # reconcile pass stores that cached candidate in cached_review.
        if st not in ("ai-current", "local-current", "current", "review") and summary.get("cached_review"):
            st = "review"
        key = {
            "ai-current": "tag_ai_current", "local-current": "tag_local_current",
            "legacy": "tag_legacy", "none": "tag_none", "stale": "tag_stale",
            "review": "tag_review", "tag-missing": "tag_missing",
        }.get(st, "tag_none")
        counts[key] += 1
        if st == "legacy":
            with contextlib.suppress(Exception):
                _, baseline = protected_tmm_tags(path_str, iid)
                counts["legacy_safe" if baseline else "legacy_unsafe"] += 1
    obj = {
        "updated_at": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
        "counts": counts,
        "ai_runtime": _load_ai_runtime(),
    }
    if save:
        save_json(PIPELINE_STATUS, obj)
    return obj


def refresh_pipeline_status(paths=None, force=False):
    if not force and PIPELINE_STATUS.exists():
        with contextlib.suppress(Exception):
            if time.time() - PIPELINE_STATUS.stat().st_mtime < 600:
                return load_json(PIPELINE_STATUS, {}) or {}
    try:
        return compute_pipeline_status(paths=paths, save=True)
    except Exception as e:
        return {"updated_at": dt.datetime.now(dt.timezone.utc).isoformat(), "error": str(e), "counts": {}}


def pipeline_scan():
    try:
        paths = _configured_nfo_paths()
    except Exception as e:
        print("❌", e, file=sys.stderr)
        return 2
    print("正在扫描数据流水线状态……", flush=True)
    obj = compute_pipeline_status(paths, save=True)
    c = obj["counts"]
    print(f"NFO：{c['nfo_total']}，Spec Ready：{c['spec_ready']}（空数据 {c['spec_empty']}）")
    print(f"Spec 待准备：{c['spec_missing']}，IMDb ID 缺失：{c['no_imdb']}，XML 错误：{c['xml_error']}")
    print(f"Tag：AI {c['tag_ai_current']} / 本地 {c['tag_local_current']} / Legacy {c['tag_legacy']} / 未生成 {c['tag_none']} / 过期 {c['tag_stale']} / 待复核 {c['tag_review']}")
    if c["legacy_unsafe"]:
        print(f"⚠️ Legacy 中 {c['legacy_unsafe']} 个没有可信原始 TMM 基线，严格模式不会删除其旧标准 Tag。")
    return 0


def _budget_reached(cfg, usage_total, request_count, cost_total):
    req_limit = int(cfg.get("run_request_limit", 0) or 0)
    token_limit = int(cfg.get("run_token_limit", 0) or 0)
    cost_limit = float(cfg.get("run_cost_limit", 0) or 0)
    _, _, tokens = _usage_tokens(usage_total)
    if req_limit and request_count >= req_limit:
        return f"已达到本次请求上限 {req_limit}"
    if token_limit and tokens >= token_limit:
        return f"已达到本次 Token 上限 {token_limit}"
    if cost_limit and cost_total >= cost_limit:
        return f"已达到本次费用上限 {cost_limit:g}"
    return ""


def _merge_usage(total, usage):
    return _usage_add(total, usage)


def _utc_now():
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()


def _empty_batch_counts():
    return {
        "scanned": 0, "spec_ready": 0, "updated": 0, "current": 0,
        "skipped_ai_current": 0, "empty": 0, "not_applicable": 0,
        "unsafe_skip": 0, "review": 0, "error": 0, "cache_hit": 0,
        "api_calls": 0, "paused": 0, "known_failure_skipped": 0,
    }


def _load_failure_queue():
    obj = load_json(AI_FAILURE_QUEUE, {}) or {}
    if not isinstance(obj, dict):
        obj = {}
    items = obj.get("items") if isinstance(obj.get("items"), list) else []
    return {"schema": 2, "items": [x for x in items if isinstance(x, dict)]}


def _save_failure_queue(obj):
    obj = dict(obj or {})
    obj["schema"] = 2
    obj["updated_at"] = _utc_now()
    save_json(AI_FAILURE_QUEUE, obj)


def _failure_upsert(entry):
    obj = _load_failure_queue()
    path_key = os.path.realpath(str(entry.get("path") or ""))
    out = []
    replaced = False
    for x in obj["items"]:
        if os.path.realpath(str(x.get("path") or "")) == path_key:
            if not replaced:
                out.append(dict(entry))
                replaced = True
        else:
            out.append(x)
    if not replaced:
        out.append(dict(entry))
    obj["items"] = out
    _save_failure_queue(obj)


def _failure_remove(path):
    obj = _load_failure_queue()
    path_key = os.path.realpath(str(path))
    out = [x for x in obj["items"] if os.path.realpath(str(x.get("path") or "")) != path_key]
    if len(out) != len(obj["items"]):
        obj["items"] = out
        _save_failure_queue(obj)


def _ai_params_hash(cfg):
    params = {
        "temperature": cfg.get("temperature", 0),
        "top_p": cfg.get("top_p", 1),
        "max_tokens": int(cfg.get("max_tokens", 2000) or 2000),
        "output_token_cap": int(cfg.get("output_token_cap", 10000) or 10000),
        "json_mode": clean(str(cfg.get("json_mode") or "auto")),
        "thinking_mode": clean(str(cfg.get("thinking_mode") or "off")),
        "prompt_cache_mode": clean(str(cfg.get("prompt_cache_mode") or "auto")),
        "extra_body": str(cfg.get("extra_body") or "{}"),
        "output_language": cfg.get("output_language", "zh-CN"),
        "language_contract": LANGUAGE_CONTRACT_VERSION,
        "result_schema": 2,
        "failure_policy": 1,
    }
    raw = json.dumps(params, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


def _failure_fingerprint(path, specs, cfg, kind):
    protocol = _ai_protocol(cfg)
    payload = {
        "path": os.path.realpath(str(path)),
        "spec_hash": _specs_hash(specs),
        "provider": clean(str(cfg.get("provider") or "")),
        "api_protocol": protocol,
        "endpoint": ai_endpoint(cfg.get("base_url") or "", protocol),
        "model": clean(str(cfg.get("model") or "")),
        "prompt_hash": hashlib.sha256(_effective_ai_prompt(cfg).encode("utf-8")).hexdigest(),
        "params_hash": _ai_params_hash(cfg),
        "failure_kind": clean(str(kind or "request")),
    }
    raw = json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


def _failure_for_path(path):
    target = os.path.realpath(str(path))
    for entry in _load_failure_queue().get("items", []):
        if os.path.realpath(str(entry.get("path") or "")) == target:
            return entry
    return None


def _known_failure_unchanged(path, specs, cfg):
    entry = _failure_for_path(path)
    if not entry:
        return False
    stored = clean(str(entry.get("fingerprint") or ""))
    if not stored:
        return True
    current = _failure_fingerprint(path, specs, cfg, entry.get("kind") or "request")
    return stored == current


def _new_task_id():
    raw = (str(time.time_ns()) + "-" + str(os.getpid())).encode("utf-8")
    return hashlib.sha256(raw).hexdigest()[:16]


def _batch_save(state):
    state = dict(state or {})
    state["schema"] = AI_BATCH_SCHEMA
    state["updated_at"] = _utc_now()
    save_json(AI_BATCH_STATE, state)


def _batch_new(paths, scope, action):
    task_id = _new_task_id()
    scope_kind = clean(str(scope or "all")) or "all"
    scope_obj = {
        "kind": scope_kind, "media_space": "", "label": scope_kind,
        "query_snapshot": {}, "resolved_paths": [str(p) for p in paths],
        "created_at": _utc_now(),
    }
    queue = {
        "schema": AI_BATCH_SCHEMA,
        "task_id": task_id,
        "engine": "ai" if action.startswith("ai-") else "local-rules",
        "scope": scope_obj,
        "scope_kind": scope_kind,
        "action": action,
        "created_at": _utc_now(),
        "paths": [str(p) for p in paths],
    }
    save_json(AI_BATCH_QUEUE, queue)
    with contextlib.suppress(Exception):
        AI_BATCH_PAUSE.unlink()
    state = {
        "schema": AI_BATCH_SCHEMA,
        "task_id": task_id,
        "engine": queue["engine"],
        "scope": scope_obj,
        "scope_kind": scope_kind,
        "action": action,
        "status": "running",
        "started_at": _utc_now(),
        "ended_at": "",
        "total": len(paths),
        "next_index": 0,
        "current_index": 0,
        "current_path": "",
        "current_title": "",
        "pause_requested": False,
        "counts": _empty_batch_counts(),
        "http_attempts": 0,
        "http_2xx": 0,
        "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
        "reasoning_tokens": 0,
        "cost": 0.0,
        "cached_usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
        "cached_cost": 0.0,
        "failures": [],
        "reviews": [],
        "unsafe": [],
    }
    _batch_save(state)
    return state, queue


def _batch_load_resume():
    state = load_json(AI_BATCH_STATE, {}) or {}
    queue = load_json(AI_BATCH_QUEUE, {}) or {}
    if not isinstance(state, dict) or not isinstance(queue, dict):
        raise ValueError("没有可继续的 AI 批量任务")
    if state.get("schema") == 1 and queue.get("schema") == 1:
        legacy_scope = clean(str(state.get("scope") or queue.get("scope") or "all")) or "all"
        paths = queue.get("paths") if isinstance(queue.get("paths"), list) else []
        scope_obj = {
            "kind": "selection" if legacy_scope == "selected" else ("retry-failed" if legacy_scope == "retry-failed" else "legacy-all"),
            "media_space": "", "label": "2.x 导入任务", "query_snapshot": {},
            "resolved_paths": list(paths), "created_at": queue.get("created_at") or _utc_now(),
        }
        for obj in (state, queue):
            obj["schema"] = AI_BATCH_SCHEMA
            obj["engine"] = "ai"
            obj["scope"] = scope_obj
            obj["scope_kind"] = legacy_scope
        _batch_save(state)
        save_json(AI_BATCH_QUEUE, queue)
    if state.get("schema") != AI_BATCH_SCHEMA or queue.get("schema") != AI_BATCH_SCHEMA:
        raise ValueError("AI 批量任务状态版本不兼容")
    if state.get("task_id") != queue.get("task_id"):
        raise ValueError("AI 批量任务队列与状态不匹配")
    paths = queue.get("paths") if isinstance(queue.get("paths"), list) else []
    if not paths:
        raise ValueError("AI 批量任务队列为空")
    next_index = max(0, min(len(paths), int(state.get("next_index", 0) or 0)))
    if next_index >= len(paths):
        raise ValueError("上一个 AI 批量任务已经处理完毕")
    with contextlib.suppress(Exception):
        AI_BATCH_PAUSE.unlink()
    state["status"] = "running"
    state["pause_requested"] = False
    state["ended_at"] = ""
    state["resumed_at"] = _utc_now()
    _batch_save(state)
    return state, queue


def _batch_pause_requested():
    return AI_BATCH_PAUSE.exists()


def request_ai_task_pause():
    AI_BATCH_PAUSE.parent.mkdir(parents=True, exist_ok=True)
    AI_BATCH_PAUSE.write_text(_utc_now() + "\n", encoding="utf-8")
    state = load_json(AI_BATCH_STATE, {}) or {}
    if isinstance(state, dict) and state:
        state["pause_requested"] = True
        _batch_save(state)
    print("⏸️ 已请求暂停 AI 批量任务；当前 NFO 完成后停止取下一项。")
    return 0


def _batch_state_add(state, key, entry):
    rows = state.get(key) if isinstance(state.get(key), list) else []
    path_key = os.path.realpath(str(entry.get("path") or ""))
    rows = [x for x in rows if os.path.realpath(str(x.get("path") or "")) != path_key]
    rows.append(dict(entry))
    state[key] = rows[-200:]


def _batch_metrics_update(state, base_meter, cached_usage, cached_cost):
    meter = _meter_snapshot()
    state["http_attempts"] = int(base_meter.get("http_attempts", 0)) + meter["http_attempts"]
    state["http_2xx"] = int(base_meter.get("http_2xx", 0)) + meter["http_2xx"]
    state["usage"] = _usage_add(base_meter.get("usage", {}), meter["usage"])
    state["reasoning_tokens"] = int(base_meter.get("reasoning_tokens", 0)) + meter["reasoning_tokens"]
    state["cost"] = float(base_meter.get("cost", 0.0) or 0.0) + meter["cost"]
    state["cached_usage"] = dict(cached_usage or {})
    state["cached_cost"] = float(cached_cost or 0.0)


def _retry_failed_paths():
    obj = _load_failure_queue()
    out = []
    seen = set()
    for x in obj["items"]:
        if x.get("retryable") is False:
            continue
        p = str(x.get("path") or "")
        rp = os.path.realpath(p)
        if p and rp not in seen and os.path.exists(p):
            seen.add(rp)
            out.append(p)
    return out


def tag_generate(mode="local", force_all=False, selected_paths=None, resume_task=False, retry_failed=False):
    if mode not in ("local", "ai"):
        raise ValueError("tag mode invalid")
    if mode == "ai":
        ok, msg = ai_ready(require_enabled=True)
        if not ok:
            print("❌ " + msg, file=sys.stderr)
            return 2

    batch_state = None
    batch_queue = None
    scope = "all"
    # Full-library enumeration only for genuinely full-library runs. Selected
    # and resume tasks validate their explicit paths instead (O(selected)).
    configured_paths = None
    paths = None
    if mode == "ai" and resume_task:
        try:
            batch_state, batch_queue = _batch_load_resume()
        except Exception as e:
            print("❌ 无法继续 AI 批量任务：" + str(e), file=sys.stderr)
            return 2
        paths = [Path(str(x)) for x in batch_queue.get("paths", [])]
        scope_value = batch_state.get("scope") if isinstance(batch_state.get("scope"), dict) else {}
        scope = clean(str(batch_state.get("scope_kind") or scope_value.get("kind") or "all")) or "all"
    else:
        if mode == "ai" and retry_failed:
            selected_paths = _retry_failed_paths()
            if not selected_paths:
                print("✅ 当前没有可重试的 AI 失败项。")
                return 0
            scope = "retry-failed"
        elif selected_paths is not None:
            scope = "selected"
        if selected_paths is None:
            try:
                configured_paths = _configured_nfo_paths()
            except Exception as e:
                print("❌", e, file=sys.stderr)
                return 2
            paths = configured_paths

        if selected_paths is not None:
            paths = _validate_selected_nfo_paths(selected_paths, strict=False)
            if not paths:
                print("❌ 没有有效的所选 NFO。", file=sys.stderr)
                return 2
        if mode == "ai":
            action = "ai-retry-failed" if retry_failed else ("ai-generate-selected" if selected_paths is not None else "ai-generate")
            batch_state, batch_queue = _batch_new(paths, scope, action)

    if mode == "ai" and scope == "selected":
        print("AI 标签指定 NFO 写入（%d 个）" % len(paths))
        print("========================================")
    elif mode == "ai" and scope == "retry-failed":
        print("AI 失败项重试（%d 个）" % len(paths))
        print("========================================")
    elif mode == "ai" and resume_task:
        print("AI 批量任务继续：从 %d / %d 开始" % (int(batch_state.get("next_index", 0)) + 1, len(paths)))
        print("========================================")

    import fcntl
    lock = LOCK.open("a+")
    try:
        try:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            _flock_exclusive(lock, "等待 Spec Agent 释放 NFO 写入锁")
        cfg = ai_config()
        cleanup_mode = cfg.get("legacy_cleanup_mode", "strict")
        counts = _empty_batch_counts()
        start_idx = 0
        base_meter = {"http_attempts": 0, "http_2xx": 0, "usage": {}, "reasoning_tokens": 0, "cost": 0.0}
        cached_usage_total = {}
        cached_cost_total = 0.0
        if mode == "ai" and batch_state is not None:
            counts.update(batch_state.get("counts") if isinstance(batch_state.get("counts"), dict) else {})
            start_idx = int(batch_state.get("next_index", 0) or 0) if resume_task else 0
            base_meter = {
                "http_attempts": int(batch_state.get("http_attempts", 0) or 0),
                "http_2xx": int(batch_state.get("http_2xx", 0) or 0),
                "usage": dict(batch_state.get("usage", {}) or {}),
                "reasoning_tokens": int(batch_state.get("reasoning_tokens", 0) or 0),
                "cost": float(batch_state.get("cost", 0.0) or 0.0),
            }
            cached_usage_total = dict(batch_state.get("cached_usage", {}) or {})
            cached_cost_total = float(batch_state.get("cached_cost", 0.0) or 0.0)
        _meter_reset()
        total = len(paths)
        terminal_status = "completed"
        progress_action = ("ai-" if mode == "ai" else "local-") + "generate"
        progress_results = []
        _progress_clear()
        _progress_write(progress_action, 0, total, "", progress_results)

        for pos in range(start_idx, total):
            idx = pos + 1
            p = Path(paths[pos])
            _progress_write(progress_action, idx - 1, total, str(p), progress_results)

            def _skip_progress(reason, name_ref=None, tag_status_ref=""):
                # Every early exit must be visible in the UI: invisible skips
                # looked like random gaps in legacy.
                progress_results.append({"path": str(p), "title": name_ref or name, "status": reason, "tag_status": tag_status_ref})
                _progress_write(progress_action, idx, total, "", progress_results)
            if mode == "ai" and _batch_pause_requested():
                counts["paused"] += 1
                terminal_status = "paused"
                batch_state["pause_requested"] = True
                batch_state["status"] = "paused"
                batch_state["next_index"] = pos
                batch_state["current_index"] = 0
                batch_state["current_path"] = ""
                batch_state["current_title"] = ""
                batch_state["counts"] = counts
                _batch_metrics_update(batch_state, base_meter, cached_usage_total, cached_cost_total)
                _batch_save(batch_state)
                print("⏸️ AI 批量任务已暂停；下次继续将从 [%d/%d] 开始。" % (idx, total), flush=True)
                break

            counts["scanned"] += 1
            info = None
            old = None
            name = p.stem
            if mode == "ai":
                batch_state["status"] = "running"
                batch_state["pause_requested"] = False
                batch_state["current_index"] = idx
                batch_state["current_path"] = str(p)
                batch_state["current_title"] = name
                _batch_save(batch_state)
            try:
                text = p.read_bytes().decode("utf-8-sig")
                info = inspect_nfo(text)
                old = existing_tech_object(text)
                if info:
                    name = info.get("display_title") or info.get("title") or p.stem
            except Exception as e:
                counts["error"] += 1
                entry = {"path": str(p), "title": name, "imdb": "", "kind": "nfo-read", "message": str(e), "index": idx, "retryable": True, "at": _utc_now()}
                if mode == "ai":
                    _failure_upsert(entry)
                    _batch_state_add(batch_state, "failures", entry)
                print("❌ [%d/%d] NFO 读取失败：%s" % (idx, total, e), flush=True)
                _skip_progress("skipped-read-error", name)
                if mode == "ai":
                    batch_state["next_index"] = idx
                    batch_state["counts"] = counts
                    _batch_metrics_update(batch_state, base_meter, cached_usage_total, cached_cost_total)
                    _batch_save(batch_state)
                continue
            if not info or not old or old.get("imdb") != (imdb_id(text) or "").lower():
                _skip_progress("skipped-unsupported", name)
                if mode == "ai":
                    _failure_remove(p)
                    batch_state["next_index"] = idx
                    batch_state["counts"] = counts
                    _batch_metrics_update(batch_state, base_meter, cached_usage_total, cached_cost_total)
                    _batch_save(batch_state)
                continue
            counts["spec_ready"] += 1
            if old.get("status") == "empty":
                counts["empty"] += 1
                _skip_progress("skipped-empty-specs", name)
                if mode == "ai":
                    _failure_remove(p)
                    batch_state["next_index"] = idx
                    batch_state["counts"] = counts
                    _batch_metrics_update(batch_state, base_meter, cached_usage_total, cached_cost_total)
                    _batch_save(batch_state)
                continue
            if not _ai_input_specs(old.get("specs", {})):
                counts["not_applicable"] += 1
                _skip_progress("skipped-not-applicable", name)
                if mode == "ai":
                    _failure_remove(p)
                    batch_state["next_index"] = idx
                    batch_state["counts"] = counts
                    _batch_metrics_update(batch_state, base_meter, cached_usage_total, cached_cost_total)
                    _batch_save(batch_state)
                continue
            state = tag_state(text, old, p)
            if not force_all:
                if mode == "ai" and state == "ai-current":
                    counts["current"] += 1
                    _skip_progress("skipped-current", name, "ai-current")
                    _failure_remove(p)
                    batch_state["next_index"] = idx
                    batch_state["counts"] = counts
                    _batch_metrics_update(batch_state, base_meter, cached_usage_total, cached_cost_total)
                    _batch_save(batch_state)
                    continue
                if mode == "local" and state == "local-current":
                    counts["current"] += 1
                    _skip_progress("skipped-current", name, "local-current")
                    continue
                if mode == "local" and state == "ai-current":
                    counts["skipped_ai_current"] += 1
                    _skip_progress("skipped-ai-current", name, "ai-current")
                    continue
            if mode == "ai" and not retry_failed and _known_failure_unchanged(p, old.get("specs", {}), cfg):
                counts["known_failure_skipped"] += 1
                previous_failure = _failure_for_path(p) or {}
                _batch_state_add(batch_state, "failures", {
                    "path": str(p), "title": name, "imdb": old.get("imdb", ""),
                    "kind": previous_failure.get("kind", "known-failure"),
                    "message": "已知失败，输入和配置未变化，本轮未再次调用 AI",
                    "index": idx, "retryable": True, "known_failure_skipped": True,
                    "fingerprint": previous_failure.get("fingerprint", ""),
                })
                batch_state["next_index"] = idx
                batch_state["counts"] = counts
                _batch_metrics_update(batch_state, base_meter, cached_usage_total, cached_cost_total)
                _batch_save(batch_state)
                print("⏭️ [%d/%d] 已知 AI 失败未变化，跳过请求：%s" % (idx, total, name), flush=True)
                _skip_progress("skipped-known-failure", name)
                continue
            if mode == "ai":
                snap = _meter_snapshot()
                run_usage = _usage_add(base_meter.get("usage", {}), snap["usage"])
                run_requests = int(base_meter.get("http_attempts", 0)) + snap["http_attempts"]
                run_cost = float(base_meter.get("cost", 0.0) or 0.0) + snap["cost"]
                budget = _budget_reached(cfg, run_usage, run_requests, run_cost)
                if budget:
                    ai_pause("budget", budget)
                    counts["paused"] += 1
                    terminal_status = "runtime-paused"
                    batch_state["status"] = terminal_status
                    batch_state["next_index"] = pos
                    batch_state["counts"] = counts
                    _batch_metrics_update(batch_state, base_meter, cached_usage_total, cached_cost_total)
                    _batch_save(batch_state)
                    print("⏸️ AI Runtime 已按预算暂停：" + budget, flush=True)
                    break
            iteration_outcome = {"path": str(p), "title": name, "status": "error", "tag_status": ""}
            try:
                status = rewrite(p, old, info, tag_mode=mode, cleanup_mode=cleanup_mode, dry_run=False)
                detail = dict(LAST_REWRITE_DETAIL)
                iteration_outcome["status"] = status
                if status in ("updated", "current", "planned"):
                    iteration_outcome["tag_status"] = "ai-current" if mode == "ai" else "local-current"
                    with contextlib.suppress(Exception):
                        refresh_path_summary(p)
                if mode == "ai":
                    if detail.get("cache_hit"):
                        counts["cache_hit"] += 1
                        cached_usage_total = _usage_add(cached_usage_total, detail.get("cached_usage", {}))
                        cached_cost_total += float(detail.get("cached_cost", 0) or 0)
                    elif detail.get("tag_engine") == "ai":
                        counts["api_calls"] += 1
                if status == "updated":
                    counts["updated"] += 1
                elif status == "unsafe-skip":
                    counts["unsafe_skip"] += 1
                elif status == "review":
                    counts["review"] += 1
                else:
                    counts["current"] += 1

                if mode == "ai":
                    _failure_remove(p)
                    if status == "review":
                        _batch_state_add(batch_state, "reviews", {"path": str(p), "title": name, "imdb": old.get("imdb", ""), "index": idx, "reason": "; ".join(detail.get("review_reasons", []) or [])[:1000]})
                    elif status == "unsafe-skip":
                        _batch_state_add(batch_state, "unsafe", {"path": str(p), "title": name, "imdb": old.get("imdb", ""), "index": idx, "reason": clean(detail.get("reason", ""))[:1000]})
                if status == "updated":
                    badge = "AI" if mode == "ai" else "本地"
                    print("✅ [%d/%d] %s Tag 已写入：%s" % (idx, total, badge, name), flush=True)
                elif status == "review":
                    print("🟡 [%d/%d] AI 结果需复核，NFO 未改：%s" % (idx, total, name), flush=True)
                elif status == "unsafe-skip":
                    print("⚠️  [%d/%d] 无法安全确认旧 Tag 所有权，NFO 未改：%s" % (idx, total, name), flush=True)
            except AIRequestError as e:
                counts["error"] += 1
                if e.kind in ("quota", "auth", "paused", "rate-limit", "budget"):
                    if e.kind == "rate-limit":
                        ai_pause("rate-limit", str(e))
                    counts["paused"] += 1
                    terminal_status = "runtime-paused"
                    batch_state["status"] = terminal_status
                    batch_state["next_index"] = pos
                    batch_state["counts"] = counts
                    _batch_metrics_update(batch_state, base_meter, cached_usage_total, cached_cost_total)
                    _batch_save(batch_state)
                    print("⏸️ AI Runtime 已暂停：%s" % e, flush=True)
                    break
                failure_specs = old.get("specs", {}) if old else {}
                entry = {
                    "path": str(p), "title": name,
                    "year": info.get("year", "") if info else "",
                    "imdb": old.get("imdb", "") if old else "",
                    "media_type": info.get("media_type", "") if info else "",
                    "kind": e.kind, "message": str(e), "index": idx,
                    "retryable": e.kind in (
                        "rate-limit", "transient", "malformed-json", "schema-invalid",
                        "output-truncated", "provider-response", "request",
                    ),
                    "finish_reason": e.finish_reason,
                    "spec_hash": _specs_hash(failure_specs),
                    "provider": cfg.get("provider", ""),
                    "api_protocol": _ai_protocol(cfg),
                    "endpoint": ai_endpoint(cfg.get("base_url", ""), _ai_protocol(cfg)),
                    "model": cfg.get("model", ""),
                    "prompt_hash": hashlib.sha256(_effective_ai_prompt(cfg).encode("utf-8")).hexdigest(),
                    "params_hash": _ai_params_hash(cfg),
                    "fingerprint": _failure_fingerprint(p, failure_specs, cfg, e.kind),
                    "task_id": batch_state.get("task_id", "") if batch_state else "",
                    "at": _utc_now(),
                }
                _failure_upsert(entry)
                _batch_state_add(batch_state, "failures", entry)
                print("❌ [%d/%d] AI：%s" % (idx, total, e), flush=True)
            except Exception as e:
                counts["error"] += 1
                entry = {"path": str(p), "title": name, "imdb": old.get("imdb", "") if old else "", "kind": "exception", "message": str(e), "index": idx, "retryable": True, "at": _utc_now()}
                if mode == "ai":
                    _failure_upsert(entry)
                    _batch_state_add(batch_state, "failures", entry)
                print("❌ [%d/%d] %s: %s" % (idx, total, p, e), flush=True)

            progress_results.append(iteration_outcome)
            _progress_write(progress_action, idx, total, "", progress_results)

            if mode == "ai":
                batch_state["next_index"] = idx
                batch_state["current_index"] = 0
                batch_state["current_path"] = ""
                batch_state["current_title"] = ""
                batch_state["counts"] = counts
                _batch_metrics_update(batch_state, base_meter, cached_usage_total, cached_cost_total)
                _batch_save(batch_state)
                if _batch_pause_requested():
                    counts["paused"] += 1
                    terminal_status = "paused"
                    batch_state["pause_requested"] = True
                    batch_state["status"] = terminal_status
                    batch_state["counts"] = counts
                    _batch_save(batch_state)
                    print("⏸️ AI 批量任务已暂停；已安全完成当前 NFO，下次从 [%d/%d] 继续。" % (idx + 1, total), flush=True)
                    break
            if idx % 25 == 0 or idx == total:
                if mode == "ai":
                    snap = _meter_snapshot()
                    run_usage = _usage_add(base_meter.get("usage", {}), snap["usage"])
                    _, _, tok = _usage_tokens(run_usage)
                    http_attempts = int(base_meter.get("http_attempts", 0)) + snap["http_attempts"]
                    run_cost = float(base_meter.get("cost", 0.0) or 0.0) + snap["cost"]
                    print("📊 " + ", ".join("%s=%s" % (k, v) for k, v in counts.items()) + ", http_attempts=%d, new_tokens=%d, new_cost=%.6f" % (http_attempts, tok, run_cost), flush=True)
                else:
                    print("📊 " + ", ".join("%s=%s" % (k, v) for k, v in counts.items()), flush=True)

        if mode == "ai" and batch_state is not None:
            if terminal_status == "completed":
                if int(batch_state.get("next_index", 0) or 0) >= total:
                    terminal_status = "completed-with-errors" if counts.get("error") else "completed"
                else:
                    # A break caused by a state not handled above should still be resumable.
                    terminal_status = clean(str(batch_state.get("status") or "paused")) or "paused"
            batch_state["status"] = terminal_status
            batch_state["counts"] = counts
            batch_state["pause_requested"] = _batch_pause_requested()
            batch_state["current_index"] = 0
            batch_state["current_path"] = ""
            batch_state["current_title"] = ""
            if terminal_status.startswith("completed"):
                batch_state["ended_at"] = _utc_now()
                batch_state["next_index"] = total
                with contextlib.suppress(Exception):
                    AI_BATCH_PAUSE.unlink()
                batch_state["pause_requested"] = False
            _batch_metrics_update(batch_state, base_meter, cached_usage_total, cached_cost_total)
            _batch_save(batch_state)

        if mode == "ai":
            snap = _meter_snapshot()
            usage_total = _usage_add(base_meter.get("usage", {}), snap["usage"])
            cost_total = float(base_meter.get("cost", 0.0) or 0.0) + snap["cost"]
            http_attempts = int(base_meter.get("http_attempts", 0)) + snap["http_attempts"]
            http_2xx = int(base_meter.get("http_2xx", 0)) + snap["http_2xx"]
            reasoning_total = int(base_meter.get("reasoning_tokens", 0)) + snap["reasoning_tokens"]
        else:
            usage_total = {}
            cost_total = 0.0
            http_attempts = 0
            http_2xx = 0
            reasoning_total = 0

        extra = {
            "engine": mode, "model": cfg.get("model", "") if mode == "ai" else LOCAL_RULES_VERSION,
            "usage": usage_total, "cost": cost_total, "runtime": _load_ai_runtime(),
            "http_attempts": http_attempts, "http_2xx": http_2xx,
            "reasoning_tokens": reasoning_total,
            "cached_usage": cached_usage_total, "cached_cost": cached_cost_total,
            "batch_state": batch_state if mode == "ai" else {},
        }
        status_mode = "local-generate"
        if mode == "ai":
            if scope == "selected":
                status_mode = "ai-generate-selected"
            elif scope == "retry-failed":
                status_mode = "ai-retry-failed"
            elif resume_task:
                status_mode = "ai-resume-task"
            else:
                status_mode = "ai-generate"
        _save_ai_status(status_mode, counts, extra)
        compute_pipeline_status(configured_paths, save=True)
        print("\n完成：", "，".join("%s=%s" % (k, v) for k, v in counts.items()))
        if mode == "ai":
            inp, out, tok = _usage_tokens(usage_total)
            cin, cout, ctok = _usage_tokens(cached_usage_total)
            print("本任务新增模型 Usage：input=%d, output=%d, total=%d, reasoning=%d, HTTP请求=%d, HTTP 2xx=%d, 估算新增费用=%.6f" % (inp, out, tok, reasoning_total, http_attempts, http_2xx, cost_total))
            if ctok or cached_cost_total:
                print("缓存命中结果历史 Usage（本次不重复扣费）：input=%d, output=%d, total=%d, 历史估算费用=%.6f" % (cin, cout, ctok, cached_cost_total))
            fq = _load_failure_queue()
            print("当前持久化 AI 失败队列：%d 项" % len(fq.get("items", [])))
        # Per-item failures are now a resumable queue, not a failure of the whole
        # batch command. Fatal setup/config errors still return non-zero above.
        return 0
    finally:
        with contextlib.suppress(Exception):
            lock.close()

def ai_resume_cli():
    ai_resume()
    print("✅ AI Runtime 暂停状态已清除。建议先执行连接测试，再继续 AI 生成队列。")
    return 0


def _save_ai_status(mode, counts, extra=None):
    obj = {
        "updated_at": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
        "mode": mode,
        "counts": counts,
    }
    if extra:
        obj.update(extra)
    save_json(AI_STATUS, obj)


def ai_test():
    ok, msg = ai_ready(require_enabled=False, allow_paused=True)
    if not ok:
        print("❌ " + msg, file=sys.stderr)
        return 2
    cfg = ai_config()
    sample = {k: [] for k in SECTIONS}
    sample["Sound mix"] = ["DTS (DTS: X)", "Dolby Atmos"]
    sample["Aspect ratio"] = ["1.43 : 1 (IMAX 70mm & Dual Laser)"]
    sample["Camera"] = [
        "Arri Alexa Plus, Hawk V-Lite, V-Plus, V-Series, Zeiss Master Prime and Angenieux Optimo Lenses"
    ]
    sample["Negative Format"] = ["35 mm (Kodak Vision3 250D 5207, Vision3 500T 5219)"]
    sample["Cinematographic Process"] = ["Digital Intermediate (4K, master format)"]
    sample["Printed Film Format"] = ["70 mm (horizontal, IMAX DMR blow-up, Kodak Vision 2383)"]
    print("AI Tag Parser 连接测试")
    print("====================")
    print("Provider:", cfg["provider"])
    print("API format:", _ai_protocol(cfg))
    print("Endpoint:", ai_endpoint(cfg["base_url"], _ai_protocol(cfg)))
    print("Model:", cfg["model"])
    print("Thinking mode:", cfg.get("thinking_mode", "off"))
    print("Prompt cache:", cfg.get("prompt_cache_mode", "auto"))
    try:
        result = ai_generate_tags(sample, force=True, ignore_pause=True)
    except Exception as e:
        print("❌ 模型调用失败：", e, file=sys.stderr)
        return 3
    rt = _load_ai_runtime()
    if rt.get("paused") and rt.get("reason_kind") in ("quota", "auth", "transient", "rate-limit"):
        ai_resume()
        print("✅ 连接测试成功，已清除之前的 AI 额度/认证/网络暂停状态。")
    print("✅ 模型返回有效 JSON。")
    print("Tags:")
    for x in result["tags"]:
        print(f'  [{x["field"]}] {x["value"]}')
    if result.get("warnings"):
        print("Warnings:")
        for x in result["warnings"]:
            print("  -", x)
    if result.get("usage"):
        print("Usage:", json.dumps(result["usage"], ensure_ascii=False))
    return 0


def ai_recover():
    """Test the current provider before clearing a quota/auth runtime pause."""
    print("AI Runtime 恢复测试")
    print("==================")
    print("将向当前模型发起一次真实连接测试；成功后才解除 AI 暂停状态。")
    return ai_test()


def ai_scan():
    try:
        paths = _configured_nfo_paths()
    except Exception as e:
        print("❌", e, file=sys.stderr)
        return 2
    counts = {
        "nfo_total": len(paths), "with_technicalspecs": 0, "ai_manifest": 0,
        "legacy_with_pristine_backup": 0, "legacy_without_pristine_backup": 0,
        "legacy_old_tags_present": 0, "xml_error": 0,
    }
    for p in paths:
        try:
            text = p.read_bytes().decode("utf-8-sig")
            info = inspect_nfo(text)
            if not info:
                continue
            old = existing_tech_object(text)
        except ET.ParseError:
            counts["xml_error"] += 1
            continue
        except Exception:
            continue
        if not old:
            continue
        counts["with_technicalspecs"] += 1
        if _manifest_values(old):
            counts["ai_manifest"] += 1
            continue
        legacy = {clean(v).casefold() for v in generated_tag_values(old["specs"], include_legacy=True) if clean(v)}
        have = {clean(v).casefold() for v in _all_normal_tags(text) if clean(v)}
        if legacy & have:
            counts["legacy_old_tags_present"] += 1
        _, baseline = protected_tmm_tags(p, old.get("imdb", ""))
        if baseline:
            counts["legacy_with_pristine_backup"] += 1
        else:
            counts["legacy_without_pristine_backup"] += 1
    _save_ai_status("scan", counts)
    print("AI 标签迁移扫描")
    print("===============")
    print(f'全部 NFO：{counts["nfo_total"]}')
    print(f'已有 technicalspecs：{counts["with_technicalspecs"]}')
    print(f'已有 AI/所有权清单：{counts["ai_manifest"]}')
    print(f'旧格式且可用原始 TMM 备份保护：{counts["legacy_with_pristine_backup"]}')
    print(f'旧格式但没有可验证原始备份：{counts["legacy_without_pristine_backup"]}')
    print(f'当前检测到旧工具 Tag 的 NFO：{counts["legacy_old_tags_present"]}')
    if counts["legacy_without_pristine_backup"]:
        print("⚠️ 严格模式会跳过没有可验证原始备份的旧 NFO，避免误删 TMM Tag。")
    else:
        print("✅ 所有旧 NFO 都有可用于保护 TMM Tag 的基线或已存在所有权清单。")
    return 0


def _ai_preview_one(p, cleanup_mode):
    p = _allowed_nfo_path(str(p))
    text = p.read_bytes().decode("utf-8-sig")
    info = inspect_nfo(text)
    old = existing_tech_object(text)
    if not info or not old or old.get("status") == "empty" or not _ai_input_specs(old.get("specs", {})):
        return None
    status = rewrite(p, old, info, tag_mode="ai", cleanup_mode=cleanup_mode, dry_run=True)
    return status, dict(LAST_REWRITE_DETAIL), info, old


def _preview_candidate_items():
    try:
        _catalog_ensure()
    except Exception as e:
        return None, str(e)
    items = []
    for summary in _catalog_query():
        if not summary.get("ai_preview_ready"):
            continue
        items.append({
            "path": summary.get("path", ""),
            "title": summary.get("title", ""),
            "year": summary.get("year", ""),
            "imdb": (summary.get("imdb") or "").lower(),
            "media_type": summary.get("media_type", ""),
            "tag_state": summary.get("tag_status", ""),
        })
    items.sort(key=lambda x: (clean(x.get("title", "")).casefold(), clean(x.get("path", "")).casefold()))
    return items, ""


def ai_preview_candidates():
    """Emit JSON for the Manager's searchable NFO picker."""
    items, error = _preview_candidate_items()
    if error:
        print(json.dumps({"error": error, "items": []}, ensure_ascii=False))
        return 2
    print(json.dumps({"items": items}, ensure_ascii=False))
    return 0


def _print_ai_preview(p, cleanup_mode, ordinal):
    try:
        res = _ai_preview_one(p, cleanup_mode)
    except Exception as e:
        print(f"❌ {p}: {e}")
        return False
    if not res:
        print(f"⚠️ {p}: 没有可供 AI Tag 预演的 Technical Specs")
        return False
    status, detail, info, old = res
    name = info.get("display_title") or info.get("title") or p.stem
    print(f"\n[{ordinal}] {name}  [{old.get('imdb','')}]")
    print("Path:", p)
    print("Ownership:", detail.get("ownership", "—"))
    print("TMM baseline:", detail.get("baseline") or "无")
    if status == "unsafe-skip":
        print("⚠️ 严格模式跳过：", detail.get("reason", ""))
        return True
    print("识别为旧 IMDb Tech Manager Tag：")
    for x in detail.get("old_owned", []): print("  =", x)
    if not detail.get("old_owned"):
        print("  - 无")
    if status == "review":
        print("⚠️ 模型结果需要复核；本次预演不会写 NFO，也不会删除任何旧 Tag。")
        print("AI 候选 Tag：")
        for x in detail.get("new_tags", []): print("  ?", x)
        if detail.get("warnings"):
            print("模型 warnings：", "; ".join(detail["warnings"]))
        if detail.get("review_reasons"):
            print("待复核原因：", "; ".join(detail["review_reasons"]))
        if detail.get("usage"):
            print("Token usage:", json.dumps(detail["usage"], ensure_ascii=False))
        return True
    print("其中本次会替换/删除的旧值：")
    for x in detail.get("replaced_owned", []): print("  -", x)
    if not detail.get("replaced_owned"):
        print("  - 无（旧值若仍由 AI 生成，会保留为最终同值 Tag）")
    print("最终 AI Tag：")
    for x in detail.get("new_tags", []): print("  >", x)
    print("相对当前 NFO 新增：")
    for x in detail.get("added", []): print("  +", x)
    if not detail.get("added"):
        print("  - 无")
    if detail.get("protected"):
        print(f'受 TMM 基线保护的原始 Tag：{len(detail["protected"])} 个')
    if detail.get("warnings"):
        print("模型 warnings：", "; ".join(detail["warnings"]))
    if detail.get("review_reasons"):
        print("⚠️ 待复核：", "; ".join(detail["review_reasons"]))
    if detail.get("usage"):
        print("Token usage:", json.dumps(detail["usage"], ensure_ascii=False))
    return True


def ai_preview(limit=3, selected_paths=None):
    ok, msg = ai_ready(require_enabled=True)
    if not ok:
        print("❌ " + msg, file=sys.stderr)
        return 2
    try:
        configured = _configured_nfo_paths()
    except Exception as e:
        print("❌", e, file=sys.stderr)
        return 2

    if selected_paths:
        allowed = {os.path.realpath(str(p)): p for p in configured}
        paths = []
        for raw in selected_paths[:10]:
            rp = os.path.realpath(str(raw))
            if rp not in allowed:
                print(f"⚠️ 跳过不在当前资料库配置中的 NFO：{raw}")
                continue
            paths.append(allowed[rp])
        title = f"AI 标签指定 NFO 预演（{len(paths)} 个，不写文件）"
    else:
        paths = configured[:]
        title = f"AI 标签迁移预演（最多 {limit} 个 NFO，不写文件）"

    cleanup_mode = ai_config().get("legacy_cleanup_mode", "strict")
    shown = 0
    print(title)
    print("========================================")
    for p in paths:
        if not selected_paths and shown >= limit:
            break
        if _print_ai_preview(p, cleanup_mode, shown + 1):
            shown += 1
    if shown == 0:
        print("没有找到可预演项目。")
    return 0


def _ai_preview_record(p, cleanup_mode):
    """Structured single-NFO preview record for the Manager approval dialog."""
    res = _ai_preview_one(p, cleanup_mode)
    if not res:
        return None
    status, detail, info, old = res
    preview_tags = _preview_tag_entries(detail)
    return {
        "path": str(p),
        "title": info.get("display_title") or info.get("title") or p.stem,
        "imdb": old.get("imdb", ""),
        "status": status,
        "ownership": detail.get("ownership", ""),
        "old_owned": detail.get("old_owned", []),
        "replaced_owned": detail.get("replaced_owned", []),
        "retained_owned": detail.get("retained_owned", []),
        "new_tags": detail.get("new_tags", []),
        "final_tags": detail.get("final_tags", []),
        "preview_tags": preview_tags,
        "added": detail.get("added", []),
        "removed": detail.get("removed", []),
        "warnings": detail.get("warnings", []),
        "review_reasons": detail.get("review_reasons", []),
        "protected": detail.get("protected", []),
        "cache_hit": detail.get("cache_hit", False),
        "usage": detail.get("usage", {}),
        "output_language": ai_config().get("output_language", "zh-CN"),
    }


def _preview_tag_entries(detail):
    """Return final preview order with authoritative generated/existing roles."""
    generated = {
        _canon_tag_value(value)
        for value in detail.get("new_tags", [])
        if clean(value)
    }
    final_values = detail.get("final_tags") or detail.get("new_tags") or []
    return [
        {
            "value": clean(value),
            "kind": "generated" if _canon_tag_value(value) in generated else "existing",
        }
        for value in _dedupe_clean(final_values)
    ]


def ai_preview_write_results(paths):
    """Run AI preview for the given paths and persist structured results.

    The model output lands in AI_CACHE; the Manager approval flow then writes
    it through ai_approve_write without calling the provider again.
    """
    ok, msg = ai_ready(require_enabled=True)
    if not ok:
        # Never leave the previous run's results behind: a failed preview
        # must not be presentable (or approvable) in the UI.
        save_json(PREVIEW_RESULTS, {"schema": 2, "engine": "ai", "created_at": _utc_now(), "records": [], "skipped": [], "error": msg, "failed": True})
        print("❌ " + msg, file=sys.stderr)
        return 2
    records = []
    skipped = []
    cleanup_mode = ai_config().get("legacy_cleanup_mode", "strict")
    for raw in paths:
        try:
            p = _allowed_nfo_path(str(raw))
            record = _ai_preview_record(p, cleanup_mode)
        except Exception as exc:
            skipped.append({"path": str(raw), "error": str(exc)})
            print("❌ %s: %s" % (raw, exc), flush=True)
            continue
        if record is None:
            skipped.append({"path": str(p), "error": "没有可供 AI Tag 预演的 Technical Specs"})
            continue
        records.append(record)
        print("✅ 预演完成：%s（%d 个候选标签）" % (record["title"], len(record["new_tags"])), flush=True)
    save_json(PREVIEW_RESULTS, {
        "schema": 2,
        "engine": "ai",
        "created_at": _utc_now(),
        "records": records,
        "skipped": skipped,
        "failed": not records,
        "error": "" if records else "没有成功预演的项目",
    })
    print("预演结果已保存：%d 成功，%d 跳过。可以在预演面板中审核并批准写入。" % (len(records), len(skipped)), flush=True)
    return 0 if records else 1


def _local_preview_record(p, cleanup_mode):
    """Build one deterministic rules preview without modifying the NFO."""
    p = _allowed_nfo_path(str(p))
    raw = p.read_bytes()
    text = raw.decode("utf-8-sig")
    info = inspect_nfo(text)
    old = existing_tech_object(text)
    if not info or not old or old.get("status") == "empty":
        return None
    if not any(old.get("specs", {}).get(section) for section in TAG_SECTIONS):
        return None
    existing = _ai_existing_tags(old) or None
    tag_result = resolve_tag_entries(old, mode="local", existing=existing)
    status = rewrite(p, old, info, tag_mode="local", cleanup_mode=cleanup_mode, dry_run=True, precomputed=tag_result)
    detail = dict(LAST_REWRITE_DETAIL)
    return {
        "path": str(p),
        "title": info.get("display_title") or info.get("title") or p.stem,
        "imdb": old.get("imdb", ""),
        "engine": "local-rules",
        "source_hash": _source_hash(raw),
        "spec_hash": _specs_hash(old.get("specs", {})),
        "rules_version": LOCAL_RULES_VERSION,
        "cleanup_mode": cleanup_mode,
        "tag_result": tag_result,
        "status": status,
        "reason": detail.get("reason", ""),
        "ownership": detail.get("ownership", ""),
        "old_owned": detail.get("old_owned", []),
        "replaced_owned": detail.get("replaced_owned", []),
        "retained_owned": detail.get("retained_owned", []),
        "new_tags": detail.get("new_tags", []),
        "final_tags": detail.get("final_tags", []),
        "preview_tags": _preview_tag_entries(detail),
        "added": detail.get("added", []),
        "removed": detail.get("removed", []),
        "warnings": detail.get("warnings", []),
        "review_reasons": detail.get("review_reasons", []),
        "protected": detail.get("protected", []),
    }


def local_preview_write_results(paths):
    """Persist deterministic rules previews for the shared approval panel."""
    records = []
    skipped = []
    cleanup_mode = ai_config().get("legacy_cleanup_mode", "strict")
    for raw in paths:
        try:
            record = _local_preview_record(Path(str(raw)), cleanup_mode)
        except Exception as exc:
            skipped.append({"path": str(raw), "error": str(exc)})
            print("❌ %s: %s" % (raw, exc), flush=True)
            continue
        if record is None:
            skipped.append({"path": str(raw), "error": "没有可供规则生成的 Technical Specs"})
            continue
        if record.get("status") == "unsafe-skip":
            skipped.append({
                "path": record.get("path", str(raw)),
                "title": record.get("title", ""),
                "error": record.get("reason") or "无法安全确认旧标签所有权，NFO 未改",
            })
            print("⚠️ 规则试写跳过：%s（%s）" % (record.get("title") or raw, skipped[-1]["error"]), flush=True)
            continue
        records.append(record)
        print("✅ 规则试写完成：%s（%d 个候选标签）" % (record["title"], len(record["new_tags"])), flush=True)
    save_json(PREVIEW_RESULTS, {
        "schema": 2,
        "engine": "local-rules",
        "created_at": _utc_now(),
        "records": records,
        "skipped": skipped,
        "failed": not records,
        "error": "" if records else "没有成功试写的项目",
    })
    print("规则试写结果已保存：%d 成功，%d 跳过。" % (len(records), len(skipped)), flush=True)
    return 0 if records else 1


def local_approve_write(paths):
    """Write only the exact rules results shown in the latest preview."""
    preview = load_json(PREVIEW_RESULTS, {}) or {}
    if preview.get("engine") != "local-rules":
        print("❌ 没有可采纳的规则试写结果；请先点击试写。", file=sys.stderr)
        return 2
    records = {
        os.path.realpath(str(record.get("path") or "")): record
        for record in preview.get("records", []) if isinstance(record, dict) and record.get("path")
    }
    cleanup_mode = ai_config().get("legacy_cleanup_mode", "strict")
    done = failed = 0
    for raw in paths:
        try:
            p = _allowed_nfo_path(str(raw))
            record = records.get(os.path.realpath(str(p)))
            if not record:
                raise ValueError("此 NFO 不在最近一次规则试写结果中")
            current_raw = p.read_bytes()
            if record.get("source_hash") != _source_hash(current_raw):
                raise EditConflictError("NFO 在试写后发生变化，请重新试写")
            text = current_raw.decode("utf-8-sig")
            info = inspect_nfo(text)
            old = existing_tech_object(text)
            if not info or not old:
                raise ValueError("NFO 不可解析或缺少 Technical Specs")
            if record.get("spec_hash") != _specs_hash(old.get("specs", {})):
                raise EditConflictError("Technical Specs 在试写后发生变化，请重新试写")
            if record.get("rules_version") != LOCAL_RULES_VERSION:
                raise ValueError("规则版本已变化，请重新试写")
            if record.get("cleanup_mode") != cleanup_mode:
                raise ValueError("标签清理策略已变化，请重新试写")
            tag_result = record.get("tag_result")
            if not isinstance(tag_result, dict) or tag_result.get("engine") != "local-rules" or not isinstance(tag_result.get("entries"), list):
                raise ValueError("规则试写结果无效，请重新试写")
            status = rewrite(p, old, info, tag_mode="local", cleanup_mode=cleanup_mode, dry_run=False, precomputed=tag_result)
            if status in ("updated", "current", "planned"):
                done += 1
                with contextlib.suppress(Exception):
                    refresh_path_summary(p)
                print("✅ 已采纳规则试写结果：%s（%s）" % (p.stem, status), flush=True)
            else:
                failed += 1
                print("⚠️ %s：%s" % (p, status), flush=True)
        except Exception as exc:
            failed += 1
            print("❌ %s: %s" % (raw, exc), flush=True)
    print("规则试写采纳完成：%d 成功，%d 失败。" % (done, failed), flush=True)
    return 0 if failed == 0 else 1


def ai_approve_write(paths):
    """Write approved AI preview results without another model request."""
    ok, msg = ai_ready(require_enabled=True)
    if not ok:
        print("❌ " + msg, file=sys.stderr)
        return 2
    cfg = ai_config()
    cleanup_mode = cfg.get("legacy_cleanup_mode", "strict")
    done = failed = 0
    for raw in paths:
        try:
            p = _allowed_nfo_path(str(raw))
            text = p.read_bytes().decode("utf-8-sig")
            info = inspect_nfo(text)
            old = existing_tech_object(text)
            if not info or not old:
                raise ValueError("NFO 不可解析或缺少 Technical Specs")
            specs = _ai_input_specs(old.get("specs", {}))
            if not specs:
                raise ValueError("没有可生成标签的 Technical Specs")
            key = _ai_cache_key(old.get("specs", {}), cfg, _ai_existing_tags(old) or None)
            cached = load_json(AI_CACHE / f"{key}.json", {}) or {}
            if not isinstance(cached, dict) or cached.get("cache_schema") != AI_CACHE_SCHEMA or not isinstance(cached.get("result"), dict):
                raise ValueError("没有对应的预演结果；请先运行 AI 预演再批准写入")
            result = _validate_ai_result(cached["result"], old.get("specs", {}), cfg.get("output_language", "zh-CN"))
            tag_result = {
                "entries": result["tags"], "engine": "ai",
                "model": cached.get("model") or cfg.get("model", ""),
                "prompt_hash": hashlib.sha256(_effective_ai_prompt(cfg).encode("utf-8")).hexdigest()[:16],
                "spec_hash": _specs_hash(old.get("specs", {})),
                "warnings": result.get("warnings", []),
                "review_reasons": [], "review_required": False,
                "usage": {}, "cost": 0.0,
                "cached_usage": cached.get("usage", {}), "cached_cost": float(cached.get("cost", 0) or 0),
                "cache_hit": True, "write_manifest": True, "state": "current",
            }
            status = rewrite(p, old, info, tag_mode="ai", cleanup_mode=cleanup_mode, dry_run=False, precomputed=tag_result)
            detail = dict(LAST_REWRITE_DETAIL)
            if status in ("updated", "current", "planned"):
                done += 1
                _failure_remove(p)
                with contextlib.suppress(Exception):
                    refresh_path_summary(p)
                print("✅ 已按预演结果写入：%s（%s）" % (p.stem, status), flush=True)
            else:
                failed += 1
                print("⚠️ %s：%s" % (p, status), flush=True)
        except Exception as exc:
            failed += 1
            print("❌ %s: %s" % (raw, exc), flush=True)
    print("批准写入完成：%d 成功，%d 失败（未再次调用 AI）。" % (done, failed), flush=True)
    return 0 if failed == 0 else 1


def ai_migrate():
    return tag_generate("ai", force_all=False)


def doctor():
    cfg = load_json(CFG, {}) or {}
    roots = [r for r in cfg.get("roots", []) if isinstance(r, str) and r.strip()]
    discovered = discover_roots()

    print("IMDb Technical Specs - Mac 诊断")
    print("================================")
    print("FORMAT_VERSION:", FORMAT_VERSION)
    print("CACHE_VERSION:", CACHE_VERSION)
    print("Python:", sys.executable)
    print("系统 WebKit Helper:", webkit_helper() or "未找到（仅源码直接运行时允许；App 包必须包含）")
    print("可选 Chrome/Chromium 回退:", chrome() or "未安装（不影响 App 与系统 WebKit 抓取）")
    print("配置文件:", CFG)
    print("缓存目录:", CACHE)
    print()
    print("Manager 已配置资料库（后台只使用这些）：")
    for r in roots:
        print("  ", "[在线]" if os.path.isdir(os.path.expanduser(r)) else "[离线]", r)
    if not roots:
        print("   (未配置)")
    print()
    print("TMM 当前可发现候选（仅供参考，不会自动加入）：")
    for r in discovered:
        print("  ", "[在线]" if os.path.isdir(os.path.expanduser(r)) else "[离线]", r)
    if not discovered:
        print("   (无)")
    print()
    try:
        cache_count = len(list(CACHE.glob("tt*.json"))) if CACHE.is_dir() else 0
    except Exception:
        cache_count = 0
    print("IMDb 缓存条目：", cache_count)
    print("运行锁：", LOCK)
    return 0

def self_test():
    """Offline smoke test for helpers/parser/debug writers used by releases."""
    required = [
        "_save_debug",
        "_save_chrome_stderr",
        "fetch_webkit",
        "fetch_chrome",
        "parse_next_data_specs",
        "extract_specs",
        "inspect_nfo",
        "rewrite",
    ]
    missing = [name for name in required if not callable(globals().get(name))]
    if missing:
        print("❌ 引擎缺少函数：" + ", ".join(missing))
        return 2

    title = {
        "runtimes": {
            "edges": [{
                "node": {
                    "displayableProperty": {"value": {"plainText": "2h 22m"}},
                    "seconds": 8520,
                }
            }]
        },
        "technicalSpecifications": {
            "cameras": {"items": [{"camera": "Arriflex 35-IIA"}]},
            "aspectRatios": {"items": [{"aspectRatio": "2.35 : 1"}]},
        },
    }
    payload = json.dumps({"props": {"pageProps": {"title": title}}})
    html_src = (
        '<html><body><script id="__NEXT_DATA__" type="application/json">'
        + payload
        + '</script></body></html>'
    )
    specs, structured = parse_next_data_specs(html_src)
    if not structured or specs.get("Runtime") != ["2h 22m (142 min)"] or specs.get("Camera") != ["Arriflex 35-IIA"]:
        print("❌ __NEXT_DATA__ 解析自检失败")
        return 3

    camera_case = (
        "Arri Alexa Mini LF, Panavision C-, D-, E- and H-Series Lenses "
        "(some scenes), Zeiss Ultra Prime and Angenieux Optimo Lenses"
    )
    expected_camera_tags = [
        "Arri Alexa Mini LF",
        "Panavision C-Series Lenses",
        "Panavision D-Series Lenses",
        "Panavision E-Series Lenses",
        "Panavision H-Series Lenses",
        "Zeiss Ultra Prime Lenses",
        "Angenieux Optimo Lenses",
    ]
    if camera_atomic_tags(camera_case) != expected_camera_tags:
        print("❌ Camera 标签断句/共享 Series 展开自检失败")
        return 4

    # Ownership manifest must never be mistaken for TMM's root-level <tag>.
    ownership_xml = (
        '<movie><uniqueid type="imdb">tt0000001</uniqueid>'
        '<tag>Marvel</tag>'
        '<technicalspecs source="IMDb" imdbid="tt0000001" fetched="2026-01-01T00:00:00+00:00" formatVersion="13">'
        '<section name="Camera"><item>Arri Alexa 65</item></section>'
        '<generatedtags owner="IMDb Tech Manager" engine="ai"><tag field="Camera" sourceIndexes="0">Arri Alexa 65</tag></generatedtags>'
        '</technicalspecs></movie>'
    )
    if _all_normal_tags(ownership_xml) != ["Marvel"]:
        print("❌ AI ownership manifest 与 TMM 根级 Tag 隔离自检失败")
        return 5

    # Old 1.1.x migration safety: a pristine backup is the protection baseline.
    with tempfile.TemporaryDirectory(prefix="imdb-tech-selftest-") as td:
        nfo = Path(td) / "movie.nfo"
        current = (
            '<movie><uniqueid type="imdb">tt0000001</uniqueid>'
            '<tag>Marvel</tag><tag>Arri Alexa 65</tag>'
            '<technicalspecs source="IMDb" imdbid="tt0000001" fetched="2026-01-01T00:00:00+00:00" formatVersion="12">'
            '<section name="Camera"><item>Arri Alexa 65</item></section>'
            '</technicalspecs></movie>'
        )
        pristine = '<movie><uniqueid type="imdb">tt0000001</uniqueid><tag>Marvel</tag></movie>'
        nfo.write_text(current, encoding="utf-8")
        Path(str(nfo) + ".imdbtech.bak").write_text(pristine, encoding="utf-8")
        protected, baseline = protected_tmm_tags(nfo, "tt0000001")
        if protected != ["Marvel"] or not baseline:
            print("❌ 旧 NFO 的 TMM Tag 基线保护自检失败")
            return 6

    # v2 Stage 1 must persist specs without changing a single root-level Tag.
    with tempfile.TemporaryDirectory(prefix="imdb-tech-v2-spec-" ) as td:
        nfo = Path(td) / "stage1.nfo"
        before = '<movie><title>Demo</title><uniqueid type="imdb">tt0000002</uniqueid><tag>Marvel</tag><tag>Dolby Atmos</tag></movie>'
        nfo.write_text(before, encoding="utf-8")
        info = inspect_nfo(before)
        source = {
            "imdb": "tt0000002", "fetched_at": "2026-08-18T00:00:00+00:00",
            "url": "https://www.imdb.com/title/tt0000002/technical/",
            "status": "ok", "specs": {k: [] for k in SECTIONS}, "ok": True, "ready": True,
        }
        source["specs"]["Camera"] = ["Arri Alexa 65, Arri Prime DNA Lenses"]
        if rewrite_specs_only(nfo, source, info) != "updated":
            print("❌ v2 Stage 1 Spec-only 写入自检失败")
            return 7
        after = nfo.read_text(encoding="utf-8")
        if _all_normal_tags(after) != ["Marvel", "Dolby Atmos"] or not nfo_spec_is_ready(after, "tt0000002"):
            print("❌ v2 Stage 1 错误修改了 TMM Tag 或 Spec 未 Ready")
            return 8

    # Confirmed IMDb-empty is a completed Spec state, not a permanent retry.
    empty_obj = {
        "imdb": "tt0000003", "fetched_at": "2026-08-18T00:00:00+00:00",
        "url": "https://www.imdb.com/title/tt0000003/technical/", "status": "empty",
        "specs": {k: [] for k in SECTIONS}, "ok": True, "ready": True,
    }
    empty_xml = '<movie><uniqueid type="imdb">tt0000003</uniqueid>' + technical_block(empty_obj, "\n", "movie", "", None) + '</movie>'
    parsed_empty = existing_tech_object(empty_xml)
    if not parsed_empty or parsed_empty.get("status") != "empty" or not parsed_empty.get("ready"):
        print("❌ IMDb empty Spec Ready 状态自检失败")
        return 9

    # Sidecar ownership survives a TMM rewrite that removes our embedded manifest.
    global OWNERSHIP_DIR, APP
    old_ownership_dir = OWNERSHIP_DIR
    with tempfile.TemporaryDirectory(prefix="imdb-tech-v2-own-") as td:
        OWNERSHIP_DIR = Path(td) / "ownership"
        nfo = Path(td) / "owned.nfo"
        spec = {
            "imdb": "tt0000004", "fetched_at": "2026-08-18T00:00:00+00:00",
            "url": "https://www.imdb.com/title/tt0000004/technical/", "status": "ok",
            "specs": {k: [] for k in SECTIONS}, "ok": True, "ready": True,
        }
        spec["specs"]["Camera"] = ["Arri Alexa 65"]
        bare = '<movie><uniqueid type="imdb">tt0000004</uniqueid><tag>Arri Alexa 65</tag>' + technical_block(spec, "\n", "movie", "", None) + '</movie>'
        nfo.write_text(bare, encoding="utf-8")
        save_ownership_record(nfo, "tt0000004", {
            "engine": "local-rules", "model": LOCAL_RULES_VERSION, "spec_hash": _specs_hash(spec["specs"]),
            "state": "current", "entries": [{"value":"Arri Alexa 65","field":"Camera","source_indexes":[0],"confidence":"high","operation":"local-rule"}],
        })
        if tag_state(bare, existing_tech_object(bare), nfo) != "local-current":
            print("❌ Sidecar ownership 恢复自检失败")
            OWNERSHIP_DIR = old_ownership_dir
            return 10
    OWNERSHIP_DIR = old_ownership_dir

    global LAST_CHROME_STDERR, LAST_CHROME_COMMAND
    old_app = APP
    old_stderr = LAST_CHROME_STDERR
    old_commands = LAST_CHROME_COMMAND
    try:
        with tempfile.TemporaryDirectory(prefix="imdb-tech-v2-debug-") as td:
            # Release self-tests must never require or mutate the user's real
            # Application Support directory.
            APP = Path(td)
            LAST_CHROME_STDERR = "self-test stderr"
            LAST_CHROME_COMMAND = [["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", "--dump-dom", "https://example.invalid/"]]
            debug_path = _save_chrome_stderr("tt0000000", "self-test")
            if not debug_path or not Path(debug_path).is_file():
                print("❌ Chrome stderr 诊断写入自检失败")
                return 11
    finally:
        APP = old_app
        LAST_CHROME_STDERR = old_stderr
        LAST_CHROME_COMMAND = old_commands

    print("✅ Mac Engine 自检通过：IMDb 解析、Camera、本地/TMM Tag 隔离、Spec-only、empty Ready、sidecar ownership、诊断写入正常。")
    return 0


def refresh_selected(paths):
    """Explicit path-scoped Spec refresh; never generates root tags."""
    if not isinstance(paths, list) or not paths:
        raise ValueError("请选择至少一个 NFO")
    counts = {}
    total = len(paths)
    done = 0
    _progress_clear()
    _progress_write("refresh-selected", 0, total, "", [])
    for raw in paths:
        path = _allowed_nfo_path(str(raw))
        done += 1
        _progress_write("refresh-selected", done, total, str(path), [])
        result = process(path, force=True, only_missing=False, verbose=True)
        if result == "spec-updated":
            with contextlib.suppress(Exception):
                refresh_path_summary(path)
        counts[result] = counts.get(result, 0) + 1
    print("完成：" + "，".join(f"{k}={v}" for k, v in sorted(counts.items())))
    return 0


def _index_cache_put_item(item):
    """Refresh one cached summary from a freshly returned inspector detail."""
    if not isinstance(item, dict) or not item.get("path"):
        return
    refresh_path_summary(item["path"], item=item)


def _index_cache_invalidate(paths=None):
    if isinstance(paths, list) and paths:
        for value in paths:
            _index_cache_drop(value)
        return
    _INDEX_CACHE_MEM["stamp"] = ""
    _INDEX_CACHE_MEM["items"] = {}
    save_json(INDEX_CACHE, {"schema": INDEX_CACHE_SCHEMA, "stamp": "", "items": {}})


def _serve_dispatch(cmd, req):
    req = req if isinstance(req, dict) else {}
    payload = req.get("payload") if isinstance(req.get("payload"), dict) else {}
    if cmd == "ping":
        return {"pong": True, "schema": 1}
    if cmd == "metrics":
        return {"metrics": metrics_snapshot()}
    if cmd == "library-index":
        # The resident RPC must answer from the last reliable snapshot.  A
        # cold NAS scan belongs to the explicit reconcile job and must never
        # block the UI request.
        return library_index(allow_cold_scan=False)
    if cmd == "library-changes":
        return library_changes(req.get("since", 0))
    if cmd == "inspector-detail":
        return {"item": inspector_detail(req.get("path"))}
    if cmd == "inspector-edit":
        result = edit_nfo(payload)
        _index_cache_put_item(result.get("item"))
        return result
    if cmd == "inspector-undo":
        result = undo_nfo(payload)
        _index_cache_put_item(result.get("item"))
        return result
    if cmd == "inspector-issues":
        result = acknowledge_issue(payload)
        _index_cache_put_item(result.get("item"))
        return result
    if cmd == "status-override":
        result = set_status_override(payload)
        _index_cache_put_item(result.get("item"))
        return result
    if cmd == "scope-preflight":
        return scope_preflight(payload)
    if cmd == "preview-candidates":
        items, error = _preview_candidate_items()
        if error:
            raise RuntimeError(error)
        return {"items": items}
    if cmd == "reload":
        paths_value = payload.get("paths")
        if not isinstance(paths_value, list) or not paths_value:
            raise ValueError("reload 请求必须包含 paths 列表")
        return {"items": _catalog_reload_paths(paths_value)}
    if cmd == "invalidate":
        _index_cache_invalidate(req.get("paths"))
        return {"ok": True}
    raise ValueError("未知 serve 命令：" + clean(str(cmd)))


def serve():
    """Resident JSON-RPC mode for the Go Core (line-delimited JSON on stdio).

    The Go manager keeps one engine process alive, removing per-request
    interpreter startup from inspector/preflight calls. All engine chatter
    is redirected to stderr so stdout stays a clean protocol channel.
    """
    protocol_stdout = sys.stdout
    sys.stdout = sys.stderr

    def emit(obj):
        protocol_stdout.write(json.dumps(obj, ensure_ascii=False) + "\n")
        protocol_stdout.flush()

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            if not isinstance(req, dict):
                raise ValueError("serve 请求必须是 JSON 对象")
        except Exception as exc:
            emit({"id": None, "ok": False, "error": "invalid request: %s" % exc, "kind": "ValueError"})
            continue
        rid = req.get("id")
        cmd = clean(str(req.get("cmd") or ""))
        try:
            result = _serve_dispatch(cmd, req)
            emit({"id": rid, "ok": True, "result": result})
        except Exception as exc:
            emit({"id": rid, "ok": False, "error": str(exc), "kind": exc.__class__.__name__})
    return 0


def main():
    APP.mkdir(parents=True, exist_ok=True)

    p = argparse.ArgumentParser()
    p.add_argument("--configure", action="store_true")
    p.add_argument("--watch-once", action="store_true")
    p.add_argument("--auto", action="store_true")
    p.add_argument("--backfill", action="store_true")
    p.add_argument("--rebuild-all", action="store_true")
    p.add_argument("--refresh-all", action="store_true")
    p.add_argument("--refresh-paths-json", default="")
    p.add_argument("--test-imdb")
    p.add_argument("--doctor", action="store_true")
    p.add_argument("--self-test", action="store_true")
    p.add_argument("--ai-test", action="store_true")
    p.add_argument("--ai-recover", action="store_true")
    p.add_argument("--ai-scan", action="store_true")
    p.add_argument("--ai-preview", action="store_true")
    p.add_argument("--list-ai-preview-candidates", action="store_true")
    p.add_argument("--ai-preview-paths-json", default="")
    p.add_argument("--ai-generate-paths-json", default="")
    p.add_argument("--local-generate-paths-json", default="")
    p.add_argument("--ai-migrate", action="store_true")
    p.add_argument("--pipeline-scan", action="store_true")
    p.add_argument("--local-generate", action="store_true")
    p.add_argument("--local-rebuild", action="store_true")
    p.add_argument("--ai-generate", action="store_true")
    p.add_argument("--ai-rebuild", action="store_true")
    p.add_argument("--ai-resume", action="store_true")
    p.add_argument("--ai-pause-task", action="store_true")
    p.add_argument("--ai-resume-task", action="store_true")
    p.add_argument("--ai-retry-failed", action="store_true")
    p.add_argument("--discover-root-candidates", action="store_true")
    p.add_argument("--library-index", action="store_true")
    p.add_argument("--library-changes-since", default="")
    p.add_argument("--inspector-path", default="")
    p.add_argument("--inspector-edit-json", default="")
    p.add_argument("--inspector-undo-json", default="")
    p.add_argument("--inspector-issues-json", default="")
    p.add_argument("--inspector-reload-json", default="")
    p.add_argument("--status-override-json", default="")
    p.add_argument("--scope-preflight-json", default="")
    p.add_argument("--reconcile-index", action="store_true")
    p.add_argument("--reconcile-index-space", choices=("movies", "tv"), default="")
    p.add_argument("--cache-maintain", action="store_true")
    p.add_argument("--cache-clear", action="store_true")
    p.add_argument("--serve", action="store_true")
    p.add_argument("--ai-preview-write-json", default="")
    p.add_argument("--ai-approve-json", default="")
    p.add_argument("--local-preview-write-json", default="")
    p.add_argument("--local-approve-json", default="")
    a = p.parse_args()

    if a.discover_root_candidates:
        discover_root_candidates_cli()
        return 0
    if a.cache_maintain or a.cache_clear:
        try:
            result = maintain_imdb_cache(clear=a.cache_clear)
            print(json.dumps(result, ensure_ascii=False))
            return 0 if result.get("state") == "ready" else 2
        except Exception as e:
            print(json.dumps({"state": "failed", "error": str(e)}, ensure_ascii=False))
            return 2
    if a.serve:
        return serve()
    if a.reconcile_index:
        try:
            counts = _catalog_reconcile(reason="manual", space=a.reconcile_index_space)
            print(json.dumps({"ok": True, "counts": counts}, ensure_ascii=False))
            return 0
        except Exception as e:
            print(json.dumps({"ok": False, "error": str(e)}, ensure_ascii=False))
            return 2
    if a.library_index:
        try:
            print(json.dumps(library_index(), ensure_ascii=False))
            return 0
        except Exception as e:
            print(json.dumps({"error": str(e)}, ensure_ascii=False))
            return 2
    if a.library_changes_since:
        try:
            print(json.dumps(library_changes(a.library_changes_since), ensure_ascii=False))
            return 0
        except Exception as e:
            print(json.dumps({"error": str(e)}, ensure_ascii=False))
            return 2
    if a.inspector_path:
        try:
            print(json.dumps({"item": inspector_detail(a.inspector_path)}, ensure_ascii=False))
            return 0
        except Exception as e:
            print(json.dumps({"error": str(e), "kind": e.__class__.__name__}, ensure_ascii=False))
            return 2
    if a.inspector_reload_json:
        try:
            payload = json.loads(a.inspector_reload_json)
            if not isinstance(payload, dict):
                raise ValueError("reload 请求必须是 JSON 对象")
            result = {"items": _catalog_reload_paths(payload.get("paths") or [])}
            print(json.dumps(result, ensure_ascii=False))
            return 0
        except Exception as e:
            print(json.dumps({"error": str(e), "kind": e.__class__.__name__}, ensure_ascii=False))
            return 2
    if a.inspector_edit_json or a.inspector_undo_json or a.inspector_issues_json or a.status_override_json:
        try:
            raw_payload = a.inspector_edit_json or a.inspector_undo_json or a.inspector_issues_json or a.status_override_json
            payload = json.loads(raw_payload)
            if not isinstance(payload, dict):
                raise ValueError("Inspector 请求必须是 JSON 对象")
            if a.inspector_edit_json:
                result = edit_nfo(payload)
            elif a.inspector_undo_json:
                result = undo_nfo(payload)
            elif a.inspector_issues_json:
                result = acknowledge_issue(payload)
            else:
                result = set_status_override(payload)
            print(json.dumps(result, ensure_ascii=False))
            return 0
        except Exception as e:
            print(json.dumps({"error": str(e), "kind": e.__class__.__name__}, ensure_ascii=False))
            return 2
    if a.scope_preflight_json:
        try:
            payload = json.loads(a.scope_preflight_json)
            if not isinstance(payload, dict):
                raise ValueError("Scope 请求必须是 JSON 对象")
            print(json.dumps(scope_preflight(payload), ensure_ascii=False))
            return 0
        except Exception as e:
            print(json.dumps({"error": str(e), "kind": e.__class__.__name__}, ensure_ascii=False))
            return 2
    if a.self_test:
        return self_test()
    if a.ai_recover:
        return ai_recover()
    if a.ai_test:
        return ai_test()
    if a.ai_scan:
        return ai_scan()
    if a.list_ai_preview_candidates:
        return ai_preview_candidates()
    if a.ai_preview_paths_json:
        try:
            selected = json.loads(a.ai_preview_paths_json)
            if not isinstance(selected, list):
                raise ValueError("paths must be a list")
        except Exception as e:
            print("❌ 指定 NFO 列表无效：" + str(e), file=sys.stderr)
            return 2
        return ai_preview(selected_paths=[str(x) for x in selected])
    if a.ai_preview_write_json:
        try:
            selected = json.loads(a.ai_preview_write_json)
            if not isinstance(selected, list) or not selected:
                raise ValueError("paths must be a non-empty list")
            if len(selected) > 10:
                raise ValueError("at most 10 paths")
        except Exception as e:
            print("❌ 指定预演 NFO 列表无效：" + str(e), file=sys.stderr)
            return 2
        return ai_preview_write_results([str(x) for x in selected])
    if a.ai_approve_json:
        try:
            selected = json.loads(a.ai_approve_json)
            if not isinstance(selected, list) or not selected:
                raise ValueError("paths must be a non-empty list")
            if len(selected) > 10000:
                raise ValueError("at most 10000 paths")
        except Exception as e:
            print("❌ 指定批准写入 NFO 列表无效：" + str(e), file=sys.stderr)
            return 2
        with _manual_task_session():
            return ai_approve_write([str(x) for x in selected])
    if a.local_preview_write_json:
        try:
            selected = json.loads(a.local_preview_write_json)
            if not isinstance(selected, list) or not selected:
                raise ValueError("paths must be a non-empty list")
            if len(selected) > 10:
                raise ValueError("at most 10 paths")
        except Exception as e:
            print("❌ 指定规则试写 NFO 列表无效：" + str(e), file=sys.stderr)
            return 2
        return local_preview_write_results([str(x) for x in selected])
    if a.local_approve_json:
        try:
            selected = json.loads(a.local_approve_json)
            if not isinstance(selected, list) or not selected:
                raise ValueError("paths must be a non-empty list")
            if len(selected) > 10000:
                raise ValueError("at most 10000 paths")
        except Exception as e:
            print("❌ 指定规则采纳 NFO 列表无效：" + str(e), file=sys.stderr)
            return 2
        with _manual_task_session():
            return local_approve_write([str(x) for x in selected])
    if a.ai_preview:
        return ai_preview()
    if a.refresh_paths_json:
        try:
            selected = json.loads(a.refresh_paths_json)
            if not isinstance(selected, list) or not selected or len(selected) > 10000:
                raise ValueError("paths must be a non-empty list of at most 10000 items")
        except Exception as e:
            print("❌ 指定刷新 NFO 列表无效：" + str(e), file=sys.stderr)
            return 2
        with _manual_task_session():
            return refresh_selected([str(x) for x in selected])
    if a.ai_generate_paths_json:
        try:
            selected = json.loads(a.ai_generate_paths_json)
            if not isinstance(selected, list) or not selected:
                raise ValueError("paths must be a non-empty list")
            if len(selected) > 10000:
                raise ValueError("at most 10000 paths")
        except Exception as e:
            print("❌ 指定写入 NFO 列表无效：" + str(e), file=sys.stderr)
            return 2
        with _manual_task_session():
            return tag_generate("ai", force_all=False, selected_paths=[str(x) for x in selected])
    if a.local_generate_paths_json:
        try:
            selected = json.loads(a.local_generate_paths_json)
            if not isinstance(selected, list) or not selected:
                raise ValueError("paths must be a non-empty list")
            if len(selected) > 10000:
                raise ValueError("at most 10000 paths")
        except Exception as e:
            print("❌ 指定写入 NFO 列表无效：" + str(e), file=sys.stderr)
            return 2
        with _manual_task_session():
            return tag_generate("local", force_all=False, selected_paths=[str(x) for x in selected])
    if a.ai_migrate:
        return ai_migrate()
    if a.pipeline_scan:
        return pipeline_scan()
    if a.local_generate or a.local_rebuild or a.ai_generate or a.ai_rebuild or a.ai_resume_task or a.ai_retry_failed:
        with _manual_task_session():
            if a.local_generate:
                return tag_generate("local", force_all=False)
            if a.local_rebuild:
                return tag_generate("local", force_all=True)
            if a.ai_generate:
                return tag_generate("ai", force_all=False)
            if a.ai_rebuild:
                return tag_generate("ai", force_all=True)
            if a.ai_resume_task:
                return tag_generate("ai", force_all=False, resume_task=True)
            return tag_generate("ai", force_all=False, retry_failed=True)
    if a.ai_pause_task:
        return request_ai_task_pause()
    if a.ai_resume:
        return ai_resume_cli()
    if a.configure:
        configure()
        return 0
    if a.watch_once:
        return run("watch")
    if a.auto:
        return run("auto")
    if a.backfill or a.rebuild_all or a.refresh_all:
        with _manual_task_session():
            if a.backfill:
                return run("backfill")
            if a.rebuild_all:
                return run("rebuild")
            return run("refresh")
    if a.test_imdb:
        return test_imdb(a.test_imdb)
    if a.doctor:
        return doctor()

    p.print_help()
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
