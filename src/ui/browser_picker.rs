//! Browser detection, selection, and compatibility evaluation UI.

use std::path::PathBuf;

use gpui_kit::component::button::Button;
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::browsers::{BrowserCompatibilityStatus, DetectedBrowser};
use crate::ui::WindowHopView;

/// Marker type identifying the browser_picker submodule.
pub struct BrowserPickerMarker;

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
                            div()
                                .p_2()
                                .rounded_md()
                                .bg(theme.danger.opacity(0.12))
                                .border_1()
                                .border_color(theme.danger)
                                .text_xs()
                                .text_color(theme.danger)
                                .child(format!("Peringatan deteksi browser: {}", err)),
                        )
                    })
                    .child({
                        if self.cached_detected_browsers.is_empty() {
                            div()
                                .p_2()
                                .rounded_md()
                                .bg(theme.muted)
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Tidak ada instalasi browser yang terdeteksi otomatis. Silakan gunakan pemilihan file executable manual di bawah.")
                        } else {
                            let current_path = self.path_input.read(cx).value().trim().to_ascii_lowercase();
                            let detected_list = self.cached_detected_browsers.clone();

                            div().child(
                                h_flex()
                                    .gap_2()
                                    .flex_wrap()
                                    .children(detected_list.into_iter().enumerate().map(|(idx, b)| {
                                        let is_selected = !current_path.is_empty()
                                            && b.executable_path.to_string_lossy().to_ascii_lowercase() == current_path;
                                        let (bg, border) = if is_selected {
                                            (theme.accent.opacity(0.3), theme.primary)
                                        } else {
                                            (theme.muted.opacity(0.3), theme.border)
                                        };
                                        let is_verified = b.is_verified();
                                        let status_badge = if is_verified {
                                            ("Terverifikasi", theme.primary)
                                        } else {
                                            ("Belum Diverifikasi", theme.warning)
                                        };

                                        let item_id = SharedString::from(format!("detected-browser-item-{}", idx));
                                        let b_clone = b.clone();

                                        div()
                                            .id(item_id)
                                            .p_2()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(border)
                                            .bg(bg)
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.select_detected_browser(&b_clone, window, cx);
                                            }))
                                            .child(
                                                v_flex()
                                                    .gap_0p5()
                                                    .child(
                                                        h_flex()
                                                            .gap_2()
                                                            .items_center()
                                                            .justify_between()
                                                            .child(
                                                                div()
                                                                    .text_xs()
                                                                    .font_weight(FontWeight::MEDIUM)
                                                                    .child(b.name.clone()),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_xs()
                                                                    .text_color(theme.primary)
                                                                    .child(if is_selected { "✓ Terpilih" } else { "" }),
                                                            ),
                                                    )
                                                    .child(
                                                        h_flex()
                                                            .gap_2()
                                                            .items_center()
                                                            .child(
                                                                div()
                                                                    .text_xs()
                                                                    .text_color(theme.muted_foreground)
                                                                    .child(format!("{}", b.architecture)),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_xs()
                                                                    .text_color(status_badge.1)
                                                                    .child(format!("[{}]", status_badge.0)),
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
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(div().text_sm().child("Jalur Executable Browser (Manual / Portable):"))
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
                            .child(
                                div()
                                    .flex_1()
                                    .child(Input::new(&self.path_input)),
                            )
                            .child(
                                Button::new("btn-browse-browser")
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
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .items_center()
                                            .child(
                                                div()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_xs()
                                                    .text_color(theme.primary)
                                                    .child("✓ Status Dukungan: Aman Diaktifkan (Terverifikasi)"),
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
                                            .text_xs()
                                            .child(format!("Bukti: {}", evidence.evidence_text)),
                                    )
                                    .when(!limit_text.is_empty(), |this| {
                                        this.child(
                                            div()
                                                .text_xs()
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
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .items_center()
                                            .child(
                                                div()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_xs()
                                                    .text_color(theme.warning)
                                                    .child("⚠ Status Dukungan: Ditemukan (Belum Diverifikasi)"),
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
                                            .text_xs()
                                            .child(format!("Bukti: {}", evidence.evidence_text)),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
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
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_xs()
                                            .text_color(theme.danger)
                                            .child("✕ Status Dukungan: Executable Tidak Tersedia / Tidak Ditemukan"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .child(format!("Kesalahan: {}", reason)),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
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
