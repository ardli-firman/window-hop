use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;
use window_hop::application::{ApplicationError, LifecycleCoordinator};
use window_hop::domain::{DraftProfileInput, ProfileStatus};
use window_hop::persistence::{FileProfileStore, MemoryProfileStore};

#[test]
fn test_create_multiple_draft_profiles_and_verify_draft_status() {
    let store = Arc::new(MemoryProfileStore::new());
    let mut coordinator = LifecycleCoordinator::new(store);

    let id1 = coordinator
        .create_draft_profile(DraftProfileInput::new(
            "Antrian Poli Gigi",
            PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
            "--app=https://antrian.poli/gigi --kiosk",
        ))
        .expect("create profile 1");

    let id2 = coordinator
        .create_draft_profile(DraftProfileInput::new(
            "Antrian Poli Mata",
            PathBuf::from(r"C:\Program Files\Mozilla Firefox\firefox.exe"),
            "-kiosk https://antrian.poli/mata",
        ))
        .expect("create profile 2");

    assert_ne!(id1, id2, "Each profile must have unique consistent id");

    let profiles = coordinator.list_profiles();
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0].id(), &id1);
    assert_eq!(profiles[0].name(), "Antrian Poli Gigi");
    assert_eq!(profiles[0].status(), ProfileStatus::DraftNotReady);

    assert_eq!(profiles[1].id(), &id2);
    assert_eq!(profiles[1].name(), "Antrian Poli Mata");
    assert_eq!(profiles[1].status(), ProfileStatus::DraftNotReady);

    // Draft profiles cannot be activated because target monitor is unselected
    let activation_result = coordinator.can_activate(&id1);
    assert!(
        activation_result.is_err(),
        "Draft profiles must not be ready for activation"
    );
    match activation_result.unwrap_err() {
        ApplicationError::DraftNotReady { id } => assert_eq!(id, id1),
        err => panic!("expected DraftNotReady error, got: {:?}", err),
    }
}

#[test]
fn test_persist_and_reopen_profiles_across_application_restarts() {
    let dir = tempdir().expect("temp dir");
    let file_path = dir.path().join("window_hop_profiles.json");

    let store1 = Arc::new(FileProfileStore::new(&file_path));
    let mut app1 = LifecycleCoordinator::new(store1);

    let id1 = app1
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Umum",
                PathBuf::from(r"C:\Browsers\Chrome\chrome.exe"),
                "--kiosk",
            ),
        )
        .expect("save profile 1");

    let id2 = app1
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Spesialis",
                PathBuf::from(r"C:\Browsers\Edge\msedge.exe"),
                "--app=https://queue.local",
            ),
        )
        .expect("save profile 2");

    // Simulate closing Window Hop and opening it again
    let store2 = Arc::new(FileProfileStore::new(&file_path));
    let mut app2 = LifecycleCoordinator::new(store2);
    app2.load().expect("load application profiles on restart");

    let reloaded = app2.list_profiles();
    assert_eq!(reloaded.len(), 2);

    let p1 = app2.get_profile(&id1).expect("profile 1 must exist");
    assert_eq!(p1.name(), "Antrian Umum");
    assert_eq!(
        p1.browser_path(),
        &PathBuf::from(r"C:\Browsers\Chrome\chrome.exe")
    );
    assert_eq!(p1.custom_parameters(), "--kiosk");
    assert_eq!(p1.status(), ProfileStatus::DraftNotReady);

    let p2 = app2.get_profile(&id2).expect("profile 2 must exist");
    assert_eq!(p2.name(), "Antrian Spesialis");
    assert_eq!(
        p2.browser_path(),
        &PathBuf::from(r"C:\Browsers\Edge\msedge.exe")
    );
    assert_eq!(p2.custom_parameters(), "--app=https://queue.local");
    assert_eq!(p2.status(), ProfileStatus::DraftNotReady);
}

