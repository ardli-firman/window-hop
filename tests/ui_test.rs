//! Integration and contract tests for the modular Window Hop UI architecture.

use std::path::PathBuf;

use gpui_kit::component::tag::TagVariant;
use gpui_kit::px;
use window_hop::domain::{
    ExitReason, MonitorDescriptor, MonitorIdentity, ProcessId, Profile, ProfileId,
    ProfileLifecycleState, ProfileStatus,
};
use window_hop::ui::sidebar::{
    filter_profiles_by_query, profile_has_unsaved_changes, should_show_profile_search,
    ProfileFormDraft, ProfileStatusBadge, SEARCH_MIN_PROFILES, STARTUP_BADGE_LABEL,
    UNSAVED_CHANGES_INDICATOR,
};
use window_hop::ui::tokens::{
    ShadcnTokens, SIDEBAR_WIDTH, WINDOW_INITIAL_HEIGHT, WINDOW_INITIAL_WIDTH, WINDOW_MIN_HEIGHT,
    WINDOW_MIN_WIDTH,
};
use window_hop::ui::WindowHopView;

#[test]
fn test_window_initial_and_minimum_dimensions_match_spec() {
    assert_eq!(WINDOW_INITIAL_WIDTH, px(1040.0));
    assert_eq!(WINDOW_INITIAL_HEIGHT, px(680.0));
    assert_eq!(WINDOW_MIN_WIDTH, px(920.0));
    assert_eq!(WINDOW_MIN_HEIGHT, px(580.0));
}

#[test]
fn test_sidebar_fixed_width_matches_shadcn_master_detail_spec() {
    assert_eq!(SIDEBAR_WIDTH, px(320.0));
}

#[test]
fn test_shadcn_zinc_and_slate_theme_tokens_are_valid() {
    let zinc_light = ShadcnTokens::zinc_light();
    let zinc_dark = ShadcnTokens::zinc_dark();
    assert_ne!(zinc_light.background, zinc_dark.background);
    assert_ne!(zinc_light.border, zinc_dark.border);

    let slate_light = ShadcnTokens::slate_light();
    let slate_dark = ShadcnTokens::slate_dark();
    assert_ne!(slate_light.background, slate_dark.background);
    assert_ne!(slate_light.border, slate_dark.border);

    // Verify WindowHopView type is publicly available
    let _ = std::any::type_name::<WindowHopView>();
}

#[test]
fn test_profile_status_badge_semantic_variants() {
    let dummy_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-1"),
        name: "Dell Monitor".to_string(),
        is_primary: true,
    };

    // 1. Running -> Aktif (Success)
    let badge = ProfileStatusBadge::for_profile(
        &ProfileLifecycleState::Running {
            pid: ProcessId::new(1234),
            target_monitor: dummy_monitor,
        },
        ProfileStatus::Ready,
    );
    assert_eq!(badge.label, "Aktif");
    assert_eq!(badge.variant, TagVariant::Success);

    // 2. WaitingForMonitor -> Menunggu (Warning)
    let badge = ProfileStatusBadge::for_profile(
        &ProfileLifecycleState::WaitingForMonitor {
            target_monitor: MonitorIdentity::new("MON-1"),
        },
        ProfileStatus::Ready,
    );
    assert_eq!(badge.label, "Menunggu");
    assert_eq!(badge.variant, TagVariant::Warning);

    // 3. Disconnected -> Terputus (Warning)
    let badge = ProfileStatusBadge::for_profile(
        &ProfileLifecycleState::Disconnected {
            target_monitor: MonitorIdentity::new("MON-1"),
        },
        ProfileStatus::Ready,
    );
    assert_eq!(badge.label, "Terputus");
    assert_eq!(badge.variant, TagVariant::Warning);

    // 4. Exited Crash -> Crash (Danger)
    let badge = ProfileStatusBadge::for_profile(
        &ProfileLifecycleState::Exited {
            reason: ExitReason::Crash,
        },
        ProfileStatus::Ready,
    );
    assert_eq!(badge.label, "Crash");
    assert_eq!(badge.variant, TagVariant::Danger);

    // 5. Exited ManualClose -> Berhenti (Secondary)
    let badge = ProfileStatusBadge::for_profile(
        &ProfileLifecycleState::Exited {
            reason: ExitReason::ManualClose,
        },
        ProfileStatus::Ready,
    );
    assert_eq!(badge.label, "Berhenti");
    assert_eq!(badge.variant, TagVariant::Secondary);

    // 6. Idle + Ready -> Siap (Primary)
    let badge = ProfileStatusBadge::for_profile(&ProfileLifecycleState::Idle, ProfileStatus::Ready);
    assert_eq!(badge.label, "Siap");
    assert_eq!(badge.variant, TagVariant::Primary);

    // 7. Idle + DraftNotReady -> Draft (Secondary)
    let badge =
        ProfileStatusBadge::for_profile(&ProfileLifecycleState::Idle, ProfileStatus::DraftNotReady);
    assert_eq!(badge.label, "Draft");
    assert_eq!(badge.variant, TagVariant::Secondary);

    // 8. Idle + TargetUnavailable -> Tidak Tersedia (Warning)
    let badge = ProfileStatusBadge::for_profile(
        &ProfileLifecycleState::Idle,
        ProfileStatus::TargetUnavailable,
    );
    assert_eq!(badge.label, "Tidak Tersedia");
    assert_eq!(badge.variant, TagVariant::Warning);

    // 9. Idle + TargetAmbiguous -> Ambigu (Danger)
    let badge = ProfileStatusBadge::for_profile(
        &ProfileLifecycleState::Idle,
        ProfileStatus::TargetAmbiguous,
    );
    assert_eq!(badge.label, "Ambigu");
    assert_eq!(badge.variant, TagVariant::Danger);
}

