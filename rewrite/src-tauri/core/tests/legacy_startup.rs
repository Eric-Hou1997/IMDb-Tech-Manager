use itm_core::{
    legacy_startup::{ensure_idle, Registration},
    lifecycle,
    store::{StartupSources, Store},
};
use std::fs;
fn plist(label: &str, args: &[&str]) -> Vec<u8> {
    let mut value = plist::Dictionary::new();
    value.insert("Label".into(), label.into());
    value.insert(
        "ProgramArguments".into(),
        plist::Value::Array(args.iter().map(|v| (*v).into()).collect()),
    );
    let mut bytes = vec![];
    plist::Value::Dictionary(value)
        .to_writer_xml(&mut bytes)
        .unwrap();
    bytes
}
#[test]
fn old_writers_block_startup_but_the_current_app_and_unrelated_processes_do_not() {
    let manager = std::path::Path::new("/Users/test/Library/Application Support/IMDb Tech Manager");
    for listing in [
        format!("123 {}/bin/imdb-tech-manager --agent", manager.display()),
        format!(
            "124 /usr/bin/python3 {}/engine/mac-engine.py --resident",
            manager.display()
        ),
        "125 /Applications/IMDb Tech Manager.app/Contents/MacOS/imdb-tech-manager --native-hosted"
            .into(),
    ] {
        assert_eq!(
            ensure_idle(&listing, manager, 999).unwrap_err().code,
            "legacy-runtime-active"
        );
    }
    ensure_idle("999 /Applications/IMDb Tech Manager.app/Contents/MacOS/itm\n123 /usr/bin/python3 another.py\n124 /Applications/Tech Card Manager.app/Contents/MacOS/tcm", manager, 999).unwrap();
}
#[test]
fn app_login_preference_is_imported_independently_and_not_reenabled_by_agent_settings() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let sources = StartupSources {
        manager: root.join("manager"),
        engine: root.join("engine"),
    };
    fs::create_dir(&sources.manager).unwrap();
    let bytes = br#"{"language":"en-US","app_auto_start":true,"app_auto_start_configured":true,"auto_mode_on_app_start":false,"auto_mode_on_app_start_configured":true}"#;
    fs::write(sources.manager.join("settings.json"), bytes).unwrap();
    let db = root.join("state.sqlite");
    let store = Store::open(&db).unwrap();
    store.restore_legacy_on_start(&sources, &|| false).unwrap();
    assert!(store.lifecycle_settings().unwrap().launch_at_login);
    assert!(!store.automatic_status().unwrap().settings.on_app_start);
    assert_eq!(
        fs::read(sources.manager.join("settings.json")).unwrap(),
        bytes
    );
    assert!(!lifecycle::legacy_settings(&serde_json::json!({"app_auto_start_configured":true,"app_auto_start":false,"auto_start":true,"auto_start_configured":true})).unwrap().unwrap().launch_at_login);
    assert!(lifecycle::legacy_settings(&serde_json::json!({"app_auto_start":"true"})).is_err());
}
#[test]
fn retirement_keeps_verified_original_bytes_and_never_overwrites_changed_or_foreign_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("old.plist");
    let label = "com.local.imdb-tech-manager.app";
    let bytes = plist(
        label,
        &[
            "/usr/bin/open",
            "-gj",
            "/Applications/IMDb Tech Manager.app",
            "--args",
            "--login-startup",
        ],
    );
    fs::write(&path, &bytes).unwrap();
    let registration = Registration::read(&path, label).unwrap().unwrap();
    let archive = root.join("archive");
    registration.backup(&archive).unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let entries: Vec<_> = fs::read_dir(&archive)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(fs::read(&entries[0]).unwrap(), bytes);
    fs::write(&path, b"changed").unwrap();
    assert_eq!(
        registration.archive(&archive).unwrap_err().code,
        "legacy-login-changed"
    );
    assert_eq!(fs::read(&path).unwrap(), b"changed");
    fs::write(&path, &bytes).unwrap();
    registration.archive(&archive).unwrap();
    assert!(!path.exists());
    assert_eq!(fs::read(&entries[0]).unwrap(), bytes);
    assert!(Registration::read(&path, label).unwrap().is_none());
    fs::write(&path, plist("some.other.app", &["/bin/sleep", "1"])).unwrap();
    assert_eq!(
        Registration::read(&path, label).err().unwrap().code,
        "legacy-login-owner"
    );
    assert!(path.exists());
    fs::write(&path, &bytes).unwrap();
    fs::write(&entries[0], b"corrupt backup").unwrap();
    assert_eq!(
        registration.archive(&archive).unwrap_err().code,
        "legacy-login-archive"
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
}
#[cfg(unix)]
#[test]
fn linked_registration_or_archive_is_rejected_without_modifying_targets() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let label = "com.local.imdb-tech-manager";
    let source = root.join("source");
    let bytes = plist(
        label,
        &[
            "/Applications/IMDb Tech Manager.app/Contents/MacOS/imdb-tech-manager",
            "--agent",
        ],
    );
    fs::write(&source, &bytes).unwrap();
    let path = root.join("old.plist");
    symlink(&source, &path).unwrap();
    assert!(Registration::read(&path, label).is_err());
    assert_eq!(fs::read(&source).unwrap(), bytes);
    let archive = root.join("archive");
    symlink(root.join("missing"), &archive).unwrap();
    assert!(Registration::read(&source, label)
        .unwrap()
        .unwrap()
        .archive(&archive)
        .is_err());
    assert_eq!(fs::read(&source).unwrap(), bytes);
}
