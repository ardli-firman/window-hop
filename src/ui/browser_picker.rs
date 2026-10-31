//! Browser detection, selection, and compatibility evaluation UI.

use std::path::PathBuf;

use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::Input;
use gpui_kit::component::tag::{Tag as Badge, TagVariant};
use gpui_kit::component::{
    h_flex, v_flex, ActiveTheme, Disableable as _, Icon, IconName, Sizable as _,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::browsers::{BrowserArchitecture, BrowserCompatibilityStatus, DetectedBrowser};
use crate::ui::WindowHopView;

/// Model visual untuk Quick Tile browser yang terdeteksi pada sistem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserQuickTileModel {
    pub name: String,
    pub executable_path: PathBuf,
    pub version: Option<String>,
    pub architecture: BrowserArchitecture,
    pub is_selected: bool,
    pub is_verified: bool,
}

impl BrowserQuickTileModel {
    pub fn from_detected(browser: &DetectedBrowser, current_selected_path: &str) -> Self {
        let is_selected = !current_selected_path.trim().is_empty()
            && browser
                .executable_path
                .to_string_lossy()
                .eq_ignore_ascii_case(current_selected_path.trim());
        Self {
            name: browser.name.clone(),
            executable_path: browser.executable_path.clone(),
            version: browser.version.clone(),
            architecture: browser.architecture,
            is_selected,
            is_verified: browser.is_verified(),
        }
    }

    pub fn brand_icon(&self) -> &'static str {
        let lower = self.name.to_ascii_lowercase();
        if lower.contains("chrome") {
            "🌐"
        } else if lower.contains("edge") {
            "🧭"
        } else if lower.contains("brave") {
            "🦁"
        } else {
            "🖥"
        }
    }

    pub fn status_badge(&self) -> (&'static str, TagVariant) {
        if self.is_verified {
            ("Terverifikasi", TagVariant::Success)
        } else {
            ("Belum Diverifikasi", TagVariant::Warning)
        }
    }

    pub fn subtitle(&self) -> String {
        let arch_str = format!("{}", self.architecture);
        if let Some(ref ver) = self.version {
            format!("{} • v{}", arch_str, ver)
        } else {
            arch_str
        }
    }
}

/// Badge informatif untuk status kompatibilitas isolasi sesi browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserCompatibilityBadge {
    pub label: String,
    pub variant: TagVariant,
    pub is_safe: bool,
}

impl BrowserCompatibilityBadge {
    pub fn for_status(status: Option<&BrowserCompatibilityStatus>) -> Self {
        match status {
            Some(BrowserCompatibilityStatus::SafeToActivate { .. }) => Self {
                label: "Terverifikasi".to_string(),
                variant: TagVariant::Success,
                is_safe: true,
            },
            Some(BrowserCompatibilityStatus::Unverified { .. }) => Self {
                label: "Belum Diverifikasi".to_string(),
                variant: TagVariant::Warning,
                is_safe: false,
            },
            Some(BrowserCompatibilityStatus::Unavailable { .. }) => Self {
                label: "Tidak Tersedia".to_string(),
                variant: TagVariant::Danger,
                is_safe: false,
            },
            None => Self {
                label: "Belum Dipilih".to_string(),
                variant: TagVariant::Secondary,
                is_safe: false,
            },
        }
    }
}

