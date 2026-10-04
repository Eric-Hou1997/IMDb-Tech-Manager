use super::*;
use crate::migration::{MigrationPlan, MigrationReceipt};
use std::path::PathBuf;

/// The two fixed v4.1 macOS data directories have separate responsibilities.
/// Callers provide paths, never arbitrary portable/TCM source discovery.
pub struct StartupSources {
    pub manager: PathBuf,
    pub engine: PathBuf,
}
const RECEIPT: &str = "startup-itm-receipt";
const PENDING: &str = "legacy-pending-roots";

fn destination_in_use(db: &Connection) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM configuration WHERE json_extract(body,'$.revision')>0)
          OR EXISTS(SELECT 1 FROM items) OR EXISTS(SELECT 1 FROM tasks)
          OR EXISTS(SELECT 1 FROM preferences)
          OR EXISTS(SELECT 1 FROM operations WHERE json_extract(result,'$.kind') IS NOT 'migration-plan')",
        [], |r| r.get(0),
    )?)
}
impl Store {
    pub fn startup_import_receipts(&self) -> Result<Option<Vec<MigrationReceipt>>> {
        let db = self.db()?;
        let body: Option<String> = db
            .query_row(
                "SELECT body FROM preferences WHERE key=?1",
                [RECEIPT],
                |r| r.get(0),
            )
            .optional()?;
        body.map(|body| serde_json::from_str(&body).map_err(Into::into))
            .transpose()
    }

    pub fn pending_legacy_roots(&self) -> Result<Vec<crate::migration::LegacyRoot>> {
        let value = self.preferences(PENDING)?;
        if value.is_array() {
            serde_json::from_value(value).map_err(Into::into)
        } else {
            Ok(vec![])
        }
    }

    pub fn confirm_library_roots(
        &self,
        id: &str,
        configuration: Configuration,
    ) -> Result<Configuration> {
        for (index, root) in configuration.roots.iter().enumerate() {
            if configuration.roots[..index]
                .iter()
                .any(|old| old.space != root.space && old.path == root.path)
            {
                return Err(
                    AppError::new("overlapping-roots", "同一目录不能同时属于电影和电视剧")
                        .at(&root.path),
                );
            }
        }
        let saved = self.configure(id, configuration)?;
        // Retrying the same confirmed save also finishes this display-only
        // acknowledgement if the process stopped after configuration committed.
        self.save_preference(PENDING, &serde_json::json!([]))?;
        self.save_preference("library-roots-confirmed", &serde_json::json!(true))?;
        Ok(saved)
    }

