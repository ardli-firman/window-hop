use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use window_hop::application::{
    ActivationResult, ApplicationError, LifecycleCoordinator, LifecycleTransitionOutcome,
    StopOutcome,
};
use window_hop::browsers::{BrowserArchitecture, BrowserCompatibilityStatus};
use window_hop::domain::{
    DraftProfileInput, ExitReason, MonitorDescriptor, MonitorIdentity, ProfileId,
    ProfileLifecycleState, ProfileStatus, TargetMonitorStatus,
};
use window_hop::persistence::{FileProfileStore, MemoryProfileStore, ProfileStore};
use window_hop::platform::{
    ActivationCommand, Clock, DetectedBrowser, LocalSingleInstanceCoordinator, ManagedProcess,
    MockBrowserDetector, MockClock, MockMonitorProvider, MockProcessLauncher, MockShortcutCreator,
    MockStartupManager, MockTrayProvider, ProcessLauncher, SingleInstanceCoordinator,
};

#[test]
fn test_create_multiple_draft_profiles_and_verify_draft_status() {
    let store = Arc::new(MemoryProfileStore::new());
    let mut coordinator = LifecycleCoordinator::from_store(store);

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
    let mut app1 = LifecycleCoordinator::from_store(store1);

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
    let mut app2 = LifecycleCoordinator::from_store(store2);
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
    let mut app = LifecycleCoordinator::from_store(memory_store.clone());

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
    let mut restart_app = LifecycleCoordinator::from_store(memory_store.clone());
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
    let mut app = LifecycleCoordinator::from_store(store);

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
    let mut app = LifecycleCoordinator::from_store(store);

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

#[test]
fn test_save_profile_with_target_monitor_and_evaluate_ready_status() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Primary Nurse Workstation".to_string(),
            is_primary: true,
        },
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-QUEUE-ROOM1"),
            name: "Room 1 Queue Display".to_string(),
            is_primary: false,
        },
    ]));
    let mut coordinator = LifecycleCoordinator::new(store, monitor_provider);

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Rawat Jalan",
                PathBuf::from("msedge.exe"),
                "--kiosk",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-ROOM1"))),
        )
        .expect("save profile with target monitor");

    let target_status = coordinator
        .get_target_monitor_status(&profile_id)
        .expect("get target monitor status");
    match target_status {
        TargetMonitorStatus::Available(desc) => {
            assert_eq!(desc.identity, MonitorIdentity::new("MON-QUEUE-ROOM1"));
            assert_eq!(desc.name, "Room 1 Queue Display");
        }
        other => panic!("expected TargetMonitorStatus::Available, got {:?}", other),
    }

    let profile_status = coordinator
        .get_profile_status(&profile_id)
        .expect("get profile status");
    assert_eq!(profile_status, ProfileStatus::Ready);

    let can_activate = coordinator
        .can_activate(&profile_id)
        .expect("profile with available target monitor should be ready to activate");
    assert!(can_activate);
}

#[test]
fn test_target_monitor_matching_is_invariant_to_enumeration_reordering() {
    let store = Arc::new(MemoryProfileStore::new());
    let m_primary = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Display 1".to_string(),
        is_primary: true,
    };
    let m_queue = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Display 2".to_string(),
        is_primary: false,
    };

    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        m_primary.clone(),
        m_queue.clone(),
    ]));
    let mut coordinator = LifecycleCoordinator::new(store, monitor_provider.clone());

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Farmasi",
                PathBuf::from("chrome.exe"),
                "--kiosk",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    // Order 1: [m_primary, m_queue]
    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::Ready
    );

    // Order 2: OS changes enumeration order to [m_queue, m_primary] (e.g. index 0 is now queue monitor)
    monitor_provider.set_monitors(vec![m_queue.clone(), m_primary.clone()]);
    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::Ready,
        "Target monitor recognition must not depend on display enumeration index"
    );

    let target_status = coordinator.get_target_monitor_status(&profile_id).unwrap();
    match target_status {
        TargetMonitorStatus::Available(desc) => {
            assert_eq!(desc.identity, MonitorIdentity::new("MON-QUEUE"));
        }
        other => panic!("expected Available, got {:?}", other),
    }
}

#[test]
fn test_target_monitor_reconnect_restores_ready_status() {
    let store = Arc::new(MemoryProfileStore::new());
    let m_primary = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Nurse Screen".to_string(),
        is_primary: true,
    };
    let m_queue = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Wall Screen".to_string(),
        is_primary: false,
    };

    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        m_primary.clone(),
        m_queue.clone(),
    ]));
    let mut coordinator = LifecycleCoordinator::new(store, monitor_provider.clone());

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Antrian Poli Bedah", PathBuf::from("chrome.exe"), "--kiosk")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::Ready
    );

    // Simulate cable disconnect / monitor powered off: only primary remains
    monitor_provider.set_monitors(vec![m_primary.clone()]);

    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::TargetUnavailable,
        "When target monitor disconnects, status must be TargetUnavailable"
    );

    // Simulate cable reconnected: monitor appears again with matching identity
    monitor_provider.set_monitors(vec![m_primary.clone(), m_queue.clone()]);

    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::Ready,
        "When matching target monitor reconnects, status must be restored to Ready"
    );
}

#[test]
fn test_can_activate_fails_when_target_monitor_unavailable_without_fallback_to_primary() {
    let store = Arc::new(MemoryProfileStore::new());
    // Only primary nurse workstation monitor is available
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY-WORKSTATION"),
        name: "Nurse Primary Screen".to_string(),
        is_primary: true,
    }]));
    let mut coordinator = LifecycleCoordinator::new(store, monitor_provider);

    // Profile targets an external queue monitor that is currently unplugged
    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Antrian Poli THT", PathBuf::from("chrome.exe"), "--kiosk")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-UNPLUGGED"))),
        )
        .expect("save profile");

    let status = coordinator.get_profile_status(&profile_id).unwrap();
    assert_eq!(status, ProfileStatus::TargetUnavailable);

    let target_status = coordinator.get_target_monitor_status(&profile_id).unwrap();
    match target_status {
        TargetMonitorStatus::Unavailable { recorded_identity } => {
            assert_eq!(
                recorded_identity,
                MonitorIdentity::new("MON-QUEUE-UNPLUGGED")
            );
        }
        other => panic!("expected TargetMonitorStatus::Unavailable, got {:?}", other),
    }

    // Must NOT fallback to primary workstation monitor; can_activate must fail
    let act_res = coordinator.can_activate(&profile_id);
    assert!(
        act_res.is_err(),
        "can_activate must fail when target monitor is unavailable; NO fallback allowed"
    );
    match act_res.unwrap_err() {
        ApplicationError::TargetMonitorUnavailable {
            id,
            recorded_identity,
        } => {
            assert_eq!(id, profile_id);
            assert_eq!(
                recorded_identity,
                MonitorIdentity::new("MON-QUEUE-UNPLUGGED")
            );
        }
        other => panic!("expected TargetMonitorUnavailable error, got {:?}", other),
    }
}

#[test]
fn test_can_activate_fails_when_target_monitor_is_ambiguous_and_prompts_reselection() {
    let store = Arc::new(MemoryProfileStore::new());
    let ambiguous_id = MonitorIdentity::new("AMBIGUOUS-ID-DUP");

    // Two identical screens plugged in with duplicate/ambiguous hardware IDs
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: ambiguous_id.clone(),
            name: "Duplicate Screen 1".to_string(),
            is_primary: false,
        },
        MonitorDescriptor {
            identity: ambiguous_id.clone(),
            name: "Duplicate Screen 2".to_string(),
            is_primary: false,
        },
    ]));
    let mut coordinator = LifecycleCoordinator::new(store, monitor_provider);

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Jantung",
                PathBuf::from("chrome.exe"),
                "--kiosk",
            )
            .with_target_monitor(Some(ambiguous_id.clone())),
        )
        .expect("save profile");

    let status = coordinator.get_profile_status(&profile_id).unwrap();
    assert_eq!(status, ProfileStatus::TargetAmbiguous);

    let target_status = coordinator.get_target_monitor_status(&profile_id).unwrap();
    match target_status {
        TargetMonitorStatus::Ambiguous {
            recorded_identity,
            match_count,
        } => {
            assert_eq!(recorded_identity, ambiguous_id);
            assert_eq!(match_count, 2);
        }
        other => panic!("expected TargetMonitorStatus::Ambiguous, got {:?}", other),
    }

    let act_res = coordinator.can_activate(&profile_id);
    assert!(
        act_res.is_err(),
        "can_activate must fail when target monitor is ambiguous"
    );
    match act_res.unwrap_err() {
        ApplicationError::TargetMonitorAmbiguous {
            id,
            recorded_identity,
            match_count,
        } => {
            assert_eq!(id, profile_id);
            assert_eq!(recorded_identity, ambiguous_id);
            assert_eq!(match_count, 2);
        }
        other => panic!("expected TargetMonitorAmbiguous error, got {:?}", other),
    }
}

#[test]
fn test_replaced_monitor_with_different_identity_requires_reselection() {
    let store = Arc::new(MemoryProfileStore::new());
    let old_id = MonitorIdentity::new("MON-OLD-DELL");
    let new_id = MonitorIdentity::new("MON-NEW-LG");

    // Initially old monitor was recorded
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: old_id.clone(),
        name: "Old Dell Screen".to_string(),
        is_primary: false,
    }]));
    let mut coordinator = LifecycleCoordinator::new(store, monitor_provider.clone());

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Antrian IGD", PathBuf::from("msedge.exe"), "--kiosk")
                .with_target_monitor(Some(old_id.clone())),
        )
        .expect("save profile");

    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::Ready
    );

    // Hardware was replaced in clinic: old monitor gone, new monitor plugged in
    monitor_provider.set_monitors(vec![MonitorDescriptor {
        identity: new_id.clone(),
        name: "New LG Screen".to_string(),
        is_primary: false,
    }]);

    // Old identity does not match -> TargetUnavailable, cannot activate
    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::TargetUnavailable
    );
    assert!(coordinator.can_activate(&profile_id).is_err());

    // Nurse reselects target monitor in UI and saves
    coordinator
        .save_draft_profile(
            Some(&profile_id),
            DraftProfileInput::new("Antrian IGD", PathBuf::from("msedge.exe"), "--kiosk")
                .with_target_monitor(Some(new_id.clone())),
        )
        .expect("reselect target monitor");

    // Now matches new monitor -> Ready!
    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::Ready
    );
    assert!(coordinator.can_activate(&profile_id).unwrap());
}

#[test]
fn test_activate_profile_fails_when_browser_unverified() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let opera_exe = create_mock_pe_executable(dir.path(), "opera.exe", 0x8664);

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Antrian Gigi Opera", opera_exe.clone(), "--kiosk")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    // Can activate must report browser not verified
    let can_res = coordinator.can_activate(&profile_id);
    assert!(can_res.is_err());
    match can_res.unwrap_err() {
        ApplicationError::BrowserNotVerified(path, reason) => {
            assert!(path.contains("opera.exe"));
            assert!(reason.contains("belum diverifikasi"));
        }
        err => panic!("expected BrowserNotVerified, got: {:?}", err),
    }

    // Attempting to activate must fail and launch NO process
    let act_res = coordinator.activate_profile(&profile_id);
    assert!(act_res.is_err());
    assert_eq!(launcher.launched_processes().len(), 0);
}

#[test]
fn test_activate_profile_fails_when_custom_parameters_conflict() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Edge Konflik",
                PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
                "--user-data-dir=C:\\NurseWorkingProfile",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    // Pre-activation check can_activate must reject conflicting isolation parameter
    let can_res = coordinator.can_activate(&profile_id);
    assert!(can_res.is_err());
    match can_res.unwrap_err() {
        ApplicationError::Browser(
            window_hop::browsers::BrowserError::ConflictingIsolationParameter { arg, .. },
        ) => {
            assert!(arg.contains("--user-data-dir"));
        }
        err => panic!("expected ConflictingIsolationParameter, got: {:?}", err),
    }

    let act_res = coordinator.activate_profile(&profile_id);
    assert!(act_res.is_err());
    match act_res.unwrap_err() {
        ApplicationError::Browser(
            window_hop::browsers::BrowserError::ConflictingIsolationParameter { arg, .. },
        ) => {
            assert!(arg.contains("--user-data-dir"));
        }
        err => panic!("expected ConflictingIsolationParameter, got: {:?}", err),
    }
    assert_eq!(launcher.launched_processes().len(), 0);
}

#[test]
fn test_activate_profile_launches_isolated_edge_on_target_monitor_preserving_nurse_browser() {
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-ROOM2"),
        name: "Room 2 Queue Monitor".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        target_mon.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_edge");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir.clone(),
    );

    // Simulate nurse's already running working Edge instance
    let nurse_working_browser_plan = window_hop::browsers::LaunchPlan {
        executable: PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
        arguments: vec!["https://hospital.simrs/nurse-dashboard".to_string()],
        user_data_dir: PathBuf::from(r"C:\Users\Nurse\AppData\Local\Microsoft\Edge\User Data"),
        login_retention: window_hop::browsers::LoginRetentionNote::SessionIsolated {
            explanation: "Nurse default session".to_string(),
        },
    };
    let nurse_process = launcher
        .launch(
            &nurse_working_browser_plan,
            &MonitorDescriptor {
                identity: MonitorIdentity::new("MON-PRIMARY"),
                name: "Workstation Main Display".to_string(),
                is_primary: true,
            },
        )
        .expect("nurse browser running");
    let nurse_pid = nurse_process.pid();

    // Now save and activate Window Hop queue profile
    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poliklinik Mata",
                PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
                r#"--kiosk "https://antrian.poli/mata?room=2""#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-ROOM2"))),
        )
        .expect("save profile");

    assert!(coordinator.can_activate(&profile_id).unwrap());

    let activation_result = coordinator
        .activate_profile(&profile_id)
        .expect("activate profile");

    match activation_result {
        ActivationResult::Launched {
            pid,
            target_monitor,
            login_retention,
        } => {
            assert_ne!(
                pid, nurse_pid,
                "Queue profile must have its own isolated PID"
            );
            assert_eq!(target_monitor, target_mon);
            match login_retention {
                window_hop::browsers::LoginRetentionNote::KioskOrInPrivateNoRetention {
                    explanation,
                } => {
                    assert!(explanation.contains("kiosk"));
                }
                other => panic!("expected KioskOrInPrivateNoRetention, got: {:?}", other),
            }

            // Verify queue process launch plan
            let launched = launcher.launched_processes();
            assert_eq!(launched.len(), 2, "Nurse process + queue process");
            let queue_proc = &launched[1];
            assert_eq!(queue_proc.pid(), pid);
            assert_eq!(queue_proc.current_monitor(), target_mon);

            let expected_user_data_dir = base_dir
                .join("profiles")
                .join(profile_id.as_str())
                .join("browser_session");
            assert!(
                queue_proc
                    .plan()
                    .arguments
                    .iter()
                    .any(|arg| arg.contains(&expected_user_data_dir.display().to_string())),
                "Queue profile must use its own isolated user-data-dir"
            );
            assert!(queue_proc
                .plan()
                .arguments
                .iter()
                .any(|arg| arg == "--kiosk"));
            assert!(queue_proc
                .plan()
                .arguments
                .iter()
                .any(|arg| arg == "https://antrian.poli/mata?room=2"));
        }
        ActivationResult::Reused { .. } => {
            panic!("first activation should be Launched, not Reused")
        }
        other => panic!("expected Launched, got: {:?}", other),
    }

    // Verify nurse working browser is still alive and was NOT touched or moved!
    assert!(
        nurse_process.is_alive(),
        "Nurse working browser must remain alive"
    );
    let nurse_mock = launcher.launched_processes()[0].clone();
    assert!(
        !nurse_mock.close_requested(),
        "Nurse browser must not receive close request"
    );
    assert!(
        !nurse_mock.force_stopped(),
        "Nurse browser must not be stopped"
    );
    assert_eq!(
        nurse_mock.current_monitor().identity.as_str(),
        "MON-PRIMARY",
        "Nurse browser must not be moved"
    );
}

#[test]
fn test_repeated_activation_reuses_running_window_and_restores_placement_without_duplication() {
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![target_mon.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_reuse");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Gigi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    // First activation: launches fresh process
    let act1 = coordinator
        .activate_profile(&profile_id)
        .expect("first activation");
    let pid1 = match act1 {
        ActivationResult::Launched {
            pid,
            target_monitor,
            ..
        } => {
            assert_eq!(target_monitor, target_mon);
            pid
        }
        ActivationResult::Reused { .. } => panic!("expected Launched on first activation"),
        other => panic!("expected Launched on first activation, got: {:?}", other),
    };

    assert_eq!(launcher.launched_processes().len(), 1);
    let proc = launcher.launched_processes()[0].clone();
    assert_eq!(proc.pid(), pid1);
    assert_eq!(proc.reposition_count(), 0);

    // Simulate window being moved accidentally by user to another monitor
    let other_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-MOVED"),
        name: "Moved Display".to_string(),
        is_primary: true,
    };
    proc.reposition(&other_monitor).expect("moved");
    assert_eq!(proc.current_monitor(), other_monitor);
    assert_eq!(proc.reposition_count(), 1);

    // Second activation (same configuration, repeated shortcut click):
    // MUST reuse existing window and restore placement to target monitor WITHOUT creating duplicate process!
    let act2 = coordinator
        .activate_profile(&profile_id)
        .expect("second activation");
    match act2 {
        ActivationResult::Reused {
            pid,
            target_monitor,
            ..
        } => {
            assert_eq!(pid, pid1, "Reused instance must have same PID");
            assert_eq!(target_monitor, target_mon);
        }
        ActivationResult::Launched { .. } => {
            panic!("expected Reused on repeated activation, not Launched")
        }
        other => panic!("expected Reused on repeated activation, got: {:?}", other),
    }

    // No duplicate process launched!
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "Must NOT launch a duplicate process"
    );
    // Reposition was called to restore window back to target monitor!
    assert_eq!(proc.reposition_count(), 2);
    assert_eq!(
        proc.current_monitor(),
        target_mon,
        "Window must be returned to target monitor"
    );

    // Now simulate browser exiting / closing
    proc.simulate_exit();
    assert!(!coordinator.is_profile_running(&profile_id));

    // Third activation after exit: should launch a new instance
    let act3 = coordinator
        .activate_profile(&profile_id)
        .expect("third activation after exit");
    match act3 {
        ActivationResult::Launched {
            pid,
            target_monitor,
            ..
        } => {
            assert_ne!(pid, pid1, "New process must have different PID");
            assert_eq!(target_monitor, target_mon);
        }
        ActivationResult::Reused { .. } => {
            panic!("expected Launched when prior process has exited")
        }
        other => panic!(
            "expected Launched when prior process has exited, got: {:?}",
            other
        ),
    }
    assert_eq!(
        launcher.launched_processes().len(),
        2,
        "Second process launched after exit"
    );
}

#[test]
fn test_activation_with_changed_configuration_stops_old_instance_and_launches_new_instance() {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let mon2 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-2"),
        name: "Queue Display 2".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone(), mon2.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_config_change");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian IGD",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/v1",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save initial profile");

    // Launch with v1 config
    let act1 = coordinator
        .activate_profile(&profile_id)
        .expect("launch v1");
    let pid1 = match act1 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched"),
    };

    let proc1 = launcher.launched_processes()[0].clone();
    assert!(proc1.is_alive());

    // Nurse updates profile configuration (new URL / parameters and new target monitor)
    coordinator
        .save_draft_profile(
            Some(&profile_id),
            DraftProfileInput::new(
                "Antrian IGD",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/v2",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-2"))),
        )
        .expect("save edited profile");

    // Next activation must apply latest saved configuration:
    // It must stop old instance and launch new instance!
    let act2 = coordinator
        .activate_profile(&profile_id)
        .expect("launch v2");
    let pid2 = match act2 {
        ActivationResult::Launched {
            pid,
            target_monitor,
            ..
        } => {
            assert_eq!(target_monitor, mon2);
            pid
        }
        ActivationResult::Reused { .. } => panic!("expected Launched with new config, not Reused"),
        other => panic!("expected Launched with new config, got: {:?}", other),
    };

    assert_ne!(pid1, pid2, "New instance must have different PID");
    assert!(!proc1.is_alive(), "Old instance must have been stopped");
    assert!(proc1.close_requested());
    assert_eq!(launcher.launched_processes().len(), 2);
    let proc2 = launcher.launched_processes()[1].clone();
    assert_eq!(proc2.pid(), pid2);
    assert_eq!(proc2.current_monitor(), mon2);
    assert!(proc2
        .plan()
        .arguments
        .iter()
        .any(|arg| arg == "https://antrian.local/v2"));
}

#[test]
fn test_stop_profile_when_not_running_returns_not_running() {
    let store = Arc::new(MemoryProfileStore::new());
    let mut coordinator = LifecycleCoordinator::from_store(store);

    let id = ProfileId::generate();
    let outcome = coordinator.stop_profile(&id).expect("stop profile");
    assert_eq!(outcome, StopOutcome::NotRunning);
}

