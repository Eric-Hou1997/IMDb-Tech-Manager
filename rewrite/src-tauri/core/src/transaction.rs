//! ITM file transactions: candidate validation, source CAS, verified backup and recovery receipts.
use crate::writing::WriteIntent;
use crate::{hash, paths, AppError, Result};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::fs::File;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Receipt {
    #[serde(default)]
    pub intent: WriteIntent,
    pub operation_id: String,
    pub path: PathBuf,
    pub before: String,
    pub after: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_backup: Option<PathBuf>,
}
pub enum Phase {
    BackupSynced,
    BeforeReplace,
    Replaced,
}
pub struct Writer {
    journal: PathBuf,
    roots: Vec<PathBuf>,
}
fn io(e: std::io::Error) -> AppError {
    AppError::new("file-transaction", e)
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(path).and_then(|f| f.sync_all()).map_err(io)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        // Windows does not expose directory fsync through File. Each journal
        // and replacement file is flushed explicitly by its write adapter.
        Ok(())
    }
}
impl Writer {
    pub fn new(journal: &Path, roots: Vec<PathBuf>) -> Result<Self> {
        fs::create_dir_all(journal).map_err(io)?;
        let journal = paths::checked(journal)?;
        let roots = roots
            .into_iter()
            .map(|p| paths::checked(&p))
            .collect::<Result<Vec<_>>>()?;
        if roots.iter().any(|r| journal.starts_with(r)) {
            return Err(AppError::new(
                "unsafe-journal-location",
                "Transaction journal must be outside media roots",
            ));
        }
        Ok(Self { journal, roots })
    }
    fn operation(&self, id: &str) -> Result<PathBuf> {
        if id.is_empty()
            || id.len() > 96
            || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(AppError::new(
                "invalid-operation-id",
                "Invalid transaction ID",
            ));
        }
        Ok(self.journal.join(id))
    }
    fn authorize(&self, path: &Path) -> Result<PathBuf> {
        let path = paths::checked(path)?;
        if !self.roots.iter().any(|r| path.starts_with(r)) {
            return Err(AppError::new(
                "outside-root",
                "Transaction target is outside configured libraries",
            ));
        }
        Ok(path)
    }
    fn save(&self, dir: &Path, receipt: &Receipt) -> Result<()> {
        let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(io)?;
        tmp.write_all(&serde_json::to_vec(receipt)?).map_err(io)?;
        tmp.as_file().sync_all().map_err(io)?;
        tmp.persist(dir.join("operation.json"))
            .map_err(|e| io(e.error))?;
        #[cfg(windows)]
        OpenOptions::new()
            .write(true)
            .open(dir.join("operation.json"))
            .and_then(|f| f.sync_all())
            .map_err(io)?;
        sync_directory(dir)
    }
    pub fn inspect(&self, id: &str) -> Result<Receipt> {
        let dir = self.operation(id)?;
        let mut receipt: Receipt =
            serde_json::from_slice(&fs::read(dir.join("operation.json")).map_err(io)?)?;
        let actual = hash(&fs::read(self.authorize(&receipt.path)?).map_err(io)?);
        if receipt.state == "prepared"
            || receipt.state == "metadata-pending"
            || receipt.state == "replaced"
        {
            receipt.state = if actual == receipt.after {
                if receipt.state == "metadata-pending" {
                    "metadata-pending"
                } else {
                    "committed"
                }
            } else if actual == receipt.before {
                "not-applied"
            } else {
                "conflict"
            }
            .into();
        }
        Ok(receipt)
    }
    pub fn commit(
        &self,
        id: &str,
        path: &Path,
        expected: &str,
        candidate: &[u8],
        hook: impl Fn(Phase) -> Result<()>,
    ) -> Result<Receipt> {
        self.commit_with_intent(id, path, expected, candidate, &WriteIntent::Specs, hook)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn commit_with_intent(
        &self,
        id: &str,
        path: &Path,
        expected: &str,
        candidate: &[u8],
        intent: &WriteIntent,
        hook: impl Fn(Phase) -> Result<()>,
    ) -> Result<Receipt> {
        let path = self.authorize(path)?;
        let dir = self.operation(id)?;
        let after = hash(candidate);
        let content =
            std::str::from_utf8(candidate).map_err(|e| AppError::new("invalid-candidate", e))?;
        roxmltree::Document::parse(content.trim_start_matches('\u{feff}'))
            .map_err(|e| AppError::new("invalid-candidate", e))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(
                self.journal
                    .join(format!("{}.lock", hash(path.to_string_lossy().as_bytes()))),
            )
            .map_err(io)?;
        lock.try_lock()
            .map_err(|e| AppError::new("write-busy", e))?;
        if dir.exists() {
            let existing = self.inspect(id)?;
            if existing.path != path
                || existing.before != expected
                || existing.after != after
                || existing.intent != *intent
            {
                return Err(AppError::new(
                    "operation-conflict",
                    "Operation ID already identifies another write",
                ));
            }
            #[cfg(windows)]
            let existing = if existing.state == "metadata-pending" {
                let mut existing = existing;
                let backup = existing.native_backup.as_ref().ok_or_else(|| {
                    AppError::new("recovery-required", "Native metadata backup is missing")
                })?;
                if hash(&fs::read(backup).map_err(io)?) != existing.before {
                    return Err(AppError::new(
                        "backup-invalid",
                        "Native metadata backup hash mismatch",
                    ));
                }
                crate::windows_replace::Security::read(backup)
                    .and_then(|security| security.apply(&path))
                    .map_err(io)?;
                existing.state = "committed".into();
                self.save(&dir, &existing)?;
                existing
            } else {
                existing
            };
            return if existing.state == "committed" {
                Ok(existing)
            } else {
                Err(AppError::new(
                    "recovery-required",
                    "Inspect and resolve the existing transaction before retrying",
                ))
            };
        }
        let original = fs::read(&path).map_err(io)?;
        if hash(&original) != expected {
            return Err(
                AppError::new("source-conflict", "NFO changed after preview").at(path.display()),
            );
        }
        match intent {
            WriteIntent::Specs => crate::specs::validate_specs_only(&original, candidate)?,
            WriteIntent::Tags { plan } => {
                if crate::tags::candidate(&original, plan)? != candidate {
                    return Err(AppError::new(
                        "unsafe-candidate",
                        "Tag candidate differs from the authorized mutation",
                    ));
                }
            }
            WriteIntent::Undo { original_id } => {
                let previous = self.inspect(original_id)?;
                if previous.path != path
                    || previous.after != expected
                    || self.original_bytes(original_id)? != candidate
                {
                    return Err(AppError::new(
                        "unsafe-undo",
                        "Undo must restore the exact verified original backup",
                    ));
                }
            }
        }
        let permissions = fs::metadata(&path).map_err(io)?.permissions();
        #[cfg(windows)]
        let security = crate::windows_replace::Security::read(&path).map_err(io)?;
        fs::create_dir(&dir).map_err(io)?;
        let mut backup = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join("original.nfo"))
            .map_err(io)?;
        backup.write_all(&original).map_err(io)?;
        backup.sync_all().map_err(io)?;
        if hash(&fs::read(dir.join("original.nfo")).map_err(io)?) != expected {
            return Err(AppError::new("backup-invalid", "Backup readback mismatch"));
        }
        hook(Phase::BackupSynced)?;
        let native_backup = if cfg!(windows) {
            Some(path.with_file_name(format!(".itm-replaced-{}.bak", hash(id.as_bytes()))))
        } else {
            None
        };
        if native_backup
            .as_ref()
            .is_some_and(|p| fs::symlink_metadata(p).is_ok())
        {
            return Err(AppError::new(
                "recovery-required",
                "Native replacement backup already exists",
            ));
        }
        let mut receipt = Receipt {
            intent: intent.clone(),
            operation_id: id.into(),
            path: path.clone(),
            before: expected.into(),
            after,
            state: "prepared".into(),
            native_backup,
        };
        self.save(&dir, &receipt)?;
        sync_directory(&self.journal)?;
        let parent = path
            .parent()
            .ok_or_else(|| AppError::new("invalid-path", "Target has no parent"))?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(io)?;
        tmp.as_file().set_permissions(permissions).map_err(io)?;
        #[cfg(windows)]
        security.apply(tmp.path()).map_err(io)?;
        tmp.write_all(candidate).map_err(io)?;
        tmp.as_file().sync_all().map_err(io)?;
        hook(Phase::BeforeReplace)?;
        if self.authorize(&path)? != path || hash(&fs::read(&path).map_err(io)?) != expected {
            return Err(
                AppError::new("source-conflict", "NFO changed before replacement")
                    .at(path.display()),
            );
        }
        #[cfg(windows)]
        {
            // ReplaceFileW opens the replacement without sharing. Close our file
            // handle first, while retaining ownership of the temporary pathname
            // so a failed replacement still cleans up the candidate.
            let candidate_path = tmp.into_temp_path();
            receipt.state = "metadata-pending".into();
            self.save(&dir, &receipt)?;
            crate::windows_replace::replace(
                &path,
                &candidate_path,
                receipt.native_backup.as_ref().expect("Windows backup"),
                expected,
            )
            .map_err(io)?;
            // ReplaceFileW merges ACLs; restore the exact original entries and
            // inheritance policy so repeated edits cannot accumulate grants.
            security.apply(&path).map_err(io)?;
            receipt.state = "replaced".into();
            self.save(&dir, &receipt)?;
        }
        #[cfg(not(windows))]
        tmp.persist(&path).map_err(|e| io(e.error))?;
        sync_directory(parent)?;
        hook(Phase::Replaced)?;
        if hash(&fs::read(&path).map_err(io)?) != receipt.after {
            return Err(
                AppError::new("postcondition-conflict", "File changed after replacement")
                    .at(path.display()),
            );
        }
        receipt.state = "committed".into();
        self.save(&dir, &receipt)?;
        Ok(receipt)
    }
    pub fn original_bytes(&self, original_id: &str) -> Result<Vec<u8>> {
        let receipt = self.inspect(original_id)?;
        if receipt.state != "committed" {
            return Err(AppError::new(
                "unsafe-undo",
                "Original write is not committed",
            ));
        }
        let original = fs::read(self.operation(original_id)?.join("original.nfo")).map_err(io)?;
        if hash(&original) != receipt.before {
            return Err(AppError::new("backup-invalid", "Backup hash mismatch"));
        }
        Ok(original)
    }
    pub fn undo(&self, undo_id: &str, original_id: &str) -> Result<Receipt> {
        let receipt = self.inspect(original_id)?;
        if receipt.state != "committed" {
            return Err(AppError::new(
                "unsafe-undo",
                "Original operation is not committed",
            ));
        }
        let original = fs::read(self.operation(original_id)?.join("original.nfo")).map_err(io)?;
        if hash(&original) != receipt.before {
            return Err(AppError::new(
                "backup-invalid",
                "Original backup hash mismatch",
            ));
        }
        self.commit_with_intent(
            undo_id,
            &receipt.path,
            &receipt.after,
            &original,
            &WriteIntent::Undo {
                original_id: original_id.into(),
            },
            |_| Ok(()),
        )
    }
}