    pub fn restore_legacy_on_start(
        &self,
        sources: &StartupSources,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<Vec<MigrationReceipt>>> {
        // A completed receipt takes precedence even if the old folders have
        // disappeared. A used destination never silently reimports old choices.
        if let Some(receipts) = self.startup_import_receipts()? {
            return Ok(Some(receipts));
        }
        let plans = self.prepare_startup_migration(sources, cancelled)?;
        if plans.is_empty() {
            return Ok(None);
        }
        self.apply_startup_migration(&plans, cancelled).map(Some)
    }

    pub fn prepare_startup_migration(
        &self,
        sources: &StartupSources,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<MigrationPlan>> {
        if self.startup_import_receipts()?.is_some() || destination_in_use(&*self.db()?)? {
            return Ok(vec![]);
        }
        let current = self.configuration()?;
        let mut plans = vec![];
        // Engine owns roots/AI/cache. Manager owns the effective presentation
        // locale and automatic-start preference, so its locale is applied last.
        for (kind, folder, primary) in [
            ("itm-engine", &sources.engine, "config.json"),
            ("itm-manager", &sources.manager, "settings.json"),
        ] {
            if cancelled() {
                return Err(AppError::new("startup-cancelled", "Startup import stopped"));
            }
            match std::fs::symlink_metadata(folder) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    return Err(AppError::new("migration-source-read", e).at(folder.display()))
                }
                Ok(_) => {}
            }
            let source = paths::checked(folder)?;
            let snapshot = match crate::migration::prepare_cancellable(
                "startup", &source, kind, &current, cancelled,
            ) {
                Err(e) if e.code == "migration-empty" => continue,
                other => other?,
            };
            if let Some(file) = snapshot.files.iter().find(|f| f.relative == primary) {
                let bytes = crate::migration::read_snapshot(&source, file)?;
                let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
                    AppError::new("migration-settings-invalid", e)
                        .at(source.join(primary).display())
                })?;
                if !value.is_object() {
                    return Err(
                        AppError::new("migration-settings-invalid", "旧版设置格式无效")
                            .at(source.join(primary).display()),
                    );
                }
                // Generic archives may retain malformed historical files;
                // startup must not silently substitute defaults for live settings.
                crate::automatic::legacy_settings(&value)?;
                crate::lifecycle::legacy_settings(&value)?;
                crate::migration::ai_profile::adapt(&value)?;
                if let Some(locale) = value.get("language") {
                    serde_json::from_value::<Locale>(locale.clone()).map_err(|e| {
                        AppError::new("migration-language-invalid", e)
                            .at(source.join(primary).display())
                    })?;
                }
            }
            let id = format!("startup-itm-{}", snapshot.fingerprint);
            let plan = self.prepare_migration_cancellable(&id, &source, kind, cancelled)?;
            if serde_json::to_vec(&snapshot.files)? != serde_json::to_vec(&plan.files)?
                || plan.configuration_revision != current.revision
            {
                return Err(
                    AppError::new("migration-source-changed", "旧版数据已改变，请重试")
                        .at(source.display()),
                );
            }
            plans.push(plan);
        }
        Ok(plans)
    }

    pub fn apply_startup_migration(
        &self,
        plans: &[MigrationPlan],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<MigrationReceipt>> {
        if plans.is_empty() || plans.len() > 2 {
            return Err(AppError::new(
                "migration-plan-mismatch",
                "Expected the original ITM directories",
            ));
        }
        let mut db = self.db()?;
        self.writable()?;
        let tx = db.transaction()?;
        if let Some(body) = tx
            .query_row(
                "SELECT body FROM preferences WHERE key=?1",
                [RECEIPT],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            return serde_json::from_str(&body).map_err(Into::into);
        }
        if destination_in_use(&tx)? {
            return Err(AppError::new(
                "migration-destination-in-use",
                "新版本数据已改变，未导入旧版设置",
            ));
        }
        let base = plans[0].configuration_revision;
        let mut kinds = std::collections::BTreeSet::new();
        let mut targets = std::collections::BTreeSet::new();
        for plan in plans {
            if !plan.id.starts_with("startup-itm-")
                || !["itm-engine", "itm-manager"].contains(&plan.source_kind.as_str())
                || !kinds.insert(&plan.source_kind)
                || plan.configuration_revision != base
            {
                return Err(AppError::new(
                    "migration-plan-mismatch",
                    "Invalid startup sources",
                ));
            }
            for adapter in &plan.adapters {
                if !targets.insert(&adapter.target) {
                    return Err(AppError::new(
                        "migration-ambiguous-settings",
                        "两个旧目录包含冲突设置，未导入",
                    ));
                }
            }
        }
        let mut receipts = vec![];
        let mut pending = vec![];
        for plan in plans {
            let receipt = Self::apply_migration_transaction(
                &tx,
                &plan.id,
                &plan.fingerprint,
                Some(base),
                cancelled,
            )?;
            pending.extend(receipt.pending_roots.clone());
            receipts.push(receipt);
        }
        if cancelled() {
            return Err(AppError::new("startup-cancelled", "Startup import stopped"));
        }
        tx.execute(
            "INSERT INTO preferences VALUES(?1,?2)",
            params![PENDING, serde_json::to_string(&pending)?],
        )?;
        tx.execute(
            "INSERT INTO preferences VALUES(?1,?2)",
            params![RECEIPT, serde_json::to_string(&receipts)?],
        )?;
        tx.commit()?;
        Ok(receipts)
    }
}