#[test]
fn test_stop_profile_cooperative_exits_gracefully_within_deadline() {
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![target_mon.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_graceful");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock.clone(),
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Gigi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let act = coordinator.activate_profile(&profile_id).expect("activate");
    let pid = match act {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched"),
    };

    let proc = launcher.launched_processes()[0].clone();
    assert!(proc.is_alive());

    // Stop profile (cooperative process simulates exit when requested)
    let outcome = coordinator.stop_profile(&profile_id).expect("stop profile");
    assert_eq!(outcome, StopOutcome::Graceful { pid });
    assert!(proc.close_requested());
    assert!(
        !proc.force_stopped(),
        "Must NOT force stop cooperative process"
    );
    assert!(!coordinator.is_profile_running(&profile_id));
}

#[test]
fn test_stop_profile_unresponsive_force_stops_after_3_seconds_preserving_nurse_browser() {
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        target_mon.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_forced");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock.clone(),
        base_dir,
    );

    // Nurse working browser is running
    let nurse_plan = window_hop::browsers::LaunchPlan {
        executable: PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
        arguments: vec!["https://hospital.simrs/nurse-dashboard".to_string()],
        user_data_dir: PathBuf::from(r"C:\Users\Nurse\AppData\Local\Microsoft\Edge\User Data"),
        login_retention: window_hop::browsers::LoginRetentionNote::SessionIsolated {
            explanation: "Nurse default session".to_string(),
        },
    };
    let nurse_process = launcher
        .launch(
            &nurse_plan,
            &MonitorDescriptor {
                identity: MonitorIdentity::new("MON-PRIMARY"),
                name: "Workstation Main Display".to_string(),
                is_primary: true,
            },
        )
        .expect("nurse browser running");

    // Save and activate queue profile
    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Gigi Kiosk",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let act = coordinator.activate_profile(&profile_id).expect("activate");
    let queue_pid = match act {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched"),
    };

    let queue_proc = launcher.launched_processes()[1].clone();
    // Simulate unresponsive queue browser (e.g. hung or confirmation dialog blocking exit)
    queue_proc.set_refuse_graceful(true);

    let t0 = clock.now();
    let outcome = coordinator.stop_profile(&profile_id).expect("stop profile");
    let t1 = clock.now();

    // Verification:
    // 1. Outcome must be Forced after 3 seconds
    assert_eq!(outcome, StopOutcome::Forced { pid: queue_pid });
    assert!(
        t1.duration_since(t0) >= std::time::Duration::from_secs(3),
        "Must wait for 3-second deadline before force stopping"
    );

    // 2. Queue process received close request first, then force stop
    assert!(
        queue_proc.close_requested(),
        "Must have requested graceful close first"
    );
    assert!(
        queue_proc.force_stopped(),
        "Must have force stopped unresponsive process"
    );
    assert!(!queue_proc.is_alive());
    assert!(!coordinator.is_profile_running(&profile_id));

    // 3. Nurse working browser MUST be completely untouched!
    assert!(
        nurse_process.is_alive(),
        "Nurse working browser must remain alive"
    );
    let nurse_mock = launcher.launched_processes()[0].clone();
    assert!(
        !nurse_mock.close_requested(),
        "Nurse browser must not receive close request"
    );
    assert!(
        !nurse_mock.force_stopped(),
        "Nurse browser must not be force stopped"
    );
}

#[test]
fn test_activation_when_target_monitor_unavailable_enters_waiting_state_without_launching_browser()
{
    let store = Arc::new(MemoryProfileStore::new());
    // Only primary workstation monitor is available; target monitor MON-QUEUE-FUTURE is missing
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Workstation Main Display".to_string(),
        is_primary: true,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_waiting_activation");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Gigi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-FUTURE"))),
        )
        .expect("save profile");

    // Activation must enter waiting state without launching a browser process
    let act_res = coordinator
        .activate_profile(&profile_id)
        .expect("activation when monitor is unavailable should succeed by waiting");

    match act_res {
        ActivationResult::WaitingForMonitor { target_monitor } => {
            assert_eq!(
                target_monitor,
                MonitorIdentity::new("MON-QUEUE-FUTURE"),
                "Must wait for the recorded target monitor"
            );
        }
        other => panic!("expected WaitingForMonitor, got: {:?}", other),
    }

    // Crucial: NO browser process is launched!
    assert!(
        launcher.launched_processes().is_empty(),
        "Must NOT launch browser when target monitor is unavailable"
    );
    assert!(
        !coordinator.is_profile_running(&profile_id),
        "Profile is waiting, not running"
    );

    // Lifecycle state must be WaitingForMonitor
    let state = coordinator
        .get_lifecycle_state(&profile_id)
        .expect("lifecycle state");
    assert_eq!(
        state,
        ProfileLifecycleState::WaitingForMonitor {
            target_monitor: MonitorIdentity::new("MON-QUEUE-FUTURE")
        }
    );
}

#[test]
fn test_profile_waiting_for_monitor_launches_automatically_when_target_monitor_becomes_available() {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-DELAYED"),
        name: "Antrian Delayed Display".to_string(),
        is_primary: false,
    };
    // Initially only primary display is available
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Workstation Main Display".to_string(),
        is_primary: true,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_waiting_recovery");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Gigi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-DELAYED"))),
        )
        .expect("save profile");

    // 1. Activate while monitor is not yet connected
    let act = coordinator
        .activate_profile(&profile_id)
        .expect("activate waiting");
    assert_eq!(
        act,
        ActivationResult::WaitingForMonitor {
            target_monitor: MonitorIdentity::new("MON-QUEUE-DELAYED")
        }
    );
    assert!(launcher.launched_processes().is_empty());
    assert!(!coordinator.is_profile_running(&profile_id));

    // 2. Delayed monitor is now plugged in
    monitor_provider.set_monitors(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor.clone(),
    ]);

    // 3. Coordinator handles monitor changes
    let transitions = coordinator
        .handle_monitors_changed()
        .expect("handle monitors changed");

    // 4. Verification: profile was automatically launched!
    assert_eq!(transitions.len(), 1);
    match &transitions[0] {
        LifecycleTransitionOutcome::Restored {
            profile_id: pid_restored,
            pid,
            target_monitor,
        } => {
            assert_eq!(pid_restored, &profile_id);
            assert_eq!(target_monitor, &queue_monitor);
            assert_eq!(launcher.launched_processes().len(), 1);
            assert_eq!(&launcher.launched_processes()[0].pid(), pid);
            assert_eq!(
                launcher.launched_processes()[0].current_monitor(),
                queue_monitor
            );
        }
        other => panic!("expected Restored outcome, got: {:?}", other),
    }

    assert!(coordinator.is_profile_running(&profile_id));
    let state = coordinator
        .get_lifecycle_state(&profile_id)
        .expect("lifecycle state");
    match state {
        ProfileLifecycleState::Running {
            pid,
            target_monitor,
        } => {
            assert_eq!(target_monitor, queue_monitor);
            assert_eq!(pid, launcher.launched_processes()[0].pid());
        }
        other => panic!("expected Running state, got: {:?}", other),
    }
}

#[test]
fn test_monitor_disconnect_requests_graceful_close_and_transitions_to_disconnected() {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_disconnect_graceful");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Gigi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let act = coordinator.activate_profile(&profile_id).expect("activate");
    let queue_pid = match act {
        ActivationResult::Launched { pid, .. } => pid,
        other => panic!("expected Launched, got: {:?}", other),
    };

    let proc = launcher.launched_processes()[0].clone();
    assert!(proc.is_alive());

    // Disconnect monitor (cable unplugged)
    monitor_provider.set_monitors(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Workstation Main Display".to_string(),
        is_primary: true,
    }]);

    let transitions = coordinator
        .handle_monitors_changed()
        .expect("handle monitors changed");

    assert_eq!(transitions.len(), 1);
    match &transitions[0] {
        LifecycleTransitionOutcome::Disconnected {
            profile_id: pid_disc,
            pid,
            forced,
        } => {
            assert_eq!(pid_disc, &profile_id);
            assert_eq!(pid, &queue_pid);
            assert!(!forced, "Cooperative process should exit gracefully");
        }
        other => panic!("expected Disconnected outcome, got: {:?}", other),
    }

    assert!(proc.close_requested());
    assert!(!proc.force_stopped());
    assert!(!proc.is_alive());
    assert!(!coordinator.is_profile_running(&profile_id));

    let state = coordinator
        .get_lifecycle_state(&profile_id)
        .expect("lifecycle state");
    assert_eq!(
        state,
        ProfileLifecycleState::Disconnected {
            target_monitor: MonitorIdentity::new("MON-QUEUE")
        }
    );
}

#[test]
fn test_monitor_disconnect_unresponsive_force_stops_after_3_seconds_and_preserves_nurse_browser() {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_disconnect_unresponsive");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock.clone(),
        base_dir,
    );

    // Nurse working browser is running on MON-PRIMARY
    let nurse_plan = window_hop::browsers::LaunchPlan {
        executable: PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
        arguments: vec!["https://hospital.simrs/nurse-dashboard".to_string()],
        user_data_dir: PathBuf::from(r"C:\Users\Nurse\AppData\Local\Microsoft\Edge\User Data"),
        login_retention: window_hop::browsers::LoginRetentionNote::SessionIsolated {
            explanation: "Nurse default session".to_string(),
        },
    };
    let nurse_process = launcher
        .launch(
            &nurse_plan,
            &MonitorDescriptor {
                identity: MonitorIdentity::new("MON-PRIMARY"),
                name: "Workstation Main Display".to_string(),
                is_primary: true,
            },
        )
        .expect("nurse browser running");

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Gigi Hung",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let act = coordinator.activate_profile(&profile_id).expect("activate");
    let queue_pid = match act {
        ActivationResult::Launched { pid, .. } => pid,
        other => panic!("expected Launched, got: {:?}", other),
    };

    let queue_proc = launcher.launched_processes()[1].clone();
    // Simulate unresponsive queue browser (e.g. hung / confirmation dialog blocking exit)
    queue_proc.set_refuse_graceful(true);

    // Monitor unplugs
    monitor_provider.set_monitors(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Workstation Main Display".to_string(),
        is_primary: true,
    }]);

    let t0 = clock.now();
    let transitions = coordinator
        .handle_monitors_changed()
        .expect("handle monitors changed");
    let t1 = clock.now();

    assert_eq!(transitions.len(), 1);
    match &transitions[0] {
        LifecycleTransitionOutcome::Disconnected {
            profile_id: pid_disc,
            pid,
            forced,
        } => {
            assert_eq!(pid_disc, &profile_id);
            assert_eq!(pid, &queue_pid);
            assert!(forced, "Unresponsive process must be force stopped");
        }
        other => panic!("expected Disconnected outcome, got: {:?}", other),
    }

    assert!(
        t1.duration_since(t0) >= std::time::Duration::from_secs(3),
        "Must wait for 3-second deadline before force stopping"
    );
    assert!(queue_proc.close_requested());
    assert!(queue_proc.force_stopped());
    assert!(!queue_proc.is_alive());
    assert!(!coordinator.is_profile_running(&profile_id));

    let state = coordinator
        .get_lifecycle_state(&profile_id)
        .expect("lifecycle state");
    assert_eq!(
        state,
        ProfileLifecycleState::Disconnected {
            target_monitor: MonitorIdentity::new("MON-QUEUE")
        }
    );

    // Nurse working browser MUST be untouched!
    assert!(nurse_process.is_alive());
    let nurse_mock = launcher.launched_processes()[0].clone();
    assert!(!nurse_mock.close_requested());
    assert!(!nurse_mock.force_stopped());
}

#[test]
fn test_reconnect_matching_identity_restores_only_waiting_or_disconnected_profiles() {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-RECONNECT"),
        name: "Queue Reconnect Screen".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_reconnect_only_target");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock,
        base_dir,
    );

    // Profile 1: Active queue profile that will experience monitor disconnect
    let p1_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Gigi 1",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/gigi",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-RECONNECT"))),
        )
        .expect("save p1");

    // Profile 2: Another profile configured for the SAME monitor, but NEVER activated (Idle)
    let p2_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Gigi 2 (Idle)",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/gigi2",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-RECONNECT"))),
        )
        .expect("save p2");

    // Activate p1
    let act1 = coordinator.activate_profile(&p1_id).expect("activate p1");
    let initial_p1_pid = match act1 {
        ActivationResult::Launched { pid, .. } => pid,
        other => panic!("expected Launched, got: {:?}", other),
    };
    assert_eq!(launcher.launched_processes().len(), 1);

    // Monitor unplugs (cable disconnect)
    monitor_provider.set_monitors(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Workstation Main Display".to_string(),
        is_primary: true,
    }]);
    let disc_transitions = coordinator
        .handle_monitors_changed()
        .expect("handle disconnect");
    assert_eq!(disc_transitions.len(), 1);
    assert_eq!(
        coordinator.get_lifecycle_state(&p1_id).unwrap(),
        ProfileLifecycleState::Disconnected {
            target_monitor: MonitorIdentity::new("MON-QUEUE-RECONNECT")
        }
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&p2_id).unwrap(),
        ProfileLifecycleState::Idle
    );

    // Cable is reconnected! Same monitor identity returns
    monitor_provider.set_monitors(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor.clone(),
    ]);

    let reconn_transitions = coordinator
        .handle_monitors_changed()
        .expect("handle reconnect");

    // Only p1 (which was Disconnected) is restored!
    // p2 (which was Idle) is NOT launched!
    assert_eq!(
        reconn_transitions.len(),
        1,
        "Only disconnected profile must be restored on reconnect"
    );
    match &reconn_transitions[0] {
        LifecycleTransitionOutcome::Restored {
            profile_id,
            pid,
            target_monitor,
        } => {
            assert_eq!(profile_id, &p1_id);
            assert_ne!(
                pid, &initial_p1_pid,
                "Restored profile must have a new process instance"
            );
            assert_eq!(target_monitor, &queue_monitor);
        }
        other => panic!("expected Restored, got: {:?}", other),
    }

    assert!(coordinator.is_profile_running(&p1_id));
    assert!(
        !coordinator.is_profile_running(&p2_id),
        "Idle profile must NOT be launched"
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&p2_id).unwrap(),
        ProfileLifecycleState::Idle
    );
}

#[test]
fn test_crash_transitions_to_exited_and_disables_reconnect_recovery_no_retry_loop() {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-CRASH"),
        name: "Queue Screen".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_crash_no_recovery");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Gigi Crash",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-CRASH"))),
        )
        .expect("save profile");

    let act = coordinator.activate_profile(&profile_id).expect("activate");
    let queue_pid = match act {
        ActivationResult::Launched { pid, .. } => pid,
        other => panic!("expected Launched, got: {:?}", other),
    };
    assert_eq!(launcher.launched_processes().len(), 1);

    // 1. Browser crashes (spontaneous process crash)
    let proc = launcher.launched_processes()[0].clone();
    proc.simulate_crash();

    let exit_outcome = coordinator.handle_process_exit(queue_pid, ExitReason::Crash);
    assert_eq!(
        exit_outcome,
        Some(LifecycleTransitionOutcome::Exited {
            profile_id: profile_id.clone(),
            pid: queue_pid,
            reason: ExitReason::Crash,
        })
    );

    // 2. Must remain stopped: NO retry loop (no automatic restart)
    assert!(
        !coordinator.is_profile_running(&profile_id),
        "Crashed profile must remain stopped"
    );
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "No new process should be launched (no retry loop)"
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Exited {
            reason: ExitReason::Crash
        }
    );

    // 3. Monitor unplugs and reconnects later
    monitor_provider.set_monitors(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Workstation Main Display".to_string(),
        is_primary: true,
    }]);
    coordinator
        .handle_monitors_changed()
        .expect("handle disconnect");

    monitor_provider.set_monitors(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor.clone(),
    ]);
    let reconn_transitions = coordinator
        .handle_monitors_changed()
        .expect("handle reconnect");

    // Reconnect MUST NOT restore crashed profile!
    assert!(
        reconn_transitions.is_empty(),
        "Reconnect must not restore crashed profile"
    );
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "Still only the original process, no restart"
    );
    assert!(!coordinator.is_profile_running(&profile_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Exited {
            reason: ExitReason::Crash
        }
    );
}

#[test]
fn test_manual_close_transitions_to_exited_and_disables_reconnect_recovery() {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-MANUAL"),
        name: "Queue Screen".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_manual_close");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Gigi Manual Close",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-MANUAL"))),
        )
        .expect("save profile");

    let act = coordinator.activate_profile(&profile_id).expect("activate");
    let queue_pid = match act {
        ActivationResult::Launched { pid, .. } => pid,
        other => panic!("expected Launched, got: {:?}", other),
    };

    // User closes browser window manually (simulate manual close)
    let proc = launcher.launched_processes()[0].clone();
    proc.simulate_manual_close();

    // Checked via handle_process_exit
    let exit_outcome = coordinator.handle_process_exit(queue_pid, ExitReason::ManualClose);
    assert_eq!(
        exit_outcome,
        Some(LifecycleTransitionOutcome::Exited {
            profile_id: profile_id.clone(),
            pid: queue_pid,
            reason: ExitReason::ManualClose,
        })
    );

    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Exited {
            reason: ExitReason::ManualClose
        }
    );

    // Cable disconnect and reconnect
    monitor_provider.set_monitors(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Workstation Main Display".to_string(),
        is_primary: true,
    }]);
    coordinator.handle_monitors_changed().expect("disconnect");

    monitor_provider.set_monitors(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor.clone(),
    ]);
    let reconn = coordinator.handle_monitors_changed().expect("reconnect");

    // Manually closed profile remains stopped
    assert!(reconn.is_empty());
    assert!(!coordinator.is_profile_running(&profile_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Exited {
            reason: ExitReason::ManualClose
        }
    );
}

#[test]
fn test_explicit_activation_can_restart_crashed_or_disconnected_profile() {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-RESTART"),
        name: "Queue Screen".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_explicit_restart");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Gigi Restartable",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-RESTART"))),
        )
        .expect("save profile");

    // 1. Initial activation
    let act1 = coordinator.activate_profile(&profile_id).expect("activate");
    let pid1 = match act1 {
        ActivationResult::Launched { pid, .. } => pid,
        other => panic!("expected Launched, got: {:?}", other),
    };

    // 2. Browser crashes
    let proc1 = launcher.launched_processes()[0].clone();
    proc1.simulate_crash();
    coordinator.handle_process_exit(pid1, ExitReason::Crash);

    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Exited {
            reason: ExitReason::Crash
        }
    );
    assert!(!coordinator.is_profile_running(&profile_id));

    // 3. Nurse explicitly clicks shortcut / activates profile again
    let act2 = coordinator
        .activate_profile(&profile_id)
        .expect("explicit restart after crash");
    let pid2 = match act2 {
        ActivationResult::Launched { pid, .. } => pid,
        other => panic!("expected Launched on explicit restart, got: {:?}", other),
    };

    assert_ne!(pid1, pid2, "Restarted profile must have fresh PID");
    assert_eq!(launcher.launched_processes().len(), 2);
    assert!(coordinator.is_profile_running(&profile_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: pid2,
            target_monitor: queue_monitor.clone(),
        }
    );

    // 4. Test explicit activation when monitor is disconnected -> should transition to WaitingForMonitor
    // Disconnect monitor
    monitor_provider.set_monitors(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Workstation Main Display".to_string(),
        is_primary: true,
    }]);
    coordinator.handle_monitors_changed().expect("disconnect");
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Disconnected {
            target_monitor: MonitorIdentity::new("MON-QUEUE-RESTART")
        }
    );

    // Explicit activation when monitor is unavailable
    let act3 = coordinator
        .activate_profile(&profile_id)
        .expect("explicit activation while monitor unavailable");
    assert_eq!(
        act3,
        ActivationResult::WaitingForMonitor {
            target_monitor: MonitorIdentity::new("MON-QUEUE-RESTART")
        }
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::WaitingForMonitor {
            target_monitor: MonitorIdentity::new("MON-QUEUE-RESTART")
        }
    );
}

#[test]
fn test_replaced_monitor_does_not_trigger_fallback_or_launch_profile() {
    let store = Arc::new(MemoryProfileStore::new());
    let old_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-ORIGINAL-OLD"),
        name: "Original Screen".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        old_monitor.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_replaced_monitor");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Gigi Replaced",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-ORIGINAL-OLD"))),
        )
        .expect("save profile");

    coordinator.activate_profile(&profile_id).expect("activate");
    assert_eq!(launcher.launched_processes().len(), 1);

    // Old monitor unplugs
    monitor_provider.set_monitors(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Workstation Main Display".to_string(),
        is_primary: true,
    }]);
    coordinator.handle_monitors_changed().expect("disconnect");
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Disconnected {
            target_monitor: MonitorIdentity::new("MON-ORIGINAL-OLD")
        }
    );

    // Replaced monitor is plugged in (different hardware identity!)
    let replaced_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-REPLACED-NEW"),
        name: "Brand New Replacement Screen".to_string(),
        is_primary: false,
    };
    monitor_provider.set_monitors(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        replaced_monitor,
    ]);

    let transitions = coordinator
        .handle_monitors_changed()
        .expect("handle monitors changed with replaced monitor");

    // Must NOT guess the replaced monitor and must NOT fallback to primary!
    assert!(
        transitions.is_empty(),
        "Must NOT launch profile on replaced monitor"
    );
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "No new process launched"
    );
    assert!(!coordinator.is_profile_running(&profile_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Disconnected {
            target_monitor: MonitorIdentity::new("MON-ORIGINAL-OLD")
        }
    );
}

