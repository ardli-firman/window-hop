//! Target monitor identification, selection, and readiness evaluation UI.

use std::collections::HashMap;
use std::time::Duration;

use gpui_kit::component::button::Button;
use gpui_kit::component::tag::{Tag as Badge, TagVariant};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::domain::{
    MonitorDescriptor, MonitorIdentity, Profile, ProfileId, ProfileLifecycleState,
    TargetMonitorStatus,
};
use crate::ui::WindowHopView;

/// Label badge abu-abu untuk menandai monitor utama kerja perawat.
pub const NURSE_PRIMARY_BADGE: &str = "[Layar Utama Perawat]";

/// Label badge hijau untuk merekomendasikan monitor antrian (layar non-primary pilihan).
pub const RECOMMENDED_QUEUE_BADGE: &str = "[Monitor Antrian Rekomendasi]";

/// Label badge status ketika sambungan monitor target terputus.
pub const DISCONNECTED_BADGE: &str = "[Terputus]";

/// Pesan peringatan ketika monitor target terputus tanpa pemindahan paksa ke primary.
pub const DISCONNECTED_WARNING: &str =
    "Monitor tujuan sedang terputus (kabel HDMI/VGA dicabut). Tidak ada pemindahan paksa ke layar primary perawat.";

/// Pesan peringatan guard proteksi saat monitor utama perawat dipilih.
pub const NURSE_PRIMARY_WARNING: &str =
    "Peringatan: Monitor ini adalah Layar Utama Perawat. Memilih monitor ini dapat menyebabkan jendela antrian menutupi layar kerja perawat.";

/// Resolusi display default saat resolusi monitor belum terdeteksi secara eksplisit.
pub const DEFAULT_DISPLAY_RESOLUTION: &str = "1920 × 1080";

/// Interval waktu background polling untuk mendeteksi perubahan sambungan kabel monitor HDMI/VGA secara berkala (2.5 detik).
pub const MONITOR_POLL_INTERVAL: Duration = Duration::from_millis(2500);

/// Model murni representasi visual untuk kartu skematik monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorCardModel {
    pub identity: MonitorIdentity,
    pub name: String,
    pub resolution: String,
    pub is_primary: bool,
    pub is_recommended: bool,
    pub is_selected: bool,
    pub is_connected: bool,
    pub occupying_profile: Option<String>,
}

