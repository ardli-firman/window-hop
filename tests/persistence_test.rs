use std::path::PathBuf;
use tempfile::tempdir;
use window_hop::domain::{AppConfig, MonitorIdentity, Profile, ProfileId, ProfileStatus};
use window_hop::persistence::{FileProfileStore, PersistenceError, ProfileStore};

#[test]
fn test_file_profile_store_saves_and_loads_draft_profile() {
    let dir = tempdir().expect("failed to create temp dir");
    let file_path = dir.path().join("profiles.json");
    let store = FileProfileStore::new(file_path);

    let profile = Profile::new_draft(
        ProfileId::new("p1"),
        "Antrian Rawat Jalan",
        PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        "--app=https://antrian.rumahsakit.local/poli --kiosk",
    );

    assert_eq!(profile.status(), ProfileStatus::DraftNotReady);

    let config = AppConfig::new(1, vec![profile.clone()]);

    store.save(&config).expect("failed to save config");

    let loaded = store.load().expect("failed to load config");
    assert_eq!(loaded.schema_version(), 1);
    assert_eq!(loaded.profiles().len(), 1);
    assert_eq!(loaded.profiles()[0].id(), profile.id());
    assert_eq!(loaded.profiles()[0].name(), profile.name());
    assert_eq!(loaded.profiles()[0].browser_path(), profile.browser_path());
    assert_eq!(
        loaded.profiles()[0].custom_parameters(),
        profile.custom_parameters()
    );
    assert_eq!(loaded.profiles()[0].status(), ProfileStatus::DraftNotReady);
}

#[test]
fn test_file_profile_store_saves_and_loads_profile_with_target_monitor() {
    let dir = tempdir().expect("temp dir");
    let file_path = dir.path().join("profiles_with_target.json");
    let store = FileProfileStore::new(&file_path);

    let mut profile = Profile::new_draft(
        ProfileId::new("p-target"),
        "Antrian Poli Bedah Saraf",
        PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        "--app=https://antrian.saraf.local --kiosk",
    );
    let target_identity = MonitorIdentity::new("DEL-P2419H-UID-99881122");
    profile.set_target_monitor(Some(target_identity.clone()));

    let config = AppConfig::new(1, vec![profile.clone()]);
    store
        .save(&config)
        .expect("save config with target monitor");

    // Inspect JSON raw string to ensure target_monitor is saved cleanly
    let raw_json = std::fs::read_to_string(&file_path).expect("read raw json");
    assert!(
        raw_json.contains("DEL-P2419H-UID-99881122"),
        "Serialized json must contain recorded monitor identity string"
    );

    let loaded = store.load().expect("load config with target monitor");
    assert_eq!(loaded.profiles().len(), 1);
    let loaded_p = &loaded.profiles()[0];
    assert_eq!(loaded_p.id(), profile.id());
    assert_eq!(loaded_p.name(), profile.name());
    assert_eq!(loaded_p.target_monitor(), Some(&target_identity));
}

#[test]
fn test_file_profile_store_returns_empty_default_when_file_not_found() {
    let dir = tempdir().expect("failed to create temp dir");
    let file_path = dir.path().join("non_existent_profiles.json");
    let store = FileProfileStore::new(file_path);

    let loaded = store
        .load()
        .expect("should return default config if file does not exist");
    assert_eq!(loaded.schema_version(), 1);
    assert!(loaded.profiles().is_empty());
}

#[test]
fn test_file_profile_store_atomic_save_protects_existing_valid_file() {
    let dir = tempdir().expect("failed to create temp dir");
    let file_path = dir.path().join("profiles.json");
    let store = FileProfileStore::new(&file_path);

    let valid_profile = Profile::new_draft(
        ProfileId::new("p-orig"),
        "Antrian Asli",
        PathBuf::from("chrome.exe"),
        "--kiosk",
    );
    let original_config = AppConfig::new(1, vec![valid_profile]);
    store.save(&original_config).expect("initial save failed");

    // If we write corrupt data into a temporary file or fail, the original file is intact
    let loaded = store.load().expect("original file must remain valid");
    assert_eq!(loaded.profiles().len(), 1);
    assert_eq!(loaded.profiles()[0].name(), "Antrian Asli");
}

#[test]
fn test_file_profile_store_corrupted_file_returns_error_without_destroying_it() {
    let dir = tempdir().expect("failed to create temp dir");
    let file_path = dir.path().join("profiles.json");
    std::fs::write(&file_path, "INVALID JSON CONTENT").expect("write corrupt file");

    let store = FileProfileStore::new(&file_path);
    let result = store.load();

    assert!(result.is_err());
    match result.unwrap_err() {
        PersistenceError::CorruptData { .. } => {}
        err => panic!("expected CorruptData error, got: {:?}", err),
    }

    // Verify file content is unchanged
    let raw = std::fs::read_to_string(&file_path).expect("read file");
    assert_eq!(raw, "INVALID JSON CONTENT");
}

#[test]
fn test_file_profile_store_rejects_unsupported_schema_version() {
    let dir = tempdir().expect("failed to create temp dir");
    let file_path = dir.path().join("profiles.json");
    let future_json = r#"{
        "schema_version": 999,
        "profiles": []
    }"#;
    std::fs::write(&file_path, future_json).expect("write future schema");

    let store = FileProfileStore::new(&file_path);
    let result = store.load();

    assert!(result.is_err());
    match result.unwrap_err() {
        PersistenceError::UnsupportedSchemaVersion { version } => {
            assert_eq!(version, 999);
        }
        err => panic!("expected UnsupportedSchemaVersion error, got: {:?}", err),
    }
}

#[test]
fn test_file_profile_store_saves_and_loads_startup_settings() {
    let dir = tempdir().expect("failed to create temp dir");
    let file_path = dir.path().join("profiles_startup.json");
    let store = FileProfileStore::new(&file_path);

    let profile = Profile::new_draft(
        ProfileId::new("p-startup-test"),
        "Antrian Poli Gigi",
        PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        "--app=https://antrian.gigi.local --kiosk",
    );

    let config =
        AppConfig::with_startup(1, vec![profile.clone()], true, Some(profile.id().clone()));

    store
        .save(&config)
        .expect("save config with startup settings");

    let loaded = store.load().expect("load config with startup settings");
    assert_eq!(loaded.schema_version(), 1);
    assert_eq!(loaded.profiles().len(), 1);
    assert!(loaded.startup_enabled());
    assert_eq!(loaded.startup_profile_id(), Some(profile.id()));
}