#[test]
fn test_ambiguous_monitor_does_not_trigger_fallback_or_launch_profile() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_id = MonitorIdentity::new("MON-AMBIGUOUS-DUP");
    // Initially only primary
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Workstation Main Display".to_string(),
        is_primary: true,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_ambiguous_reconnect");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Gigi Ambiguous",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(monitor_id.clone())),
        )
        .expect("save profile");

    let act = coordinator
        .activate_profile(&profile_id)
        .expect("activate waiting");
    assert_eq!(
        act,
        ActivationResult::WaitingForMonitor {
            target_monitor: monitor_id.clone()
        }
    );

    // Two identical screens plugged in with duplicate/ambiguous hardware IDs
    monitor_provider.set_monitors(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        MonitorDescriptor {
            identity: monitor_id.clone(),
            name: "Duplicate Screen 1".to_string(),
            is_primary: false,
        },
        MonitorDescriptor {
            identity: monitor_id.clone(),
            name: "Duplicate Screen 2".to_string(),
            is_primary: false,
        },
    ]);

    let transitions = coordinator
        .handle_monitors_changed()
        .expect("handle monitors changed with ambiguous monitor");

    // Must NOT launch on ambiguous match and must NOT fallback to primary!
    assert!(
        transitions.is_empty(),
        "Must NOT launch profile when monitor is ambiguous"
    );
    assert!(launcher.launched_processes().is_empty());
    assert!(!coordinator.is_profile_running(&profile_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::WaitingForMonitor {
            target_monitor: monitor_id
        }
    );
}

#[test]
fn test_late_exit_event_from_old_instance_does_not_mutate_state_or_resurrect_profile() {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-LATE"),
        name: "Queue Screen".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_late_events");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Gigi Late",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-LATE"))),
        )
        .expect("save profile");

    // Launch instance 1
    let act1 = coordinator.activate_profile(&profile_id).expect("activate");
    let pid1 = match act1 {
        ActivationResult::Launched { pid, .. } => pid,
        other => panic!("expected Launched, got: {:?}", other),
    };

    // Monitor disconnects: instance 1 is stopped by Window Hop and state becomes Disconnected
    monitor_provider.set_monitors(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Workstation Main Display".to_string(),
        is_primary: true,
    }]);
    coordinator.handle_monitors_changed().expect("disconnect");
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Disconnected {
            target_monitor: MonitorIdentity::new("MON-QUEUE-LATE")
        }
    );

    // Now, a delayed exit event arrives for the OLD instance pid1 (e.g. OS watcher reported exit late)
    let late_outcome = coordinator.handle_process_exit(pid1, ExitReason::Crash);
    assert_eq!(
        late_outcome, None,
        "Late exit event from stopped instance must return None"
    );

    // Crucial: The profile state must REMAIN Disconnected, NOT mutated to Exited!
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Disconnected {
            target_monitor: MonitorIdentity::new("MON-QUEUE-LATE")
        },
        "Late event must NOT overwrite Disconnected to Exited"
    );

    // When monitor reconnects, profile MUST STILL be restored!
    monitor_provider.set_monitors(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor.clone(),
    ]);
    let reconn = coordinator.handle_monitors_changed().expect("reconnect");
    assert_eq!(reconn.len(), 1, "Profile must be restored upon reconnect");
    let pid2 = match &reconn[0] {
        LifecycleTransitionOutcome::Restored { pid, .. } => *pid,
        other => panic!("expected Restored, got: {:?}", other),
    };
    assert_ne!(pid1, pid2);
    assert!(coordinator.is_profile_running(&profile_id));

    // Now, another late exit event arrives for pid1 while pid2 is actively running:
    let another_late_event = coordinator.handle_process_exit(pid1, ExitReason::Crash);
    assert_eq!(
        another_late_event, None,
        "Stale PID event must be safely ignored"
    );

    // The running instance pid2 must be completely undisturbed!
    assert!(coordinator.is_profile_running(&profile_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: pid2,
            target_monitor: queue_monitor,
        }
    );
}

#[test]
fn test_stop_profile_cancels_waiting_for_monitor_and_prevents_launch_on_reconnect() {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-CANCEL-WAIT"),
        name: "Queue Screen".to_string(),
        is_primary: false,
    };
    // Initially only primary display is available
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Workstation Main Display".to_string(),
        is_primary: true,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_stop_waiting");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Batal Tunggu",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-CANCEL-WAIT"))),
        )
        .expect("save profile");

    // Activate while monitor is unavailable -> WaitingForMonitor
    let act = coordinator.activate_profile(&profile_id).expect("activate");
    assert_eq!(
        act,
        ActivationResult::WaitingForMonitor {
            target_monitor: MonitorIdentity::new("MON-QUEUE-CANCEL-WAIT")
        }
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::WaitingForMonitor {
            target_monitor: MonitorIdentity::new("MON-QUEUE-CANCEL-WAIT")
        }
    );

    // Nurse decides to stop the waiting profile before monitor connects
    let stop_res = coordinator.stop_profile(&profile_id).expect("stop profile");
    assert_eq!(stop_res, StopOutcome::NotRunning);

    // Profile state MUST be reset to Idle!
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Idle,
        "Stopping a waiting profile must reset its lifecycle state to Idle"
    );

    // Now monitor is plugged in
    monitor_provider.set_monitors(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor,
    ]);

    let transitions = coordinator
        .handle_monitors_changed()
        .expect("handle monitors changed");

    // Must NOT launch because the profile was stopped (Idle)
    assert!(
        transitions.is_empty(),
        "Stopped waiting profile must NOT be restored on reconnect"
    );
    assert!(launcher.launched_processes().is_empty());
    assert!(!coordinator.is_profile_running(&profile_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Idle
    );
}

#[test]
fn test_stop_profile_cancels_disconnected_and_prevents_launch_on_reconnect() {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-CANCEL-DISC"),
        name: "Queue Screen".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_stop_disconnected");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Batal Disconnected",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-CANCEL-DISC"))),
        )
        .expect("save profile");

    coordinator.activate_profile(&profile_id).expect("activate");
    assert_eq!(launcher.launched_processes().len(), 1);

    // Disconnect monitor -> state becomes Disconnected
    monitor_provider.set_monitors(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Workstation Main Display".to_string(),
        is_primary: true,
    }]);
    coordinator.handle_monitors_changed().expect("disconnect");
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Disconnected {
            target_monitor: MonitorIdentity::new("MON-QUEUE-CANCEL-DISC")
        }
    );

    // Nurse clicks Stop while monitor is still disconnected
    let stop_res = coordinator.stop_profile(&profile_id).expect("stop profile");
    assert_eq!(stop_res, StopOutcome::NotRunning);

    // Profile state MUST be reset to Idle!
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Idle,
        "Stopping a disconnected profile must reset its lifecycle state to Idle"
    );

    // Monitor reconnects later
    monitor_provider.set_monitors(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_monitor,
    ]);

    let transitions = coordinator
        .handle_monitors_changed()
        .expect("handle monitors changed");

    // Must NOT launch because the profile was explicitly stopped (Idle)
    assert!(
        transitions.is_empty(),
        "Stopped disconnected profile must NOT be restored on reconnect"
    );
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "No new process launched"
    );
    assert!(!coordinator.is_profile_running(&profile_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Idle
    );
}

#[test]
fn test_reconnect_error_in_one_profile_does_not_block_restoration_of_other_profiles() {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Screen 1".to_string(),
        is_primary: false,
    };
    let queue_mon2 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-2"),
        name: "Queue Screen 2".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Workstation Main Display".to_string(),
        is_primary: true,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_error_isolation");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock,
        base_dir,
    );

    // Profile 1: unverified browser that fails during launch_profile_instance
    // We manually set its lifecycle state to WaitingForMonitor
    let _p1_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Bermasalah",
                PathBuf::from("unsupported_browser.exe"),
                "--kiosk",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save p1");

    // Profile 2: healthy profile
    let p2_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Sehat",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/good",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-2"))),
        )
        .expect("save p2");

    // Both profiles are in WaitingForMonitor
    coordinator.activate_profile(&p2_id).expect("activate p2");
    // For p1, simulate that it was previously WaitingForMonitor
    // (or directly insert into coordinator's lifecycle)
    // Note: activate_profile(&p1_id) fails can_activate because browser is unverified,
    // so we can test what happens if launch_profile_instance returns an error during reconnect:
    // We simulate launch error on launcher when p1 launches:
    launcher.set_launch_error(Some("Launch error for p1".to_string()));

    // Both monitors connect simultaneously
    monitor_provider.set_monitors(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_mon1,
        queue_mon2.clone(),
    ]);

    let res = coordinator.handle_monitors_changed();
    // Before fix, res is Err(PlatformError) and aborts!
    // After fix, res is Ok, p1 failure is logged to last_error, and doesn't crash the coordinator!
    assert!(
        res.is_ok(),
        "handle_monitors_changed should not fail whole batch on one profile launch error"
    );
    assert!(coordinator.last_error().is_some());
}

#[test]
fn test_activate_profile_on_same_monitor_stops_previous_profile_before_launching_new_profile() {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_mon.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_switch");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let p_a_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Gigi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/gigi",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save p_a");

    let p_b_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Anak",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/anak",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save p_b");

    // 1. Activate profile A
    let res_a = coordinator.activate_profile(&p_a_id).expect("activate p_a");
    let pid_a = match res_a {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for p_a"),
    };

    assert_eq!(
        coordinator.get_lifecycle_state(&p_a_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: pid_a,
            target_monitor: queue_mon.clone(),
        }
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&p_b_id).unwrap(),
        ProfileLifecycleState::Idle
    );

    let proc_a = launcher
        .launched_processes()
        .into_iter()
        .find(|p| p.pid() == pid_a)
        .expect("process a must exist");
    assert!(proc_a.is_alive());

    // 2. Activate profile B on the same target monitor
    let res_b = coordinator.activate_profile(&p_b_id).expect("activate p_b");
    let pid_b = match res_b {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for p_b"),
    };
    assert_ne!(pid_a, pid_b, "Profile B must have a new process instance");

    // Profile A must be stopped before B was launched
    assert!(
        !proc_a.is_alive(),
        "Profile A must be stopped when Profile B takes over the monitor"
    );
    assert!(proc_a.close_requested());
    assert_eq!(
        coordinator.get_lifecycle_state(&p_a_id).unwrap(),
        ProfileLifecycleState::Idle,
        "Profile A must transition to Idle"
    );

    // Profile B must be Running
    assert_eq!(
        coordinator.get_lifecycle_state(&p_b_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: pid_b,
            target_monitor: queue_mon.clone(),
        }
    );

    let proc_b = launcher
        .launched_processes()
        .into_iter()
        .find(|p| p.pid() == pid_b)
        .expect("process b must exist");
    assert!(proc_b.is_alive());

    // Verify only one profile is running in active_instances
    assert!(!coordinator.is_profile_running(&p_a_id));
    assert!(coordinator.is_profile_running(&p_b_id));
}

#[test]
fn test_activate_profile_replaces_slow_profile_with_3s_deadline_force_stop_before_launching_replacement(
) {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_mon.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_switch_slow");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock.clone(),
        base_dir,
    );

    let p_a_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Gigi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/gigi",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save p_a");

    let p_b_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Anak",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/anak",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save p_b");

    // 1. Activate profile A
    let res_a = coordinator.activate_profile(&p_a_id).expect("activate p_a");
    let pid_a = match res_a {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for p_a"),
    };

    let proc_a = launcher
        .launched_processes()
        .into_iter()
        .find(|p| p.pid() == pid_a)
        .expect("process a must exist");
    assert!(proc_a.is_alive());

    // Make Profile A unresponsive to graceful close (e.g. modal prompt or freeze)
    proc_a.set_refuse_graceful(true);

    // 2. Activate profile B on the same target monitor
    let t0 = clock.now();
    let res_b = coordinator.activate_profile(&p_b_id).expect("activate p_b");
    let t1 = clock.now();

    let pid_b = match res_b {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for p_b"),
    };
    assert_ne!(pid_a, pid_b);

    // Verification:
    // 3-second deadline must have elapsed before forcing termination
    assert!(
        t1.duration_since(t0) >= std::time::Duration::from_secs(3),
        "Switching must wait 3 seconds before force stopping the previous unresponsive profile"
    );

    // Profile A must have been gracefully requested and then force stopped
    assert!(proc_a.close_requested());
    assert!(proc_a.force_stopped());
    assert!(!proc_a.is_alive(), "Profile A must be dead");
    assert_eq!(
        coordinator.get_lifecycle_state(&p_a_id).unwrap(),
        ProfileLifecycleState::Idle
    );

    // Profile B must be running
    assert_eq!(
        coordinator.get_lifecycle_state(&p_b_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: pid_b,
            target_monitor: queue_mon,
        }
    );
    assert!(!coordinator.is_profile_running(&p_a_id));
    assert!(coordinator.is_profile_running(&p_b_id));
}

#[test]
fn test_activate_profile_on_one_monitor_does_not_affect_profile_on_another_monitor_or_nurse_browser(
) {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let mon2 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-2"),
        name: "Queue Display 2".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        mon1.clone(),
        mon2.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_multi_mon");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    // 0. Simulate Nurse working browser running on primary display
    let nurse_plan = window_hop::browsers::LaunchPlan {
        executable: PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
        arguments: vec!["https://hospital.simrs/nurse-dashboard".to_string()],
        user_data_dir: PathBuf::from(r"C:\Users\Nurse\AppData\Local\Microsoft\Edge\User Data"),
        login_retention: window_hop::browsers::LoginRetentionNote::SessionIsolated {
            explanation: "Nurse default session".to_string(),
        },
    };
    let nurse_proc = launcher
        .launch(
            &nurse_plan,
            &MonitorDescriptor {
                identity: MonitorIdentity::new("MON-PRIMARY"),
                name: "Workstation Main Display".to_string(),
                is_primary: true,
            },
        )
        .expect("nurse browser running");
    let nurse_pid = nurse_proc.pid();

    // 1. Profile A on Monitor 1
    let p_a_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Gigi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/gigi",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save p_a");

    // 2. Profile C on Monitor 2
    let p_c_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Farmasi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/farmasi",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-2"))),
        )
        .expect("save p_c");

    // 3. Profile B on Monitor 1 (replacement for A)
    let p_b_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Anak",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/anak",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save p_b");

    // Activate Profile A (Monitor 1) and Profile C (Monitor 2)
    coordinator.activate_profile(&p_a_id).expect("activate p_a");
    coordinator.activate_profile(&p_c_id).expect("activate p_c");

    assert!(coordinator.is_profile_running(&p_a_id));
    assert!(coordinator.is_profile_running(&p_c_id));

    let proc_c = launcher
        .launched_processes()
        .into_iter()
        .find(|p| {
            if let Some(pid) = coordinator.active_instance(&p_c_id).map(|i| i.pid()) {
                p.pid() == pid
            } else {
                false
            }
        })
        .expect("process c must exist");
    assert!(proc_c.is_alive());

    // 4. Activate Profile B on Monitor 1
    coordinator.activate_profile(&p_b_id).expect("activate p_b");

    // Verification:
    // Profile A on Monitor 1 is stopped
    assert!(!coordinator.is_profile_running(&p_a_id));
    // Profile B on Monitor 1 is running
    assert!(coordinator.is_profile_running(&p_b_id));

    // Profile C on Monitor 2 is STILL RUNNING and was NOT stopped or disturbed!
    assert!(
        coordinator.is_profile_running(&p_c_id),
        "Profile C on Monitor 2 must remain running"
    );
    assert!(
        proc_c.is_alive(),
        "Process C on Monitor 2 must still be alive"
    );
    assert!(
        !proc_c.close_requested(),
        "Process C on Monitor 2 must not have received close request"
    );
    assert!(
        !proc_c.force_stopped(),
        "Process C on Monitor 2 must not have been force stopped"
    );

    // Nurse browser on Primary is STILL RUNNING and untouched!
    let current_nurse_proc = launcher
        .launched_processes()
        .into_iter()
        .find(|p| p.pid() == nurse_pid)
        .expect("nurse proc exists");
    assert!(
        current_nurse_proc.is_alive(),
        "Nurse working browser must remain alive"
    );
    assert!(
        !current_nurse_proc.close_requested(),
        "Nurse working browser must never receive close request"
    );
    assert!(
        !current_nurse_proc.force_stopped(),
        "Nurse working browser must never be force stopped"
    );
}

#[test]
fn test_activate_profile_switch_fails_if_previous_profile_fails_to_stop_without_killing_by_name() {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_mon.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_switch_fail");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let p_a_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Gigi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/gigi",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save p_a");

    let p_b_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Anak",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/anak",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save p_b");

    // 1. Activate Profile A
    let res_a = coordinator.activate_profile(&p_a_id).expect("activate p_a");
    let pid_a = match res_a {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for p_a"),
    };

    let proc_a = launcher
        .launched_processes()
        .into_iter()
        .find(|p| p.pid() == pid_a)
        .expect("proc a exists");
    assert!(proc_a.is_alive());

    // 2. Configure Profile A to refuse graceful close AND fail force stop (e.g. permission/handle failure)
    proc_a.set_refuse_graceful(true);
    proc_a.set_fail_force_stop(true);

    // 3. Attempt to activate Profile B on the same monitor
    let res_b = coordinator.activate_profile(&p_b_id);
    assert!(
        res_b.is_err(),
        "Activating Profile B must fail when Profile A cannot be stopped"
    );
    match res_b.unwrap_err() {
        ApplicationError::SwitchProfileFailed {
            previous_profile_id,
            previous_profile_name,
            target_monitor,
            reason,
        } => {
            assert_eq!(previous_profile_id, p_a_id);
            assert_eq!(previous_profile_name, "Antrian Poli Gigi");
            assert_eq!(target_monitor, MonitorIdentity::new("MON-QUEUE-1"));
            assert!(!reason.is_empty());
        }
        err => panic!("Expected SwitchProfileFailed, got {:?}", err),
    }

    // Profile B must NOT have been launched
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "No new process for Profile B may be launched"
    );
    assert!(
        !coordinator.is_profile_running(&p_b_id),
        "Profile B must not be running"
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&p_b_id).unwrap(),
        ProfileLifecycleState::Idle
    );

    // Profile A must still be tracked as running (not dropped or left in unknown state)
    assert!(proc_a.is_alive());
    assert!(coordinator.is_profile_running(&p_a_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&p_a_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: pid_a,
            target_monitor: queue_mon,
        }
    );
}

#[test]
fn test_repeated_and_alternating_activation_between_two_profiles_on_same_monitor() {
    let store = Arc::new(MemoryProfileStore::new());
    let queue_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        queue_mon.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_alternating");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let p_a_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Gigi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/gigi",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save p_a");

    let p_b_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Anak",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/anak",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save p_b");

    // 1. Activate Profile A
    let res_a1 = coordinator.activate_profile(&p_a_id).expect("activate p_a");
    let pid_a1 = match res_a1 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for p_a1"),
    };
    assert!(coordinator.is_profile_running(&p_a_id));
    assert!(!coordinator.is_profile_running(&p_b_id));
    assert_eq!(launcher.launched_processes().len(), 1);

    // 2. Repeated activation of Profile A (clicks shortcut again)
    let res_a2 = coordinator
        .activate_profile(&p_a_id)
        .expect("activate p_a again");
    match res_a2 {
        ActivationResult::Reused { pid, .. } => {
            assert_eq!(
                pid, pid_a1,
                "Must reuse same PID without launching new instance"
            );
        }
        _ => panic!("expected Reused for repeated p_a"),
    }
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "No second instance for Profile A"
    );
    assert!(coordinator.is_profile_running(&p_a_id));

    // 3. Switch to Profile B on the same monitor
    let res_b1 = coordinator.activate_profile(&p_b_id).expect("activate p_b");
    let pid_b1 = match res_b1 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for p_b1"),
    };
    assert_ne!(pid_a1, pid_b1);
    assert!(!coordinator.is_profile_running(&p_a_id));
    assert!(coordinator.is_profile_running(&p_b_id));
    assert_eq!(launcher.launched_processes().len(), 2);

    // 4. Repeated activation of Profile B (clicks shortcut again)
    let res_b2 = coordinator
        .activate_profile(&p_b_id)
        .expect("activate p_b again");
    match res_b2 {
        ActivationResult::Reused { pid, .. } => {
            assert_eq!(
                pid, pid_b1,
                "Must reuse same PID for Profile B without duplicate"
            );
        }
        _ => panic!("expected Reused for repeated p_b"),
    }
    assert_eq!(
        launcher.launched_processes().len(),
        2,
        "No duplicate instance for Profile B"
    );
    assert!(coordinator.is_profile_running(&p_b_id));

    // 5. Switch back to Profile A on the same monitor
    let res_a3 = coordinator
        .activate_profile(&p_a_id)
        .expect("switch back to p_a");
    let pid_a3 = match res_a3 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for p_a3"),
    };
    assert_ne!(pid_a3, pid_b1);
    assert!(coordinator.is_profile_running(&p_a_id));
    assert!(!coordinator.is_profile_running(&p_b_id));
    assert_eq!(launcher.launched_processes().len(), 3);
}