impl MonitorCardModel {
    /// Membuat instance kartu monitor baru secara eksplisit.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        identity: MonitorIdentity,
        name: impl Into<String>,
        resolution: impl Into<String>,
        is_primary: bool,
        is_recommended: bool,
        is_selected: bool,
        is_connected: bool,
        occupying_profile: Option<String>,
    ) -> Self {
        Self {
            identity,
            name: name.into(),
            resolution: resolution.into(),
            is_primary,
            is_recommended,
            is_selected,
            is_connected,
            occupying_profile,
        }
    }

    /// Membuat kartu monitor dari deskriptor monitor yang terhubung aktif.
    pub fn from_descriptor(
        desc: &MonitorDescriptor,
        is_recommended: bool,
        is_selected: bool,
        occupying_profile: Option<String>,
    ) -> Self {
        Self {
            identity: desc.identity.clone(),
            name: desc.name.clone(),
            resolution: DEFAULT_DISPLAY_RESOLUTION.to_string(),
            is_primary: desc.is_primary,
            is_recommended,
            is_selected,
            is_connected: true,
            occupying_profile,
        }
    }

    /// Membuat kartu representasi untuk monitor tujuan yang sedang terputus (kabel dicabut).
    pub fn new_disconnected(
        identity: MonitorIdentity,
        name: impl Into<String>,
        is_selected: bool,
        occupying_profile: Option<String>,
    ) -> Self {
        Self {
            identity,
            name: name.into(),
            resolution: "Tidak terdeteksi".to_string(),
            is_primary: false,
            is_recommended: false,
            is_selected,
            is_connected: false,
            occupying_profile,
        }
    }

    /// Mengatur resolusi kustom pada kartu monitor.
    pub fn with_resolution(mut self, resolution: impl Into<String>) -> Self {
        self.resolution = resolution.into();
        self
    }

    /// Mengambil badge peran semantik monitor (Layar Utama Perawat / Monitor Antrian Rekomendasi / Terputus).
    pub fn role_badge(&self) -> Option<(&'static str, TagVariant)> {
        if !self.is_connected {
            Some((DISCONNECTED_BADGE, TagVariant::Warning))
        } else if self.is_primary {
            Some((NURSE_PRIMARY_BADGE, TagVariant::Secondary))
        } else if self.is_recommended {
            Some((RECOMMENDED_QUEUE_BADGE, TagVariant::Success))
        } else {
            None
        }
    }

    /// Menghasilkan pesan peringatan proteksi jika monitor utama perawat sedang dipilih.
    pub fn selection_warning(&self) -> Option<&'static str> {
        if self.is_selected && self.is_primary {
            Some(NURSE_PRIMARY_WARNING)
        } else {
            None
        }
    }

    /// Menghasilkan label teks penguasaan jika monitor sedang digunakan oleh profil aktif lain.
    pub fn occupancy_text(&self) -> Option<String> {
        self.occupying_profile
            .as_ref()
            .map(|name| format!("Dikuasai oleh profil: {}", name))
    }

    /// Menentukan apakah border kartu harus bergaris putus-putus (misal saat terputus).
    pub fn is_dashed_border(&self) -> bool {
        !self.is_connected
    }

    /// Menentukan apakah kartu harus ditampilkan secara redup (dimmed / lowered opacity).
    pub fn is_dimmed(&self) -> bool {
        !self.is_connected
    }

    /// Menghasilkan peringatan bahwa monitor sedang terputus tanpa fallback ke primary.
    pub fn disconnected_warning(&self) -> Option<&'static str> {
        if !self.is_connected {
            Some(DISCONNECTED_WARNING)
        } else {
            None
        }
    }
}

