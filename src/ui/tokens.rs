//! Design and layout tokens for Window Hop UI, aligned with shadcn Zinc/Slate.

use gpui_kit::component::Theme;
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

/// Semantic tokens for shadcn master-detail layout styling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadcnTokens {
    pub background: Hsla,
    pub foreground: Hsla,
    pub card: Hsla,
    pub card_foreground: Hsla,
    pub border: Hsla,
    pub muted: Hsla,
    pub muted_foreground: Hsla,
    pub accent: Hsla,
    pub primary: Hsla,
}

impl ShadcnTokens {
    /// Zinc light palette tokens.
    pub fn zinc_light() -> Self {
        Self {
            background: hsla(0.0, 0.0, 1.0, 1.0),
            foreground: hsla(240.0 / 360.0, 0.10, 0.039, 1.0),
            card: hsla(0.0, 0.0, 1.0, 1.0),
            card_foreground: hsla(240.0 / 360.0, 0.10, 0.039, 1.0),
            border: hsla(240.0 / 360.0, 0.059, 0.90, 1.0),
            muted: hsla(240.0 / 360.0, 0.048, 0.959, 1.0),
            muted_foreground: hsla(240.0 / 360.0, 0.038, 0.461, 1.0),
            accent: hsla(240.0 / 360.0, 0.048, 0.959, 1.0),
            primary: hsla(240.0 / 360.0, 0.059, 0.10, 1.0),
        }
    }

    /// Zinc dark palette tokens.
    pub fn zinc_dark() -> Self {
        Self {
            background: hsla(240.0 / 360.0, 0.10, 0.039, 1.0),
            foreground: hsla(0.0, 0.0, 0.98, 1.0),
            card: hsla(240.0 / 360.0, 0.10, 0.039, 1.0),
            card_foreground: hsla(0.0, 0.0, 0.98, 1.0),
            border: hsla(240.0 / 360.0, 0.037, 0.159, 1.0),
            muted: hsla(240.0 / 360.0, 0.037, 0.159, 1.0),
            muted_foreground: hsla(240.0 / 360.0, 0.05, 0.649, 1.0),
            accent: hsla(240.0 / 360.0, 0.037, 0.159, 1.0),
            primary: hsla(0.0, 0.0, 0.98, 1.0),
        }
    }

    /// Slate light palette tokens.
    pub fn slate_light() -> Self {
        Self {
            background: hsla(0.0, 0.0, 1.0, 1.0),
            foreground: hsla(222.2 / 360.0, 0.84, 0.049, 1.0),
            card: hsla(0.0, 0.0, 1.0, 1.0),
            card_foreground: hsla(222.2 / 360.0, 0.84, 0.049, 1.0),
            border: hsla(214.3 / 360.0, 31.8 / 100.0, 0.914, 1.0),
            muted: hsla(210.0 / 360.0, 0.40, 0.961, 1.0),
            muted_foreground: hsla(215.4 / 360.0, 0.163, 0.469, 1.0),
            accent: hsla(210.0 / 360.0, 0.40, 0.961, 1.0),
            primary: hsla(222.2 / 360.0, 0.474, 0.112, 1.0),
        }
    }

    /// Slate dark palette tokens.
    pub fn slate_dark() -> Self {
        Self {
            background: hsla(222.2 / 360.0, 0.84, 0.049, 1.0),
            foreground: hsla(210.0 / 360.0, 0.40, 0.98, 1.0),
            card: hsla(222.2 / 360.0, 0.84, 0.049, 1.0),
            card_foreground: hsla(210.0 / 360.0, 0.40, 0.98, 1.0),
            border: hsla(217.2 / 360.0, 0.326, 0.175, 1.0),
            muted: hsla(217.2 / 360.0, 0.326, 0.175, 1.0),
            muted_foreground: hsla(215.0 / 360.0, 0.202, 0.651, 1.0),
            accent: hsla(217.2 / 360.0, 0.326, 0.175, 1.0),
            primary: hsla(210.0 / 360.0, 0.40, 0.98, 1.0),
        }
    }

    /// Resolves semantic shadcn tokens based on the active theme mode.
    pub fn for_theme(theme: &Theme) -> Self {
        if theme.mode.is_dark() {
            Self::zinc_dark()
        } else {
            Self::zinc_light()
        }
    }
}