#[test]
fn test_profile_switching_failure_preserves_other_monitors_and_records_clear_error_for_ui() {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let mon2 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-2"),
        name: "Queue Display 2".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        mon1.clone(),
        mon2.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_ui_err");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    // Profile A (Monitor 1)
    let p_a_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Gigi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/gigi",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save p_a");

    // Profile C (Monitor 2)
    let p_c_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Farmasi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/farmasi",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-2"))),
        )
        .expect("save p_c");

    // Profile B (Replacement on Monitor 1)
    let p_b_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Anak",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/anak",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save p_b");

    // Activate A on Mon 1 and C on Mon 2
    let act_a = coordinator.activate_profile(&p_a_id).expect("activate p_a");
    let pid_a = match act_a {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for p_a"),
    };
    let act_c = coordinator.activate_profile(&p_c_id).expect("activate p_c");
    let pid_c = match act_c {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for p_c"),
    };

    assert_eq!(
        coordinator.get_lifecycle_state(&p_a_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: pid_a,
            target_monitor: mon1.clone(),
        }
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&p_c_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: pid_c,
            target_monitor: mon2.clone(),
        }
    );

    // Simulate Profile A refusing graceful close and failing force stop
    let proc_a = launcher
        .launched_processes()
        .into_iter()
        .find(|p| p.pid() == pid_a)
        .expect("proc a exists");
    proc_a.set_refuse_graceful(true);
    proc_a.set_fail_force_stop(true);

    // Now attempt to switch: Activate Profile B on Monitor 1
    let switch_res = coordinator.activate_profile(&p_b_id);
    assert!(switch_res.is_err());

    // 1. Clear error message recorded for UI display
    let last_err = coordinator
        .last_error()
        .expect("Coordinator must record last_error for UI display");
    assert!(
        last_err.contains("Gagal mengganti profil aktif pada monitor 'MON-QUEUE-1'"),
        "Error message must specify monitor identity: {}",
        last_err
    );
    assert!(
        last_err.contains("Antrian Poli Gigi"),
        "Error message must specify previous profile name: {}",
        last_err
    );

    // 2. Profile C on Monitor 2 is completely unaffected and still Running
    assert!(
        coordinator.is_profile_running(&p_c_id),
        "Profile C on Monitor 2 must remain running"
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&p_c_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: pid_c,
            target_monitor: mon2,
        }
    );

    // 3. Profile A on Monitor 1 remains Running (since it could not be stopped)
    assert!(
        coordinator.is_profile_running(&p_a_id),
        "Profile A must still be running"
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&p_a_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: pid_a,
            target_monitor: mon1,
        }
    );

    // 4. Profile B remains Idle
    assert!(
        !coordinator.is_profile_running(&p_b_id),
        "Profile B must not be running"
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&p_b_id).unwrap(),
        ProfileLifecycleState::Idle
    );
}

#[test]
fn test_saving_changes_does_not_alter_running_display_or_supervision_until_explicit_restart_or_next_activation(
) {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let mon2 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-2"),
        name: "Queue Display 2".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone(), mon2.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_save_without_disrupt");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Bedah",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/bedah-v1",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile v1");

    // Launch initial configuration
    let act = coordinator
        .activate_profile(&profile_id)
        .expect("activate profile v1");
    let pid1 = match act {
        ActivationResult::Launched {
            pid,
            target_monitor,
            ..
        } => {
            assert_eq!(target_monitor, mon1);
            pid
        }
        _ => panic!("expected Launched"),
    };

    let proc1 = launcher.launched_processes()[0].clone();
    assert!(proc1.is_alive());
    assert_eq!(proc1.current_monitor(), mon1);
    assert!(!coordinator.has_running_config_changes(&profile_id));

    // Nurse edits parameters, browser, and target monitor, then saves
    coordinator
        .save_draft_profile(
            Some(&profile_id),
            DraftProfileInput::new(
                "Antrian Poli Bedah",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/bedah-v2",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-2"))),
        )
        .expect("save edited profile");

    // Acceptance criterion 1:
    // Saving changes must NOT change the running display or the supervision of the running instance!
    assert!(
        proc1.is_alive(),
        "Running process must remain alive after saving changes"
    );
    assert_eq!(
        proc1.current_monitor(),
        mon1,
        "Running display must remain on the original monitor (MON-QUEUE-1)"
    );
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "No new process launched on save"
    );
    assert!(coordinator.is_profile_running(&profile_id));
    assert!(
        coordinator.has_running_config_changes(&profile_id),
        "Coordinator should detect that saved config has unapplied changes"
    );

    // Lifecycle state still reports the running monitor and PID
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: pid1,
            target_monitor: mon1.clone(),
        }
    );

    // Supervision test:
    // If the newly configured target monitor (MON-QUEUE-2) disconnects from the system,
    // the running instance on MON-QUEUE-1 must NOT be affected!
    let transitions = coordinator
        .process_monitors_changed(std::slice::from_ref(&mon1))
        .expect("monitors changed: only mon1 available");
    assert!(
        transitions.is_empty(),
        "No transition for mon1 instance when mon2 is disconnected"
    );
    assert!(proc1.is_alive(), "Process on mon1 must still be alive");

    // If the currently supervised monitor (MON-QUEUE-1) disconnects,
    // the running instance MUST be closed with 3-second deadline and transition to Disconnected!
    let transitions = coordinator
        .process_monitors_changed(&[])
        .expect("monitors changed: mon1 disconnected");
    assert_eq!(transitions.len(), 1);
    match &transitions[0] {
        LifecycleTransitionOutcome::Disconnected {
            profile_id: p_id,
            pid,
            forced,
        } => {
            assert_eq!(p_id, &profile_id);
            assert_eq!(pid, &pid1);
            assert!(!forced);
        }
        other => panic!("expected Disconnected outcome, got: {:?}", other),
    }
    assert!(
        !proc1.is_alive(),
        "Process must be stopped when its running monitor disconnects"
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Disconnected {
            target_monitor: MonitorIdentity::new("MON-QUEUE-1"),
        }
    );
}

#[test]
fn test_explicit_restart_profile_restarts_running_instance_with_saved_changes() {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_explicit_restart");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Farmasi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/farmasi-v1",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile v1");

    let act1 = coordinator
        .activate_profile(&profile_id)
        .expect("activate v1");
    let pid1 = match act1 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched"),
    };
    let proc1 = launcher.launched_processes()[0].clone();
    assert!(proc1.is_alive());

    // Nurse edits parameters and saves
    coordinator
        .save_draft_profile(
            Some(&profile_id),
            DraftProfileInput::new(
                "Antrian Farmasi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/farmasi-v2",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile v2");

    assert!(coordinator.has_running_config_changes(&profile_id));
    assert!(
        proc1.is_alive(),
        "Proc1 still alive before explicit restart"
    );

    // Explicit restart
    let act2 = coordinator
        .restart_profile(&profile_id)
        .expect("restart profile");
    let pid2 = match act2 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched after restart"),
    };

    assert_ne!(pid1, pid2);
    assert!(
        !proc1.is_alive(),
        "Old instance proc1 must have been stopped"
    );
    assert_eq!(launcher.launched_processes().len(), 2);
    let proc2 = launcher.launched_processes()[1].clone();
    assert_eq!(proc2.pid(), pid2);
    assert!(proc2
        .plan()
        .arguments
        .iter()
        .any(|a| a == "https://antrian.local/farmasi-v2"));
    assert!(!coordinator.has_running_config_changes(&profile_id));
}

#[test]
fn test_subsequent_activation_reuses_if_unchanged_and_restarts_if_changed() {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_subsequent_activation");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Lab",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/lab",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile");

    let act1 = coordinator
        .activate_profile(&profile_id)
        .expect("first activation");
    let pid1 = match act1 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched"),
    };

    // Subsequent activation with UNCHANGED config: MUST reuse window, NO restart!
    let act_reuse = coordinator
        .activate_profile(&profile_id)
        .expect("reuse activation");
    match act_reuse {
        ActivationResult::Reused { pid, .. } => {
            assert_eq!(pid, pid1, "Reused window must keep same PID");
        }
        _ => panic!("expected Reused when config is unchanged"),
    }
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "No second process launched"
    );
    let proc1 = launcher.launched_processes()[0].clone();
    assert!(proc1.is_alive());

    // Now change parameters
    coordinator
        .save_draft_profile(
            Some(&profile_id),
            DraftProfileInput::new(
                "Antrian Lab",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/lab-updated",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save updated profile");

    // Subsequent activation with CHANGED config: MUST restart (stop old, launch new)!
    let act_restart = coordinator
        .activate_profile(&profile_id)
        .expect("changed activation");
    let pid2 = match act_restart {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched when config changed"),
    };

    assert_ne!(pid1, pid2);
    assert!(!proc1.is_alive(), "Old process must be stopped");
    assert_eq!(launcher.launched_processes().len(), 2);
}

#[test]
fn test_target_monitor_change_to_unavailable_stops_old_instance_and_waits() {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_target_change_unavailable");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Radiologi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/radiologi",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile on mon1");

    let act1 = coordinator
        .activate_profile(&profile_id)
        .expect("activate on mon1");
    let _pid1 = match act1 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched"),
    };
    let proc1 = launcher.launched_processes()[0].clone();
    assert!(proc1.is_alive());

    // Nurse changes target monitor to MON-QUEUE-2 (which is currently unavailable / not connected)
    coordinator
        .save_draft_profile(
            Some(&profile_id),
            DraftProfileInput::new(
                "Antrian Radiologi",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/radiologi",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-2"))),
        )
        .expect("save profile targeting mon2");

    // Applying change (next activation):
    // Old instance on mon1 must be stopped, and profile transitions to WaitingForMonitor(MON-QUEUE-2)
    let act2 = coordinator
        .activate_profile(&profile_id)
        .expect("apply target change");
    match act2 {
        ActivationResult::WaitingForMonitor { target_monitor } => {
            assert_eq!(target_monitor, MonitorIdentity::new("MON-QUEUE-2"));
        }
        other => panic!("expected WaitingForMonitor, got: {:?}", other),
    }

    assert!(
        !proc1.is_alive(),
        "Old instance on mon1 must be stopped when target is changed to mon2"
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::WaitingForMonitor {
            target_monitor: MonitorIdentity::new("MON-QUEUE-2"),
        }
    );
}

#[test]
fn test_target_monitor_change_respects_per_monitor_exclusivity_and_stops_old_instances() {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let mon2 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-2"),
        name: "Queue Display 2".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone(), mon2.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_target_change_exclusivity");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    // Profile A on Mon 1
    let p_a_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Profil A (Poli Gigi)",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/gigi",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile a");

    // Profile B on Mon 2
    let p_b_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Profil B (Poli Mata)",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/mata",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-2"))),
        )
        .expect("save profile b");

    // Activate both profiles
    let act_a1 = coordinator.activate_profile(&p_a_id).expect("activate a");
    let pid_a1 = match act_a1 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for a"),
    };
    let act_b1 = coordinator.activate_profile(&p_b_id).expect("activate b");
    let pid_b1 = match act_b1 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for b"),
    };

    let proc_a1 = launcher
        .launched_processes()
        .iter()
        .find(|p| p.pid() == pid_a1)
        .unwrap()
        .clone();
    let proc_b1 = launcher
        .launched_processes()
        .iter()
        .find(|p| p.pid() == pid_b1)
        .unwrap()
        .clone();
    assert!(proc_a1.is_alive());
    assert!(proc_b1.is_alive());

    // Nurse edits Profile A to target MON-QUEUE-2 instead of MON-QUEUE-1
    coordinator
        .save_draft_profile(
            Some(&p_a_id),
            DraftProfileInput::new(
                "Profil A (Poli Gigi)",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/gigi",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-2"))),
        )
        .expect("save profile a targeting mon2");

    // Applying change:
    // 1. Instance A1 on Mon 1 must be closed.
    // 2. Conflicting Instance B1 on Mon 2 must be closed (respecting 1 active profile per monitor).
    // 3. New Instance A2 must be launched on Mon 2.
    let act_a2 = coordinator
        .activate_profile(&p_a_id)
        .expect("activate a on mon2");
    let pid_a2 = match act_a2 {
        ActivationResult::Launched {
            pid,
            target_monitor,
            ..
        } => {
            assert_eq!(target_monitor, mon2);
            pid
        }
        _ => panic!("expected Launched for a on mon2"),
    };

    assert_ne!(pid_a1, pid_a2);
    assert!(
        !proc_a1.is_alive(),
        "Profile A old instance on mon1 must be stopped"
    );
    assert!(
        !proc_b1.is_alive(),
        "Profile B instance on mon2 must be stopped before replacement"
    );

    let proc_a2 = launcher
        .launched_processes()
        .iter()
        .find(|p| p.pid() == pid_a2)
        .unwrap()
        .clone();
    assert!(proc_a2.is_alive());
    assert_eq!(proc_a2.current_monitor(), mon2);

    assert!(coordinator.is_profile_running(&p_a_id));
    assert!(!coordinator.is_profile_running(&p_b_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&p_b_id).unwrap(),
        ProfileLifecycleState::Idle
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&p_a_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: pid_a2,
            target_monitor: mon2,
        }
    );
}

#[test]
fn test_target_monitor_change_fails_if_conflicting_profile_on_new_target_fails_to_stop() {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let mon2 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-2"),
        name: "Queue Display 2".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone(), mon2.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_target_change_conflict_fail");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let p_a_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Profil A",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/a",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile a");

    let p_b_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Profil B",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/b",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-2"))),
        )
        .expect("save profile b");

    coordinator.activate_profile(&p_a_id).expect("activate a");
    let act_b = coordinator.activate_profile(&p_b_id).expect("activate b");
    let pid_b = match act_b {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for b"),
    };

    let proc_b = launcher
        .launched_processes()
        .iter()
        .find(|p| p.pid() == pid_b)
        .unwrap()
        .clone();
    // Simulate Profile B refusing to close and failing force stop
    proc_b.set_refuse_graceful(true);
    proc_b.set_fail_force_stop(true);

    // Profile A target changed to Mon 2
    coordinator
        .save_draft_profile(
            Some(&p_a_id),
            DraftProfileInput::new(
                "Profil A",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/a",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-2"))),
        )
        .expect("save profile a targeting mon2");

    let res = coordinator.activate_profile(&p_a_id);
    assert!(res.is_err());
    match res.unwrap_err() {
        ApplicationError::SwitchProfileFailed {
            previous_profile_id,
            previous_profile_name,
            target_monitor,
            ..
        } => {
            assert_eq!(previous_profile_id, p_b_id);
            assert_eq!(previous_profile_name, "Profil B");
            assert_eq!(target_monitor, MonitorIdentity::new("MON-QUEUE-2"));
        }
        other => panic!("expected SwitchProfileFailed, got: {:?}", other),
    }

    assert!(proc_b.is_alive(), "Profile B must still be alive");
    assert!(coordinator.is_profile_running(&p_b_id));
    assert!(coordinator.last_error().is_some());
}

#[test]
fn test_delete_active_profile_cooperative_stops_within_deadline_and_removes_from_store() {
    let dir = tempdir().expect("temp dir");
    let file_path = dir.path().join("profiles.json");
    let store = Arc::new(FileProfileStore::new(&file_path));
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_delete_active");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Kasir",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/kasir",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile");

    let act = coordinator.activate_profile(&profile_id).expect("activate");
    let pid = match act {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched"),
    };
    let proc = launcher.launched_processes()[0].clone();
    assert!(proc.is_alive());

    // Deleting active profile
    let stop_outcome = coordinator
        .delete_profile(&profile_id)
        .expect("delete active profile");
    assert_eq!(stop_outcome, StopOutcome::Graceful { pid });
    assert!(!proc.is_alive(), "Process must be stopped");
    assert!(proc.close_requested());

    // Profile must be removed from coordinator
    assert!(coordinator.get_profile(&profile_id).is_none());
    assert_eq!(coordinator.list_profiles().len(), 0);
    assert!(!coordinator.is_profile_running(&profile_id));
    assert_eq!(
        coordinator
            .get_lifecycle_state(&profile_id)
            .unwrap_err()
            .to_string(),
        format!("Profil dengan ID {} tidak ditemukan", profile_id)
    );

    // Profile must be removed from persisted store
    let reload_store = Arc::new(FileProfileStore::new(&file_path));
    let mut reloaded_coord = LifecycleCoordinator::from_store(reload_store);
    reloaded_coord.load().expect("load store");
    assert_eq!(reloaded_coord.list_profiles().len(), 0);
}

#[test]
fn test_delete_active_profile_unresponsive_force_stops_after_3s_deadline() {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_delete_unresponsive");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock.clone(),
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Lab Macet",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/lab",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile");

    let act = coordinator.activate_profile(&profile_id).expect("activate");
    let pid = match act {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched"),
    };
    let proc = launcher.launched_processes()[0].clone();
    proc.set_refuse_graceful(true);

    let stop_outcome = coordinator
        .delete_profile(&profile_id)
        .expect("delete unresponsive profile");
    assert_eq!(stop_outcome, StopOutcome::Forced { pid });
    assert!(
        !proc.is_alive(),
        "Process must be forced stopped after deadline"
    );
    assert!(proc.close_requested());
    assert!(proc.force_stopped());
    assert!(
        clock.now().duration_since(std::time::Instant::now()) <= std::time::Duration::from_secs(5)
    );

    assert!(coordinator.get_profile(&profile_id).is_none());
}

#[test]
fn test_delete_profile_does_not_trigger_reconnect_recovery() {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_delete_reconnect_suppress");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Anak",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/anak",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile");

    coordinator.activate_profile(&profile_id).expect("activate");

    // Mon 1 disconnects -> profile stops and enters Disconnected
    coordinator
        .process_monitors_changed(&[])
        .expect("disconnect mon1");
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::Disconnected {
            target_monitor: MonitorIdentity::new("MON-QUEUE-1"),
        }
    );

    // Nurse deletes the profile while disconnected
    let outcome = coordinator
        .delete_profile(&profile_id)
        .expect("delete profile");
    assert_eq!(outcome, StopOutcome::NotRunning);

    // Now Mon 1 reconnects: MUST NOT trigger recovery of the deleted profile!
    let transitions = coordinator
        .process_monitors_changed(std::slice::from_ref(&mon1))
        .expect("mon1 reconnects");
    assert!(
        transitions.is_empty(),
        "Deleted profile must never be restored on reconnect"
    );
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "No new process launched for deleted profile"
    );
}

#[test]
fn test_delete_inactive_or_draft_profile_removes_from_store_without_error() {
    let store = Arc::new(MemoryProfileStore::new());
    let mut coordinator = LifecycleCoordinator::from_store(store);

    let draft_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Draft Profil", PathBuf::from("chrome.exe"), ""),
        )
        .expect("save draft");

    assert_eq!(coordinator.list_profiles().len(), 1);

    let outcome = coordinator
        .delete_profile(&draft_id)
        .expect("delete draft profile");
    assert_eq!(outcome, StopOutcome::NotRunning);
    assert_eq!(coordinator.list_profiles().len(), 0);
    assert!(coordinator.get_profile(&draft_id).is_none());
}

#[test]
fn test_referencing_deleted_profile_returns_clear_error_without_fallback_or_launching_replacements()
{
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_deleted_profile_ref");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    // Profile A to be deleted
    let p_a_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Profil A (Akan Dihapus)",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/a",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile a");

    // Profile B as other existing profile
    let p_b_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Profil B (Profil Lain)",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/b",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile b");

    // Delete Profile A
    coordinator
        .delete_profile(&p_a_id)
        .expect("delete profile a");
    assert_eq!(coordinator.list_profiles().len(), 1);
    assert_eq!(coordinator.list_profiles()[0].id(), &p_b_id);

    // 1. Activating deleted profile A must fail with ProfileNotFound
    let err_act = coordinator.activate_profile(&p_a_id).unwrap_err();
    match err_act {
        ApplicationError::ProfileNotFound { id } => assert_eq!(id, p_a_id),
        other => panic!("expected ProfileNotFound, got: {:?}", other),
    }

    // 2. Restarting deleted profile A must fail with ProfileNotFound
    let err_restart = coordinator.restart_profile(&p_a_id).unwrap_err();
    match err_restart {
        ApplicationError::ProfileNotFound { id } => assert_eq!(id, p_a_id),
        other => panic!("expected ProfileNotFound, got: {:?}", other),
    }

    // 3. can_activate on deleted profile A must fail with ProfileNotFound
    let err_can_act = coordinator.can_activate(&p_a_id).unwrap_err();
    match err_can_act {
        ApplicationError::ProfileNotFound { id } => assert_eq!(id, p_a_id),
        other => panic!("expected ProfileNotFound, got: {:?}", other),
    }

    // 4. Status queries on deleted profile A must fail with ProfileNotFound
    let err_status = coordinator.get_profile_status(&p_a_id).unwrap_err();
    match err_status {
        ApplicationError::ProfileNotFound { id } => assert_eq!(id, p_a_id),
        other => panic!("expected ProfileNotFound, got: {:?}", other),
    }

    let err_target_status = coordinator.get_target_monitor_status(&p_a_id).unwrap_err();
    match err_target_status {
        ApplicationError::ProfileNotFound { id } => assert_eq!(id, p_a_id),
        other => panic!("expected ProfileNotFound, got: {:?}", other),
    }

    let err_lifecycle = coordinator.get_lifecycle_state(&p_a_id).unwrap_err();
    match err_lifecycle {
        ApplicationError::ProfileNotFound { id } => assert_eq!(id, p_a_id),
        other => panic!("expected ProfileNotFound, got: {:?}", other),
    }

    // 5. Deleting already deleted profile must fail with ProfileNotFound
    let err_del_again = coordinator.delete_profile(&p_a_id).unwrap_err();
    match err_del_again {
        ApplicationError::ProfileNotFound { id } => assert_eq!(id, p_a_id),
        other => panic!("expected ProfileNotFound, got: {:?}", other),
    }

    // Crucial: No other profile (like Profile B) was run as fallback/replacement!
    assert_eq!(
        launcher.launched_processes().len(),
        0,
        "No fallback profile may be launched when referencing deleted profile"
    );
    assert!(!coordinator.is_profile_running(&p_b_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&p_b_id).unwrap(),
        ProfileLifecycleState::Idle
    );
}

#[test]
fn test_delete_profile_cancellation_preserves_running_display_and_configuration() {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_delete_cancel");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let p_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian IGD Prioritas",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/igd",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile");

    coordinator.activate_profile(&p_id).expect("activate");
    let proc = launcher.launched_processes()[0].clone();
    assert!(proc.is_alive());

    // Acceptance criterion 4:
    // User initiates deletion -> prompt for confirmation -> user cancels deletion.
    // Display must remain unchanged and process must continue running!
    assert!(
        proc.is_alive(),
        "Running instance must not be stopped on cancel"
    );
    assert!(
        !proc.close_requested(),
        "No close request sent when cancelled"
    );
    assert!(coordinator.is_profile_running(&p_id));
    assert_eq!(coordinator.list_profiles().len(), 1);
    assert_eq!(
        coordinator.get_lifecycle_state(&p_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: proc.pid(),
            target_monitor: mon1,
        }
    );
}