/// Menemukan profil aktif lain (jika ada) yang saat ini sedang menguasai monitor tertentu.
/// Mengabaikan profil yang saat ini sedang diedit agar tidak terhitung bentrok dengan dirinya sendiri.
pub fn find_occupying_profile<'a>(
    monitor_id: &MonitorIdentity,
    current_editing_profile_id: Option<&ProfileId>,
    profiles: &'a [Profile],
    lifecycle_states: &HashMap<ProfileId, ProfileLifecycleState>,
) -> Option<&'a str> {
    for profile in profiles {
        if Some(profile.id()) == current_editing_profile_id {
            continue;
        }
        if let Some(ProfileLifecycleState::Running { target_monitor, .. }) =
            lifecycle_states.get(profile.id())
        {
            if &target_monitor.identity == monitor_id {
                return Some(profile.name());
            }
        }
    }
    None
}

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
                    .children({
                        let first_secondary_id =
                            available_monitors.iter().find(|m| !m.is_primary).map(|m| &m.identity);

                        available_monitors.iter().map(move |m| {
                            let is_this_selected =
                                self.selected_target_monitor.as_ref() == Some(&m.identity);
                            let is_recommended = first_secondary_id == Some(&m.identity);
                            let occupying_name = find_occupying_profile(
                                &m.identity,
                                self.selected_profile_id.as_ref(),
                                &self.cached_profiles,
                                &self.cached_lifecycle_states,
                            );
                            let card_model = MonitorCardModel::from_descriptor(
                                m,
                                is_recommended,
                                is_this_selected,
                                occupying_name.map(String::from),
                            );

                            self.render_schematic_card(&card_model, cx)
                        })
                    })
                    // Visual kartu redup bergaris putus-putus jika monitor tujuan sedang terputus
                    .when_some(
                        self.selected_target_monitor
                            .as_ref()
                            .filter(|id| !available_monitors.iter().any(|m| &m.identity == *id)),
                        |this, missing_id| {
                            let occupying_name = find_occupying_profile(
                                missing_id,
                                self.selected_profile_id.as_ref(),
                                &self.cached_profiles,
                                &self.cached_lifecycle_states,
                            );
                            let disconnected_card = MonitorCardModel::new_disconnected(
                                missing_id.clone(),
                                "Monitor Antrian Terputus",
                                true,
                                occupying_name.map(String::from),
                            );

                            this.child(self.render_schematic_card(&disconnected_card, cx))
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

    /// Merender satu kartu skematik visual monitor (baik yang terhubung aktif maupun terputus).
    fn render_schematic_card(
        &self,
        card: &MonitorCardModel,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, border) = if !card.is_connected {
            (theme.warning.opacity(0.08), theme.warning)
        } else if card.is_selected {
            (theme.accent.opacity(0.2), theme.primary)
        } else {
            (theme.muted.opacity(0.15), theme.border)
        };

        let card_identity = card.identity.clone();
        let item_id = SharedString::from(format!("monitor-item-{}", card.identity));

        div()
            .id(item_id)
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(border)
            .bg(bg)
            .when(card.is_dashed_border(), |this| this.border_dashed())
            .when(card.is_dimmed(), |this| this.opacity(0.75))
            .when(card.is_connected, |this| {
                this.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_target_monitor = Some(card_identity.clone());
                        cx.notify();
                    }))
            })
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .when(!card.is_connected, |this| {
                                                this.text_color(theme.warning)
                                            })
                                            .child(format!(
                                                "{} {}",
                                                if card.is_connected {
                                                    "📺"
                                                } else {
                                                    "⚠ 📺"
                                                },
                                                card.name
                                            )),
                                    )
                                    .when_some(
                                        card.role_badge(),
                                        |this, (badge_label, badge_variant)| {
                                            this.child(
                                                Badge::new()
                                                    .with_variant(badge_variant)
                                                    .child(badge_label),
                                            )
                                        },
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if card.is_connected {
                                        theme.primary
                                    } else {
                                        theme.warning
                                    })
                                    .child(if card.is_selected {
                                        if card.is_connected {
                                            "✓ Terpilih"
                                        } else {
                                            "✓ Terpilih (Kabel Terputus)"
                                        }
                                    } else {
                                        ""
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_4()
                            .items_center()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if card.is_connected {
                                        theme.foreground
                                    } else {
                                        theme.muted_foreground
                                    })
                                    .child(format!("Resolusi: {}", card.resolution)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if card.is_connected {
                                        theme.muted_foreground
                                    } else {
                                        theme.warning
                                    })
                                    .child(if card.is_connected {
                                        "Status: Terhubung (Aktif)"
                                    } else {
                                        "Status: Terputus / Kabel Dicabut"
                                    }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("Identitas: {}", card.identity)),
                            ),
                    )
                    .when_some(card.occupancy_text(), |this, occ_text| {
                        this.child(
                            div()
                                .p_1p5()
                                .rounded_md()
                                .bg(theme.warning.opacity(0.12))
                                .border_1()
                                .border_color(theme.warning)
                                .text_xs()
                                .text_color(theme.warning)
                                .child(format!("⚠ {} (Mencegah bentrok tampilan)", occ_text)),
                        )
                    })
                    .when_some(card.selection_warning(), |this, warn| {
                        this.child(
                            div()
                                .p_2()
                                .rounded_md()
                                .bg(theme.warning.opacity(0.15))
                                .border_1()
                                .border_color(theme.warning)
                                .text_xs()
                                .text_color(theme.warning)
                                .font_weight(FontWeight::MEDIUM)
                                .child(format!("🛡 {}", warn)),
                        )
                    })
                    .when_some(card.disconnected_warning(), |this, warn| {
                        this.child(
                            div()
                                .p_2()
                                .rounded_md()
                                .bg(theme.warning.opacity(0.15))
                                .border_1()
                                .border_color(theme.warning)
                                .text_xs()
                                .text_color(theme.warning)
                                .child(format!("⚠ {}", warn)),
                        )
                    }),
            )
    }
}
