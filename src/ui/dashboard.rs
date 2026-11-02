//! Dashboard presenting the complete profile list with direct quick actions.

use std::collections::HashSet;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::tag::{Tag as Badge, TagVariant};
use gpui_kit::component::{
    h_flex, v_flex, ActiveTheme, Disableable as _, Icon, IconName, Sizable as _,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::browsers::LaunchParameters;
use crate::domain::{MonitorDescriptor, Profile, ProfileId, ProfileLifecycleState, ProfileStatus};
use crate::ui::action_bar::{ActionBarModel, ActionKind};
use crate::ui::sidebar::{
    filter_profiles_by_query, should_show_profile_search, ProfileStatusBadge, STARTUP_BADGE_LABEL,
};
use crate::ui::WindowHopView;

/// Number of profile cards per row on the dashboard grid.
pub const DASHBOARD_GRID_COLUMNS: u16 = 2;

/// Model aksi tombol di header dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderButtonModel {
    pub id: &'static str,
    pub label: &'static str,
    pub icon_name: &'static str,
    pub is_disabled: bool,
}

/// Model representasi header dashboard dan tombol aksinya.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardHeaderModel {
    pub settings_button: HeaderButtonModel,
    pub add_profile_button: HeaderButtonModel,
}

impl DashboardHeaderModel {
    pub fn evaluate(is_busy: bool) -> Self {
        Self {
            settings_button: HeaderButtonModel {
                id: "btn-open-settings",
                label: "Pengaturan",
                icon_name: "Settings",
                is_disabled: is_busy,
            },
            add_profile_button: HeaderButtonModel {
                id: "btn-add-profile",
                label: "+ Profil Baru",
                icon_name: "Plus",
                is_disabled: is_busy,
            },
        }
    }
}

/// Ringkasan angka pada strip preferensi dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardStats {
    pub total: usize,
    pub active: usize,
    pub startup_profile_name: Option<String>,
}

impl DashboardStats {
    /// Menghitung ringkasan dashboard dari profil dan status lifecycle yang tersedia.
    pub fn from_profiles(
        profiles: &[Profile],
        running_ids: &HashSet<ProfileId>,
        startup_profile_id: Option<&ProfileId>,
    ) -> Self {
        let startup_profile_name = startup_profile_id.and_then(|startup_id| {
            profiles
                .iter()
                .find(|profile| profile.id() == startup_id)
                .map(|profile| profile.name().to_string())
        });

        Self {
            total: profiles.len(),
            active: profiles
                .iter()
                .filter(|profile| running_ids.contains(profile.id()))
                .count(),
            startup_profile_name,
        }
    }

    /// Label ringkas jumlah profil dan profil yang sedang aktif.
    pub fn summary_label(&self) -> String {
        format!("{} profil · {} aktif", self.total, self.active)
    }
}

/// Data tampilan satu kartu profil pada dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileCardModel {
    pub id: ProfileId,
    pub name: String,
    pub browser_name: String,
    pub url: String,
    pub monitor_label: String,
    pub is_startup: bool,
    pub is_running: bool,
    pub has_running_changes: bool,
    pub status: ProfileStatusBadge,
    pub profile_status: ProfileStatus,
    pub lifecycle: ProfileLifecycleState,
}

impl ProfileCardModel {
    /// Menyusun data kartu dari profil, monitor aktif, dan status lifecycle-nya.
    pub fn from_profile(
        profile: &Profile,
        monitors: &[MonitorDescriptor],
        lifecycle: &ProfileLifecycleState,
        startup_profile_id: Option<&ProfileId>,
    ) -> Self {
        let parsed = LaunchParameters::parse(profile.custom_parameters()).unwrap_or_default();
        let url = if parsed.url.trim().is_empty() {
            "Tanpa URL antrian".to_string()
        } else {
            parsed.url
        };

        let monitor_label = match profile.target_monitor() {
            None => "Belum dipilih (draft)".to_string(),
            Some(identity) => monitors
                .iter()
                .find(|monitor| &monitor.identity == identity)
                .map(|monitor| monitor.name.clone())
                .unwrap_or_else(|| format!("{} (tidak terhubung)", identity)),
        };

        // Handle both Windows and POSIX separators so the card stays correct in tests
        // and when a custom path is pasted on another platform.
        let browser_path = profile.browser_path().to_string_lossy().to_string();
        let browser_name = browser_path
            .rsplit(['\\', '/'])
            .find(|part| !part.is_empty())
            .unwrap_or(&browser_path)
            .to_string();

        let profile_status = profile.evaluate_status(monitors);
        let status = ProfileStatusBadge::for_profile(lifecycle, profile_status.clone());

        Self {
            id: profile.id().clone(),
            name: profile.name().to_string(),
            browser_name,
            url,
            monitor_label,
            is_startup: startup_profile_id == Some(profile.id()),
            is_running: lifecycle.is_running(),
            has_running_changes: false,
            status,
            profile_status,
            lifecycle: lifecycle.clone(),
        }
    }