#[test]
fn test_restart_profile_with_invalid_params_fails_without_stopping_running_instance() {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_restart_invalid_params");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let p_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian IGD",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/igd",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile");

    coordinator.activate_profile(&p_id).expect("activate");
    let proc = launcher.launched_processes()[0].clone();
    assert!(proc.is_alive());

    // Nurse edits config with conflicting parameter (violating session isolation)
    coordinator
        .save_draft_profile(
            Some(&p_id),
            DraftProfileInput::new(
                "Antrian IGD",
                PathBuf::from("msedge.exe"),
                r"--user-data-dir=C:\WorkingNurseData",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save invalid profile");

    // Deliberate restart with invalid config: MUST fail without terminating live queue window!
    let res = coordinator.restart_profile(&p_id);
    assert!(res.is_err(), "Restart must reject invalid parameters");
    assert!(
        proc.is_alive(),
        "Running queue display must NOT be stopped when validation fails"
    );
    assert!(!proc.close_requested());
}

#[test]
fn test_reconnect_old_monitor_does_not_restore_profile_after_target_changed_to_another_monitor() {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let mon2 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-2"),
        name: "Queue Display 2".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone(), mon2.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_reconnect_target_change");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let p_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Jantung",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/jantung",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile");

    coordinator.activate_profile(&p_id).expect("activate");

    // Monitor 1 disconnects -> profile stops and enters Disconnected(MON-QUEUE-1)
    coordinator
        .process_monitors_changed(&[])
        .expect("disconnect mon1");
    assert_eq!(
        coordinator.get_lifecycle_state(&p_id).unwrap(),
        ProfileLifecycleState::Disconnected {
            target_monitor: MonitorIdentity::new("MON-QUEUE-1"),
        }
    );

    // Nurse edits saved target monitor to MON-QUEUE-2
    coordinator
        .save_draft_profile(
            Some(&p_id),
            DraftProfileInput::new(
                "Antrian Poli Jantung",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/jantung",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-2"))),
        )
        .expect("save profile targeting mon2");

    // Now Monitor 1 reconnects: MUST NOT restore onto Monitor 1 because target was changed to Mon 2!
    let transitions = coordinator
        .process_monitors_changed(std::slice::from_ref(&mon1))
        .expect("mon1 reconnects");
    assert!(
        transitions.is_empty(),
        "Old monitor reconnection must not restore profile whose target changed to another monitor"
    );
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "No new process launched on old monitor"
    );
}

#[test]
fn test_delete_profile_persistence_failure_preserves_running_instance() {
    let store = Arc::new(MemoryProfileStore::new());
    let mon1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![mon1.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_delete_persistence_fail");

    let mut coordinator = LifecycleCoordinator::with_services(
        store.clone(),
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let p_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Rawat Jalan",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local/rawat-jalan",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-1"))),
        )
        .expect("save profile");

    coordinator.activate_profile(&p_id).expect("activate");
    let proc = launcher.launched_processes()[0].clone();
    assert!(proc.is_alive());

    // Simulate disk/store I/O error
    store.set_simulate_io_error(true);

    // Deleting profile fails due to persistence error
    let res = coordinator.delete_profile(&p_id);
    assert!(res.is_err(), "Delete must fail when store fails to save");

    // Running process must NOT have been terminated!
    assert!(
        proc.is_alive(),
        "Running instance must be preserved if persistence fails"
    );
    assert!(!proc.close_requested());
    assert!(coordinator.is_profile_running(&p_id));
}

#[test]
fn test_create_profile_shortcut_default_desktop_and_custom_folder() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let shortcut_creator = Arc::new(MockShortcutCreator::new(PathBuf::from(
        r"C:\Users\Perawat\Desktop",
    )));
    let base_dir = PathBuf::from(r"C:\AppData\WindowHop");

    let mut coordinator =
        LifecycleCoordinator::with_services(store, monitor_provider, launcher, clock, base_dir);
    coordinator.set_shortcut_creator(shortcut_creator.clone());

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Rawat Jalan",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.local",
            ),
        )
        .expect("save profile");

    // 1. Create shortcut with default folder (Desktop)
    let default_path = coordinator
        .create_profile_shortcut(&profile_id, None)
        .expect("create shortcut default desktop");

    assert_eq!(
        default_path,
        PathBuf::from(r"C:\Users\Perawat\Desktop").join("Antrian Rawat Jalan.lnk")
    );

    let created = shortcut_creator.created_shortcuts();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].profile_id, profile_id);
    assert_eq!(created[0].profile_name, "Antrian Rawat Jalan");
    assert_eq!(
        created[0].target_dir,
        PathBuf::from(r"C:\Users\Perawat\Desktop")
    );
    assert_eq!(
        created[0].arguments,
        vec![
            "--activate-profile".to_string(),
            profile_id.as_str().to_string()
        ]
    );

    // 2. Create shortcut with custom alternate folder
    let custom_dir = PathBuf::from(r"D:\Pintasan Khusus Antrian");
    let custom_path = coordinator
        .create_profile_shortcut(&profile_id, Some(custom_dir.clone()))
        .expect("create shortcut custom folder");

    assert_eq!(custom_path, custom_dir.join("Antrian Rawat Jalan.lnk"));

    let created = shortcut_creator.created_shortcuts();
    assert_eq!(created.len(), 2);
    assert_eq!(created[1].target_dir, custom_dir);
}

#[test]
fn test_create_profile_shortcut_handles_spaces_in_paths_and_carries_profile_reference() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let shortcut_creator = Arc::new(MockShortcutCreator::new(PathBuf::from(
        r"C:\Users\Perawat Poliklinik\Desktop",
    )));
    let base_dir = PathBuf::from(r"C:\App Data\Window Hop");

    let mut coordinator =
        LifecycleCoordinator::with_services(store, monitor_provider, launcher, clock, base_dir);
    coordinator.set_shortcut_creator(shortcut_creator.clone());

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Gigi & Mulut",
                PathBuf::from("msedge.exe"),
                "--kiosk",
            ),
        )
        .expect("save profile");

    let app_exe_with_spaces = PathBuf::from(r"C:\Program Files\Window Hop\window-hop.exe");
    let target_dir_with_spaces = PathBuf::from(r"D:\Folder Pintasan Perawat\Klinik Utama\");

    let shortcut_path = coordinator
        .create_profile_shortcut_with_exe(
            &profile_id,
            Some(target_dir_with_spaces.clone()),
            app_exe_with_spaces.clone(),
        )
        .expect("create shortcut with spaces");

    let created = shortcut_creator.created_shortcuts();
    assert_eq!(created.len(), 1);
    let spec = &created[0];

    // Reference profile
    assert_eq!(spec.profile_id, profile_id);
    assert_eq!(spec.app_executable, app_exe_with_spaces);
    assert_eq!(spec.target_dir, target_dir_with_spaces);

    // Command line arguments must include profile activation flag and profile ID
    assert_eq!(
        spec.arguments,
        vec![
            "--activate-profile".to_string(),
            profile_id.as_str().to_string()
        ]
    );

    // Shortcut file path handles sanitized filename
    assert!(shortcut_path
        .to_string_lossy()
        .contains("Antrian Poli Gigi & Mulut.lnk"));
}

#[test]
fn test_create_profile_shortcut_fails_when_profile_not_found() {
    let store = Arc::new(MemoryProfileStore::new());
    let coordinator = LifecycleCoordinator::from_store(store);

    let nonexistent_id = ProfileId::generate();
    let res = coordinator.create_profile_shortcut(&nonexistent_id, None);

    assert!(res.is_err());
    match res.unwrap_err() {
        ApplicationError::ProfileNotFound { id } => assert_eq!(id, nonexistent_id),
        err => panic!("expected ProfileNotFound, got: {:?}", err),
    }
}

#[test]
fn test_create_profile_shortcut_reports_error_when_platform_fails() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let shortcut_creator = Arc::new(MockShortcutCreator::new(PathBuf::from(r"C:\Desktop")));
    shortcut_creator.set_error(Some("Akses folder ditolak oleh OS".to_string()));

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher,
        clock,
        PathBuf::from("base"),
    );
    coordinator.set_shortcut_creator(shortcut_creator);

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Antrian Poli THT", PathBuf::from("msedge.exe"), "--kiosk"),
        )
        .expect("save profile");

    let res = coordinator.create_profile_shortcut(&profile_id, None);
    assert!(res.is_err());
    match res.unwrap_err() {
        ApplicationError::Platform(err) => {
            assert!(err.to_string().contains("Akses folder ditolak oleh OS"));
        }
        err => panic!("expected ApplicationError::Platform, got: {:?}", err),
    }
}

#[test]
fn test_handle_shortcut_activation_launches_profile_and_subsequent_clicks_reuse_window_without_duplicates(
) {
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-TARGET-1"),
        name: "Monitor Antrian 1".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![target_mon.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("base");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian IGD",
                PathBuf::from("msedge.exe"),
                "--kiosk https://antrian.hospital/igd",
            )
            .with_target_monitor(Some(target_mon.identity.clone())),
        )
        .expect("save profile");

    // 1. First shortcut activation launches the profile
    let result1 = coordinator
        .handle_shortcut_activation(&profile_id)
        .expect("first activation");

    let pid1 = match result1 {
        ActivationResult::Launched {
            pid,
            target_monitor,
            ..
        } => {
            assert_eq!(target_monitor, target_mon);
            pid
        }
        other => panic!("expected Launched result, got: {:?}", other),
    };

    assert_eq!(launcher.launched_processes().len(), 1);
    let proc = launcher.launched_processes()[0].clone();
    assert!(proc.is_alive());
    assert_eq!(proc.pid(), pid1);
    assert_eq!(proc.reposition_count(), 0);

    // Simulate nurse or user accidentally moving window to another display
    proc.reposition(&MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Primary Display".to_string(),
        is_primary: true,
    })
    .expect("simulate move");
    assert_eq!(proc.reposition_count(), 1);

    // 2. Second shortcut activation (repeated click) reuses the window and restores placement
    let result2 = coordinator
        .handle_shortcut_activation(&profile_id)
        .expect("second activation");

    match result2 {
        ActivationResult::Reused {
            pid,
            target_monitor,
            ..
        } => {
            assert_eq!(pid, pid1, "Must reuse same PID");
            assert_eq!(target_monitor, target_mon);
        }
        other => panic!("expected Reused result, got: {:?}", other),
    }

    // No duplicate process launched!
    assert_eq!(launcher.launched_processes().len(), 1);
    // Reposition was called again to restore placement to target monitor
    assert_eq!(proc.reposition_count(), 2);
    assert_eq!(proc.current_monitor(), target_mon);
}

#[test]
fn test_handle_shortcut_activation_when_target_monitor_unavailable_enters_waiting_state_no_fallback(
) {
    let store = Arc::new(MemoryProfileStore::new());
    // Only primary monitor is available, target monitor is missing
    let primary_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Primary Display".to_string(),
        is_primary: true,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![primary_mon]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        PathBuf::from("base"),
    );

    let target_identity = MonitorIdentity::new("MON-DISCONNECTED");
    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Farmasi",
                PathBuf::from("msedge.exe"),
                "--kiosk",
            )
            .with_target_monitor(Some(target_identity.clone())),
        )
        .expect("save profile");

    // Activating via shortcut when target monitor is unavailable
    let result = coordinator
        .handle_shortcut_activation(&profile_id)
        .expect("activate with unavailable monitor");

    match result {
        ActivationResult::WaitingForMonitor { target_monitor } => {
            assert_eq!(target_monitor, target_identity);
        }
        other => panic!("expected WaitingForMonitor, got: {:?}", other),
    }

    // Must NOT launch browser on primary!
    assert_eq!(launcher.launched_processes().len(), 0);
    // State is recorded as WaitingForMonitor
    assert_eq!(
        coordinator.get_lifecycle_state(&profile_id).unwrap(),
        ProfileLifecycleState::WaitingForMonitor {
            target_monitor: target_identity
        }
    );
}

#[test]
fn test_handle_shortcut_activation_when_profile_missing_returns_error_status() {
    let store = Arc::new(MemoryProfileStore::new());
    let mut coordinator = LifecycleCoordinator::from_store(store);

    let missing_id = ProfileId::generate();
    let res = coordinator.handle_shortcut_activation(&missing_id);

    assert!(res.is_err());
    match res.unwrap_err() {
        ApplicationError::ProfileNotFound { id } => assert_eq!(id, missing_id),
        err => panic!("expected ProfileNotFound, got: {:?}", err),
    }

    // Status kegagalan harus tercatat pada koordinator (teramati pada status/tray tanpa membuka jendela pengaturan)
    assert!(coordinator.last_error().is_some());
    assert!(coordinator
        .last_error()
        .unwrap()
        .contains(&missing_id.to_string()));
}

#[test]
fn test_shortcut_activation_through_single_instance_ipc_routes_to_coordinator() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-1"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![target_mon.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());

    let coordinator = Arc::new(std::sync::Mutex::new(LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        dir.path().to_path_buf(),
    )));

    let profile_id = coordinator
        .lock()
        .unwrap()
        .save_draft_profile(
            None,
            DraftProfileInput::new("Antrian Radiologi", PathBuf::from("msedge.exe"), "--kiosk")
                .with_target_monitor(Some(target_mon.identity.clone())),
        )
        .expect("save profile");

    // Primary instance starts IPC listener
    let primary_ipc = LocalSingleInstanceCoordinator::new(dir.path().to_path_buf());
    let coord_for_listener = coordinator.clone();
    primary_ipc
        .start_listener(Box::new(move |command| {
            if let ActivationCommand::ActivateProfile(pid) = command {
                let _ = coord_for_listener
                    .lock()
                    .unwrap()
                    .handle_shortcut_activation(&pid);
            }
        }))
        .expect("start listener");

    // Secondary instance (shortcut click) sends activation
    let secondary_ipc = LocalSingleInstanceCoordinator::new(dir.path().to_path_buf());
    let sent = secondary_ipc
        .send_activation(&ActivationCommand::ActivateProfile(profile_id.clone()))
        .expect("send activation");
    assert!(
        sent,
        "Must successfully send activation to primary coordinator"
    );

    // Wait briefly for coordinator to process
    std::thread::sleep(Duration::from_millis(150));

    // Profile should now be running on target monitor
    let is_running = coordinator.lock().unwrap().is_profile_running(&profile_id);
    assert!(
        is_running,
        "Profile must be running after shortcut IPC activation"
    );
    assert_eq!(launcher.launched_processes().len(), 1);

    // Repeated click from another secondary instance
    let sent2 = secondary_ipc
        .send_activation(&ActivationCommand::ActivateProfile(profile_id.clone()))
        .expect("repeated click");
    assert!(sent2);

    std::thread::sleep(Duration::from_millis(150));

    // Still only 1 process launched! (reused)
    assert_eq!(launcher.launched_processes().len(), 1);
}

#[test]
fn test_tray_provides_useful_status_summary_for_running_waiting_and_idle_states() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor1 = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY1\MONITOR1"),
        name: "Monitor Antrian 1".to_string(),
        is_primary: false,
    };
    let monitor2 = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY2\MONITOR2"),
        name: "Monitor Antrian 2".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        monitor1.clone(),
        monitor2.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_tray_summary");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher,
        clock,
        base_dir,
    );
    let mock_tray = Arc::new(MockTrayProvider::new());
    coordinator.set_tray_provider(mock_tray.clone());

    // 1. Initial idle state
    let summary = coordinator.status_summary();
    assert_eq!(summary.running_count, 0);
    assert_eq!(summary.waiting_count, 0);
    assert!(!summary.has_error);
    assert!(summary.status_text.contains("Siap"));

    // 2. Create profile 1 targeting monitor 1
    let p1_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Antrian Poli Umum".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli/umum --kiosk".to_string(),
                target_monitor: Some(monitor1.identity.clone()),
            },
        )
        .expect("save profile 1");

    // 3. Activate profile 1
    coordinator
        .activate_profile(&p1_id)
        .expect("activate profile 1");
    let summary1 = coordinator.status_summary();
    assert_eq!(summary1.running_count, 1);
    assert_eq!(summary1.waiting_count, 0);
    assert!(!summary1.has_error);
    assert!(summary1.status_text.contains("Antrian Poli Umum"));
    assert!(summary1.tooltip.contains("Monitor Antrian 1"));

    // Tray provider was updated automatically
    let tray_status1 = mock_tray.last_status().expect("tray status 1");
    assert_eq!(tray_status1.running_count, 1);
    assert!(tray_status1.status_text.contains("Antrian Poli Umum"));

    // 4. Create and activate profile 2 targeting monitor 2
    let p2_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Antrian Poli Gigi".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli/gigi --kiosk".to_string(),
                target_monitor: Some(monitor2.identity.clone()),
            },
        )
        .expect("save profile 2");

    coordinator
        .activate_profile(&p2_id)
        .expect("activate profile 2");
    let summary2 = coordinator.status_summary();
    assert_eq!(summary2.running_count, 2);
    assert!(summary2.status_text.contains("2 profil berjalan"));

    // 5. Disconnect monitor 1
    monitor_provider.set_monitors(vec![monitor2.clone()]);
    let transitions = coordinator
        .handle_monitors_changed()
        .expect("handle disconnect");
    assert_eq!(transitions.len(), 1);

    let summary3 = coordinator.status_summary();
    assert_eq!(summary3.running_count, 1);
    assert_eq!(summary3.waiting_count, 1);
    assert!(summary3.status_text.contains("Antrian Poli Gigi"));

    // 6. Disconnect monitor 2 as well
    monitor_provider.set_monitors(vec![]);
    coordinator
        .handle_monitors_changed()
        .expect("handle disconnect all");

    let summary4 = coordinator.status_summary();
    assert_eq!(summary4.running_count, 0);
    assert_eq!(summary4.waiting_count, 2);
    assert!(
        summary4.status_text.contains("Monitor tujuan terputus")
            || summary4.status_text.contains("Menunggu")
    );
}

#[test]
fn test_automatic_failure_updates_tray_status_without_popup_or_focus_theft() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY1\MONITOR1"),
        name: "Monitor Antrian".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![monitor.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_failure_status");

    let mut coordinator =
        LifecycleCoordinator::with_services(store, monitor_provider, launcher, clock, base_dir);
    let mock_tray = Arc::new(MockTrayProvider::new());
    coordinator.set_tray_provider(mock_tray.clone());

    // Create profile with conflicting parameters that violate session isolation
    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Antrian Konflik".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--user-data-dir=C:\\custom_dir".to_string(),
                target_monitor: Some(monitor.identity.clone()),
            },
        )
        .expect("save profile");

    // Activate profile via shortcut handler
    let res = coordinator.handle_shortcut_activation(&profile_id);
    assert!(
        res.is_err(),
        "Activation must fail due to conflicting parameters"
    );

    // Verify coordinator error state
    assert!(coordinator.last_error().is_some());
    let err_msg = coordinator.last_error().unwrap().to_string();
    assert!(err_msg.contains("berbenturan dengan isolasi sesi"));

    // Verify tray status was updated automatically with error information without any popup
    let tray_status = mock_tray.last_status().expect("tray status");
    assert!(tray_status.has_error);
    assert!(tray_status.status_text.starts_with("Kesalahan:"));
    assert!(tray_status.tooltip.contains("Window Hop - Kesalahan:"));

    // Clearing error restores clean tray status
    coordinator.clear_error();
    assert!(coordinator.last_error().is_none());
    let clean_status = mock_tray.last_status().expect("clean tray status");
    assert!(!clean_status.has_error);
}

#[test]
fn test_shutdown_closes_all_managed_profiles_within_deadline_preserving_unmanaged_processes() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor1 = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY1\MONITOR1"),
        name: "Monitor Antrian 1".to_string(),
        is_primary: false,
    };
    let monitor2 = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY2\MONITOR2"),
        name: "Monitor Antrian 2".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        monitor1.clone(),
        monitor2.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_shutdown_cleanup");

    // Start unmanaged browser process (e.g. nurse's working Edge browser)
    let nurse_plan = window_hop::browsers::LaunchPlan {
        executable: PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
        arguments: vec!["--profile-directory=Default".to_string()],
        user_data_dir: PathBuf::from(r"C:\Users\Nurse\AppData\Local\Microsoft\Edge\User Data"),
        login_retention: window_hop::browsers::LoginRetentionNote::SessionIsolated {
            explanation: "Nurse default session".to_string(),
        },
    };
    let nurse_process = launcher
        .launch(&nurse_plan, &monitor1)
        .expect("launch nurse browser");
    assert!(nurse_process.is_alive());

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );
    let mock_tray = Arc::new(MockTrayProvider::new());
    coordinator.set_tray_provider(mock_tray.clone());

    // Create and activate Profile 1
    let p1_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Poli Umum".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli/umum --kiosk".to_string(),
                target_monitor: Some(monitor1.identity.clone()),
            },
        )
        .expect("save p1");
    coordinator.activate_profile(&p1_id).expect("activate p1");

    // Create and activate Profile 2
    let p2_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Poli Anak".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli/anak --kiosk".to_string(),
                target_monitor: Some(monitor2.identity.clone()),
            },
        )
        .expect("save p2");
    coordinator.activate_profile(&p2_id).expect("activate p2");

    assert_eq!(launcher.launched_processes().len(), 3);
    let managed_proc1 = launcher.launched_processes()[1].clone();
    let managed_proc2 = launcher.launched_processes()[2].clone();
    assert!(managed_proc1.is_alive());
    assert!(managed_proc2.is_alive());
    assert!(nurse_process.is_alive());

    // Perform shutdown from tray
    let outcomes = coordinator.shutdown().expect("shutdown");
    assert_eq!(outcomes.len(), 2);

    // Both managed profiles are stopped gracefully
    assert!(!managed_proc1.is_alive());
    assert!(!managed_proc2.is_alive());
    assert!(managed_proc1.close_requested());
    assert!(managed_proc2.close_requested());

    // Coordinator reports no profiles running
    assert!(!coordinator.is_profile_running(&p1_id));
    assert!(!coordinator.is_profile_running(&p2_id));

    // The nurse's working browser is COMPLETELY UNTOUCHED and still alive!
    assert!(
        nurse_process.is_alive(),
        "Nurse's working browser must remain alive and untouched"
    );

    // Tray was removed
    assert!(mock_tray.is_removed());
}

