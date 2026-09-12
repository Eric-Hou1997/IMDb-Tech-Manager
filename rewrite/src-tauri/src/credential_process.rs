//! Read-only Keychain isolation. No caller can use this as a general credential
//! export command: the OS-reported parent must be this exact application image.
//! Every child and output reader has a bounded lifetime and is synchronously joined.
use product_core::{AppError, Result};
use std::{
    io::{Read, Write},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
const FLAG: &str = "--itm-internal-credential-read";
const LIMIT: u64 = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);
#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecKeychainSetUserInteractionAllowed(allowed: u8) -> i32;
}
unsafe extern "C" {
    fn getppid() -> i32;
    fn proc_pidpath(pid: i32, buffer: *mut std::ffi::c_void, size: u32) -> i32;
}
fn trusted_parent() -> bool {
    use std::os::unix::ffi::OsStrExt;
    let parent = unsafe { getppid() };
    if parent <= 1 {
        return false;
    }
    let mut path = [0_u8; 4096];
    let size = unsafe { proc_pidpath(parent, path.as_mut_ptr().cast(), path.len() as u32) };
    if size <= 0 || unsafe { getppid() } != parent {
        return false;
    }
    let end = path.iter().position(|b| *b == 0).unwrap_or(path.len());
    let parent_image = std::path::Path::new(std::ffi::OsStr::from_bytes(&path[..end]));
    match (
        parent_image.canonicalize(),
        std::env::current_exe().and_then(|p| p.canonicalize()),
    ) {
        (Ok(parent), Ok(current)) => parent == current,
        _ => false,
    }
}
fn read_error(error: keyring::Error) -> AppError {
    match error {
        keyring::Error::BadEncoding(_) => {
            AppError::new("credential-encoding", "Stored credential is not UTF-8")
        }
        keyring::Error::NoStorageAccess(_) => AppError::new(
            "credential-store-unavailable",
            "The macOS Keychain store is unavailable",
        ),
        keyring::Error::PlatformFailure(error) => {
            match error.downcast_ref::<security_framework::base::Error>().map(|error| error.code()) {
                Some(-25308 | -25293 | -128) => AppError::new("credential-authorization-required", "The stored credential requires macOS authorization; authorize the application or save a new key in AI settings"),
                Some(code) => AppError::new("credential-read", format!("macOS Keychain returned {code}")),
                None => AppError::new("credential-read", "The platform credential reader failed"),
            }
        }
        _ => AppError::new("credential-read", "The credential reader failed"),
    }
}
pub fn entry() -> Option<i32> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some(FLAG) {
        return None;
    }
    if args.len() != 4 || !trusted_parent() {
        return Some(64);
    }
    // Safe here because this dedicated process performs no other Keychain work
    // and never initializes Tauri or a UI. The Manager's interaction policy stays intact.
    let result = if unsafe { SecKeychainSetUserInteractionAllowed(0) } != 0 {
        Err(AppError::new(
            "credential-read",
            "Could not disable Keychain interaction in the reader",
        ))
    } else {
        match keyring::Entry::new(&args[2], &args[3]).and_then(|entry| entry.get_password()) {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(read_error(error)),
        }
    };
    let bytes = match serde_json::to_vec(&result) {
        Ok(bytes) if bytes.len() as u64 <= LIMIT => bytes,
        _ => return Some(65),
    };
    Some(if std::io::stdout().lock().write_all(&bytes).is_ok() {
        0
    } else {
        66
    })
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        // Killing this process can interrupt only a read. It cannot partially
        // commit settings or credentials; those writes live in the Manager.
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
pub fn read(service: &str, account: &str) -> Result<Option<String>> {
    let executable =
        std::env::current_exe().map_err(|e| AppError::new("credential-reader-start", e))?;
    let child = Command::new(executable)
        .args([FLAG, service, account])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| AppError::new("credential-reader-start", e))?;
    collect(child, TIMEOUT)
}
fn collect(child: Child, timeout: Duration) -> Result<Option<String>> {
    let mut child = OwnedChild(child);
    let output =
        child.0.stdout.take().ok_or_else(|| {
            AppError::new("credential-reader-start", "Missing private output pipe")
        })?;
    let reader = std::thread::Builder::new()
        .name("credential-output".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            output
                .take(LIMIT + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        })
        .map_err(|e| AppError::new("credential-reader-start", e))?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.0.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Err(e) => break Err(AppError::new("credential-reader-wait", e)),
            Ok(None) if Instant::now() >= deadline => break Err(AppError::new("credential-read-timeout", "macOS credential reading timed out; the reader was stopped. Settings remain available.")),
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    drop(child); // Always terminate/wait before joining the pipe reader.
    let bytes = reader
        .join()
        .map_err(|_| AppError::new("credential-reader-output", "Output reader panicked"))?
        .map_err(|e| AppError::new("credential-reader-output", e))?;
    if !status?.success() || bytes.len() as u64 > LIMIT {
        return Err(AppError::new(
            "credential-reader-output",
            "Credential reader failed or returned an oversized response",
        ));
    }
    serde_json::from_slice::<Result<Option<String>>>(&bytes).map_err(|_| {
        AppError::new(
            "credential-reader-output",
            "Credential reader returned an invalid response",
        )
    })?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authorization_storage_and_encoding_errors_remain_distinct() {
        let platform = |code| {
            keyring::Error::PlatformFailure(Box::new(security_framework::base::Error::from_code(
                code,
            )))
        };
        assert_eq!(
            read_error(platform(-25308)).code,
            "credential-authorization-required"
        );
        assert_eq!(read_error(platform(-50)).code, "credential-read");
        assert_eq!(
            read_error(keyring::Error::BadEncoding(vec![255])).code,
            "credential-encoding"
        );
        assert_eq!(
            read_error(keyring::Error::NoStorageAccess(Box::new(
                std::io::Error::other("unavailable")
            )))
            .code,
            "credential-store-unavailable"
        );
    }
    #[test]
    fn unrelated_parent_cannot_invoke_the_private_reader() {
        assert!(!trusted_parent());
    }
    #[test]
    fn stuck_reader_is_terminated_reaped_and_reported_as_unverified() {
        let child = Command::new("/bin/sleep")
            .arg("30")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let started = Instant::now();
        assert_eq!(
            collect(child, Duration::from_millis(80)).unwrap_err().code,
            "credential-read-timeout"
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
