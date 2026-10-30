//! Integration and contract tests for the modular Window Hop UI architecture.

use gpui_kit::px;
use window_hop::ui::tokens::{
    SIDEBAR_WIDTH, WINDOW_INITIAL_HEIGHT, WINDOW_INITIAL_WIDTH, WINDOW_MIN_HEIGHT, WINDOW_MIN_WIDTH,
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
fn test_ui_submodules_are_accessible_and_modular() {
    // Verify that the requested submodules are exposed under window_hop::ui
    let _ = std::any::type_name::<window_hop::ui::sidebar::SidebarMarker>();
    let _ = std::any::type_name::<window_hop::ui::form::FormMarker>();
    let _ = std::any::type_name::<window_hop::ui::monitor_card::MonitorCardMarker>();
    let _ = std::any::type_name::<window_hop::ui::browser_picker::BrowserPickerMarker>();
    let _ = std::any::type_name::<window_hop::ui::action_bar::ActionBarMarker>();
    let _ = std::any::type_name::<window_hop::ui::dialogs::DialogsMarker>();
    let _ = std::any::type_name::<WindowHopView>();
}
