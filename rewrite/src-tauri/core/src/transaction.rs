//! ITM-only journal prototype. Not exposed through Tauri until platform/ownership gates pass.
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
    pub operation_id: String,
    pub path: PathBuf,
    pub before: String,
    pub after: String,
    pub state: String,
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
        Err(AppError::new(
            "platform-write-unverified",
            "Native durable replacement adapter has not passed acceptance",
        ))
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
        sync_directory(dir)
    }
    pub fn inspect(&self, id: &str) -> Result<Receipt> {
        let dir = self.operation(id)?;
        let mut receipt: Receipt =
            serde_json::from_slice(&fs::read(dir.join("operation.json")).map_err(io)?)?;
        let actual = hash(&fs::read(self.authorize(&receipt.path)?).map_err(io)?);
        if receipt.state == "prepared" {
            receipt.state = if actual == receipt.after {
                "committed"
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
            if existing.path != path || existing.before != expected || existing.after != after {
                return Err(AppError::new(
                    "operation-conflict",
                    "Operation ID already identifies another write",
                ));
            }
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
        crate::specs::validate_specs_only(&original, candidate)?;
        let permissions = fs::metadata(&path).map_err(io)?.permissions();
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
        let mut receipt = Receipt {
            operation_id: id.into(),
            path: path.clone(),
            before: expected.into(),
            after,
            state: "prepared".into(),
        };
        self.save(&dir, &receipt)?;
        sync_directory(&self.journal)?;
        let parent = path
            .parent()
            .ok_or_else(|| AppError::new("invalid-path", "Target has no parent"))?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(io)?;
        tmp.as_file().set_permissions(permissions).map_err(io)?;
        tmp.write_all(candidate).map_err(io)?;
        tmp.as_file().sync_all().map_err(io)?;
        hook(Phase::BeforeReplace)?;
        if self.authorize(&path)? != path || hash(&fs::read(&path).map_err(io)?) != expected {
            return Err(
                AppError::new("source-conflict", "NFO changed before replacement")
                    .at(path.display()),
            );
        }
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
        self.commit(
            undo_id,
            &receipt.path,
            &receipt.after,
            &original,
            |_| Ok(()),
        )
    }
}