#[test]
fn test_startup_badge_indicator() {
    assert!(STARTUP_BADGE_LABEL.contains("[Startup]"));
    assert!(STARTUP_BADGE_LABEL.contains("📌"));
}

#[test]
fn test_unsaved_configuration_changes_form_draft() {
    assert_eq!(UNSAVED_CHANGES_INDICATOR, "●");

    let p1 = Profile::new_draft(
        ProfileId::new("p1"),
        "Poli Gigi",
        PathBuf::from(r"C:\Chrome\chrome.exe"),
        "--kiosk",
    );

    // 1. None draft -> false
    assert!(!profile_has_unsaved_changes(&p1, None));

    // 2. Draft matching profile -> false
    let matching_draft = ProfileFormDraft {
        name: Some("Poli Gigi"),
        browser_path: Some(r"C:\Chrome\chrome.exe"),
        custom_parameters: Some("--kiosk"),
        target_monitor: Some(&None),
    };
    assert!(!profile_has_unsaved_changes(&p1, Some(&matching_draft)));

    // 3. Modified name -> true
    let dirty_name = ProfileFormDraft {
        name: Some("Poli Gigi Baru"),
        browser_path: Some(r"C:\Chrome\chrome.exe"),
        custom_parameters: Some("--kiosk"),
        target_monitor: Some(&None),
    };
    assert!(profile_has_unsaved_changes(&p1, Some(&dirty_name)));

    // 4. Modified browser path -> true
    let dirty_path = ProfileFormDraft {
        name: Some("Poli Gigi"),
        browser_path: Some(r"C:\Edge\edge.exe"),
        custom_parameters: Some("--kiosk"),
        target_monitor: Some(&None),
    };
    assert!(profile_has_unsaved_changes(&p1, Some(&dirty_path)));

    // 5. Modified custom parameters -> true
    let dirty_params = ProfileFormDraft {
        name: Some("Poli Gigi"),
        browser_path: Some(r"C:\Chrome\chrome.exe"),
        custom_parameters: Some("--kiosk --incognito"),
        target_monitor: Some(&None),
    };
    assert!(profile_has_unsaved_changes(&p1, Some(&dirty_params)));

    // 6. Modified target monitor -> true
    let mon = Some(MonitorIdentity::new("MON-2"));
    let dirty_mon = ProfileFormDraft {
        name: Some("Poli Gigi"),
        browser_path: Some(r"C:\Chrome\chrome.exe"),
        custom_parameters: Some("--kiosk"),
        target_monitor: Some(&mon),
    };
    assert!(profile_has_unsaved_changes(&p1, Some(&dirty_mon)));

    // 7. Profile with monitor: matching monitor -> false, cleared monitor -> true
    let mut p2 = p1.clone();
    p2.set_target_monitor(Some(MonitorIdentity::new("MON-1")));
    let same_mon = Some(MonitorIdentity::new("MON-1"));
    let matching_mon_draft = ProfileFormDraft {
        name: Some("Poli Gigi"),
        browser_path: Some(r"C:\Chrome\chrome.exe"),
        custom_parameters: Some("--kiosk"),
        target_monitor: Some(&same_mon),
    };
    assert!(!profile_has_unsaved_changes(&p2, Some(&matching_mon_draft)));

    let cleared_mon_draft = ProfileFormDraft {
        name: Some("Poli Gigi"),
        browser_path: Some(r"C:\Chrome\chrome.exe"),
        custom_parameters: Some("--kiosk"),
        target_monitor: Some(&None),
    };
    assert!(profile_has_unsaved_changes(&p2, Some(&cleared_mon_draft)));
}

#[test]
fn test_search_box_threshold_and_instant_filtering() {
    assert_eq!(SEARCH_MIN_PROFILES, 4);
    assert!(!should_show_profile_search(0));
    assert!(!should_show_profile_search(1));
    assert!(!should_show_profile_search(3));
    assert!(should_show_profile_search(4));
    assert!(should_show_profile_search(10));

    let profiles = vec![
        Profile::new_draft(
            ProfileId::new("p1"),
            "Poli Umum",
            PathBuf::from("chrome.exe"),
            "",
        ),
        Profile::new_draft(
            ProfileId::new("p2"),
            "Poli Gigi",
            PathBuf::from("chrome.exe"),
            "",
        ),
        Profile::new_draft(
            ProfileId::new("p3"),
            "Poli Anak",
            PathBuf::from("chrome.exe"),
            "",
        ),
        Profile::new_draft(
            ProfileId::new("p4"),
            "Poli Mata",
            PathBuf::from("chrome.exe"),
            "",
        ),
        Profile::new_draft(
            ProfileId::new("p5"),
            "Farmasi Rawat Jalan",
            PathBuf::from("chrome.exe"),
            "",
        ),
    ];

    let res = filter_profiles_by_query(&profiles, "");
    assert_eq!(res.len(), 5);

    let res = filter_profiles_by_query(&profiles, "   ");
    assert_eq!(res.len(), 5);

    let res = filter_profiles_by_query(&profiles, "gigi");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].name(), "Poli Gigi");

    let res = filter_profiles_by_query(&profiles, "POLI");
    assert_eq!(res.len(), 4);

    let res = filter_profiles_by_query(&profiles, "rawat");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].name(), "Farmasi Rawat Jalan");

    let res = filter_profiles_by_query(&profiles, "Laboratorium");
    assert!(res.is_empty());
}