#[test]
fn test_shutdown_handles_unresponsive_browser_with_3s_deadline_force_stop() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY1\MONITOR1"),
        name: "Monitor Antrian".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![monitor.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_shutdown_unresponsive");

    // Nurse working browser
    let nurse_plan = window_hop::browsers::LaunchPlan {
        executable: PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
        arguments: vec!["--profile-directory=Default".to_string()],
        user_data_dir: PathBuf::from(r"C:\Users\Nurse\AppData\Local\Microsoft\Edge\User Data"),
        login_retention: window_hop::browsers::LoginRetentionNote::SessionIsolated {
            explanation: "Nurse default session".to_string(),
        },
    };
    let nurse_process = launcher
        .launch(&nurse_plan, &monitor)
        .expect("launch nurse browser");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock.clone(),
        base_dir,
    );
    let mock_tray = Arc::new(MockTrayProvider::new());
    coordinator.set_tray_provider(mock_tray.clone());

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Antrian Macet".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli/macet --kiosk".to_string(),
                target_monitor: Some(monitor.identity.clone()),
            },
        )
        .expect("save profile");

    let act = coordinator.activate_profile(&profile_id).expect("activate");
    let queue_pid = match act {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched"),
    };

    let queue_proc = launcher.launched_processes()[1].clone();
    queue_proc.set_refuse_graceful(true);

    let t0 = clock.now();
    let outcomes = coordinator.shutdown().expect("shutdown");
    let t1 = clock.now();

    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0],
        (profile_id.clone(), StopOutcome::Forced { pid: queue_pid })
    );
    assert!(
        t1.duration_since(t0) >= std::time::Duration::from_secs(3),
        "Shutdown must wait for 3s deadline before force stopping unresponsive instance"
    );

    assert!(queue_proc.close_requested());
    assert!(queue_proc.force_stopped());
    assert!(!queue_proc.is_alive());
    assert!(!coordinator.is_profile_running(&profile_id));

    // Nurse process remains alive
    assert!(nurse_process.is_alive(), "Nurse browser must remain alive");
    assert!(mock_tray.is_removed());
}

#[test]
fn test_shutdown_when_idle_succeeds_cleanly() {
    let store = Arc::new(MemoryProfileStore::new());
    let mut coordinator = LifecycleCoordinator::from_store(store);
    let mock_tray = Arc::new(MockTrayProvider::new());
    coordinator.set_tray_provider(mock_tray.clone());

    let outcomes = coordinator.shutdown().expect("shutdown");
    assert!(outcomes.is_empty());
    assert!(mock_tray.is_removed());
}

#[test]
fn test_shutdown_continues_stopping_other_profiles_when_one_fails() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor1 = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY1\MONITOR1"),
        name: "Monitor Antrian 1".to_string(),
        is_primary: false,
    };
    let monitor2 = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY2\MONITOR2"),
        name: "Monitor Antrian 2".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        monitor1.clone(),
        monitor2.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_shutdown_partial_failure");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );
    let mock_tray = Arc::new(MockTrayProvider::new());
    coordinator.set_tray_provider(mock_tray.clone());

    // Profile 1
    let p1_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Poli Satu".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli/1 --kiosk".to_string(),
                target_monitor: Some(monitor1.identity.clone()),
            },
        )
        .expect("save p1");
    coordinator.activate_profile(&p1_id).expect("activate p1");

    // Profile 2
    let p2_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Poli Dua".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli/2 --kiosk".to_string(),
                target_monitor: Some(monitor2.identity.clone()),
            },
        )
        .expect("save p2");
    coordinator.activate_profile(&p2_id).expect("activate p2");

    assert_eq!(launcher.launched_processes().len(), 2);
    let proc1 = launcher.launched_processes()[0].clone();
    let proc2 = launcher.launched_processes()[1].clone();

    // Make proc1 fail stop operations by refusing graceful close and failing force stop
    proc1.set_refuse_graceful(true);
    proc1.set_fail_force_stop(true);

    let res = coordinator.shutdown();
    assert!(
        res.is_err(),
        "shutdown should return error if one profile failed to stop"
    );

    // Crucial: coordinator must have STILL attempted and succeeded in stopping proc2!
    assert!(
        proc2.close_requested(),
        "proc2 must still have been stopped even though proc1 failed"
    );
    assert!(!proc2.is_alive());

    // Tray must be removed regardless
    assert!(mock_tray.is_removed());
}

#[test]
fn test_startup_settings_persist_across_restarts() {
    let dir = tempdir().expect("tempdir");
    let file_path = dir.path().join("profiles_startup_persist.json");
    let store = Arc::new(FileProfileStore::new(&file_path));

    let monitor = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY1\MONITOR1"),
        name: "Monitor Antrian".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![monitor.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store.clone(),
        monitor_provider.clone(),
        launcher.clone(),
        clock.clone(),
        base_dir.clone(),
    );
    let startup_manager = Arc::new(MockStartupManager::new(false));
    coordinator.set_startup_manager(startup_manager.clone());

    let p_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Antrian Poliklinik".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli/display --kiosk".to_string(),
                target_monitor: Some(monitor.identity.clone()),
            },
        )
        .expect("save profile");

    assert!(!coordinator.startup_enabled());
    assert!(coordinator.startup_profile_id().is_none());

    // Enable startup and select startup profile
    coordinator
        .set_startup_enabled(true)
        .expect("enable startup");
    assert!(coordinator.startup_enabled());
    assert!(startup_manager.is_registered());

    coordinator
        .set_startup_profile_id(Some(p_id.clone()))
        .expect("set startup profile");
    assert_eq!(coordinator.startup_profile_id(), Some(&p_id));
    assert_eq!(coordinator.get_startup_profile().unwrap().id(), &p_id);

    // Simulate closing and restarting application with fresh coordinator
    drop(coordinator);

    let mut new_coordinator = LifecycleCoordinator::with_services(
        store.clone(),
        monitor_provider,
        launcher,
        clock,
        base_dir,
    );
    let new_startup_manager = Arc::new(MockStartupManager::new(true));
    new_coordinator.set_startup_manager(new_startup_manager.clone());
    new_coordinator.load().expect("load from store");

    assert!(new_coordinator.startup_enabled());
    assert_eq!(new_coordinator.startup_profile_id(), Some(&p_id));
    let startup_profile = new_coordinator
        .get_startup_profile()
        .expect("startup profile should exist");
    assert_eq!(startup_profile.name(), "Antrian Poliklinik");

    // Unset startup settings and verify persistence
    new_coordinator
        .set_startup_enabled(false)
        .expect("disable startup");
    assert!(!new_coordinator.startup_enabled());
    assert!(!new_startup_manager.is_registered());

    new_coordinator
        .set_startup_profile_id(None)
        .expect("clear startup profile id");
    assert!(new_coordinator.startup_profile_id().is_none());

    // Reload again
    let mut third_coordinator = LifecycleCoordinator::from_store(store);
    third_coordinator.load().expect("reload");
    assert!(!third_coordinator.startup_enabled());
    assert!(third_coordinator.startup_profile_id().is_none());
}

#[test]
fn test_startup_activation_launches_selected_profile_in_tray() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY1\MONITOR1"),
        name: "Monitor Antrian Poliklinik".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![monitor.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_startup_launch");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );
    let mock_tray = Arc::new(MockTrayProvider::new());
    coordinator.set_tray_provider(mock_tray.clone());
    let mock_startup = Arc::new(MockStartupManager::new(false));
    coordinator.set_startup_manager(mock_startup);

    let p_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Antrian Rawat Jalan".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli/rawat-jalan --kiosk".to_string(),
                target_monitor: Some(monitor.identity.clone()),
            },
        )
        .expect("save profile");

    coordinator
        .set_startup_enabled(true)
        .expect("enable startup");
    coordinator
        .set_startup_profile_id(Some(p_id.clone()))
        .expect("set startup profile");

    // Perform startup activation
    let result = coordinator
        .handle_startup_activation()
        .expect("startup activation should succeed");

    assert!(result.is_some());
    match result.unwrap() {
        ActivationResult::Launched {
            pid,
            target_monitor,
            login_retention,
        } => {
            assert_eq!(target_monitor.identity, monitor.identity);
            assert_eq!(launcher.launched_processes().len(), 1);
            assert_eq!(launcher.launched_processes()[0].pid(), pid);
            assert!(!login_retention.explanation().is_empty());
        }
        other => panic!("Expected Launched, got {:?}", other),
    }

    assert!(coordinator.is_profile_running(&p_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&p_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: launcher.launched_processes()[0].pid(),
            target_monitor: monitor,
        }
    );

    let summary = coordinator.status_summary();
    assert_eq!(summary.running_count, 1);
    assert!(summary.status_text.contains("Antrian Rawat Jalan"));
    assert!(!summary.has_error);
    assert!(coordinator.last_error().is_none());
}

#[test]
fn test_startup_disabled_does_not_launch_profile() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY1\MONITOR1"),
        name: "Monitor Antrian".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![monitor.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_startup_disabled");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let p_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Antrian Rawat Jalan".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli/rawat-jalan --kiosk".to_string(),
                target_monitor: Some(monitor.identity.clone()),
            },
        )
        .expect("save profile");

    coordinator
        .set_startup_profile_id(Some(p_id.clone()))
        .expect("set startup profile");
    assert!(!coordinator.startup_enabled());

    let result = coordinator
        .handle_startup_activation()
        .expect("handle startup activation");

    assert!(result.is_none(), "Startup disabled must return Ok(None)");
    assert_eq!(launcher.launched_processes().len(), 0);
    assert!(!coordinator.is_profile_running(&p_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&p_id).unwrap(),
        ProfileLifecycleState::Idle
    );
}

#[test]
fn test_startup_activation_can_restart_previously_crashed_or_manually_closed_profile() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY1\MONITOR1"),
        name: "Monitor Antrian".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![monitor.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_startup_restart_crashed");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock,
        base_dir,
    );
    coordinator.set_startup_manager(Arc::new(MockStartupManager::new(false)));

    let p_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Antrian Poli".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli --kiosk".to_string(),
                target_monitor: Some(monitor.identity.clone()),
            },
        )
        .expect("save profile");

    coordinator
        .set_startup_enabled(true)
        .expect("enable startup");
    coordinator
        .set_startup_profile_id(Some(p_id.clone()))
        .expect("set startup profile");

    // Initial activation
    coordinator
        .activate_profile(&p_id)
        .expect("initial activation");
    assert_eq!(launcher.launched_processes().len(), 1);
    let proc1 = launcher.launched_processes()[0].clone();

    // Browser crashes
    proc1.simulate_exit();
    let outcome = coordinator
        .handle_process_exit(proc1.pid(), ExitReason::Crash)
        .expect("handle crash");
    assert_eq!(
        outcome,
        LifecycleTransitionOutcome::Exited {
            profile_id: p_id.clone(),
            pid: proc1.pid(),
            reason: ExitReason::Crash,
        }
    );
    assert_eq!(
        coordinator.get_lifecycle_state(&p_id).unwrap(),
        ProfileLifecycleState::Exited {
            reason: ExitReason::Crash
        }
    );

    // Verify monitor reconnect does NOT resurrect the crashed profile (acceptance requirement)
    monitor_provider.set_monitors(vec![monitor.clone()]);
    let reconnect_res = coordinator.handle_monitors_changed().expect("reconnect");
    assert!(
        reconnect_res.is_empty(),
        "Crashed profile must not be resurrected by reconnect"
    );
    assert_eq!(launcher.launched_processes().len(), 1);

    // On next Windows login (handle_startup_activation): profile is restarted cleanly!
    let startup_res = coordinator
        .handle_startup_activation()
        .expect("startup activation after crash");
    assert!(startup_res.is_some());
    assert_eq!(launcher.launched_processes().len(), 2);
    let proc2 = launcher.launched_processes()[1].clone();
    assert_ne!(proc1.pid(), proc2.pid());
    assert!(coordinator.is_profile_running(&p_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&p_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: proc2.pid(),
            target_monitor: monitor,
        }
    );
}

#[test]
fn test_startup_activation_when_target_monitor_unavailable_enters_waiting_state_no_fallback() {
    let store = Arc::new(MemoryProfileStore::new());
    let target_identity = MonitorIdentity::new(r"\\.\DISPLAY2\MONITOR2");
    let primary_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY1\MONITOR1"),
        name: "Monitor Utama Perawat".to_string(),
        is_primary: true,
    };
    // Target monitor 2 is disconnected/unavailable at boot time
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![primary_monitor.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_startup_monitor_unavailable");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider.clone(),
        launcher.clone(),
        clock,
        base_dir,
    );
    let mock_tray = Arc::new(MockTrayProvider::new());
    coordinator.set_tray_provider(mock_tray.clone());
    coordinator.set_startup_manager(Arc::new(MockStartupManager::new(false)));

    let p_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Antrian Kamar Bedah".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.bedah --kiosk".to_string(),
                target_monitor: Some(target_identity.clone()),
            },
        )
        .expect("save profile");

    coordinator
        .set_startup_enabled(true)
        .expect("enable startup");
    coordinator
        .set_startup_profile_id(Some(p_id.clone()))
        .expect("set startup profile");

    // Login with startup active, but target monitor not connected
    let startup_res = coordinator
        .handle_startup_activation()
        .expect("handle startup activation should transition to waiting");

    assert_eq!(
        startup_res,
        Some(ActivationResult::WaitingForMonitor {
            target_monitor: target_identity.clone(),
        })
    );
    // Crucial: No browser launched on primary monitor!
    assert_eq!(launcher.launched_processes().len(), 0);
    assert!(!coordinator.is_profile_running(&p_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&p_id).unwrap(),
        ProfileLifecycleState::WaitingForMonitor {
            target_monitor: target_identity.clone(),
        }
    );

    // Tray status reflects waiting without popup or stealing focus
    let summary = coordinator.status_summary();
    assert_eq!(summary.waiting_count, 1);
    assert_eq!(summary.running_count, 0);
    assert_eq!(summary.status_text, "Menunggu monitor tujuan");

    // Target monitor connects later
    let target_monitor = MonitorDescriptor {
        identity: target_identity.clone(),
        name: "Monitor Antrian Bedah".to_string(),
        is_primary: false,
    };
    monitor_provider.set_monitors(vec![primary_monitor, target_monitor.clone()]);
    let restored = coordinator
        .handle_monitors_changed()
        .expect("handle reconnect");

    assert_eq!(restored.len(), 1);
    assert_eq!(launcher.launched_processes().len(), 1);
    assert!(coordinator.is_profile_running(&p_id));
    assert_eq!(
        coordinator.get_lifecycle_state(&p_id).unwrap(),
        ProfileLifecycleState::Running {
            pid: launcher.launched_processes()[0].pid(),
            target_monitor,
        }
    );
}

#[test]
fn test_startup_activation_fails_without_fallback_when_startup_profile_unselected_or_invalid() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY1\MONITOR1"),
        name: "Monitor Antrian".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![monitor.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_startup_invalid_ref");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );
    let mock_tray = Arc::new(MockTrayProvider::new());
    coordinator.set_tray_provider(mock_tray.clone());
    coordinator.set_startup_manager(Arc::new(MockStartupManager::new(false)));

    // Case A: Startup enabled, but startup_profile_id is None
    coordinator
        .set_startup_enabled(true)
        .expect("enable startup");
    assert!(coordinator.startup_profile_id().is_none());

    let res_a = coordinator.handle_startup_activation();
    assert!(
        res_a.is_err(),
        "Must return error when no startup profile chosen"
    );
    match res_a.unwrap_err() {
        ApplicationError::InvalidStartupProfileReference(msg) => {
            assert!(msg.contains("belum ada profil startup"));
        }
        other => panic!("Expected InvalidStartupProfileReference, got {:?}", other),
    }
    assert_eq!(launcher.launched_processes().len(), 0);
    assert!(coordinator.last_error().is_some());
    assert!(coordinator.status_summary().has_error);

    // Case B: Startup enabled, but startup_profile_id references a non-existent / deleted profile
    let fake_id = ProfileId::new("non-existent-profile-12345");
    // Manually set invalid startup_profile_id on internal config to simulate external file edit/corruption
    coordinator.set_startup_profile_id_unchecked_for_test(Some(fake_id));

    let res_b = coordinator.handle_startup_activation();
    assert!(res_b.is_err(), "Must return error without fallback");
    match res_b.unwrap_err() {
        ApplicationError::InvalidStartupProfileReference(msg) => {
            assert!(msg.contains("tidak valid atau telah dihapus"));
        }
        other => panic!("Expected InvalidStartupProfileReference, got {:?}", other),
    }
    // No fallback to any other profile or primary monitor
    assert_eq!(launcher.launched_processes().len(), 0);
    assert!(coordinator.last_error().is_some());
}

#[test]
fn test_delete_profile_clears_startup_profile_id_when_deleted_profile_was_startup_profile() {
    let dir = tempdir().expect("tempdir");
    let file_path = dir.path().join("profiles_delete_startup.json");
    let store = Arc::new(FileProfileStore::new(&file_path));

    let monitor = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY1\MONITOR1"),
        name: "Monitor Antrian".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![monitor.clone()]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store.clone(),
        monitor_provider,
        launcher,
        clock,
        base_dir,
    );
    coordinator.set_startup_manager(Arc::new(MockStartupManager::new(false)));

    let p1_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Antrian 1".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli/1 --kiosk".to_string(),
                target_monitor: Some(monitor.identity.clone()),
            },
        )
        .expect("save p1");

    let p2_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Antrian 2".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli/2 --kiosk".to_string(),
                target_monitor: Some(monitor.identity.clone()),
            },
        )
        .expect("save p2");

    coordinator
        .set_startup_enabled(true)
        .expect("enable startup");
    coordinator
        .set_startup_profile_id(Some(p1_id.clone()))
        .expect("set p1 as startup profile");

    assert_eq!(coordinator.startup_profile_id(), Some(&p1_id));

    // Delete p1 (which was the startup profile)
    coordinator.delete_profile(&p1_id).expect("delete p1");

    // Verify startup_profile_id is now None
    assert!(coordinator.startup_profile_id().is_none());

    // Verify persisted store also has None for startup_profile_id
    let reloaded = store.load().expect("reload from store");
    assert_eq!(reloaded.profiles().len(), 1);
    assert_eq!(reloaded.profiles()[0].id(), &p2_id);
    assert!(reloaded.startup_enabled());
    assert!(reloaded.startup_profile_id().is_none());
}

#[test]
fn test_startup_settings_os_registration_failure_preserves_state() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(Vec::new()));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_startup_os_fail");

    let mut coordinator = LifecycleCoordinator::with_services(
        store.clone(),
        monitor_provider,
        launcher,
        clock,
        base_dir,
    );

    let startup_manager = Arc::new(MockStartupManager::new(false));
    coordinator.set_startup_manager(startup_manager.clone());

    // Simulate platform OS registration error
    startup_manager.set_error(Some("Registry permission denied".to_string()));

    let res = coordinator.set_startup_enabled(true);
    assert!(res.is_err());
    match res.unwrap_err() {
        ApplicationError::Platform(err) => {
            assert!(err.to_string().contains("Registry permission denied"));
        }
        other => panic!("Expected Platform error, got {:?}", other),
    }

    assert!(!coordinator.startup_enabled());
    assert!(!startup_manager.is_registered());

    // Stored config should also still be false
    let stored = store.load().unwrap();
    assert!(!stored.startup_enabled());
}

#[test]
fn test_concurrent_shortcut_and_startup_activation_share_single_coordinator() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor1 = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY1\MONITOR1"),
        name: "Monitor Antrian 1".to_string(),
        is_primary: false,
    };
    let monitor2 = MonitorDescriptor {
        identity: MonitorIdentity::new(r"\\.\DISPLAY2\MONITOR2"),
        name: "Monitor Antrian 2".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        monitor1.clone(),
        monitor2.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = PathBuf::from("/tmp/window_hop_test_concurrent_startup_shortcut");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );
    coordinator.set_startup_manager(Arc::new(MockStartupManager::new(false)));

    let startup_p_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Antrian Startup".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli/startup --kiosk".to_string(),
                target_monitor: Some(monitor1.identity.clone()),
            },
        )
        .expect("save startup profile");

    let shortcut_p_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput {
                name: "Antrian Shortcut".to_string(),
                browser_path: PathBuf::from(
                    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
                ),
                custom_parameters: "--app=https://antrian.poli/shortcut --kiosk".to_string(),
                target_monitor: Some(monitor2.identity.clone()),
            },
        )
        .expect("save shortcut profile");

    coordinator
        .set_startup_enabled(true)
        .expect("enable startup");
    coordinator
        .set_startup_profile_id(Some(startup_p_id.clone()))
        .expect("set startup profile");

    // Rapid concurrent launch: shortcut clicked while startup runs
    let res_shortcut = coordinator
        .handle_shortcut_activation(&shortcut_p_id)
        .expect("shortcut activation");
    assert!(matches!(res_shortcut, ActivationResult::Launched { .. }));

    let res_startup = coordinator
        .handle_startup_activation()
        .expect("startup activation");
    assert!(matches!(
        res_startup,
        Some(ActivationResult::Launched { .. })
    ));

    // Both profiles run on their respective target monitors under the single coordinator
    assert_eq!(launcher.launched_processes().len(), 2);
    assert!(coordinator.is_profile_running(&shortcut_p_id));
    assert!(coordinator.is_profile_running(&startup_p_id));

    // Repeated startup activation for the already running startup profile reuses the window without duplicate
    let res_startup_repeat = coordinator
        .handle_startup_activation()
        .expect("repeat startup activation");
    assert!(matches!(
        res_startup_repeat,
        Some(ActivationResult::Reused { .. })
    ));
    assert_eq!(launcher.launched_processes().len(), 2);
}