    /// Apakah kartu perlu mengarahkan pengguna melengkapi monitor sebelum aktivasi.
    pub fn needs_monitor_setup(&self) -> bool {
        matches!(
            &self.profile_status,
            ProfileStatus::DraftNotReady
                | ProfileStatus::TargetUnavailable
                | ProfileStatus::TargetAmbiguous
        ) && !matches!(
            &self.lifecycle,
            ProfileLifecycleState::Running { .. }
                | ProfileLifecycleState::WaitingForMonitor { .. }
                | ProfileLifecycleState::Disconnected { .. }
        )
    }

    /// Label aksi utama yang ramah untuk profil yang belum siap diaktifkan.
    pub fn monitor_setup_label(&self) -> &'static str {
        match self.profile_status {
            ProfileStatus::DraftNotReady => "Lengkapi Profil",
            ProfileStatus::TargetUnavailable | ProfileStatus::TargetAmbiguous => "Pilih Monitor",
            ProfileStatus::Ready => "Aktifkan Profil",
        }
    }

    /// Menandai kartu sebagai memiliki perubahan konfigurasi yang belum diterapkan.
    pub fn with_running_changes(mut self, has_changes: bool) -> Self {
        self.has_running_changes = has_changes;
        self
    }
}

impl WindowHopView {
    /// Merender dashboard utama: header, preferensi startup, pencarian, dan daftar profil.
    pub(crate) fn render_dashboard(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let profiles = self.cached_profiles.clone();
        let stats = DashboardStats::from_profiles(
            &profiles,
            &self.cached_running_ids,
            self.cached_startup_profile_id.as_ref(),
        );
        let header_model = DashboardHeaderModel::evaluate(self.is_busy);
        let search_query = self.search_input.read(cx).value().to_string();
        let show_search = should_show_profile_search(profiles.len());
        let filtered_profiles = filter_profiles_by_query(&profiles, &search_query);

        v_flex()
            .flex_1()
            .min_h_0()
            .gap_4()
            .overflow_hidden()
            // Header dashboard + tombol aksi utama
            .child(
                h_flex()
                    .flex_shrink_0()
                    .justify_between()
                    .items_start()
                    .gap_4()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child("Profil Antrian"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child("Atur jendela antrian poliklinik dan monitor tujuannya dari satu tempat."),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new(header_model.settings_button.id)
                                    .icon(IconName::Settings)
                                    .label(header_model.settings_button.label)
                                    .disabled(header_model.settings_button.is_disabled)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.open_settings_modal(cx);
                                    })),
                            )
                            .child(
                                Button::new(header_model.add_profile_button.id)
                                    .primary()
                                    .icon(IconName::Plus)
                                    .label(header_model.add_profile_button.label)
                                    .disabled(header_model.add_profile_button.is_disabled)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_new_profile_modal(window, cx);
                                    })),
                            ),
                    ),
            )
            // Judul daftar profil + pencarian
            .child(
                h_flex()
                    .flex_shrink_0()
                    .justify_between()
                    .items_center()
                    .flex_wrap()
                    .gap_3()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Daftar Profil"),
                            )
                            .child(
                                Badge::new()
                                    .with_variant(TagVariant::Secondary)
                                    .child(stats.summary_label()),
                            ),
                    )
                    .when(show_search, |this| {
                        this.child(
                            div()
                                .w(px(280.0))
                                .max_w_full()
                                .child(
                                    Input::new(&self.search_input).cleanable(true).prefix(
                                        Icon::new(IconName::Search)
                                            .size_3p5()
                                            .text_color(theme.muted_foreground),
                                    ),
                                ),
                        )
                    }),
            )
            // Area daftar profil (langsung terlihat dan dapat digulir)
            .child(
                v_flex()
                    .id("dashboard-profile-scroll")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .child(if profiles.is_empty() {
                        self.render_empty_dashboard(&theme, cx).into_any_element()
                    } else if filtered_profiles.is_empty() {
                        render_no_search_results(&theme).into_any_element()
                    } else {
                        div()
                            .grid()
                            .grid_cols(DASHBOARD_GRID_COLUMNS)
                            .gap_3()
                            .children(filtered_profiles.into_iter().map(|profile| {
                                let lifecycle = self
                                    .cached_lifecycle_states
                                    .get(profile.id())
                                    .cloned()
                                    .unwrap_or(ProfileLifecycleState::Idle);
                                let mut model = ProfileCardModel::from_profile(
                                    profile,
                                    &self.cached_monitors,
                                    &lifecycle,
                                    self.cached_startup_profile_id.as_ref(),
                                );
                                model.has_running_changes =
                                    self.cached_has_running_changes.contains(profile.id());
                                self.render_profile_card(&model, cx).into_any_element()
                            }))
                            .into_any_element()
                    }),
            )
    }

    /// Merender satu kartu profil dengan status dan aksi cepat kontekstual.
    fn render_profile_card(
        &self,
        model: &ProfileCardModel,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let pid = model.id.clone();
        let edit_pid = pid.clone();
        let action_pid = pid.clone();
        let stop_pid = pid.clone();

        let primary_id = SharedString::from(format!("card-primary-{}", pid));
        // Profil draft/monitor bermasalah diarahkan ke langkah Monitor, bukan ke
        // aktivasi yang pasti gagal. Ini membuat aksi utama selalu masuk akal.
        let primary_button =
            if model.needs_monitor_setup() {
                Button::new(primary_id)
                    .primary()
                    .small()
                    .icon(IconName::SquareTerminal)
                    .label(model.monitor_setup_label())
                    .disabled(self.is_busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_edit_profile_modal_at(
                            action_pid.clone(),
                            crate::ui::ProfileWizardStep::Monitor,
                            window,
                            cx,
                        );
                    }))
            } else {
                let action_model = ActionBarModel::evaluate(
                    Some(&model.lifecycle),
                    model.has_running_changes,
                    true,
                    self.is_busy,
                );
                let primary = action_model.primary_action;
                let mut button = Button::new(primary_id)
                    .primary()
                    .small()
                    .label(primary.label)
                    .disabled(primary.is_disabled || self.is_busy);

                button =
                    match primary.kind {
                        ActionKind::ApplyRestart => button.icon(IconName::RotateCw).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.restart_profile(action_pid.clone(), cx);
                            }),
                        ),
                        ActionKind::OpenWindow => button.icon(IconName::ExternalLink).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.activate_profile(action_pid.clone(), cx);
                            }),
                        ),
                        ActionKind::Reconnect => button.icon(IconName::Network).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.activate_profile(action_pid.clone(), cx);
                            }),
                        ),
                        _ => button.icon(IconName::Play).on_click(cx.listener(
                            move |this, _, _, cx| {
                                this.activate_profile(action_pid.clone(), cx);
                            },
                        )),
                    };
                button
            };

        v_flex()
            .id(SharedString::from(format!("profile-card-{}", pid)))
            .w_full()
            .min_w_0()
            .gap_3()
            .justify_between()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(if model.is_running {
                theme.primary.opacity(0.45)
            } else {
                theme.border
            })
            .bg(theme.tokens.group_box)
            .hover(|style| style.bg(theme.accent.opacity(0.12)))
            .child(
                h_flex()
                    .justify_between()
                    .items_start()
                    .gap_3()
                    .w_full()
                    .min_w_0()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .flex_wrap()
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.foreground)
                                            .truncate()
                                            .child(model.name.clone()),
                                    )
                                    .when(model.is_startup, |this| {
                                        this.child(
                                            div().flex_shrink_0().child(
                                                Badge::new()
                                                    .with_variant(TagVariant::Warning)
                                                    .child(STARTUP_BADGE_LABEL),
                                            ),
                                        )
                                    }),
                            )
                            .child(
                                h_flex()
                                    .gap_1p5()
                                    .items_center()
                                    .w_full()
                                    .min_w_0()
                                    .child(
                                        Icon::new(IconName::Globe)
                                            .size_3()
                                            .text_color(theme.muted_foreground),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .truncate()
                                            .child(model.url.clone()),
                                    ),
                            ),
                    )
                    .child(
                        div().flex_shrink_0().child(
                            Badge::new()
                                .with_variant(model.status.variant)
                                .child(model.status.label),
                        ),
                    ),
            )
            .child(
                v_flex()
                    .gap_1p5()
                    .w_full()
                    .min_w_0()
                    .child(render_card_info_row(
                        &theme,
                        IconName::SquareTerminal,
                        "Monitor",
                        model.monitor_label.clone(),
                    ))
                    .child(render_card_info_row(
                        &theme,
                        IconName::FileText,
                        "Browser",
                        model.browser_name.clone(),
                    )),
            )
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .flex_wrap()
                    .pt_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        Button::new(SharedString::from(format!("card-edit-{}", edit_pid)))
                            .ghost()
                            .small()
                            .icon(IconName::Settings2)
                            .label("Edit")
                            .disabled(self.is_busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_edit_profile_modal(edit_pid.clone(), window, cx);
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .when(model.is_running, |this| {
                                this.child(
                                    Button::new(SharedString::from(format!(
                                        "card-stop-{}",
                                        stop_pid
                                    )))
                                    .outline()
                                    .danger()
                                    .small()
                                    .icon(IconName::Square)
                                    .label("Hentikan")
                                    .disabled(self.is_busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.stop_profile(stop_pid.clone(), cx);
                                    })),
                                )
                            })
                            .child(primary_button),
                    ),
            )
    }

    /// Merender empty state ketika belum ada profil sama sekali.
    fn render_empty_dashboard(
        &self,
        theme: &gpui_kit::component::Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .items_center()
            .justify_center()
            .gap_3()
            .p_8()
            .min_h(px(250.0))
            .rounded_lg()
            .border_1()
            .border_dashed()
            .border_color(theme.border)
            .bg(theme.muted.opacity(0.3))
            .child(
                div()
                    .p_3()
                    .rounded_full()
                    .bg(theme.primary.opacity(0.1))
                    .child(
                        Icon::new(IconName::LayoutDashboard)
                            .size_6()
                            .text_color(theme.primary),
                    ),
            )
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Belum ada profil antrian"),
            )
            .child(
                div()
                    .max_w(px(420.0))
                    .text_sm()
                    .text_center()
                    .text_color(theme.muted_foreground)
                    .child("Buat profil pertama untuk menampilkan halaman antrian di monitor poliklinik."),
            )
            .child(
                Button::new("btn-empty-new-profile")
                    .primary()
                    .icon(IconName::Plus)
                    .label("Buat Profil Pertama")
                    .disabled(self.is_busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_new_profile_modal(window, cx);
                    })),
            )
    }
}

fn render_card_info_row(
    theme: &gpui_kit::component::Theme,
    icon: IconName,
    label: &'static str,
    value: String,
) -> impl IntoElement {
    h_flex()
        .gap_1p5()
        .items_center()
        .w_full()
        .min_w_0()
        .child(Icon::new(icon).size_3().text_color(theme.muted_foreground))
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format!("{}:", label)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .text_color(theme.foreground)
                .truncate()
                .child(value),
        )
}

fn render_no_search_results(theme: &gpui_kit::component::Theme) -> impl IntoElement {
    v_flex()
        .items_center()
        .justify_center()
        .gap_2()
        .p_8()
        .min_h(px(180.0))
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child("Profil tidak ditemukan"),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Coba kata kunci lain atau hapus isi kolom pencarian."),
        )
}
