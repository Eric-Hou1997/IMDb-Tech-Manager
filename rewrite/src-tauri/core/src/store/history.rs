use super::*;
use crate::history::*;
impl Store {
    pub fn task_job(&self, id: &str) -> Result<crate::task_log::TaskJob> {
        let task = self.task_summary(id)?;
        let mut job = task.job();
        if task
            .batch
            .as_ref()
            .is_some_and(|b| b.engine == crate::batch::BatchEngine::Ai)
        {
            let db = self.db()?;
            let bodies = db.prepare("SELECT result FROM operations WHERE json_extract(result,'$.kind')='ai' AND json_extract(result,'$.result.batch_id')=?1 ORDER BY rowid DESC LIMIT 100")?.query_map([id], |r| r.get::<_, String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
            for body in bodies.iter().rev() {
                if let OperationResult::Ai(record) = serde_json::from_str(body)? {
                    let labels = match task.locale {
                        Locale::Simplified => [
                            "HTTP 请求",
                            "成功响应",
                            "本次用量",
                            "历史缓存用量",
                            "本次费用",
                        ],
                        Locale::Traditional => [
                            "HTTP 請求",
                            "成功回應",
                            "本次用量",
                            "歷史快取用量",
                            "本次費用",
                        ],
                        _ => [
                            "HTTP attempts",
                            "Successful responses",
                            "Current usage",
                            "Historical cache usage",
                            "Current cost",
                        ],
                    };
                    let labels = labels.map(|text| task.localized(text, text, text));
                    job.log.push_str(&format!("\n{} · {} · {} · {} · {} · {}\n{}: {} · {}: {} · {}: {} · {}: {} · {}: {}\n", record.finished_at.as_deref().unwrap_or(&record.started_at), record.title, record.year, record.imdb, record.path, task.result_label(&record.phase), labels[0], record.meter.attempts, labels[1], record.meter.successful_http, labels[2], record.meter.current.total, labels[3], record.meter.historical_cache.total, labels[4], record.cost));
                }
            }
            if job.log.len() > 40_000 {
                let mut start = job.log.len() - 40_000;
                while !job.log.is_char_boundary(start) {
                    start += 1;
                }
                job.log.drain(..start);
            }
        }
        Ok(job)
    }
    /// The original History tab contains completed job summaries, including
    /// imported records. It never turns a historical running flag into work.
    pub fn job_history(&self) -> Result<Vec<crate::task_log::TaskJob>> {
        let mut jobs = self
            .tasks()?
            .into_iter()
            .filter(|t| t.state.terminal())
            .take(100)
            .map(|t| t.job())
            .collect::<Vec<_>>();
        let mut archive_offset = 0;
        loop {
            let archives = self.history_archives(archive_offset, 100)?;
            for archive in archives
                .items
                .iter()
                .filter(|a| a.path.ends_with("task-history.json"))
            {
                let mut offset = 0;
                loop {
                    let page = self.history_page(&archive.import_id, &archive.path, offset)?;
                    if page.unit != "records" {
                        return Err(AppError::new("history-format", "Historical task summaries could not be read; the original bytes are preserved").at(&archive.path));
                    }
                    for value in page.tasks {
                        let mut job: crate::task_log::TaskJob = serde_json::from_value(value)
                            .map_err(|e| AppError::new("history-format", e).at(&archive.path))?;
                        job.log.clear();
                        jobs.push(job);
                    }
                    if offset >= 50 {
                        break;
                    }
                    match page.next_offset {
                        Some(next) => offset = next,
                        None => break,
                    }
                }
            }
            archive_offset += archives.items.len() as u32;
            if archive_offset >= archives.total {
                break;
            }
        }
        let time = |job: &crate::task_log::TaskJob| {
            chrono::DateTime::parse_from_rfc3339(if job.ended_at.is_empty() {
                &job.started_at
            } else {
                &job.ended_at
            })
            .ok()
            .map(|value| value.timestamp_millis())
        };
        jobs.sort_by_key(|job| std::cmp::Reverse(time(job)));
        jobs.truncate(100);
        Ok(jobs)
    }
    pub fn history_archives(&self, offset: u32, limit: u32) -> Result<HistoryArchives> {
        if !(1..=100).contains(&limit) {
            return Err(AppError::new(
                "history-limit",
                "Choose 1–100 historical files per page",
            ));
        }
        let db = self.db()?;
        let total = db.query_row(
            &format!("SELECT COUNT(*) FROM legacy_artifacts WHERE {ALLOWED}"),
            [],
            |r| r.get(0),
        )?;
        let items = db.prepare(&format!("SELECT import_id,path,sha256,length(body) FROM legacy_artifacts WHERE {ALLOWED} ORDER BY import_id,path LIMIT ?1 OFFSET ?2"))?.query_map(params![limit,offset], |r| Ok(HistoryArchive{import_id:r.get(0)?,path:r.get(1)?,sha256:r.get(2)?,bytes:r.get(3)?}))?.collect::<std::result::Result<_,_>>()?;
        Ok(HistoryArchives { total, items })
    }
    pub fn history_page(&self, import_id: &str, path: &str, offset: u32) -> Result<HistoryPage> {
        let db = self.db()?;
        let (archive, raw) = db.query_row(&format!("SELECT import_id,path,sha256,length(body),body FROM legacy_artifacts WHERE import_id=?1 AND path=?2 AND {ALLOWED}"), params![import_id,path], |r|Ok((HistoryArchive{import_id:r.get(0)?,path:r.get(1)?,sha256:r.get(2)?,bytes:r.get(3)?},r.get::<_,Vec<u8>>(4)?))).optional()?.ok_or_else(||AppError::new("history-not-found", "No imported history exists at this location").at(path))?;
        crate::history::page(archive, raw, offset)
    }
}
