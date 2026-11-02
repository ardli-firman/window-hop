//! GPUI presentation for Window Hop profile management.
//!
//! Structured as a modular domain package under `src/ui/`:
//! - `dashboard`: Profile dashboard with the complete profile list and quick actions
//! - `profile_wizard`: Step-by-step modal for creating/editing a profile
//! - `form`: Wizard step content (application, appearance, monitor, review)
//! - `browser_picker`: Detected browsers and executable resolution
//! - `monitor_card`: Target monitor selection and readiness evaluation
//! - `action_bar`: Dashboard quick-action model plus activation lifecycle commands
//! - `dialogs`: Notification banner and active profile delete confirmation dialog
//! - `tokens`: Design and layout dimensions matching shadcn Zinc/Slate

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gpui_kit::component::input::InputState;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Icon, IconName, TitleBar};
use gpui_kit::*;

use crate::application::LifecycleCoordinator;
use crate::browsers::{BrowserCompatibilityStatus, DetectedBrowser};
use crate::domain::{
    DraftProfileInput, MonitorDescriptor, MonitorIdentity, Profile, ProfileId,
    ProfileLifecycleState,
};

pub(crate) mod action_bar;
pub(crate) mod browser_picker;
pub mod dashboard;
pub(crate) mod dialogs;
pub(crate) mod form;
pub(crate) mod monitor_card;
pub mod profile_wizard;
pub mod sidebar;
pub mod tokens;

pub use action_bar::{ActionBarAction, ActionBarModel, ActionKind, ShortcutOptionsModel};
pub use browser_picker::{BrowserCompatibilityBadge, BrowserQuickTileModel};
pub use dashboard::{
    DashboardHeaderModel, DashboardStats, HeaderActionKind, HeaderButtonModel, ProfileCardModel,
};
pub use dialogs::{DeleteConfirmationModalModel, SettingsModalModel};
pub use form::StartupCardModel;
pub use monitor_card::{
    find_occupying_profile, MonitorCardModel, DISCONNECTED_BADGE, DISCONNECTED_WARNING,
    MONITOR_POLL_INTERVAL, NURSE_PRIMARY_BADGE, NURSE_PRIMARY_WARNING, RECOMMENDED_QUEUE_BADGE,
};
pub use profile_wizard::{
    validate_wizard_step, ProfileWizardStep, WizardValidationError, WIZARD_STEP_COUNT,
};
pub use tokens::*;

/// Tampilan utama pengaturan profil Window Hop.
pub struct WindowHopView {
    pub(crate) coordinator: Arc<Mutex<LifecycleCoordinator>>,
    pub(crate) cached_profiles: Vec<Profile>,
    pub(crate) cached_lifecycle_states: HashMap<ProfileId, ProfileLifecycleState>,
    pub(crate) cached_running_ids: HashSet<ProfileId>,
    pub(crate) cached_has_running_changes: HashSet<ProfileId>,
    pub(crate) cached_startup_enabled: bool,
    pub(crate) cached_startup_profile_id: Option<ProfileId>,
    pub(crate) is_busy: bool,
    pub(crate) selected_profile_id: Option<ProfileId>,
    pub(crate) cached_monitors: Vec<MonitorDescriptor>,
    pub(crate) monitors_error: Option<String>,
    pub(crate) cached_detected_browsers: Vec<DetectedBrowser>,
    pub(crate) detected_browsers_error: Option<String>,
    pub(crate) last_evaluated_browser_path: Option<String>,
    pub(crate) cached_browser_compatibility: Option<BrowserCompatibilityStatus>,
    pub(crate) window_handle: AnyWindowHandle,
    pub(crate) name_input: Entity<InputState>,
    pub(crate) path_input: Entity<InputState>,
    pub(crate) url_input: Entity<InputState>,
    pub(crate) kiosk_mode: bool,
    pub(crate) advanced_params_input: Entity<InputState>,
    pub(crate) is_advanced_params_open: bool,
    pub(crate) advanced_params_error: Option<String>,
    pub(crate) shortcut_folder_input: Entity<InputState>,
    pub(crate) search_input: Entity<InputState>,
    pub(crate) status_message: Option<String>,
    pub(crate) is_error: bool,
    pub(crate) confirming_delete_profile: Option<ProfileId>,
    pub(crate) is_settings_modal_open: bool,
    pub(crate) is_profile_modal_open: bool,
    pub(crate) wizard_step: ProfileWizardStep,
    pub(crate) wizard_error: Option<String>,
    pub(crate) wizard_session: u64,
    pub(crate) selected_target_monitor: Option<MonitorIdentity>,
}