#[test]
fn test_save_failure_reports_error_and_does_not_mutate_in_memory_state() {
    let memory_store = Arc::new(MemoryProfileStore::new());
    let mut app = LifecycleCoordinator::new(memory_store.clone());

    app.save_draft_profile(
        None,
        DraftProfileInput::new("Profil Valid Awal", PathBuf::from("chrome.exe"), "--kiosk"),
    )
    .expect("initial save should succeed");

    assert_eq!(app.list_profiles().len(), 1);

    // Simulate I/O failure on next save
    memory_store.set_simulate_io_error(true);

    let save_res = app.save_draft_profile(
        None,
        DraftProfileInput::new(
            "Profil Tambahan Gagal Simpan",
            PathBuf::from("firefox.exe"),
            "",
        ),
    );
    assert!(
        save_res.is_err(),
        "Save must fail when store returns I/O error"
    );
    assert!(
        app.last_error().is_some(),
        "Coordinator must record error for UI"
    );

    // Assert that in-memory state was NOT mutated/dirtied on failure!
    assert_eq!(
        app.list_profiles().len(),
        1,
        "In-memory profiles must not be mutated when disk save fails"
    );
    assert_eq!(app.list_profiles()[0].name(), "Profil Valid Awal");

    // Verify last valid state in store was NOT corrupted
    memory_store.set_simulate_io_error(false);
    let mut restart_app = LifecycleCoordinator::new(memory_store.clone());
    restart_app.load().expect("load stored data");

    let profiles = restart_app.list_profiles();
    assert_eq!(
        profiles.len(),
        1,
        "Only the last successfully saved configuration should persist"
    );
    assert_eq!(profiles[0].name(), "Profil Valid Awal");
}

#[test]
fn test_corrupt_load_prevents_subsequent_save_from_overwriting_corrupted_file() {
    let dir = tempdir().expect("temp dir");
    let file_path = dir.path().join("profiles.json");
    std::fs::write(&file_path, "INVALID JSON CONTENT").expect("write corrupt file");

    let store = Arc::new(FileProfileStore::new(&file_path));
    let mut app = LifecycleCoordinator::new(store);

    let load_res = app.load();
    assert!(load_res.is_err(), "Load should fail on corrupted file");
    assert!(app.last_error().is_some());

    // Attempting to save now must be blocked to prevent destroying the user's damaged file
    let save_res = app.save_draft_profile(
        None,
        DraftProfileInput::new("New Profile", PathBuf::from("chrome.exe"), ""),
    );
    assert!(save_res.is_err());
    match save_res.unwrap_err() {
        ApplicationError::BlockedByCorruptState(_) => {}
        err => panic!("expected BlockedByCorruptState, got: {:?}", err),
    }

    // Verify corrupted file on disk was not overwritten
    let raw = std::fs::read_to_string(&file_path).expect("read file");
    assert_eq!(raw, "INVALID JSON CONTENT");
}

#[test]
fn test_validation_rejects_empty_name_or_empty_path() {
    let store = Arc::new(MemoryProfileStore::new());
    let mut app = LifecycleCoordinator::new(store);

    let err_empty_name = app.create_draft_profile(DraftProfileInput::new(
        "   ",
        PathBuf::from("chrome.exe"),
        "",
    ));
    assert!(err_empty_name.is_err());
    match err_empty_name.unwrap_err() {
        ApplicationError::ValidationError(msg) => {
            assert!(msg.to_lowercase().contains("nama"));
        }
        err => panic!("expected ValidationError, got: {:?}", err),
    }

    let err_empty_path =
        app.create_draft_profile(DraftProfileInput::new("Profil", PathBuf::from(""), ""));
    assert!(err_empty_path.is_err());
    match err_empty_path.unwrap_err() {
        ApplicationError::ValidationError(msg) => {
            assert!(
                msg.to_lowercase().contains("browser")
                    || msg.to_lowercase().contains("jalur")
                    || msg.to_lowercase().contains("path")
            );
        }
        err => panic!("expected ValidationError, got: {:?}", err),
    }
}
