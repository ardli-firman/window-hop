//! Integration and contract tests for the modular Window Hop UI architecture.

use gpui_kit::px;
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
