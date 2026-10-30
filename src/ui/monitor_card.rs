//! Target monitor identification, selection, and readiness evaluation UI.

use gpui_kit::component::button::Button;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::domain::TargetMonitorStatus;
use crate::ui::WindowHopView;

/// Marker type identifying the monitor_card submodule.
pub struct MonitorCardMarker;

impl WindowHopView {
    /// Renders the target monitor selection options and readiness status card.
    pub(crate) fn render_monitor_card(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let available_monitors = &self.cached_monitors;
        let is_busy = self.is_busy;

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
                            .child("Monitor Tujuan (Direkam Berdasarkan Identitas):"),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("{} monitor aktif", available_monitors.len())),
                            )
                            .child(
                                Button::new("btn-refresh-monitors")
                                    .label("Segarkan Monitor")
                                    .disabled(is_busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.refresh_monitors(cx);
                                    })),
                            ),
                    ),
            )
            .when_some(self.monitors_error.as_ref(), |this, err| {
                this.child(
                    div()
                        .p_2()
                        .rounded_md()
                        .bg(theme.danger.opacity(0.1))
                        .border_1()
                        .border_color(theme.danger)
                        .text_xs()
                        .text_color(theme.danger)
                        .child(format!("Peringatan sistem display: {}", err)),
                )
            })
            .child(
                v_flex()
                    .gap_2()
                    // Pilihan 1: Tanpa Monitor Tujuan (Draft)
                    .child({
                        let is_draft_selected = self.selected_target_monitor.is_none();
                        let (bg, border) = if is_draft_selected {
                            (theme.accent.opacity(0.3), theme.primary)
                        } else {
                            (theme.muted.opacity(0.3), theme.border)
                        };
                        div()
                            .id("monitor-option-draft")
                            .p_2()
                            .rounded_md()
                            .border_1()
                            .border_color(border)
                            .bg(bg)
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.selected_target_monitor = None;
                                cx.notify();
                            }))
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child("Draft (Belum Memilih Monitor)"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.primary)
                                            .child(if is_draft_selected { "✓ Terpilih" } else { "" }),
                                    ),
                            )
                    })
                    // Pilihan untuk setiap monitor yang terhubung
                    .children(available_monitors.iter().map(|m| {
                        let is_this_selected =
                            self.selected_target_monitor.as_ref() == Some(&m.identity);
                        let (bg, border) = if is_this_selected {
                            (theme.accent.opacity(0.3), theme.primary)
                        } else {
                            (theme.muted.opacity(0.3), theme.border)
                        };
                        let m_identity = m.identity.clone();
                        let item_id = SharedString::from(format!("monitor-item-{}", m.identity));

                        div()
                            .id(item_id)
                            .p_2()
                            .rounded_md()
                            .border_1()
                            .border_color(border)
                            .bg(bg)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_target_monitor = Some(m_identity.clone());
                                cx.notify();
                            }))
                            .child(
                                v_flex()
                                    .gap_0p5()
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .items_center()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(format!(
                                                        "{}{}",
                                                        m.name,
                                                        if m.is_primary {
                                                            " (Primary)"
                                                        } else {
                                                            ""
                                                        }
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme.primary)
                                                    .child(if is_this_selected {
                                                        "✓ Terpilih"
                                                    } else {
                                                        ""
                                                    }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(format!("Identitas: {}", m.identity)),
                                    ),
                            )
                    }))
                    // Indikator jika monitor yang tersimpan sedang terputus atau diganti
                    .when_some(
                        self.selected_target_monitor
                            .as_ref()
                            .filter(|id| !available_monitors.iter().any(|m| &m.identity == *id)),
                        |this, missing_id| {
                            this.child(
                                div()
                                    .p_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.warning)
                                    .bg(theme.warning.opacity(0.1))
                                    .child(
                                        v_flex()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(theme.warning)
                                                    .child(
                                                        "⚠ Monitor Terpilih Sedang Tidak Terhubung / Telah Diganti:",
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme.muted_foreground)
                                                    .child(format!("Identitas: {}", missing_id)),
                                            ),
                                    ),
                            )
                        },
                    ),
            )
            // Keterangan Status Kesiapan
            .child({
                let target_eval = TargetMonitorStatus::evaluate(
                    self.selected_target_monitor.as_ref(),
                    available_monitors,
                );

                let (status_text, status_color, status_bg) = match target_eval {
                    TargetMonitorStatus::Unset => (
                        "ℹ Status Kesiapan: Draft (Belum Siap Diaktifkan). Monitor tujuan belum dipilih; aplikasi tidak meluncurkan browser.".to_string(),
                        theme.muted_foreground,
                        theme.muted,
                    ),
                    TargetMonitorStatus::Available(desc) => (
                        format!(
                            "✓ Status Kesiapan: Siap Diaktifkan. Monitor tujuan terhubung: {} (Identitas: {}).",
                            desc.name, desc.identity
                        ),
                        theme.primary,
                        theme.primary.opacity(0.12),
                    ),
                    TargetMonitorStatus::Unavailable { recorded_identity } => (
                        format!(
                            "⚠ Status Kesiapan: Monitor Tujuan Tidak Tersedia. Monitor '{}' tidak terhubung atau telah diganti. Silakan sambungkan kembali monitor yang cocok atau pilih ulang monitor tujuan dari daftar. Tidak ada fallback otomatis ke primary; profil belum dapat diaktifkan.",
                            recorded_identity
                        ),
                        theme.warning,
                        theme.warning.opacity(0.12),
                    ),
                    TargetMonitorStatus::Ambiguous { recorded_identity, match_count } => (
                        format!(
                            "⚠ Status Kesiapan: Monitor Ambigu. Ditemukan {} monitor aktif dengan identitas '{}'. Silakan pilih ulang monitor tujuan yang benar; tidak ada fallback otomatis ke primary.",
                            match_count, recorded_identity
                        ),
                        theme.danger,
                        theme.danger.opacity(0.12),
                    ),
                };

                div()
                    .p_3()
                    .rounded_md()
                    .bg(status_bg)
                    .border_1()
                    .border_color(status_color)
                    .text_xs()
                    .text_color(status_color)
                    .child(status_text)
            })
    }

    /// Memperbarui cache daftar monitor yang terhubung ke sistem.
    pub fn refresh_monitors(&mut self, cx: &mut Context<Self>) {
        let coordinator_arc = self.coordinator.clone();
        if let Ok(coord) = coordinator_arc.try_lock() {
            match coord.available_monitors() {
                Ok(monitors) => {
                    self.cached_monitors = monitors;
                    self.monitors_error = None;
                }
                Err(err) => {
                    let err_msg = err.to_string();
                    self.monitors_error = Some(err_msg.clone());
                    self.status_message = Some(format!("Gagal mendeteksi monitor: {}", err_msg));
                    self.is_error = true;
                }
            }
        }
        cx.notify();
    }
}
