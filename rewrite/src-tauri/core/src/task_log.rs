//! The original task drawer's persisted log and read-only job presentation.
use crate::batch::{BatchEngine, BatchMode};
use crate::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct TaskJournal {
    pub started_at: String,
    pub ended_at: String,
    pub text: String,
    #[serde(default)]
    pub language_pack_revision: u32,
    #[serde(default)]
    pub language_catalog_hash: String,
    #[serde(default)]
    pub messages: std::collections::BTreeMap<String, String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct TaskJob {
    pub job_id: String,
    pub running: bool,
    pub action: String,
    pub started_at: String,
    pub ended_at: String,
    pub exit_code: i32,
    pub message: String,
    pub language: String,
    pub language_pack_revision: u32,
    pub log: String,
}
pub fn action(task: &Task) -> &'static str {
    match task.batch.as_ref().map(|b| (&b.engine, &b.mode)) {
        None => "reconcile-index",
        Some((BatchEngine::Specs, _)) => "refresh-selected",
        Some((BatchEngine::Ai, BatchMode::Preview)) => "ai-preview-write-selected",
        Some((BatchEngine::Rules, BatchMode::Preview)) => "local-preview-write-selected",
        Some((BatchEngine::Ai, BatchMode::AdoptPreview)) => "ai-approve-selected",
        Some((BatchEngine::Rules, BatchMode::AdoptPreview)) => "local-approve-selected",
        Some((BatchEngine::Ai, _)) => "ai-generate-selected",
        Some((BatchEngine::Rules, _)) => "local-generate-selected",
    }
}
pub fn state_message(task: &Task) -> String {
    let (cn, tw, en) = match task.state {
        TaskState::Requested => ("等待执行", "等待執行", "Waiting"),
        TaskState::Running => ("运行中", "執行中", "Running"),
        TaskState::Paused => ("已暂停", "已暫停", "Paused"),
        TaskState::Interrupted => ("执行中断", "執行中斷", "Interrupted"),
        TaskState::Completed => ("完成", "完成", "Completed"),
        TaskState::Failed => ("失败", "失敗", "Failed"),
        TaskState::Cancelled => ("已取消", "已取消", "Cancelled"),
    };
    task.localized(cn, tw, en)
}
impl Task {
    pub(crate) fn localized(&self, cn: &str, tw: &str, en: &str) -> String {
        match self.locale {
            Locale::Simplified => cn.into(),
            Locale::Traditional => tw.into(),
            _ => self
                .journal
                .as_ref()
                .and_then(|j| j.messages.get(&crate::languages::stable_id(en)))
                .cloned()
                .unwrap_or_else(|| en.into()),
        }
    }
    pub(crate) fn begin_log(&mut self) {
        let now = ai::job::now();
        let labels = [
            self.localized("任务", "任務", "Job"),
            self.localized("操作", "操作", "Action"),
            self.localized("开始", "開始", "Started"),
        ];
        let mut journal = self.journal.take().unwrap_or_default();
        journal.started_at = now.clone();
        journal.ended_at.clear();
        journal.text = format!(
            "IMDb Tech Manager v{}\n{}{}{}\n{}{}{}\n{}{}{}\n\n",
            env!("CARGO_PKG_VERSION"),
            labels[0],
            if matches!(self.locale, Locale::Simplified | Locale::Traditional) {
                "："
            } else {
                ": "
            },
            self.id,
            labels[1],
            if matches!(self.locale, Locale::Simplified | Locale::Traditional) {
                "："
            } else {
                ": "
            },
            action(self),
            labels[2],
            if matches!(self.locale, Locale::Simplified | Locale::Traditional) {
                "："
            } else {
                ": "
            },
            now
        );
        self.journal = Some(journal);
    }
    pub(crate) fn log_line(&mut self, line: &str) {
        let journal = self.journal.get_or_insert_with(TaskJournal::default);
        journal.text.push_str(line);
        journal.text.push('\n');
        // Match the original current-log tail without splitting UTF-8 bytes.
        if journal.text.len() > 40_000 {
            let mut start = journal.text.len() - 40_000;
            while !journal.text.is_char_boundary(start) {
                start += 1;
            }
            journal.text.drain(..start);
        }
    }
    /// Human-readable log values only; protocol fields and NFO facts stay intact.
    pub(crate) fn result_label(&self, phase: &str) -> String {
        let (cn, tw, en) = match phase {
            "pending" | "requested" => ("等待执行", "等待執行", "Waiting"),
            "running" | "dispatched" => ("运行中", "執行中", "Running"),
            "paused" => ("已暂停", "已暫停", "Paused"),
            "interrupted" => ("执行中断", "執行中斷", "Interrupted"),
            "cancelled" => ("已取消", "已取消", "Cancelled"),
            "failed" => ("失败", "失敗", "Failed"),
            "committed" | "completed" => ("完成", "完成", "Completed"),
            "unchanged" => ("未变化", "未變更", "Unchanged"),
            "review-ready" | "review" => ("待复核", "待複核", "Needs Review"),
            "spec-missing" => ("缺 Spec", "缺 Spec", "Missing Specs"),
            "spec-empty" => ("无数据", "無資料", "No Data"),
            "ai-complete" => ("AI 完成", "AI 完成", "AI complete"),
            "local-complete" => ("规则完成", "規則完成", "Rules complete"),
            "no-tags" => ("Spec 就绪", "Spec 就緒", "Specs Ready"),
            "stale" => ("待同步", "待同步", "Out of Sync"),
            "tag-missing" => ("Tag 缺失", "Tag 缺失", "Missing Tags"),
            "legacy" => ("旧数据", "舊資料", "Legacy Data"),
            "manual-spec" => ("人工规格", "人工規格", "Manual Specs"),
            "xml-error" => ("XML 解析错误", "XML 解析錯誤", "XML Parse Error"),
            "read-error" => ("NFO 读取错误", "NFO 讀取錯誤", "NFO Read Error"),
            "not-applicable" => ("不适用", "不適用", "Not Applicable"),
            "index-refresh-required" => ("索引待刷新", "索引待重新整理", "Index refresh required"),
            value if value.starts_with("skipped-") => ("已跳过", "已略過", "Skipped"),
            _ => return phase.into(),
        };
        // Retain distinct skip reasons and transaction phases as diagnostic codes.
        format!("{} [{phase}]", self.localized(cn, tw, en))
    }
    pub(crate) fn error_message(&self, error: &AppError) -> String {
        let (cn, tw, en) = match error.code.as_str() {
            "auth" => ("AI 认证失败，请检查 API Key 并测试连接后恢复。", "AI 認證失敗，請檢查 API Key 並測試連線後恢復。", "AI authentication failed. Check the API Key and test the connection before resuming."),
            "quota" => ("AI 额度不足，请检查供应商额度并测试连接后恢复。", "AI 額度不足，請檢查供應商額度並測試連線後恢復。", "AI quota is exhausted. Check the provider quota and test the connection before resuming."),
            "rate-limit" => ("AI 请求限流，请等待限流解除并测试连接后恢复。", "AI 請求限流，請等待限流解除並測試連線後恢復。", "AI requests are rate limited. Wait for the limit to clear and test the connection before resuming."),
            "budget-exhausted" => ("本次 AI 预算已用尽，已完成结果保留；请检查运行预算后恢复。", "本次 AI 預算已用盡，已完成結果保留；請檢查執行預算後恢復。", "The AI run budget is exhausted. Completed results are preserved; review the run budget before resuming."),
            "output-truncated" => ("AI 输出被截断，请提高输出上限后显式重试。", "AI 輸出被截斷，請提高輸出上限後明確重試。", "AI output was truncated. Increase the output limit before an explicit retry."),
            "malformed-json" | "schema-invalid" => ("AI 输出 JSON 或标签结构无效，请检查模型响应后显式重试。", "AI 輸出 JSON 或標籤結構無效，請檢查模型回應後明確重試。", "AI output JSON or Tag structure is invalid. Inspect the model response before an explicit retry."),
            "source-changed" | "source-hash-mismatch" | "nfo-changed" | "source-conflict" => ("NFO 已在审核后改变，请刷新并重新审核后再写入。", "NFO 已在審核後變更，請重新整理並再次審核後再寫入。", "The NFO changed after review. Refresh and review it again before writing."),
            "unsafe-skip" | "ownership-mismatch" => ("安全校验拒绝本次写入，请刷新 Inspector 并检查受影响的 NFO。", "安全驗證拒絕本次寫入，請重新整理 Inspector 並檢查受影響的 NFO。", "Safety validation rejected this write. Refresh Inspector and inspect the affected NFO."),
            "nfo-read" | "read-error" | "path-unavailable" | "read-failed" => ("NFO 无法读取，请检查资料库挂载状态与读取权限后重试。", "NFO 無法讀取，請檢查資料庫掛載狀態與讀取權限後重試。", "The NFO cannot be read. Check the library mount and read permissions before retrying."),
            "xml-error" | "invalid-xml" | "nfo-xml" => ("NFO XML 无效，请修复文件并刷新索引后重试。", "NFO XML 無效，請修復檔案並重新整理索引後重試。", "NFO XML is invalid. Repair the file and refresh the index before retrying."),
            "paused" => ("AI 已暂停，请测试连接后恢复。", "AI 已暫停，請測試連線後恢復。", "AI is paused. Test the connection before resuming."),
            "cancelled" => ("已取消", "已取消", "Cancelled"),
            "transient" | "request" => ("AI 网络请求失败，请检查网络或代理后重试。", "AI 網路請求失敗，請檢查網路或代理後重試。", "The AI network request failed. Check the network or proxy before retrying."),
            _ => {
                // Unknown/native diagnostics remain lossless and structured;
                // translate an exact original catalog message when available.
                return format!("{}: {}", error.code, self.localized(&error.message, &error.message, &error.message));
            }
        };
        format!(
            "{} [{}]{}",
            self.localized(cn, tw, en),
            error.code,
            error
                .path
                .as_ref()
                .map(|p| format!(" · {p}"))
                .unwrap_or_default()
        )
    }
    pub(crate) fn log_state(&mut self, previous: &TaskState) {
        if previous != &self.state {
            let now = ai::job::now();
            let journal = self.journal.get_or_insert_with(TaskJournal::default);
            journal.ended_at = if self.state.terminal() {
                now.clone()
            } else {
                String::new()
            };
            self.log_line(&format!(
                "{now} · {} · {} · {}",
                state_message(self),
                self.processed,
                self.errors
            ));
        }
    }
    pub fn job(&self) -> TaskJob {
        let journal = self.journal.clone().unwrap_or_default();
        TaskJob {
            job_id: self.id.clone(),
            running: matches!(self.state, TaskState::Requested | TaskState::Running),
            action: action(self).into(),
            started_at: journal.started_at,
            ended_at: journal.ended_at,
            exit_code: i32::from(matches!(
                self.state,
                TaskState::Failed | TaskState::Interrupted | TaskState::Cancelled
            )),
            message: self
                .failure
                .as_ref()
                .map(|e| self.error_message(e))
                .unwrap_or_else(|| state_message(self)),
            language: serde_json::to_value(&self.locale)
                .unwrap_or_default()
                .as_str()
                .unwrap_or("zh-CN")
                .into(),
            language_pack_revision: journal.language_pack_revision,
            log: journal.text,
        }
    }
}