fn create_mock_pe_executable(dir: &std::path::Path, filename: &str, machine: u16) -> PathBuf {
    let file_path = dir.join(filename);
    let mut data = vec![0u8; 256];
    data[0] = b'M';
    data[1] = b'Z';
    let pe_offset: u32 = 0x80;
    data[0x3C..0x40].copy_from_slice(&pe_offset.to_le_bytes());
    data[0x80] = b'P';
    data[0x81] = b'E';
    data[0x82] = 0;
    data[0x83] = 0;
    data[0x84..0x86].copy_from_slice(&machine.to_le_bytes());
    std::fs::write(&file_path, data).expect("write mock pe executable");
    file_path
}

#[test]
fn test_coordinator_detects_installed_browsers_and_handles_error() {
    let store = Arc::new(MemoryProfileStore::new());
    let mut coordinator = LifecycleCoordinator::from_store(store);

    let b1 = DetectedBrowser {
        name: "Microsoft Edge".to_string(),
        executable_path: PathBuf::from(
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        ),
        version: Some("120.0.0.0".to_string()),
        architecture: BrowserArchitecture::X64,
    };
    let b2 = DetectedBrowser {
        name: "Google Chrome".to_string(),
        executable_path: PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        version: Some("121.0.0.0".to_string()),
        architecture: BrowserArchitecture::X86,
    };

    let detector = Arc::new(MockBrowserDetector::new(vec![b1.clone(), b2.clone()]));
    coordinator.set_browser_detector(detector.clone());

    let detected = coordinator.detected_browsers().expect("detected browsers");
    assert_eq!(detected.len(), 2);
    assert_eq!(detected[0], b1);
    assert_eq!(detected[1], b2);

    // Error handling
    detector.set_error(Some("Gagal membaca registry".to_string()));
    let err = coordinator.detected_browsers().unwrap_err();
    assert!(err.to_string().contains("registry"));
}

#[test]
fn test_save_and_reopen_profile_with_detected_or_portable_browser_and_check_compatibility() {
    let dir = tempdir().expect("tempdir");
    let file_path = dir.path().join("profiles.json");
    let store1 = Arc::new(FileProfileStore::new(&file_path));
    let mut app1 = LifecycleCoordinator::from_store(store1);

    // Create a mock portable browser executable in tempdir
    let portable_exe = create_mock_pe_executable(dir.path(), "my_portable_browser.exe", 0x014c); // x86
    let edge_exe = create_mock_pe_executable(dir.path(), "msedge.exe", 0x8664); // x64

    let p1_id = app1
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Profil Portable",
                portable_exe.clone(),
                "--app=https://antrian.local",
            ),
        )
        .expect("save portable profile");

    let p2_id = app1
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Profil Edge Terverifikasi",
                edge_exe.clone(),
                "--kiosk https://antrian.local",
            ),
        )
        .expect("save edge profile");

    // Reopen in a new coordinator instance (simulate restarting Window Hop)
    let store2 = Arc::new(FileProfileStore::new(&file_path));
    let mut app2 = LifecycleCoordinator::from_store(store2);
    app2.load().expect("load profiles on reopen");

    let p1 = app2.get_profile(&p1_id).expect("get p1");
    assert_eq!(p1.browser_path(), &portable_exe);
    let compat1 = app2.check_browser_compatibility(p1.browser_path());
    match compat1 {
        BrowserCompatibilityStatus::Unverified { evidence } => {
            assert_eq!(evidence.architecture, BrowserArchitecture::X86);
            assert!(!evidence.is_safe_to_activate);
            assert!(evidence.evidence_text.contains("belum diverifikasi"));
        }
        other => panic!("expected Unverified, got {:?}", other),
    }

    let p2 = app2.get_profile(&p2_id).expect("get p2");
    assert_eq!(p2.browser_path(), &edge_exe);
    let compat2 = app2.check_browser_compatibility(p2.browser_path());
    match compat2 {
        BrowserCompatibilityStatus::SafeToActivate { adapter, evidence } => {
            assert_eq!(adapter.browser_name(), "Microsoft Edge");
            assert_eq!(evidence.architecture, BrowserArchitecture::X64);
            assert!(evidence.is_safe_to_activate);
            assert!(evidence.evidence_text.contains("user-data-dir"));
        }
        other => panic!("expected SafeToActivate, got {:?}", other),
    }
}

#[test]
fn test_activate_profile_fails_when_browser_executable_missing_or_unavailable() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let missing_path = dir.path().join("does_not_exist_msedge.exe");
    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Antrian Edge Hilang", missing_path.clone(), "--kiosk")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let act_res = coordinator.activate_profile(&profile_id);
    assert!(act_res.is_err());
    match act_res.unwrap_err() {
        ApplicationError::BrowserExecutableNotFound(path) => {
            assert_eq!(path, missing_path);
        }
        err => panic!("expected BrowserExecutableNotFound, got: {:?}", err),
    }
    assert_eq!(launcher.launched_processes().len(), 0);
}

#[test]
fn test_activate_profile_fails_when_browser_unverified_even_if_executable_exists_on_disk() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    // Create an existing unverified browser executable on disk
    let unverified_exe = create_mock_pe_executable(dir.path(), "custom_browser.exe", 0x8664);

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Antrian Custom Browser", unverified_exe.clone(), "--kiosk")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let can_res = coordinator.can_activate(&profile_id);
    assert!(can_res.is_err());
    match can_res.unwrap_err() {
        ApplicationError::BrowserNotVerified(path, reason) => {
            assert!(path.contains("custom_browser.exe"));
            assert!(reason.contains("belum diverifikasi"));
        }
        err => panic!("expected BrowserNotVerified, got: {:?}", err),
    }

    let act_res = coordinator.activate_profile(&profile_id);
    assert!(act_res.is_err());
    assert_eq!(launcher.launched_processes().len(), 0);
}

#[test]
fn test_activate_profile_fails_when_chrome_has_corrupt_binary_architecture() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    // Create a corrupt non-PE chrome.exe
    let corrupt_chrome = dir.path().join("chrome.exe");
    std::fs::write(&corrupt_chrome, b"not a pe binary").expect("write corrupt chrome");

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Antrian Chrome Corrupt", corrupt_chrome.clone(), "--kiosk")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let can_res = coordinator.can_activate(&profile_id);
    assert!(can_res.is_err());
    match can_res.unwrap_err() {
        ApplicationError::BrowserNotVerified(path, reason) => {
            assert!(path.contains("chrome.exe"));
            assert!(reason.contains("belum diverifikasi"));
        }
        err => panic!("expected BrowserNotVerified, got: {:?}", err),
    }

    let act_res = coordinator.activate_profile(&profile_id);
    assert!(act_res.is_err());
    assert_eq!(launcher.launched_processes().len(), 0);
}

#[test]
fn test_activate_profile_launches_isolated_chrome_on_target_monitor_preserving_nurse_browser() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-CHROME"),
        name: "Queue Display Chrome".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        target_mon.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir.clone(),
    );

    // 1. Simulate nurse's already running working Chrome instance on primary monitor
    let nurse_working_browser_plan = window_hop::browsers::LaunchPlan {
        executable: PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        arguments: vec!["https://hospital.simrs/nurse-portal".to_string()],
        user_data_dir: PathBuf::from(r"C:\Users\Nurse\AppData\Local\Google\Chrome\User Data"),
        login_retention: window_hop::browsers::LoginRetentionNote::SessionIsolated {
            explanation: "Nurse default working session".to_string(),
        },
    };
    let nurse_process = launcher
        .launch(
            &nurse_working_browser_plan,
            &MonitorDescriptor {
                identity: MonitorIdentity::new("MON-PRIMARY"),
                name: "Workstation Main Display".to_string(),
                is_primary: true,
            },
        )
        .expect("nurse chrome browser running");
    let nurse_pid = nurse_process.pid();

    // 2. Configure Window Hop queue profile with Chrome
    let chrome_exe = create_mock_pe_executable(dir.path(), "chrome.exe", 0x8664);
    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poliklinik THT Chrome",
                chrome_exe.clone(),
                r#"--incognito --kiosk "https://antrian.poli/tht?room=3""#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-CHROME"))),
        )
        .expect("save chrome profile");

    assert!(coordinator.can_activate(&profile_id).unwrap());

    // 3. Activate Chrome profile
    let activation_result = coordinator
        .activate_profile(&profile_id)
        .expect("activate chrome profile");

    match activation_result {
        ActivationResult::Launched {
            pid,
            target_monitor,
            login_retention,
        } => {
            assert_ne!(
                pid, nurse_pid,
                "Chrome queue profile must have its own isolated PID and not merge with nurse browser"
            );
            assert_eq!(target_monitor, target_mon);

            // Chrome incognito/kiosk login retention note check
            match login_retention {
                window_hop::browsers::LoginRetentionNote::KioskOrInPrivateNoRetention {
                    explanation,
                } => {
                    assert!(explanation.contains("Google Chrome"));
                    assert!(explanation.contains("incognito") || explanation.contains("kiosk"));
                }
                other => panic!("expected KioskOrInPrivateNoRetention, got: {:?}", other),
            }

            // Verify queue process launch plan
            let launched = launcher.launched_processes();
            assert_eq!(launched.len(), 2, "Nurse process + queue process");
            let queue_proc = &launched[1];
            assert_eq!(queue_proc.pid(), pid);
            assert_eq!(queue_proc.current_monitor(), target_mon);

            let expected_user_data_dir = base_dir
                .join("profiles")
                .join(profile_id.as_str())
                .join("browser_session");
            assert!(
                queue_proc
                    .plan()
                    .arguments
                    .iter()
                    .any(|arg| arg.contains(&expected_user_data_dir.display().to_string())),
                "Queue profile must use its own isolated user-data-dir"
            );

            // Verify custom quoted argument was passed correctly without improper splitting
            assert!(
                queue_proc
                    .plan()
                    .arguments
                    .iter()
                    .any(|arg| arg == "https://antrian.poli/tht?room=3"),
                "Quoted URL parameter must be preserved intact"
            );
        }
        other => panic!("expected Launched result, got: {:?}", other),
    }

    // 4. Repeated activation reuses window profil and restores placement without duplicate process
    let reuse_result = coordinator
        .activate_profile(&profile_id)
        .expect("reactivate chrome profile");

    match reuse_result {
        ActivationResult::Reused {
            pid,
            target_monitor,
            ..
        } => {
            assert_ne!(pid, nurse_pid);
            assert_eq!(target_monitor, target_mon);
            assert_eq!(
                launcher.launched_processes().len(),
                2,
                "No new process spawned on reactivation"
            );
        }
        other => panic!("expected Reused, got: {:?}", other),
    }

    // 5. Nurse's browser is still running and completely unaffected
    assert!(
        nurse_process.is_alive(),
        "Nurse's working Chrome browser must remain running"
    );
}

#[test]
fn test_activate_profile_fails_when_chrome_custom_parameters_conflict() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let chrome_exe = create_mock_pe_executable(dir.path(), "chrome.exe", 0x8664);

    // Profile with conflicting --user-data-dir
    let p_conflict1 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Conflict Chrome 1",
                chrome_exe.clone(),
                r#"--user-data-dir="C:\CustomData""#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let err1 = coordinator.can_activate(&p_conflict1).unwrap_err();
    match err1 {
        ApplicationError::Browser(
            window_hop::browsers::BrowserError::ConflictingIsolationParameter { arg, reason },
        ) => {
            assert!(arg.contains("--user-data-dir"));
            assert!(reason.contains("Window Hop secara otomatis mengelola"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Profile with conflicting --profile-directory
    let p_conflict2 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Conflict Chrome 2",
                chrome_exe,
                r#"--profile-directory="Profile 2""#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let err2 = coordinator.activate_profile(&p_conflict2).unwrap_err();
    match err2 {
        ApplicationError::Browser(
            window_hop::browsers::BrowserError::ConflictingIsolationParameter { arg, reason },
        ) => {
            assert!(arg.contains("--profile-directory"));
            assert!(reason.contains("profil sesi browser tersendiri"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    assert_eq!(launcher.launched_processes().len(), 0);
}

#[test]
fn test_chrome_lifecycle_graceful_close_and_3s_deadline_force_stop_preserves_nurse_browser() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        target_mon.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock.clone(),
        base_dir,
    );

    // Nurse Chrome process
    let nurse_plan = window_hop::browsers::LaunchPlan {
        executable: PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        arguments: vec!["https://hospital.simrs".to_string()],
        user_data_dir: PathBuf::from(r"C:\Users\Nurse\AppData\Local\Google\Chrome\User Data"),
        login_retention: window_hop::browsers::LoginRetentionNote::SessionIsolated {
            explanation: "Nurse".to_string(),
        },
    };
    let nurse_proc = launcher
        .launch(
            &nurse_plan,
            &MonitorDescriptor {
                identity: MonitorIdentity::new("MON-PRIMARY"),
                name: "Workstation Main Display".to_string(),
                is_primary: true,
            },
        )
        .expect("nurse process");

    // Queue Chrome process with unresponsive behavior to test 3s deadline force stop
    let chrome_exe = create_mock_pe_executable(dir.path(), "chrome.exe", 0x8664);
    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Chrome Queue", chrome_exe, "https://antrian.poli/queue")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let act_res = coordinator.activate_profile(&profile_id).expect("activate");
    let queue_pid = match act_res {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched"),
    };

    // Configure queue process to refuse graceful close (simulate confirmation dialog hang)
    let queue_proc = launcher.launched_processes()[1].clone();
    queue_proc.set_refuse_graceful(true);

    let t0 = clock.now();
    let outcome = coordinator.stop_profile(&profile_id).expect("stop profile");
    let t1 = clock.now();

    // 1. Outcome must be Forced after 3 seconds
    assert_eq!(outcome, StopOutcome::Forced { pid: queue_pid });
    assert!(
        t1.duration_since(t0) >= std::time::Duration::from_secs(3),
        "Must wait for 3-second deadline before force stopping"
    );

    // 2. Queue process received close request first, then force stop
    assert!(
        queue_proc.close_requested(),
        "Must have requested graceful close first"
    );
    assert!(
        queue_proc.force_stopped(),
        "Must have force stopped unresponsive process"
    );
    assert!(!queue_proc.is_alive());

    // Nurse Chrome process must remain completely untouched!
    assert!(
        nurse_proc.is_alive(),
        "Nurse's Chrome browser must NEVER be stopped or force-terminated"
    );
}

#[test]
fn test_chrome_profile_switching_on_same_monitor_stops_previous_before_launching_new() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        target_mon,
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let chrome_exe = create_mock_pe_executable(dir.path(), "chrome.exe", 0x8664);

    let p1 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Chrome Poli 1", chrome_exe.clone(), "https://antrian/1")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save p1");

    let p2 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Chrome Poli 2", chrome_exe, "https://antrian/2")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save p2");

    let act1 = coordinator.activate_profile(&p1).expect("activate p1");
    let pid1 = match act1 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched p1"),
    };

    // Activate p2 on same monitor: must stop p1 before launching p2
    let act2 = coordinator.activate_profile(&p2).expect("activate p2");
    let pid2 = match act2 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched p2"),
    };

    assert_ne!(pid1, pid2);
    let launched = launcher.launched_processes();
    let proc1 = launched.iter().find(|p| p.pid() == pid1).unwrap();
    let proc2 = launched.iter().find(|p| p.pid() == pid2).unwrap();

    assert!(!proc1.is_alive(), "p1 Chrome instance must be stopped");
    assert!(proc2.is_alive(), "p2 Chrome instance must be running");
}

#[test]
fn test_shortcut_and_startup_activation_with_chrome() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        target_mon,
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );
    coordinator.set_startup_manager(Arc::new(MockStartupManager::new(false)));

    let chrome_exe = create_mock_pe_executable(dir.path(), "chrome.exe", 0x8664);

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Shortcut Chrome",
                chrome_exe,
                r#"--kiosk "https://antrian/shortcut""#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    // 1. Shortcut activation launches Chrome
    let shortcut_status = coordinator
        .handle_shortcut_activation(&profile_id)
        .expect("shortcut activation");
    assert!(matches!(shortcut_status, ActivationResult::Launched { .. }));
    assert_eq!(launcher.launched_processes().len(), 1);

    // 2. Second shortcut activation reuses window profil
    let shortcut_status2 = coordinator
        .handle_shortcut_activation(&profile_id)
        .expect("second shortcut activation");
    assert!(matches!(shortcut_status2, ActivationResult::Reused { .. }));
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "No duplicate process spawned"
    );

    // 3. Configure as startup profile
    coordinator
        .set_startup_enabled(true)
        .expect("enable startup");
    coordinator
        .set_startup_profile_id(Some(profile_id.clone()))
        .expect("set startup profile");

    let startup_status = coordinator
        .handle_startup_activation()
        .expect("startup activation");
    assert!(matches!(
        startup_status,
        Some(ActivationResult::Reused { .. })
    ));
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "Already running instance is reused during startup activation"
    );
}

#[test]
fn test_activate_profile_fails_when_firefox_has_corrupt_binary_architecture() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    // Create a corrupt non-PE firefox.exe
    let corrupt_firefox = dir.path().join("firefox.exe");
    std::fs::write(&corrupt_firefox, b"not a pe binary").expect("write corrupt firefox");

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Firefox Corrupt",
                corrupt_firefox.clone(),
                "--kiosk",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let can_res = coordinator.can_activate(&profile_id);
    assert!(can_res.is_err());
    match can_res.unwrap_err() {
        ApplicationError::BrowserNotVerified(path, reason) => {
            assert!(path.contains("firefox.exe"));
            assert!(reason.contains("belum diverifikasi"));
        }
        err => panic!("expected BrowserNotVerified, got: {:?}", err),
    }

    let act_res = coordinator.activate_profile(&profile_id);
    assert!(act_res.is_err());
    assert_eq!(launcher.launched_processes().len(), 0);
}

#[test]
fn test_activate_profile_launches_isolated_firefox_on_target_monitor_preserving_nurse_browser() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-FIREFOX"),
        name: "Queue Display Firefox".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        target_mon.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir.clone(),
    );

    // 1. Simulate nurse's already running working Firefox instance on primary monitor
    let nurse_working_browser_plan = window_hop::browsers::LaunchPlan {
        executable: PathBuf::from(r"C:\Program Files\Mozilla Firefox\firefox.exe"),
        arguments: vec![
            "-P".to_string(),
            "NurseSession".to_string(),
            "https://hospital.simrs/nurse-portal".to_string(),
        ],
        user_data_dir: PathBuf::from(
            r"C:\Users\Nurse\AppData\Roaming\Mozilla\Firefox\Profiles\nurse",
        ),
        login_retention: window_hop::browsers::LoginRetentionNote::SessionIsolated {
            explanation: "Nurse default working session".to_string(),
        },
    };
    let nurse_process = launcher
        .launch(
            &nurse_working_browser_plan,
            &MonitorDescriptor {
                identity: MonitorIdentity::new("MON-PRIMARY"),
                name: "Workstation Main Display".to_string(),
                is_primary: true,
            },
        )
        .expect("nurse firefox browser running");
    let nurse_pid = nurse_process.pid();

    // 2. Configure Window Hop queue profile with Firefox
    let firefox_exe = create_mock_pe_executable(dir.path(), "firefox.exe", 0x8664);
    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poliklinik Mata Firefox",
                firefox_exe.clone(),
                r#"-private-window --kiosk "https://antrian.poli/mata?room=2""#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-FIREFOX"))),
        )
        .expect("save firefox profile");

    assert!(coordinator.can_activate(&profile_id).unwrap());

    // 3. Activate Firefox profile
    let activation_result = coordinator
        .activate_profile(&profile_id)
        .expect("activate firefox profile");

    match activation_result {
        ActivationResult::Launched {
            pid,
            target_monitor,
            login_retention,
        } => {
            assert_ne!(
                pid, nurse_pid,
                "Firefox queue profile must have its own isolated PID and not merge with nurse browser"
            );
            assert_eq!(target_monitor, target_mon);

            // Firefox private mode login retention note check
            match login_retention {
                window_hop::browsers::LoginRetentionNote::KioskOrInPrivateNoRetention {
                    explanation,
                } => {
                    assert!(explanation.contains("Mozilla Firefox"));
                    assert!(explanation.contains("privat"));
                }
                other => panic!("expected KioskOrInPrivateNoRetention, got: {:?}", other),
            }

            // Verify queue process launch plan
            let launched = launcher.launched_processes();
            assert_eq!(launched.len(), 2, "Nurse process + queue process");
            let queue_proc = &launched[1];
            assert_eq!(queue_proc.pid(), pid);
            assert_eq!(queue_proc.current_monitor(), target_mon);

            let expected_profile_dir = base_dir
                .join("profiles")
                .join(profile_id.as_str())
                .join("browser_session");

            // Isolation arguments: -no-remote and -profile <path>
            let args = &queue_proc.plan().arguments;
            assert_eq!(args[0], "-no-remote");
            assert_eq!(args[1], "-profile");
            assert_eq!(args[2], expected_profile_dir.to_string_lossy().to_string());

            // Custom arguments: -private-window, --kiosk, URL
            assert!(args.iter().any(|arg| arg == "-private-window"));
            assert!(args.iter().any(|arg| arg == "--kiosk"));
            assert!(
                args.iter()
                    .any(|arg| arg == "https://antrian.poli/mata?room=2"),
                "Quoted URL parameter must be preserved intact"
            );
        }
        other => panic!("expected Launched result, got: {:?}", other),
    }

    // 4. Repeated activation reuses window profil and restores placement without duplicate process
    let reuse_result = coordinator
        .activate_profile(&profile_id)
        .expect("reactivate firefox profile");

    match reuse_result {
        ActivationResult::Reused {
            pid,
            target_monitor,
            ..
        } => {
            assert_ne!(pid, nurse_pid);
            assert_eq!(target_monitor, target_mon);
            assert_eq!(
                launcher.launched_processes().len(),
                2,
                "No new process spawned on reactivation"
            );
        }
        other => panic!("expected Reused, got: {:?}", other),
    }

    // 5. Nurse's browser is still running and completely unaffected
    assert!(
        nurse_process.is_alive(),
        "Nurse's working Firefox browser must remain running"
    );
}

