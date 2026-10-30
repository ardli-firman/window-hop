//! Design and layout tokens for Window Hop UI, aligned with shadcn Zinc/Slate.

use gpui_kit::gpui::{hsla, px, Hsla, Pixels};

/// Initial window width when Window Hop settings window opens (1040 px).
pub const WINDOW_INITIAL_WIDTH: Pixels = px(1040.0);

/// Initial window height when Window Hop settings window opens (680 px).
pub const WINDOW_INITIAL_HEIGHT: Pixels = px(680.0);

/// Minimum allowable window width (920 px).
pub const WINDOW_MIN_WIDTH: Pixels = px(920.0);

/// Minimum allowable window height (580 px).
pub const WINDOW_MIN_HEIGHT: Pixels = px(580.0);

/// Fixed width for the master sidebar navigation column (320 px).
pub const SIDEBAR_WIDTH: Pixels = px(320.0);

/// Semantic color tokens aligned with shadcn Zinc theme.
pub struct ShadcnZinc;

impl ShadcnZinc {
    /// Zinc 950 (dark background)
    pub const ZINC_950: Hsla = hsla(0.0, 0.0, 0.04, 1.0);
    /// Zinc 900 (dark surface / border)
    pub const ZINC_900: Hsla = hsla(0.0, 0.0, 0.09, 1.0);
    /// Zinc 800 (muted border / card surface)
    pub const ZINC_800: Hsla = hsla(0.0, 0.0, 0.15, 1.0);
    /// Zinc 400 (muted text)
    pub const ZINC_400: Hsla = hsla(0.0, 0.0, 0.64, 1.0);
    /// Zinc 100 (light surface / text)
    pub const ZINC_100: Hsla = hsla(0.0, 0.0, 0.96, 1.0);
    /// Zinc 50 (light background)
    pub const ZINC_50: Hsla = hsla(0.0, 0.0, 0.98, 1.0);
}