impl WindowHopView {
    /// Renders the detected browser selection list, manual executable path input, and compatibility status card.
    pub(crate) fn render_browser_picker(&mut self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let is_busy = self.is_busy;

        v_flex()
            .gap_4()
            // Pilihan Browser Terdeteksi pada Sistem
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Browser Terdeteksi pada Sistem:"),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(format!(
                                                "{} terdeteksi",
                                                self.cached_detected_browsers.len()
                                            )),
                                    )
                                    .child(
                                        Button::new("btn-refresh-browsers")
                                            .outline()
                                            .small()
                                            .icon(IconName::RefreshCw)
                                            .label("Segarkan Browser")
                                            .disabled(is_busy)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.refresh_detected_browsers(cx);
                                            })),
                                    ),
                            ),
                    )
                    .when_some(self.detected_browsers_error.as_ref(), |this, err| {
                        this.child(
                            Alert::warning("detected-browsers-error", format!("Peringatan deteksi browser: {}", err))
                                .title("Peringatan Sistem"),
                        )
                    })
                    .child({
                        if self.cached_detected_browsers.is_empty() {
                            div()
                                .p_3()
                                .rounded_md()
                                .bg(theme.tokens.group_box)
                                .border_1()
                                .border_color(theme.border)
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .items_center()
                                        .child(Icon::new(IconName::Info).size_4().text_color(theme.muted_foreground))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .child("Tidak ada instalasi browser yang terdeteksi otomatis pada sistem. Silakan gunakan pemilihan file executable .exe manual di bawah."),
                                        ),
                                )
                        } else {
                            let current_path = self.path_input.read(cx).value().trim().to_string();
                            let detected_list = self.cached_detected_browsers.clone();

                            div().child(
                                h_flex()
                                    .gap_3()
                                    .flex_wrap()
                                    .children(detected_list.into_iter().enumerate().map(|(idx, b)| {
                                        let tile = BrowserQuickTileModel::from_detected(&b, &current_path);
                                        let (status_label, status_variant) = tile.status_badge();
                                        let is_selected = tile.is_selected;

                                        let (bg, border) = if is_selected {
                                            (theme.primary.opacity(0.12), theme.primary)
                                        } else {
                                            (*theme.tokens.group_box, theme.border)
                                        };

                                        let item_id = SharedString::from(format!("detected-browser-item-{}", idx));
                                        let b_clone = b.clone();

                                        div()
                                            .id(item_id)
                                            .p_3()
                                            .min_w(px(210.0))
                                            .flex_1()
                                            .rounded_lg()
                                            .border_1()
                                            .border_color(border)
                                            .bg(bg)
                                            .hover(|s| s.bg(theme.accent.opacity(0.18)))
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.select_detected_browser(&b_clone, window, cx);
                                            }))
                                            .child(
                                                v_flex()
                                                    .gap_1p5()
                                                    .w_full()
                                                    .min_w_0()
                                                    .child(
                                                        h_flex()
                                                            .justify_between()
                                                            .items_center()
                                                            .gap_2()
                                                            .w_full()
                                                            .child(
                                                                h_flex()
                                                                    .flex_1()
                                                                    .min_w_0()
                                                                    .gap_2()
                                                                    .items_center()
                                                                    .child(div().flex_shrink_0().text_base().child(tile.brand_icon()))
                                                                    .child(
                                                                        div()
                                                                            .flex_1()
                                                                            .min_w_0()
                                                                            .truncate()
                                                                            .text_sm()
                                                                            .font_weight(FontWeight::SEMIBOLD)
                                                                            .text_color(theme.foreground)
                                                                            .child(tile.name.clone()),
                                                                    ),
                                                            )
                                                            .child(if is_selected {
                                                                div()
                                                                    .flex_shrink_0()
                                                                    .text_xs()
                                                                    .font_weight(FontWeight::BOLD)
                                                                    .text_color(theme.primary)
                                                                    .child("✓ Terpilih")
                                                            } else {
                                                                div()
                                                            }),
                                                    )
                                                    .child(
                                                        h_flex()
                                                            .justify_between()
                                                            .items_center()
                                                            .flex_wrap()
                                                            .gap_1()
                                                            .w_full()
                                                            .child(
                                                                div()
                                                                    .flex_1()
                                                                    .min_w_0()
                                                                    .truncate()
                                                                    .text_xs()
                                                                    .text_color(theme.muted_foreground)
                                                                    .child(tile.subtitle()),
                                                            )
                                                            .child(
                                                                div()
                                                                    .flex_shrink_0()
                                                                    .child(
                                                                        Badge::new()
                                                                            .with_variant(status_variant)
                                                                            .child(status_label),
                                                                    ),
                                                            ),
                                                    ),
                                            )
                                    })),
                            )
                        }
                    }),
            )
            // Field Executable Browser + Tombol Browse Manual
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .flex_wrap()
                            .gap_1()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Jalur Executable Browser (Manual / Portable):"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Dapat memilih file .exe portabel atau lokasi instalasi khusus"),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .w_full()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(Input::new(&self.path_input)),
                            )
                            .child(
                                Button::new("btn-browse-browser")
                                    .flex_shrink_0()
                                    .outline()
                                    .icon(IconName::FolderOpen)
                                    .label("Pilih File .exe...")
                                    .disabled(is_busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.browse_browser_executable(cx);
                                    })),
                            ),
                    ),
            )
            // Kartu Evaluasi Kompatibilitas Browser
            .child({
                let raw_path = self.path_input.read(cx).value().trim().to_string();
                if self.last_evaluated_browser_path.as_deref() != Some(&raw_path) {
                    self.last_evaluated_browser_path = Some(raw_path.clone());
                    if raw_path.is_empty() {
                        self.cached_browser_compatibility = None;
                    } else if let Ok(coord) = self.coordinator.try_lock() {
                        self.cached_browser_compatibility =
                            Some(coord.check_browser_compatibility(&PathBuf::from(&raw_path)));
                    }
                }

                let compat_badge = BrowserCompatibilityBadge::for_status(self.cached_browser_compatibility.as_ref());

                match self.cached_browser_compatibility.as_ref() {
                    None => {
                        div()
                            .p_2()
                            .rounded_md()
                            .bg(theme.muted)
                            .border_1()
                            .border_color(theme.border)
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("ℹ Masukkan jalur file executable atau pilih browser terdeteksi di atas untuk memeriksa status kompatibilitas.")
                    }
                    Some(BrowserCompatibilityStatus::SafeToActivate { adapter: _, evidence }) => {
                        let limit_text = evidence.limitations.clone().unwrap_or_default();
                        let version_text = evidence
                            .version
                            .as_ref()
                            .map(|v| format!(" v{}", v))
                            .unwrap_or_default();
                        div()
                            .p_3()
                            .rounded_md()
                            .bg(theme.primary.opacity(0.12))
                            .border_1()
                            .border_color(theme.primary)
                            .child(
                                v_flex()
                                    .gap_1()
                                    .w_full()
                                    .min_w_0()
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .items_center()
                                            .flex_wrap()
                                            .gap_2()
                                            .w_full()
                                            .child(
                                                h_flex()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .gap_2()
                                                    .items_center()
                                                    .flex_wrap()
                                                    .child(
                                                        div()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_xs()
                                                            .text_color(theme.primary)
                                                            .child("Status Dukungan: Aman Diaktifkan"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(theme.muted_foreground)
                                                            .child(format!("{}{} [{}]", evidence.browser_name, version_text, evidence.architecture)),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex_shrink_0()
                                                    .id("badge-compat-safe")
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.status_message = Some(
                                                            "Browser Terverifikasi: Mendukung isolasi sesi penuh (--user-data-dir terpisah) dan kepemilikan instance profil tanpa mengganggu sesi perawat.".to_string(),
                                                        );
                                                        this.is_error = false;
                                                        cx.notify();
                                                    }))
                                                    .child(
                                                        Badge::new()
                                                            .with_variant(compat_badge.variant)
                                                            .child(compat_badge.label.clone()),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .w_full()
                                            .min_w_0()
                                            .child(format!("Bukti: {}", evidence.evidence_text)),
                                    )
                                    .when(!limit_text.is_empty(), |this| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .w_full()
                                                .min_w_0()
                                                .text_color(theme.muted_foreground)
                                                .child(format!("Batasan: {}", limit_text)),
                                        )
                                    }),
                            )
                    }
                    Some(BrowserCompatibilityStatus::Unverified { evidence }) => {
                        let version_text = evidence
                            .version
                            .as_ref()
                            .map(|v| format!(" v{}", v))
                            .unwrap_or_default();
                        div()
                            .p_3()
                            .rounded_md()
                            .bg(theme.warning.opacity(0.12))
                            .border_1()
                            .border_color(theme.warning)
                            .child(
                                v_flex()
                                    .gap_1()
                                    .w_full()
                                    .min_w_0()
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .items_center()
                                            .flex_wrap()
                                            .gap_2()
                                            .w_full()
                                            .child(
                                                h_flex()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .gap_2()
                                                    .items_center()
                                                    .flex_wrap()
                                                    .child(
                                                        div()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_xs()
                                                            .text_color(theme.warning)
                                                            .child("Status Dukungan: Ditemukan"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(theme.muted_foreground)
                                                            .child(format!("{}{} [{}]", evidence.browser_name, version_text, evidence.architecture)),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex_shrink_0()
                                                    .id("badge-compat-unverified")
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.status_message = Some(
                                                            "Browser Belum Diverifikasi: Executable ditemukan pada disk namun belum terdaftar sebagai adapter teruji isolasi sesi. Gate aktivasi ditahan demi keamanan.".to_string(),
                                                        );
                                                        this.is_error = false;
                                                        cx.notify();
                                                    }))
                                                    .child(
                                                        Badge::new()
                                                            .with_variant(compat_badge.variant)
                                                            .child(compat_badge.label.clone()),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .w_full()
                                            .min_w_0()
                                            .child(format!("Bukti: {}", evidence.evidence_text)),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .w_full()
                                            .min_w_0()
                                            .text_color(theme.muted_foreground)
                                            .child("ℹ Profil dengan browser ini tetap dapat disimpan untuk persiapan konfigurasi, namun gate aktivasi diblokir demi keamanan data perawat."),
                                    ),
                            )
                    }
                    Some(BrowserCompatibilityStatus::Unavailable { path, reason }) => {
                        div()
                            .p_3()
                            .rounded_md()
                            .bg(theme.danger.opacity(0.12))
                            .border_1()
                            .border_color(theme.danger)
                            .child(
                                v_flex()
                                    .gap_1()
                                    .w_full()
                                    .min_w_0()
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .items_center()
                                            .flex_wrap()
                                            .gap_2()
                                            .w_full()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_xs()
                                                    .text_color(theme.danger)
                                                    .child("Status Dukungan: Executable Tidak Tersedia"),
                                            )
                                            .child(
                                                div()
                                                    .flex_shrink_0()
                                                    .child(
                                                        Badge::new()
                                                            .with_variant(compat_badge.variant)
                                                            .child(compat_badge.label.clone()),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .w_full()
                                            .min_w_0()
                                            .child(format!("Kesalahan: {}", reason)),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .w_full()
                                            .min_w_0()
                                            .text_color(theme.muted_foreground)
                                            .child(format!("Jalur: {}", path.display())),
                                    ),
                            )
                    }
                }
            })
    }

    /// Memperbarui cache daftar browser yang terdeteksi secara manual atau saat ada perubahan.
    pub fn refresh_detected_browsers(&mut self, cx: &mut Context<Self>) {
        let coordinator_arc = self.coordinator.clone();
        if let Ok(coord) = coordinator_arc.try_lock() {
            match coord.detected_browsers() {
                Ok(browsers) => {
                    self.cached_detected_browsers = browsers;
                    self.detected_browsers_error = None;
                }
                Err(err) => {
                    self.detected_browsers_error = Some(err.to_string());
                }
            }
        }
        cx.notify();
    }

    /// Memilih browser yang terdeteksi, mengisi form jalur executable dan nama secara cerdas.
    pub fn select_detected_browser(
        &mut self,
        browser: &DetectedBrowser,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path_str = browser.executable_path.to_string_lossy().to_string();
        let browser_name = browser.name.clone();

        self.path_input.update(cx, |input, cx| {
            input.set_value(path_str, window, cx);
        });
        self.name_input.update(cx, |input, cx| {
            if input.value().trim().is_empty() {
                input.set_value(format!("Antrian {}", browser_name), window, cx);
            }
        });
        cx.notify();
    }

    /// Membuka dialog pemilih file asli OS untuk memilih executable browser secara manual.
    pub fn browse_browser_executable(&mut self, cx: &mut Context<Self>) {
        let window_handle = self.window_handle;
        let path_input = self.path_input.clone();

        let prompt_task = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Pilih File Executable Browser".into()),
        });

        cx.spawn(move |_this, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                if let Ok(Ok(Some(paths))) = prompt_task.await {
                    if let Some(path) = paths.into_iter().next() {
                        let path_str = path.to_string_lossy().to_string();
                        cx.update(|cx| {
                            let _ = window_handle.update(cx, |_view, window, cx| {
                                path_input.update(cx, |input, cx| {
                                    input.set_value(path_str, window, cx);
                                });
                            });
                        });
                    }
                }
            }
        })
        .detach();
    }
}