#[test]
fn test_activate_profile_fails_when_firefox_custom_parameters_conflict() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let firefox_exe = create_mock_pe_executable(dir.path(), "firefox.exe", 0x8664);

    // Profile with conflicting -profile
    let p_conflict1 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Conflict Firefox 1",
                firefox_exe.clone(),
                r#"-profile "C:\CustomProfile""#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let err1 = coordinator.can_activate(&p_conflict1).unwrap_err();
    match err1 {
        ApplicationError::Browser(
            window_hop::browsers::BrowserError::ConflictingIsolationParameter { arg, reason },
        ) => {
            assert!(arg.contains("-profile"));
            assert!(reason.contains("direktori profil terisolasi"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Profile with conflicting -P
    let p_conflict2 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Conflict Firefox 2",
                firefox_exe.clone(),
                r#"-P "InternalProfile""#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let err2 = coordinator.activate_profile(&p_conflict2).unwrap_err();
    match err2 {
        ApplicationError::Browser(
            window_hop::browsers::BrowserError::ConflictingIsolationParameter { arg, reason },
        ) => {
            assert!(arg.contains("-P"));
            assert!(reason.contains("-P"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Profile with conflicting -no-remote
    let p_conflict3 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Conflict Firefox 3",
                firefox_exe.clone(),
                "-no-remote https://antrian.local",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let err3 = coordinator.activate_profile(&p_conflict3).unwrap_err();
    match err3 {
        ApplicationError::Browser(
            window_hop::browsers::BrowserError::ConflictingIsolationParameter { arg, reason },
        ) => {
            assert!(arg.contains("-no-remote"));
            assert!(reason.contains("-no-remote"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Profile with conflicting Chromium argument --user-data-dir
    let p_conflict4 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Conflict Firefox 4",
                firefox_exe,
                r#"--user-data-dir="C:\ChromeData""#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let err4 = coordinator.activate_profile(&p_conflict4).unwrap_err();
    match err4 {
        ApplicationError::Browser(
            window_hop::browsers::BrowserError::ConflictingIsolationParameter { arg, reason },
        ) => {
            assert!(arg.contains("--user-data-dir"));
            assert!(reason.contains("Chromium"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    assert_eq!(launcher.launched_processes().len(), 0);
}

#[test]
fn test_firefox_lifecycle_graceful_close_and_3s_deadline_force_stop_preserves_nurse_browser() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        target_mon.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock.clone(),
        base_dir,
    );

    // Nurse Firefox process
    let nurse_plan = window_hop::browsers::LaunchPlan {
        executable: PathBuf::from(r"C:\Program Files\Mozilla Firefox\firefox.exe"),
        arguments: vec!["https://hospital.simrs".to_string()],
        user_data_dir: PathBuf::from(
            r"C:\Users\Nurse\AppData\Roaming\Mozilla\Firefox\Profiles\nurse",
        ),
        login_retention: window_hop::browsers::LoginRetentionNote::SessionIsolated {
            explanation: "Nurse".to_string(),
        },
    };
    let nurse_proc = launcher
        .launch(
            &nurse_plan,
            &MonitorDescriptor {
                identity: MonitorIdentity::new("MON-PRIMARY"),
                name: "Workstation Main Display".to_string(),
                is_primary: true,
            },
        )
        .expect("nurse process");

    // Queue Firefox process with unresponsive behavior to test 3s deadline force stop
    let firefox_exe = create_mock_pe_executable(dir.path(), "firefox.exe", 0x8664);
    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Firefox Queue", firefox_exe, "https://antrian.poli/queue")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let act_res = coordinator.activate_profile(&profile_id).expect("activate");
    let queue_pid = match act_res {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched"),
    };

    // Configure queue process to refuse graceful close (simulate confirmation dialog hang)
    let queue_proc = launcher.launched_processes()[1].clone();
    queue_proc.set_refuse_graceful(true);

    let t0 = clock.now();
    let outcome = coordinator.stop_profile(&profile_id).expect("stop profile");
    let t1 = clock.now();

    // 1. Outcome must be Forced after 3 seconds
    assert_eq!(outcome, StopOutcome::Forced { pid: queue_pid });
    assert!(
        t1.duration_since(t0) >= std::time::Duration::from_secs(3),
        "Must wait for 3-second deadline before force stopping"
    );

    // 2. Queue process received close request first, then force stop
    assert!(
        queue_proc.close_requested(),
        "Must have requested graceful close first"
    );
    assert!(
        queue_proc.force_stopped(),
        "Must have force stopped unresponsive process"
    );
    assert!(!queue_proc.is_alive());

    // Nurse Firefox process must remain completely untouched!
    assert!(
        nurse_proc.is_alive(),
        "Nurse's Firefox browser must NEVER be stopped or force-terminated"
    );
}

#[test]
fn test_firefox_profile_switching_on_same_monitor_stops_previous_before_launching_new() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        target_mon,
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let firefox_exe = create_mock_pe_executable(dir.path(), "firefox.exe", 0x8664);

    let p1 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Firefox Poli 1", firefox_exe.clone(), "https://antrian/1")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save p1");

    let p2 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Firefox Poli 2", firefox_exe, "https://antrian/2")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save p2");

    let act1 = coordinator.activate_profile(&p1).expect("activate p1");
    let pid1 = match act1 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched p1"),
    };

    // Activate p2 on same monitor: must stop p1 before launching p2
    let act2 = coordinator.activate_profile(&p2).expect("activate p2");
    let pid2 = match act2 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched p2"),
    };

    assert_ne!(pid1, pid2);
    let launched = launcher.launched_processes();
    let proc1 = launched.iter().find(|p| p.pid() == pid1).unwrap();
    let proc2 = launched.iter().find(|p| p.pid() == pid2).unwrap();

    assert!(!proc1.is_alive(), "p1 Firefox instance must be stopped");
    assert!(proc2.is_alive(), "p2 Firefox instance must be running");
}

#[test]
fn test_shortcut_and_startup_activation_with_firefox() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        target_mon,
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );
    coordinator.set_startup_manager(Arc::new(MockStartupManager::new(false)));

    let firefox_exe = create_mock_pe_executable(dir.path(), "firefox.exe", 0x8664);

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Shortcut Firefox",
                firefox_exe,
                r#"--kiosk "https://antrian/shortcut""#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    // 1. Shortcut activation launches Firefox
    let shortcut_status = coordinator
        .handle_shortcut_activation(&profile_id)
        .expect("shortcut activation");
    assert!(matches!(shortcut_status, ActivationResult::Launched { .. }));
    assert_eq!(launcher.launched_processes().len(), 1);

    // 2. Second shortcut activation reuses window profil
    let shortcut_status2 = coordinator
        .handle_shortcut_activation(&profile_id)
        .expect("second shortcut activation");
    assert!(matches!(shortcut_status2, ActivationResult::Reused { .. }));
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "No duplicate process spawned"
    );

    // 3. Configure as startup profile
    coordinator
        .set_startup_enabled(true)
        .expect("enable startup");
    coordinator
        .set_startup_profile_id(Some(profile_id.clone()))
        .expect("set startup profile");

    let startup_status = coordinator
        .handle_startup_activation()
        .expect("startup activation");
    assert!(matches!(
        startup_status,
        Some(ActivationResult::Reused { .. })
    ));
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "Already running instance is reused during startup activation"
    );
}

#[test]
fn test_activate_profile_launches_isolated_brave_on_target_monitor_preserving_nurse_browser() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE-BRAVE"),
        name: "Queue Display Brave".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        target_mon.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir.clone(),
    );

    // 1. Simulate nurse's already running working Brave instance on primary monitor
    let nurse_working_browser_plan = window_hop::browsers::LaunchPlan {
        executable: PathBuf::from(
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
        ),
        arguments: vec![
            "--profile-directory=Default".to_string(),
            "https://hospital.simrs/nurse-portal".to_string(),
        ],
        user_data_dir: PathBuf::from(
            r"C:\Users\Nurse\AppData\Local\BraveSoftware\Brave-Browser\User Data",
        ),
        login_retention: window_hop::browsers::LoginRetentionNote::SessionIsolated {
            explanation: "Nurse default working session".to_string(),
        },
    };
    let nurse_process = launcher
        .launch(
            &nurse_working_browser_plan,
            &MonitorDescriptor {
                identity: MonitorIdentity::new("MON-PRIMARY"),
                name: "Workstation Main Display".to_string(),
                is_primary: true,
            },
        )
        .expect("nurse brave browser running");
    let nurse_pid = nurse_process.pid();

    // 2. Configure Window Hop queue profile with Brave
    let brave_exe = create_mock_pe_executable(dir.path(), "brave.exe", 0x8664);
    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poliklinik Anak Brave",
                brave_exe.clone(),
                r#"--incognito --kiosk "https://antrian.poli/anak?counter=3""#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-BRAVE"))),
        )
        .expect("save brave profile");

    assert!(coordinator.can_activate(&profile_id).unwrap());

    // 3. Activate Brave profile
    let activation_result = coordinator
        .activate_profile(&profile_id)
        .expect("activate brave profile");

    match activation_result {
        ActivationResult::Launched {
            pid,
            target_monitor,
            login_retention,
        } => {
            assert_ne!(
                pid, nurse_pid,
                "Brave queue profile must have its own isolated PID and not merge with nurse browser"
            );
            assert_eq!(target_monitor, target_mon);

            // Brave incognito mode login retention note check
            match login_retention {
                window_hop::browsers::LoginRetentionNote::KioskOrInPrivateNoRetention {
                    explanation,
                } => {
                    assert!(explanation.contains("Brave Browser"));
                    assert!(explanation.contains("incognito") || explanation.contains("Tor"));
                }
                other => panic!("expected KioskOrInPrivateNoRetention, got: {:?}", other),
            }

            // Verify queue process launch plan
            let launched = launcher.launched_processes();
            assert_eq!(launched.len(), 2, "Nurse process + queue process");
            let queue_proc = &launched[1];
            assert_eq!(queue_proc.pid(), pid);
            assert_eq!(queue_proc.current_monitor(), target_mon);

            let plan = queue_proc.plan();
            assert_eq!(plan.executable, brave_exe);
            // Session isolation parameter must be present and point inside profile data directory
            assert!(
                plan.arguments
                    .iter()
                    .any(|arg| arg.starts_with("--user-data-dir=")),
                "Brave must be launched with --user-data-dir session isolation"
            );
            assert!(
                plan.arguments.iter().any(|arg| arg == "--incognito"),
                "Custom --incognito parameter preserved"
            );
            assert!(
                plan.arguments.iter().any(|arg| arg == "--kiosk"),
                "Custom --kiosk parameter preserved"
            );
            assert!(
                plan.arguments
                    .iter()
                    .any(|arg| arg == "https://antrian.poli/anak?counter=3"),
                "Target URL preserved"
            );
        }
        other => panic!("expected Launched result, got: {:?}", other),
    }

    // 4. Repeated activation reuses window profil and restores placement without duplicate process
    let reuse_result = coordinator
        .activate_profile(&profile_id)
        .expect("reactivate brave profile");

    match reuse_result {
        ActivationResult::Reused {
            pid,
            target_monitor,
            ..
        } => {
            assert_ne!(pid, nurse_pid);
            assert_eq!(target_monitor, target_mon);
            assert_eq!(
                launcher.launched_processes().len(),
                2,
                "No new process spawned on reactivation"
            );
        }
        other => panic!("expected Reused, got: {:?}", other),
    }

    // 5. Nurse's browser is still running and completely unaffected
    assert!(
        nurse_process.is_alive(),
        "Nurse's working Brave browser must remain running"
    );
}

#[test]
fn test_activate_profile_fails_when_brave_custom_parameters_conflict() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let brave_exe = create_mock_pe_executable(dir.path(), "brave.exe", 0x8664);

    // Profile with conflicting --user-data-dir
    let p_conflict1 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Conflict Brave 1",
                brave_exe.clone(),
                r#"--user-data-dir="C:\CustomBraveData""#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let err1 = coordinator.can_activate(&p_conflict1).unwrap_err();
    match err1 {
        ApplicationError::Browser(
            window_hop::browsers::BrowserError::ConflictingIsolationParameter { arg, reason },
        ) => {
            assert!(arg.contains("--user-data-dir"));
            assert!(
                reason.contains("Window Hop secara otomatis mengelola direktori data terisolasi")
            );
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Profile with conflicting --profile-directory
    let p_conflict2 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Conflict Brave 2",
                brave_exe.clone(),
                r#"--profile-directory="Profile 1""#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let err2 = coordinator.activate_profile(&p_conflict2).unwrap_err();
    match err2 {
        ApplicationError::Browser(
            window_hop::browsers::BrowserError::ConflictingIsolationParameter { arg, reason },
        ) => {
            assert!(arg.contains("--profile-directory"));
            assert!(reason.contains("parameter profil internal dilarang"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Profile with conflicting case-insensitive /USER-DATA-DIR
    let p_conflict3 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Conflict Brave 3",
                brave_exe.clone(),
                r#"/USER-DATA-DIR=D:\brave"#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let err3 = coordinator.activate_profile(&p_conflict3).unwrap_err();
    match err3 {
        ApplicationError::Browser(
            window_hop::browsers::BrowserError::ConflictingIsolationParameter { arg, .. },
        ) => {
            assert!(arg.contains("/USER-DATA-DIR"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Profile with conflicting -profile-directory:Default
    let p_conflict4 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Conflict Brave 4",
                brave_exe,
                r#"-profile-directory:Default"#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let err4 = coordinator.activate_profile(&p_conflict4).unwrap_err();
    match err4 {
        ApplicationError::Browser(
            window_hop::browsers::BrowserError::ConflictingIsolationParameter { arg, .. },
        ) => {
            assert!(arg.contains("-profile-directory"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    assert_eq!(launcher.launched_processes().len(), 0);
}

#[test]
fn test_activate_profile_fails_when_brave_has_corrupt_binary_architecture() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    }]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    // Create a corrupt non-PE brave.exe
    let corrupt_brave = dir.path().join("brave.exe");
    std::fs::write(&corrupt_brave, b"not a pe binary").expect("write corrupt brave");

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Corrupt Brave Profile",
                corrupt_brave.clone(),
                "https://antrian.poli/queue",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let err = coordinator.can_activate(&profile_id).unwrap_err();
    match err {
        ApplicationError::BrowserNotVerified(path, reason) => {
            assert!(path.contains("brave.exe"));
            assert!(reason.contains("belum diverifikasi"));
        }
        other => panic!("expected BrowserNotVerified, got: {:?}", other),
    }

    let err2 = coordinator.activate_profile(&profile_id).unwrap_err();
    assert!(matches!(err2, ApplicationError::BrowserNotVerified(..)));
    assert_eq!(launcher.launched_processes().len(), 0);
}

#[test]
fn test_brave_lifecycle_graceful_close_and_3s_deadline_force_stop_preserves_nurse_browser() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        target_mon.clone(),
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock.clone(),
        base_dir,
    );

    // Nurse Brave process
    let nurse_plan = window_hop::browsers::LaunchPlan {
        executable: PathBuf::from(
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
        ),
        arguments: vec!["https://hospital.simrs".to_string()],
        user_data_dir: PathBuf::from(
            r"C:\Users\Nurse\AppData\Local\BraveSoftware\Brave-Browser\User Data",
        ),
        login_retention: window_hop::browsers::LoginRetentionNote::SessionIsolated {
            explanation: "Nurse".to_string(),
        },
    };
    let nurse_proc = launcher
        .launch(
            &nurse_plan,
            &MonitorDescriptor {
                identity: MonitorIdentity::new("MON-PRIMARY"),
                name: "Workstation Main Display".to_string(),
                is_primary: true,
            },
        )
        .expect("nurse process");

    // Queue Brave process with unresponsive behavior to test 3s deadline force stop
    let brave_exe = create_mock_pe_executable(dir.path(), "brave.exe", 0x8664);
    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Brave Queue", brave_exe, "https://antrian.poli/queue")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    let act_res = coordinator.activate_profile(&profile_id).expect("activate");
    let queue_pid = match act_res {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched"),
    };

    // Configure queue process to refuse graceful close (simulate confirmation dialog hang)
    let queue_proc = launcher.launched_processes()[1].clone();
    queue_proc.set_refuse_graceful(true);

    let t0 = clock.now();
    let outcome = coordinator.stop_profile(&profile_id).expect("stop profile");
    let t1 = clock.now();

    // 1. Outcome must be Forced after 3 seconds
    assert_eq!(outcome, StopOutcome::Forced { pid: queue_pid });
    assert!(
        t1.duration_since(t0) >= std::time::Duration::from_secs(3),
        "Must wait for 3-second deadline before force stopping"
    );

    // 2. Queue process received close request first, then force stop
    assert!(
        queue_proc.close_requested(),
        "Must have requested graceful close first"
    );
    assert!(
        queue_proc.force_stopped(),
        "Must have force stopped unresponsive process"
    );
    assert!(!queue_proc.is_alive());

    // Nurse Brave process must remain completely untouched!
    assert!(
        nurse_proc.is_alive(),
        "Nurse's working Brave browser must NOT be killed"
    );
}

#[test]
fn test_brave_profile_switching_on_same_monitor_stops_previous_before_launching_new() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        target_mon,
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );

    let brave_exe = create_mock_pe_executable(dir.path(), "brave.exe", 0x8664);

    let p1 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Brave Poli 1", brave_exe.clone(), "https://antrian/1")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save p1");

    let p2 = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Brave Poli 2", brave_exe, "https://antrian/2")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save p2");

    // Activate P1
    let act1 = coordinator.activate_profile(&p1).expect("activate p1");
    let p1_pid = match act1 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for p1"),
    };
    let p1_proc = launcher.launched_processes()[0].clone();
    assert!(p1_proc.is_alive());

    // Activate P2 on the same monitor
    let act2 = coordinator.activate_profile(&p2).expect("activate p2");
    let p2_pid = match act2 {
        ActivationResult::Launched { pid, .. } => pid,
        _ => panic!("expected Launched for p2"),
    };
    let p2_proc = launcher.launched_processes()[1].clone();

    // Verify P1 stopped before P2 launched
    assert_ne!(p1_pid, p2_pid);
    assert!(!p1_proc.is_alive(), "P1 must be stopped");
    assert!(p1_proc.close_requested());
    assert!(p2_proc.is_alive(), "P2 must be running");
}

#[test]
fn test_shortcut_and_startup_activation_with_brave() {
    let dir = tempdir().expect("tempdir");
    let store = Arc::new(MemoryProfileStore::new());
    let target_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Workstation Main Display".to_string(),
            is_primary: true,
        },
        target_mon,
    ]));
    let launcher = Arc::new(MockProcessLauncher::new());
    let clock = Arc::new(MockClock::new());
    let base_dir = dir.path().join("base_data");

    let mut coordinator = LifecycleCoordinator::with_services(
        store,
        monitor_provider,
        launcher.clone(),
        clock,
        base_dir,
    );
    coordinator.set_startup_manager(Arc::new(MockStartupManager::new(false)));

    let brave_exe = create_mock_pe_executable(dir.path(), "brave.exe", 0x8664);

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Shortcut Brave",
                brave_exe,
                r#"--kiosk "https://antrian/shortcut""#,
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    // 1. Shortcut activation launches Brave
    let shortcut_status = coordinator
        .handle_shortcut_activation(&profile_id)
        .expect("shortcut activation");
    assert!(matches!(shortcut_status, ActivationResult::Launched { .. }));
    assert_eq!(launcher.launched_processes().len(), 1);

    // 2. Second shortcut activation reuses window profil
    let shortcut_status2 = coordinator
        .handle_shortcut_activation(&profile_id)
        .expect("second shortcut activation");
    assert!(matches!(shortcut_status2, ActivationResult::Reused { .. }));
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "No duplicate process spawned"
    );

    // 3. Configure as startup profile
    coordinator
        .set_startup_enabled(true)
        .expect("enable startup");
    coordinator
        .set_startup_profile_id(Some(profile_id.clone()))
        .expect("set startup profile");

    let startup_status = coordinator
        .handle_startup_activation()
        .expect("startup activation");
    assert!(matches!(
        startup_status,
        Some(ActivationResult::Reused { .. })
    ));
    assert_eq!(
        launcher.launched_processes().len(),
        1,
        "Already running instance is reused during startup activation"
    );
}