impl WindowHopView {
    /// Membuat instance baru WindowHopView dari koordinator yang dimiliki.
    pub fn new(
        coordinator: LifecycleCoordinator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_shared_coordinator(Arc::new(Mutex::new(coordinator)), window, cx)
    }

    /// Membuat instance baru WindowHopView dengan koordinator yang dibagikan (shared Arc<Mutex>).
    pub fn with_shared_coordinator(
        coordinator: Arc<Mutex<LifecycleCoordinator>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("misal: Antrian Poli Rawat Jalan"));
        let path_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
        });
        let url_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(r"misal: https://antrian.rumahsakit.local/poli")
        });
        let advanced_params_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("misal: --disable-gpu --lang=id"));
        let shortcut_folder_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Default: Desktop (atau masukkan jalur folder alternatif)")
        });
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Cari nama profil..."));

        // Observasi input untuk re-rendering instan saat pengguna mengetik (filter pencarian & preview parameter)
        cx.observe(&search_input, |_, _, cx| cx.notify()).detach();
        cx.observe(&name_input, |_, _, cx| cx.notify()).detach();
        cx.observe(&path_input, |_, _, cx| cx.notify()).detach();
        cx.observe(&url_input, |_, _, cx| cx.notify()).detach();
        cx.observe(&advanced_params_input, |this, _, cx| {
            let raw = this.advanced_params_input.read(cx).value().to_string();
            let adapter = this
                .cached_browser_compatibility
                .as_ref()
                .and_then(|c| c.adapter());
            this.advanced_params_error =
                crate::browsers::LaunchParameters::validate_advanced_isolation(
                    &raw,
                    adapter.as_ref(),
                )
                .err()
                .map(|e| e.to_string());
            cx.notify();
        })
        .detach();

        let (
            initial_error,
            cached_monitors,
            monitors_error,
            cached_profiles,
            cached_startup_enabled,
            cached_startup_profile_id,
            cached_detected_browsers,
            detected_browsers_error,
        ) = {
            let coord = coordinator.lock().unwrap();
            let initial_error = coord.last_error().map(|s| s.to_string());
            let (monitors, m_err) = match coord.available_monitors() {
                Ok(m) => (m, None),
                Err(e) => (Vec::new(), Some(e.to_string())),
            };
            let (browsers, b_err) = match coord.detected_browsers() {
                Ok(b) => (b, None),
                Err(e) => (Vec::new(), Some(e.to_string())),
            };
            let profiles = coord.list_profiles();
            let startup_enabled = coord.startup_enabled();
            let startup_profile_id = coord.startup_profile_id().cloned();
            (
                initial_error,
                monitors,
                m_err,
                profiles,
                startup_enabled,
                startup_profile_id,
                browsers,
                b_err,
            )
        };

        let is_error = initial_error.is_some();

        let (cached_lifecycle_states, cached_running_ids, cached_has_running_changes) = {
            let coord = coordinator.lock().unwrap();
            let states = cached_profiles
                .iter()
                .map(|p| {
                    (
                        p.id().clone(),
                        coord
                            .get_lifecycle_state(p.id())
                            .unwrap_or(ProfileLifecycleState::Idle),
                    )
                })
                .collect();
            let running = cached_profiles
                .iter()
                .filter(|p| coord.is_profile_running(p.id()))
                .map(|p| p.id().clone())
                .collect();
            let changes = cached_profiles
                .iter()
                .filter(|p| coord.has_running_config_changes(p.id()))
                .map(|p| p.id().clone())
                .collect();
            (states, running, changes)
        };

        // Background polling berkala (2.5 detik) untuk mendeteksi perubahan sambungan kabel HDMI/VGA
        let polling_coordinator = coordinator.clone();
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                loop {
                    cx.background_executor()
                        .timer(crate::ui::monitor_card::MONITOR_POLL_INTERVAL)
                        .await;

                    let Some(view_entity) = this.upgrade() else {
                        break;
                    };

                    let coord_arc = polling_coordinator.clone();
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            if let Ok(coord) = coord_arc.try_lock() {
                                Some(coord.available_monitors())
                            } else {
                                None
                            }
                        })
                        .await;

                    if let Some(res) = result {
                        cx.update(|cx| {
                            view_entity.update(cx, |view, cx| match res {
                                Ok(monitors) => {
                                    if view.cached_monitors != monitors {
                                        view.cached_monitors = monitors;
                                        view.monitors_error = None;
                                        cx.notify();
                                    }
                                }
                                Err(err) => {
                                    let err_str = err.to_string();
                                    if view.monitors_error.as_deref() != Some(&err_str) {
                                        view.monitors_error = Some(err_str);
                                        cx.notify();
                                    }
                                }
                            });
                        });
                    }
                }
            }
        })
        .detach();

        Self {
            coordinator,
            cached_profiles,
            cached_lifecycle_states,
            cached_running_ids,
            cached_has_running_changes,
            cached_startup_enabled,
            cached_startup_profile_id,
            is_busy: false,
            selected_profile_id: None,
            cached_monitors,
            monitors_error,
            cached_detected_browsers,
            detected_browsers_error,
            last_evaluated_browser_path: None,
            cached_browser_compatibility: None,
            window_handle: window.window_handle(),
            name_input,
            path_input,
            url_input,
            kiosk_mode: true,
            advanced_params_input,
            is_advanced_params_open: false,
            advanced_params_error: None,
            shortcut_folder_input,
            search_input,
            status_message: initial_error,
            is_error,
            confirming_delete_profile: None,
            is_settings_modal_open: false,
            is_profile_modal_open: false,
            wizard_step: ProfileWizardStep::Application,
            wizard_error: None,
            wizard_session: 0,
            selected_target_monitor: None,
        }
    }

    /// Menyinkronkan cache tampilan lokal dari instance koordinator.
    pub(crate) fn sync_cache_from_coordinator(&mut self, coord: &LifecycleCoordinator) {
        self.cached_profiles = coord.list_profiles();
        self.cached_startup_enabled = coord.startup_enabled();
        self.cached_startup_profile_id = coord.startup_profile_id().cloned();
        self.cached_lifecycle_states = self
            .cached_profiles
            .iter()
            .map(|p| {
                (
                    p.id().clone(),
                    coord
                        .get_lifecycle_state(p.id())
                        .unwrap_or(ProfileLifecycleState::Idle),
                )
            })
            .collect();
        self.cached_running_ids = self
            .cached_profiles
            .iter()
            .filter(|p| coord.is_profile_running(p.id()))
            .map(|p| p.id().clone())
            .collect();
        self.cached_has_running_changes = self
            .cached_profiles
            .iter()
            .filter(|p| coord.has_running_config_changes(p.id()))
            .map(|p| p.id().clone())
            .collect();

        // Jika tidak ada pesan status aktif dari aksi pengguna dan koordinator mencatat kesalahan otomatis,
        // tampilkan pesan kesalahan otomatis pada UI detail tanpa popup atau mencuri fokus
        if self.status_message.is_none() {
            if let Some(err) = coord.last_error() {
                self.status_message = Some(err.to_string());
                self.is_error = true;
            }
        }
    }

    /// Mencari profil yang ter-cache berdasarkan ID.
    pub(crate) fn find_cached_profile(&self, pid: &ProfileId) -> Option<&Profile> {
        self.cached_profiles.iter().find(|p| p.id() == pid)
    }

    /// Memuat profil ke dalam input form (tanpa membuka modal).
    pub fn select_profile(
        &mut self,
        id: Option<ProfileId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_profile_id = id.clone();
        self.status_message = None;
        self.is_error = false;
        self.confirming_delete_profile = None;

        // Segarkan cache monitor dan profil saat memilih profil jika koordinator tidak terkunci
        let coordinator_arc = self.coordinator.clone();
        if let Ok(coord) = coordinator_arc.try_lock() {
            if let Ok(monitors) = coord.available_monitors() {
                self.cached_monitors = monitors;
                self.monitors_error = None;
            }
            self.sync_cache_from_coordinator(&coord);
        }

        if let Some(ref pid) = id {
            let profile_opt = self.cached_profiles.iter().find(|p| p.id() == pid).cloned();
            if let Some(profile) = profile_opt {
                let name = profile.name().to_string();
                let path = profile.browser_path().to_string_lossy().to_string();
                let params = profile.custom_parameters().to_string();
                let parsed = crate::browsers::LaunchParameters::parse(&params).unwrap_or_default();
                self.selected_target_monitor = profile.target_monitor().cloned();
                self.kiosk_mode = parsed.kiosk;
                self.is_advanced_params_open = !parsed.advanced_parameters.is_empty();
                self.advanced_params_error = None;

                self.name_input.update(cx, |input, cx| {
                    input.set_value(name, window, cx);
                });
                self.path_input.update(cx, |input, cx| {
                    input.set_value(path, window, cx);
                });
                self.url_input.update(cx, |input, cx| {
                    input.set_value(parsed.url, window, cx);
                });
                self.advanced_params_input.update(cx, |input, cx| {
                    input.set_value(parsed.advanced_parameters, window, cx);
                });
                self.shortcut_folder_input.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                });
            }
        } else {
            self.reset_form_inputs(window, cx);
        }

        cx.notify();
    }

    /// Mengatur ulang input formulir ke kondisi kosong / profil baru.
    pub(crate) fn reset_form_inputs(&mut self, window: &mut Window, cx: &mut App) {
        self.selected_target_monitor = None;
        self.kiosk_mode = true;
        self.is_advanced_params_open = false;
        self.advanced_params_error = None;
        self.name_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        self.path_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        self.url_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        self.advanced_params_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        self.shortcut_folder_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
    }

    /// Membuka modal dialog Pengaturan Aplikasi.
    pub fn open_settings_modal(&mut self, cx: &mut Context<Self>) {
        let coordinator_arc = self.coordinator.clone();
        if let Ok(coord) = coordinator_arc.try_lock() {
            self.sync_cache_from_coordinator(&coord);
        }
        self.is_settings_modal_open = true;
        cx.notify();
    }

    /// Menutup modal dialog Pengaturan Aplikasi dan mengembalikan fokus ke dashboard.
    pub fn close_settings_modal(&mut self, cx: &mut Context<Self>) {
        self.is_settings_modal_open = false;
        cx.notify();
    }

    /// Menyimpan profil draft atau profil yang diedit secara asinkron tanpa memblokir thread UI.
    /// Operasi disk dijalankan di background executor, dan state memori hanya dikomit
    /// setelah persistensi berhasil.
    pub fn save_profile(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy {
            return;
        }

        let name = self.name_input.read(cx).value().to_string();
        let path_str = self.path_input.read(cx).value().to_string();
        let url = self.url_input.read(cx).value().to_string();
        let kiosk = self.kiosk_mode;
        let advanced = self.advanced_params_input.read(cx).value().to_string();

        // Validasi parameter lanjutan terhadap parameter isolasi sesi terlarang
        let adapter = self
            .cached_browser_compatibility
            .as_ref()
            .and_then(|c| c.adapter());
        let lp = crate::browsers::LaunchParameters::new(&url, kiosk, &advanced);
        if let Err(err) = lp.validate_isolation(adapter.as_ref()) {
            self.advanced_params_error = Some(err.to_string());
            self.is_advanced_params_open = true;
            self.wizard_step = ProfileWizardStep::Appearance;
            self.wizard_error = Some(format!("Parameter baris perintah ditolak: {}", err));
            self.status_message = self.wizard_error.clone();
            self.is_error = true;
            cx.notify();
            return;
        }
        self.advanced_params_error = None;
        let params = lp.compose_for(adapter.as_ref());

        let browser_path = PathBuf::from(path_str.trim());
        let input = DraftProfileInput::new(name, browser_path, params)
            .with_target_monitor(self.selected_target_monitor.clone());

        let prepare_result = self
            .coordinator
            .lock()
            .unwrap()
            .prepare_draft_save(self.selected_profile_id.as_ref(), &input);

        let (candidate_config, _target_id) = match prepare_result {
            Ok(res) => res,
            Err(err) => {
                self.wizard_error = Some(err.to_string());
                self.status_message = Some(err.to_string());
                self.is_error = true;
                cx.notify();
                return;
            }
        };

        let store = self.coordinator.lock().unwrap().store().clone();
        self.is_busy = true;
        self.status_message = Some("Menyimpan profil...".to_string());
        self.is_error = false;
        cx.notify();

        let coordinator = self.coordinator.clone();
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                // Jalankan penulisan disk atomik di background executor
                let to_save = candidate_config.clone();
                let save_result = cx
                    .background_executor()
                    .spawn(async move { store.save(&to_save) })
                    .await;

                cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
                        view.is_busy = false;
                        match save_result {
                            Ok(()) => {
                                let mut coord = coordinator.lock().unwrap();
                                coord.commit_saved_config(candidate_config);
                                view.sync_cache_from_coordinator(&coord);
                                drop(coord);
                                view.selected_profile_id = None;
                                view.is_profile_modal_open = false;
                                view.wizard_step = ProfileWizardStep::Application;
                                view.wizard_error = None;
                                view.status_message = Some("Profil berhasil disimpan.".to_string());
                                view.is_error = false;
                            }
                            Err(err) => {
                                let message = format!("Gagal menyimpan ke penyimpanan: {}", err);
                                view.wizard_error = Some(message.clone());
                                view.status_message = Some(message);
                                view.is_error = true;
                            }
                        }
                        cx.notify();
                    });
                });
            }
        })
        .detach();
    }

    /// Memeriksa apakah profil memiliki perubahan konfigurasi form yang belum disimpan.
    pub(crate) fn has_unsaved_changes(&self, pid: &ProfileId, cx: &App) -> bool {
        let Some(profile) = self.cached_profiles.iter().find(|p| p.id() == pid) else {
            return false;
        };
        if self.selected_profile_id.as_ref() != Some(pid) {
            return false;
        }

        let name = self.name_input.read(cx).value().to_string();
        let path = self.path_input.read(cx).value().to_string();
        let url = self.url_input.read(cx).value().to_string();
        let kiosk = self.kiosk_mode;
        let advanced = self.advanced_params_input.read(cx).value().to_string();
        let params = crate::browsers::LaunchParameters::compose_from(&url, kiosk, &advanced);
        let draft = sidebar::ProfileFormDraft {
            name: Some(&name),
            browser_path: Some(&path),
            custom_parameters: Some(&params),
            target_monitor: Some(&self.selected_target_monitor),
        };

        sidebar::profile_has_unsaved_changes(profile, Some(&draft))
    }
}

