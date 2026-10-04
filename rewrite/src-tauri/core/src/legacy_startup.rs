//! Read-only old-writer guard and reversible, identity-checked login handoff.
use crate::{hash, paths, AppError, Result};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub fn ensure_idle(listing: &str, manager: &Path, current_pid: u32) -> Result<()> {
    let engine = manager
        .join("engine/mac-engine.py")
        .to_string_lossy()
        .into_owned();
    let bare = manager
        .join("bin/imdb-tech-manager")
        .to_string_lossy()
        .into_owned();
    for line in listing.lines() {
        let line = line.trim();
        let Some((pid, command)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let command = command.trim();
        if pid.parse::<u32>().is_err() || pid.parse::<u32>().ok() == Some(current_pid) {
            continue;
        }
        if command.starts_with(&format!("{bare} "))
            || command == bare
            || command.contains("IMDb Tech Manager.app/Contents/MacOS/")
            || (command.split_whitespace().next().is_some_and(|exe| {
                Path::new(exe)
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("python"))
            }) && command.contains(&engine))
        {
            return Err(AppError::new(
                "legacy-runtime-active",
                "旧版应用或后台仍在运行。请从旧版正常退出，完成当前 NFO 写入后再重试。",
            ));
        }
    }
    Ok(())
}

pub struct Registration {
    pub path: PathBuf,
    pub label: String,
    bytes: Vec<u8>,
}
impl Registration {
    pub fn read(path: &Path, label: &str) -> Result<Option<Self>> {
        match fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(AppError::new("legacy-login-read", e).at(path.display())),
            Ok(meta) if !meta.is_file() || meta.len() > (1 << 20) => {
                return Err(
                    AppError::new("legacy-login-owner", "旧登录项不是可验证的普通文件")
                        .at(path.display()),
                )
            }
            _ => {}
        }
        paths::checked(path)?;
        let bytes =
            fs::read(path).map_err(|e| AppError::new("legacy-login-read", e).at(path.display()))?;
        let value = plist::Value::from_reader(std::io::Cursor::new(&bytes))
            .map_err(|e| AppError::new("legacy-login-owner", e).at(path.display()))?;
        let dictionary = value
            .as_dictionary()
            .ok_or_else(|| AppError::new("legacy-login-owner", "旧登录项格式无效"))?;
        let arguments = dictionary
            .get("ProgramArguments")
            .and_then(|v| v.as_array())
            .and_then(|args| {
                args.iter()
                    .map(|v| v.as_string())
                    .collect::<Option<Vec<_>>>()
            });
        let owned = dictionary.get("Label").and_then(|v| v.as_string()) == Some(label)
            && match (label, arguments.as_deref()) {
                ("com.local.imdb-tech-manager", Some([exe, "--agent"])) => {
                    exe.ends_with("/IMDb Tech Manager.app/Contents/MacOS/imdb-tech-manager")
                        || exe.ends_with(
                            "/Library/Application Support/IMDb Tech Manager/bin/imdb-tech-manager",
                        )
                }
                (
                    "com.local.imdb-tech-manager.app",
                    Some(["/usr/bin/open", "-gj", app, "--args", "--login-startup"]),
                ) => Path::new(app)
                    .file_name()
                    .is_some_and(|name| name == "IMDb Tech Manager.app"),
                _ => false,
            };
        if !owned {
            return Err(AppError::new(
                "legacy-login-owner",
                "旧登录项的归属无法验证，未接管或移除",
            )
            .at(path.display()));
        }
        Ok(Some(Self {
            path: path.into(),
            label: label.into(),
            bytes,
        }))
    }
    pub fn unchanged(&self) -> Result<()> {
        paths::checked(&self.path)?;
        if fs::read(&self.path).map_err(|e| AppError::new("legacy-login-read", e))? != self.bytes {
            return Err(
                AppError::new("legacy-login-changed", "旧登录项已发生变化，请重试")
                    .at(self.path.display()),
            );
        }
        Ok(())
    }
    /// First save verified original bytes, then retire only that exact source.
    /// Every step is repeatable after interruption; archives are never overwritten.
    pub fn backup(&self, directory: &Path) -> Result<()> {
        self.unchanged()?;
        paths::checked_or_missing(directory)?;
        fs::create_dir_all(directory).map_err(|e| AppError::new("legacy-login-archive", e))?;
        paths::checked(directory)?;
        let target = directory.join(format!("{}-{}.plist", self.label, hash(&self.bytes)));
        if target.exists() {
            paths::checked(&target)?;
            if fs::read(&target).map_err(|e| AppError::new("legacy-login-archive", e))?
                != self.bytes
            {
                return Err(AppError::new(
                    "legacy-login-archive",
                    "旧登录项备份校验失败",
                ));
            }
        } else {
            let mut temp = tempfile::NamedTempFile::new_in(directory)
                .map_err(|e| AppError::new("legacy-login-archive", e))?;
            temp.write_all(&self.bytes)
                .map_err(|e| AppError::new("legacy-login-archive", e))?;
            temp.as_file()
                .sync_all()
                .map_err(|e| AppError::new("legacy-login-archive", e))?;
            temp.persist_noclobber(&target)
                .map_err(|e| AppError::new("legacy-login-archive", e))?;
        }
        #[cfg(unix)]
        fs::File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(|e| AppError::new("legacy-login-archive", e))?;
        Ok(())
    }
    pub fn archive(&self, directory: &Path) -> Result<()> {
        self.backup(directory)?;
        self.unchanged()?;
        fs::remove_file(&self.path).map_err(|e| AppError::new("legacy-login-retire", e))?;
        if self.path.exists() {
            return Err(AppError::new(
                "legacy-login-retire",
                "旧登录项撤除结果未确认",
            ));
        }
        #[cfg(unix)]
        if let Some(parent) = self.path.parent() {
            fs::File::open(parent)
                .and_then(|file| file.sync_all())
                .map_err(|e| AppError::new("legacy-login-retire", e))?;
        }
        Ok(())
    }
}
