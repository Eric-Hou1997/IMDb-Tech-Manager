# 原始持久化与迁移契约

状态：源码路径及迁移规则已登记；没有迁移、读取或修改用户真实数据。所有目的模块属于规划，必须在实际迁移测试通过后标记完成。

原路径根：M = `~/Library/Application Support/IMDb Tech Manager`；E = `~/Library/Application Support/tmm-imdb-tech`。

| 数据编号 | 原始产物 | 新版责任及行为契约 | 必须验证 |
| --- | --- | --- | --- |
| ITM-D-01 | M/settings.json、ui-layout.json；E/config.json | settings/migration：合并配置来源时列明优先级；保留独立 Movie/TV 根、过滤、范围、布局和启动偏好；未知字段先保留 | 缺字段、两份配置冲突、旧目录不可读、重复导入；不能默认全库 |
| ITM-D-02 | E/cache/tt*.json、raw-tt*.json、raw-tt*.html.gz、cache-status.json | imdb-cache：保留事实缓存与原始页面、过期/失败状态、容量和解析版本；重解析不自动重抓 | 缓存版本不符、压缩损坏、部分清理、离线和同 IMDb 复用 |
| ITM-D-03 | E/ai-cache、ai-status.json、ai-runtime.json、ai-failure-queue.json | ai-accounting：保留历史 usage、失败输入/配置指纹及重试条件；当前运行成本与历史分开 | 缓存命中不产生本次费用；畸形响应/截断仍按真实 HTTP 记账 |
| ITM-D-04 | E/ai-batch-state.json、ai-batch-queue.json、ai-batch-pause.flag、pipeline-status.json、job-progress.json、manager-status.json、preview-results.json | tasks：导入为明确的历史/中断/待恢复状态；原版预演审批不得无条件跨版本执行 | 中断、重复恢复、输入已改变、任务语言、部分队列损坏 |
| ITM-D-05 | M/task-history.json、logs；E/issue-acknowledgements.json、status-overrides.json、root-health.json、index-cache.json | history/catalog：保留旧日志字节和时间；确认/覆盖继续绑定源 hash；索引重建不改 NFO | 外部文件变化使旧确认失效；离线根不能当作全部删除 |
| ITM-D-06 | 媒体 NFO 内 ownership manifest；E/ownership、undo | ownership/transaction：嵌入归属先于 sidecar；保留权威来源和撤销后置 hash；不以文本猜测归属 | 未知/冲突 unsafe-skip；手动与外部标签保护；旧撤销失效 |
| ITM-D-07 | NFO.imdbtech.bak、NFO.imdbtech.original.bak | transaction：原始备份不可当作可随意覆盖的缓存；仅验证过的撤销/恢复可写媒体 | BOM、换行、权限、不可写盘、备份损坏、并发修改 |
| ITM-D-08 | Keychain service `local.imdb-tech-manager.ai` / account `api-key` | credentials：经系统授权读取已有凭据并验证可用；不可记录明文，不因新 bundle ID 直接清空旧条目 | 新签名身份的 Keychain 授权、用户拒绝、失败回退；本轮未读取真实条目 |
| ITM-D-09 | M/updates/update-state.json；语言包根及 locale/rN 下 manifest/web 等文件 | update/locale：原更新快照不视为新版包授权；语言 descriptor/hash/released_with 继续精确绑定 | 旧缓存过期、错产品/架构、坏签名、语言包不匹配；不迁移旧日志语言 |
| ITM-D-10 | LaunchAgents、M/agent.pid/heartbeat/cycle；E/run.lock、cache-maintenance.lock、manual-task.flag、chrome-profile | lifecycle：进程、锁和浏览器会话不能按文件存在判定为存活；新应用使用嵌入 WebView；旧资源清理必须证明归属 | PID 复用、旧 agent 仍存活、重复启动、升级/回退；浏览器目录不作为业务数据直接导入 |

源码依据：[平台数据根与启动项](../../macos/platform_darwin.go#L22)、[引擎文件与 Schema](../../macos/engine/mac-engine.py#L9)、[Manager 文件](../../macos/main.go#L2191)、[历史记录](../../macos/main.go#L1630)、[Keychain](../../macos/platform_darwin.go#L1109)、[语言包路径](../../macos/language_packs.go#L138)。具体字段见 entrypoints.json 的 serialized-field；这些路径分组不替代字段级语义和真实升级验收。

导入协议需要先读取快照并校验，生成候选与备份，事务提交后才切换新数据根。失败保留旧数据可启动；旧进程未退出时禁止双写。跨平台路径映射需要用户可见核对，不能简单替换斜线。