impl Render for WindowHopView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let shadcn = ShadcnTokens::for_theme(&theme);
        // Non-blocking try_lock: jangan memblokir render loop GPUI jika koordinator sedang di-lock oleh task background
        let coordinator_arc = self.coordinator.clone();
        if let Ok(coord) = coordinator_arc.try_lock() {
            self.sync_cache_from_coordinator(&coord);
        }

        v_flex()
            .size_full()
            .relative()
            .bg(shadcn.background)
            .text_color(shadcn.foreground)
            .child(
                TitleBar::new().child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Icon::new(IconName::SquareTerminal)
                                .size_4()
                                .text_color(theme.primary),
                        )
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Window Hop - Profil Antrian"),
                        ),
                ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .p_5()
                    .gap_3()
                    .overflow_hidden()
                    // Banner notifikasi / status error
                    .children(self.render_status_banner(cx))
                    // Dashboard: daftar profil langsung terlihat tanpa panel detail
                    .child(self.render_dashboard(cx)),
            )
            // Modal dialog Pengaturan Aplikasi
            .children(self.render_settings_dialog(cx))
            // Modal wizard tambah/edit profil (overlay bertahap)
            .children(self.render_profile_wizard(cx))
            // Modal AlertDialog Konfirmasi Penghapusan (di atas wizard)
            .children(self.render_delete_confirmation_dialog(cx))
    }
}
