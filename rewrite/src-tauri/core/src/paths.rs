use crate::{AppError, Result};
use std::path::{Component, Path, PathBuf};

// Platform path policy lives here; callers never authorize a path using a string prefix.
pub fn checked(path: &Path) -> Result<PathBuf> {
    checked_path(path, false)
}
/// Retain an already configured offline library without granting file access.
/// Existing ancestors still must pass the same symlink/reparse policy; every
/// actual NFO operation continues to use `checked`/`within`.
pub fn checked_or_missing(path: &Path) -> Result<PathBuf> {
    checked_path(path, true)
}
fn checked_path(path: &Path, allow_missing: bool) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(AppError::new("invalid-path", "Absolute path required").at(path.display()));
    }
    let mut walked = PathBuf::new();
    let mut missing = false;
    for part in path.components() {
        if matches!(part, Component::ParentDir) {
            return Err(
                AppError::new("ambiguous-path", "Parent traversal rejected").at(path.display())
            );
        }
        walked.push(part);
        // A Windows drive/UNC prefix is not a filesystem object until RootDir.
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        let meta = match std::fs::symlink_metadata(&walked) {
            Err(e) if allow_missing && e.kind() == std::io::ErrorKind::NotFound => {
                missing = true;
                continue;
            }
            result => {
                result.map_err(|e| AppError::new("path-unavailable", e).at(walked.display()))?
            }
        };
        if meta.file_type().is_symlink() {
            return Err(AppError::new(
                "ambiguous-path",
                "Symbolic links require an explicit real root",
            )
            .at(walked.display()));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err(
                    AppError::new("ambiguous-path", "Reparse point rejected").at(walked.display())
                );
            }
        }
    }
    if missing {
        return Ok(walked);
    }
    path.canonicalize()
        .map_err(|e| AppError::new("path-unavailable", e).at(path.display()))
}
pub fn within(root: &Path, path: &Path) -> Result<PathBuf> {
    let root = checked(root)?;
    let path = checked(path)?;
    if !path.starts_with(root) {
        return Err(
            AppError::new("outside-root", "Path is outside configured root").at(path.display()),
        );
    }
    Ok(path)
}
