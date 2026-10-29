//! Application coordination, lifecycle management, and profile operations.

use crate::browsers::{BrowserError, LoginRetentionNote};
use crate::domain::{
    AppConfig, DraftProfileInput, ExitReason, MonitorDescriptor, MonitorIdentity, ProcessId,
    Profile, ProfileId, ProfileLifecycleState, ProfileStatus, TargetMonitorStatus,
    TrayStatusSummary,
};
use crate::persistence::{PersistenceError, ProfileStore};
use crate::platform::{
    default_clock, default_monitor_provider, default_process_launcher, default_shortcut_creator,
    default_tray_provider, Clock, ManagedProcess, MonitorProvider, PlatformError, ProcessLauncher,
    ShortcutCreator, ShortcutSpec, TrayProvider, ACTIVATE_PROFILE_FLAG,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Hasil aktivasi profil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationResult {
    /// Profil baru diluncurkan sebagai instance proses baru.
    Launched {
        pid: ProcessId,
        target_monitor: MonitorDescriptor,
        login_retention: LoginRetentionNote,
    },
    /// Profil yang sudah berjalan digunakan kembali (reuse window) dan penempatannya dipastikan ulang.
    Reused {
        pid: ProcessId,
        target_monitor: MonitorDescriptor,
        login_retention: LoginRetentionNote,
    },
    /// Profil diminta aktif namun monitor tujuannya belum tersedia;
    /// masuk status menunggu tanpa meluncurkan browser.
    WaitingForMonitor { target_monitor: MonitorIdentity },
}

/// Informasi instance profil yang sedang aktif.
#[derive(Clone)]
struct ActiveProfileInstance {
    process: Arc<dyn ManagedProcess>,
    launch_browser_path: PathBuf,
    launch_parameters: String,
    target_monitor: MonitorDescriptor,
    login_retention: LoginRetentionNote,
    stopping: bool,
}

impl ActiveProfileInstance {
    /// Memeriksa apakah konfigurasi instance yang sedang berjalan identik dengan konfigurasi profil
    /// dan deskriptor monitor tujuan yang diberikan.
    fn matches_config(&self, profile: &Profile, target_desc: &MonitorDescriptor) -> bool {
        self.launch_browser_path == *profile.browser_path()
            && self.launch_parameters == profile.custom_parameters()
            && self.target_monitor == *target_desc
    }

    /// Memeriksa apakah konfigurasi instance yang sedang berjalan identik dengan konfigurasi profil tersimpan.
    fn matches_saved_profile(&self, profile: &Profile) -> bool {
        self.launch_browser_path == *profile.browser_path()
            && self.launch_parameters == profile.custom_parameters()
            && Some(&self.target_monitor.identity) == profile.target_monitor()
    }
}

/// Hasil operasi penghentian profil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopOutcome {
    /// Ditutup secara normal (graceful) sebelum batas tenggat 3 detik.
    Graceful { pid: ProcessId },
    /// Dihentikan secara paksa (force stop) setelah batas tenggat 3 detik terlampaui.
    Forced { pid: ProcessId },
    /// Profil tidak sedang berjalan.
    NotRunning,
}

/// Hasil transisi siklus hidup profil yang teramati akibat perubahan status monitor atau proses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleTransitionOutcome {
    /// Profil ditutup karena monitor tujuannya terputus.
    Disconnected {
        profile_id: ProfileId,
        pid: ProcessId,
        forced: bool,
    },
    /// Profil dipulihkan secara otomatis (diluncurkan) karena monitor tujuannya kembali tersambung.
    Restored {
        profile_id: ProfileId,
        pid: ProcessId,
        target_monitor: MonitorDescriptor,
    },
    /// Profil terdeteksi keluar (crash atau ditutup manual oleh pengguna).
    Exited {
        profile_id: ProfileId,
        pid: ProcessId,
        reason: ExitReason,
    },
}

/// Kesalahan pada lapisan koordinasi aplikasi.
#[derive(Debug, thiserror::Error)]
pub enum ApplicationError {
    #[error("Kesalahan penyimpanan: {0}")]
    Persistence(#[from] PersistenceError),

    #[error("Kesalahan platform OS: {0}")]
    Platform(#[from] PlatformError),

    #[error("Kesalahan browser: {0}")]
    Browser(#[from] BrowserError),

    #[error("Dukungan pemisahan sesi dan kepemilikan instance untuk browser '{0}' belum diverifikasi: {1}")]
    BrowserNotVerified(String, String),

    #[error("Validasi input gagal: {0}")]
    ValidationError(String),

    #[error("Profil {id} masih berupa draft dan belum memiliki monitor tujuan sehingga belum siap diaktifkan")]
    DraftNotReady { id: ProfileId },

    #[error("Monitor tujuan '{recorded_identity}' untuk profil {id} tidak tersedia")]
    TargetMonitorUnavailable {
        id: ProfileId,
        recorded_identity: MonitorIdentity,
    },

    #[error("Monitor tujuan '{recorded_identity}' untuk profil {id} ambigu ({match_count} monitor cocok); perlu pemilihan ulang")]
    TargetMonitorAmbiguous {
        id: ProfileId,
        recorded_identity: MonitorIdentity,
        match_count: usize,
    },

    #[error("Profil dengan ID {id} tidak ditemukan")]
    ProfileNotFound { id: ProfileId },

    #[error("Penyimpanan dicegah karena data konfigurasi sebelumnya rusak atau gagal dimuat: {0}")]
    BlockedByCorruptState(String),

    #[error("Gagal mengganti profil aktif pada monitor '{target_monitor}': profil sebelumnya '{previous_profile_name}' gagal dihentikan: {reason}")]
    SwitchProfileFailed {
        previous_profile_id: ProfileId,
        previous_profile_name: String,
        target_monitor: MonitorIdentity,
        reason: String,
    },
}

/// Mengembalikan lokasi direktori dasar data sesi Window Hop.
fn default_base_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local_app_data).join("WindowHop");
        }
    }
    std::env::temp_dir().join("window_hop")
}

/// Koordinator siklus hidup dan konfigurasi Window Hop.
pub struct LifecycleCoordinator {
    store: Arc<dyn ProfileStore>,
    monitor_provider: Arc<dyn MonitorProvider>,
    process_launcher: Arc<dyn ProcessLauncher>,
    clock: Arc<dyn Clock>,
    shortcut_creator: Arc<dyn ShortcutCreator>,
    tray_provider: Arc<dyn TrayProvider>,
    base_data_dir: PathBuf,
    config: AppConfig,
    active_instances: HashMap<ProfileId, ActiveProfileInstance>,
    lifecycle_states: HashMap<ProfileId, ProfileLifecycleState>,
    last_error: Option<String>,
    load_failed: bool,
}

impl LifecycleCoordinator {
    /// Membuat instance baru LifecycleCoordinator dengan dependensi layanan yang dapat disubstitusi.
    pub fn with_services(
        store: Arc<dyn ProfileStore>,
        monitor_provider: Arc<dyn MonitorProvider>,
        process_launcher: Arc<dyn ProcessLauncher>,
        clock: Arc<dyn Clock>,
        base_data_dir: PathBuf,
    ) -> Self {
        Self {
            store,
            monitor_provider,
            process_launcher,
            clock,
            shortcut_creator: default_shortcut_creator(),
            tray_provider: default_tray_provider(),
            base_data_dir,
            config: AppConfig::default(),
            active_instances: HashMap::new(),
            lifecycle_states: HashMap::new(),
            last_error: None,
            load_failed: false,
        }
    }

    /// Membuat instance baru LifecycleCoordinator dengan ProfileStore dan MonitorProvider yang ditentukan.
    pub fn new(store: Arc<dyn ProfileStore>, monitor_provider: Arc<dyn MonitorProvider>) -> Self {
        Self::with_services(
            store,
            monitor_provider,
            default_process_launcher(),
            default_clock(),
            default_base_data_dir(),
        )
    }

    /// Membuat instance baru LifecycleCoordinator dengan MonitorProvider default sistem.
    pub fn from_store(store: Arc<dyn ProfileStore>) -> Self {
        Self::new(store, default_monitor_provider())
    }

    /// Memvalidasi data input profil draft.
    fn validate_input(input: &DraftProfileInput) -> Result<String, ApplicationError> {
        let trimmed_name = input.name.trim();
        if trimmed_name.is_empty() {
            return Err(ApplicationError::ValidationError(
                "Nama profil tidak boleh kosong".to_string(),
            ));
        }

        if input.browser_path.as_os_str().is_empty() {
            return Err(ApplicationError::ValidationError(
                "Jalur executable browser tidak boleh kosong".to_string(),
            ));
        }

        Ok(trimmed_name.to_string())
    }

    /// Memuat konfigurasi dari store.
    pub fn load(&mut self) -> Result<(), ApplicationError> {
        match self.store.load() {
            Ok(config) => {
                self.config = config;
                self.last_error = None;
                self.load_failed = false;
                Ok(())
            }
            Err(err) => {
                let msg = err.to_string();
                self.last_error = Some(msg);
                self.load_failed = true;
                Err(ApplicationError::Persistence(err))
            }
        }
    }

    /// Menyimpan profil draft secara transaksional: data disimpan ke store terlebih dahulu,
    /// dan in-memory state hanya diperbarui jika penyimpanan berhasil (tidak merusak konfigurasi
    /// yang valid jika terjadi kegagalan I/O).
    pub fn save_draft_profile(
        &mut self,
        id: Option<&ProfileId>,
        input: DraftProfileInput,
    ) -> Result<ProfileId, ApplicationError> {
        if self.load_failed {
            let reason = self
                .last_error
                .clone()
                .unwrap_or_else(|| "Gagal memuat konfigurasi awal".to_string());
            return Err(ApplicationError::BlockedByCorruptState(reason));
        }

        let trimmed_name = Self::validate_input(&input)?;
        let mut candidate_config = self.config.clone();

        let profile_id = match id {
            Some(existing_id) => {
                let profile = candidate_config
                    .profiles_mut()
                    .iter_mut()
                    .find(|p| p.id() == existing_id)
                    .ok_or_else(|| ApplicationError::ProfileNotFound {
                        id: existing_id.clone(),
                    })?;
                profile.set_name(trimmed_name);
                profile.set_browser_path(input.browser_path);
                profile.set_custom_parameters(input.custom_parameters);
                profile.set_target_monitor(input.target_monitor);
                existing_id.clone()
            }
            None => {
                let new_id = ProfileId::generate();
                let mut profile = Profile::new_draft(
                    new_id.clone(),
                    trimmed_name,
                    input.browser_path,
                    input.custom_parameters,
                );
                profile.set_target_monitor(input.target_monitor);
                candidate_config.profiles_mut().push(profile);
                new_id
            }
        };

        // Simpan ke underlying store terlebih dahulu
        match self.store.save(&candidate_config) {
            Ok(()) => {
                // Komit ke in-memory state hanya setelah I/O berhasil
                self.config = candidate_config;
                self.last_error = None;
                Ok(profile_id)
            }
            Err(err) => {
                let msg = err.to_string();
                self.last_error = Some(msg);
                Err(ApplicationError::Persistence(err))
            }
        }
    }

    /// Menyiapkan konfigurasi kandidat untuk penyimpanan asinkron tanpa memblokir thread UI.
    pub fn prepare_draft_save(
        &self,
        id: Option<&ProfileId>,
        input: &DraftProfileInput,
    ) -> Result<(AppConfig, ProfileId), ApplicationError> {
        if self.load_failed {
            let reason = self
                .last_error
                .clone()
                .unwrap_or_else(|| "Gagal memuat konfigurasi awal".to_string());
            return Err(ApplicationError::BlockedByCorruptState(reason));
        }

        let trimmed_name = Self::validate_input(input)?;
        let mut candidate_config = self.config.clone();

        let profile_id = match id {
            Some(existing_id) => {
                let profile = candidate_config
                    .profiles_mut()
                    .iter_mut()
                    .find(|p| p.id() == existing_id)
                    .ok_or_else(|| ApplicationError::ProfileNotFound {
                        id: existing_id.clone(),
                    })?;
                profile.set_name(trimmed_name);
                profile.set_browser_path(input.browser_path.clone());
                profile.set_custom_parameters(input.custom_parameters.clone());
                profile.set_target_monitor(input.target_monitor.clone());
                existing_id.clone()
            }
            None => {
                let new_id = ProfileId::generate();
                let mut profile = Profile::new_draft(
                    new_id.clone(),
                    trimmed_name,
                    input.browser_path.clone(),
                    input.custom_parameters.clone(),
                );
                profile.set_target_monitor(input.target_monitor.clone());
                candidate_config.profiles_mut().push(profile);
                new_id
            }
        };

        Ok((candidate_config, profile_id))
    }

    /// Melakukan komit konfigurasi yang telah berhasil disimpan ke disk ke state memori.
    pub fn commit_saved_config(&mut self, config: AppConfig) {
        self.config = config;
        self.last_error = None;
    }

    /// Membuat profil draft baru secara langsung dalam memori (untuk backward-compatibility atau staging).
    pub fn create_draft_profile(
        &mut self,
        input: DraftProfileInput,
    ) -> Result<ProfileId, ApplicationError> {
        let trimmed_name = Self::validate_input(&input)?;
        let id = ProfileId::generate();
        let mut profile = Profile::new_draft(
            id.clone(),
            trimmed_name,
            input.browser_path,
            input.custom_parameters,
        );
        profile.set_target_monitor(input.target_monitor);
        self.config.profiles_mut().push(profile);
        Ok(id)
    }

    /// Menyimpan konfigurasi profil saat ini ke store.
    pub fn save(&mut self) -> Result<(), ApplicationError> {
        if self.load_failed {
            let reason = self
                .last_error
                .clone()
                .unwrap_or_else(|| "Gagal memuat konfigurasi awal".to_string());
            return Err(ApplicationError::BlockedByCorruptState(reason));
        }

        match self.store.save(&self.config) {
            Ok(()) => {
                self.last_error = None;
                Ok(())
            }
            Err(err) => {
                let msg = err.to_string();
                self.last_error = Some(msg);
                Err(ApplicationError::Persistence(err))
            }
        }
    }

    /// Mengambil daftar semua profil.
    pub fn list_profiles(&self) -> Vec<Profile> {
        self.config.profiles().to_vec()
    }

    /// Mengambil profil berdasarkan ID.
    pub fn get_profile(&self, id: &ProfileId) -> Option<&Profile> {
        self.config.profiles().iter().find(|p| p.id() == id)
    }

    /// Mengambil referensi penyedia monitor.
    pub fn monitor_provider(&self) -> &Arc<dyn MonitorProvider> {
        &self.monitor_provider
    }

    /// Mengatur pembuat shortcut platform.
    pub fn set_shortcut_creator(&mut self, shortcut_creator: Arc<dyn ShortcutCreator>) {
        self.shortcut_creator = shortcut_creator;
    }

    /// Mengambil referensi pembuat shortcut.
    pub fn shortcut_creator(&self) -> &Arc<dyn ShortcutCreator> {
        &self.shortcut_creator
    }

    /// Mengatur penyedia system tray platform.
    pub fn set_tray_provider(&mut self, tray_provider: Arc<dyn TrayProvider>) {
        self.tray_provider = tray_provider;
    }

    /// Mengambil referensi penyedia system tray.
    pub fn tray_provider(&self) -> &Arc<dyn TrayProvider> {
        &self.tray_provider
    }

    /// Memperbarui status system tray dengan ringkasan status terkini koordinator.
    pub fn update_tray_status(&self) -> Result<(), ApplicationError> {
        let summary = self.status_summary();
        self.tray_provider
            .update_status(&summary)
            .map_err(ApplicationError::Platform)
    }

    /// Menghasilkan ringkasan status keseluruhan Window Hop untuk ditampilkan pada system tray.
    ///
    /// Menampilkan jumlah profil yang sedang berjalan, profil yang menunggu monitor tujuan,
    /// atau kesalahan otomatis tanpa popup atau merebut fokus.
    pub fn status_summary(&self) -> TrayStatusSummary {
        let running_profiles: Vec<(&ProfileId, &ActiveProfileInstance)> = self
            .active_instances
            .iter()
            .filter(|(_, inst)| inst.process.is_alive())
            .collect();
        let running_count = running_profiles.len();

        let mut waiting_count = 0;
        let mut disconnected_count = 0;
        for state in self.lifecycle_states.values() {
            match state {
                ProfileLifecycleState::WaitingForMonitor { .. } => waiting_count += 1,
                ProfileLifecycleState::Disconnected { .. } => disconnected_count += 1,
                _ => {}
            }
        }

        let has_error = self.last_error.is_some();
        let last_error = self.last_error.clone();

        let (status_text, tooltip) = if let Some(ref err) = self.last_error {
            let short_err = if err.chars().count() > 60 {
                format!("{}...", err.chars().take(57).collect::<String>())
            } else {
                err.clone()
            };
            (
                format!("Kesalahan: {}", short_err),
                format!("Window Hop - Kesalahan: {}", err),
            )
        } else if running_count > 0 {
            if running_count == 1 {
                let (pid, inst) = running_profiles[0];
                let name = self.get_profile(pid).map(|p| p.name()).unwrap_or("Profil");
                (
                    format!("Profil '{}' berjalan", name),
                    format!(
                        "Window Hop\nProfil '{}' berjalan di monitor '{}'",
                        name, inst.target_monitor.name
                    ),
                )
            } else {
                (
                    format!("{} profil berjalan", running_count),
                    format!("Window Hop\n{} profil aktif berjalan", running_count),
                )
            }
        } else if waiting_count > 0 {
            (
                "Menunggu monitor tujuan".to_string(),
                "Window Hop\nMenunggu monitor tujuan tersambung".to_string(),
            )
        } else if disconnected_count > 0 {
            (
                "Monitor tujuan terputus".to_string(),
                "Window Hop\nMonitor tujuan terputus".to_string(),
            )
        } else {
            (
                "Siap (tidak ada profil aktif)".to_string(),
                "Window Hop - Siap".to_string(),
            )
        };

        // Pastikan tooltip tidak melebihi batas 120 karakter untuk keamanan notifikasi tray Win32
        let safe_tooltip = if tooltip.chars().count() > 120 {
            format!("{}...", tooltip.chars().take(117).collect::<String>())
        } else {
            tooltip
        };

        TrayStatusSummary {
            status_text,
            tooltip: safe_tooltip,
            running_count,
            waiting_count: waiting_count + disconnected_count,
            has_error,
            last_error,
        }
    }

    /// Membuat shortcut untuk profil tersimpan ke folder target (default Desktop jika None).
    pub fn create_profile_shortcut(
        &self,
        id: &ProfileId,
        custom_dir: Option<PathBuf>,
    ) -> Result<PathBuf, ApplicationError> {
        let app_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("window-hop.exe"));
        self.create_profile_shortcut_with_exe(id, custom_dir, app_exe)
    }

    /// Membuat shortcut untuk profil tersimpan dengan path executable aplikasi yang ditentukan
    /// (misal untuk verifikasi penanganan path ber-spasi atau instalasi khusus).
    pub fn create_profile_shortcut_with_exe(
        &self,
        id: &ProfileId,
        custom_dir: Option<PathBuf>,
        app_exe: PathBuf,
    ) -> Result<PathBuf, ApplicationError> {
        let profile = self
            .get_profile(id)
            .ok_or_else(|| ApplicationError::ProfileNotFound { id: id.clone() })?;

        let target_dir = match custom_dir {
            Some(dir) => {
                if dir.as_os_str().is_empty() {
                    return Err(ApplicationError::ValidationError(
                        "Folder shortcut tujuan tidak boleh kosong".to_string(),
                    ));
                }
                dir
            }
            None => self
                .shortcut_creator
                .default_desktop_dir()
                .map_err(ApplicationError::Platform)?,
        };

        let spec = ShortcutSpec {
            profile_id: id.clone(),
            profile_name: profile.name().to_string(),
            target_dir,
            app_executable: app_exe,
            arguments: vec![ACTIVATE_PROFILE_FLAG.to_string(), id.as_str().to_string()],
        };

        self.shortcut_creator
            .create_shortcut(&spec)
            .map_err(ApplicationError::Platform)
    }

    /// Menangani aktivasi profil yang dipicu dari shortcut profil atau baris perintah.
    ///
    /// Aturan:
    /// - Mengaktifkan profil melalui koordinator yang sama tanpa membuka jendela pengaturan.
    /// - Jika profil sudah berjalan dengan konfigurasi yang sama, memakai kembali window yang ada
    ///   dan memastikan penempatannya kembali ke monitor tujuan (Reused).
    /// - Jika monitor tujuan tidak tersedia atau identitasnya ambigu, transisi ke status menunggu/ambigu
    ///   tanpa melakukan fallback ke monitor primary.
    /// - Jika profil tidak ditemukan atau gagal, kegagalan dicatat pada status koordinator tanpa membuka pengaturan.
    pub fn handle_shortcut_activation(
        &mut self,
        id: &ProfileId,
    ) -> Result<ActivationResult, ApplicationError> {
        match self.activate_profile(id) {
            Ok(result) => {
                self.last_error = None;
                let _ = self.update_tray_status();
                Ok(result)
            }
            Err(err) => {
                self.last_error = Some(err.to_string());
                let _ = self.update_tray_status();
                Err(err)
            }
        }
    }

    /// Mengambil daftar semua monitor yang terdeteksi dan aktif saat ini.
    pub fn available_monitors(&self) -> Result<Vec<MonitorDescriptor>, ApplicationError> {
        self.monitor_provider
            .available_monitors()
            .map_err(ApplicationError::Platform)
    }

    /// Mengevaluasi status kecocokan monitor tujuan untuk profil tertentu.
    pub fn get_target_monitor_status(
        &self,
        id: &ProfileId,
    ) -> Result<TargetMonitorStatus, ApplicationError> {
        let profile = self
            .get_profile(id)
            .ok_or_else(|| ApplicationError::ProfileNotFound { id: id.clone() })?;

        if profile.target_monitor().is_none() {
            return Ok(TargetMonitorStatus::Unset);
        }

        let monitors = self.available_monitors()?;
        Ok(profile.evaluate_target_monitor(&monitors))
    }

    /// Mengevaluasi status kesiapan profil tertentu berdasarkan ketersediaan monitor tujuan saat ini.
    pub fn get_profile_status(&self, id: &ProfileId) -> Result<ProfileStatus, ApplicationError> {
        let profile = self
            .get_profile(id)
            .ok_or_else(|| ApplicationError::ProfileNotFound { id: id.clone() })?;

        if profile.target_monitor().is_none() {
            return Ok(ProfileStatus::DraftNotReady);
        }

        let monitors = self.available_monitors()?;
        Ok(profile.evaluate_status(&monitors))
    }

    /// Mengambil status siklus hidup runtime suatu profil.
    pub fn get_lifecycle_state(
        &self,
        id: &ProfileId,
    ) -> Result<ProfileLifecycleState, ApplicationError> {
        if self.get_profile(id).is_none() {
            return Err(ApplicationError::ProfileNotFound { id: id.clone() });
        }

        if let Some(active) = self.active_instances.get(id) {
            if active.process.is_alive() {
                return Ok(ProfileLifecycleState::Running {
                    pid: active.process.pid(),
                    target_monitor: active.target_monitor.clone(),
                });
            } else if !active.stopping {
                let reason = active
                    .process
                    .exit_reason()
                    .unwrap_or(ExitReason::ManualClose);
                return Ok(ProfileLifecycleState::Exited { reason });
            }
        }

        Ok(self
            .lifecycle_states
            .get(id)
            .cloned()
            .unwrap_or(ProfileLifecycleState::Idle))
    }

    /// Memeriksa apakah suatu profil dapat diaktifkan.
    /// Profil hanya diizinkan untuk diaktifkan jika monitor tujuan cocok dan tersedia (tanpa fallback),
    /// adapter browser memiliki bukti pemisahan sesi dan penanganan kepemilikan,
    /// serta parameter peluncuran kustom tidak berbenturan dengan isolasi sesi profil.
    pub fn can_activate(&self, id: &ProfileId) -> Result<bool, ApplicationError> {
        let profile = self
            .get_profile(id)
            .ok_or_else(|| ApplicationError::ProfileNotFound { id: id.clone() })?;

        if profile.target_monitor().is_none() {
            return Err(ApplicationError::DraftNotReady { id: id.clone() });
        }

        let monitors = self.available_monitors()?;
        match profile.evaluate_target_monitor(&monitors) {
            TargetMonitorStatus::Unset => {
                return Err(ApplicationError::DraftNotReady { id: id.clone() })
            }
            TargetMonitorStatus::Unavailable { recorded_identity } => {
                return Err(ApplicationError::TargetMonitorUnavailable {
                    id: id.clone(),
                    recorded_identity,
                });
            }
            TargetMonitorStatus::Ambiguous {
                recorded_identity,
                match_count,
            } => {
                return Err(ApplicationError::TargetMonitorAmbiguous {
                    id: id.clone(),
                    recorded_identity,
                    match_count,
                });
            }
            TargetMonitorStatus::Available(_) => {}
        }

        // Verifikasi kapabilitas browser
        let adapter = match crate::browsers::detect_browser_support(profile.browser_path()) {
            crate::browsers::BrowserSupportStatus::Verified(adapter) => adapter,
            crate::browsers::BrowserSupportStatus::Unverified => {
                return Err(ApplicationError::BrowserNotVerified(
                    profile.browser_path().display().to_string(),
                    format!(
                        "Dukungan pemisahan sesi dan kepemilikan instance untuk browser '{}' belum diverifikasi.",
                        profile.browser_path().display()
                    ),
                ));
            }
        };

        // Verifikasi parameter kustom: tolak argumen yang berbenturan sebelum aktivasi
        adapter.validate_custom_parameters(profile.custom_parameters())?;

        Ok(true)
    }

    /// Mengaktifkan profil Window Hop pada monitor tujuannya dengan pemisahan sesi browser.
    ///
    /// Aturan:
    /// - Hanya diizinkan jika monitor tujuan cocok dan browser terverifikasi.
    /// - Memeriksa argumen kustom; menolak argumen yang berbenturan dengan isolasi sesi.
    /// - Jika profil sudah memiliki window/instance yang berjalan dengan konfigurasi yang sama,
    ///   window tersebut digunakan kembali (reuse window) dan penempatannya dipastikan ulang ke
    ///   monitor tujuan tanpa meluncurkan proses baru.
    /// - Jika konfigurasi telah diubah dan disimpan, aktivasi menerapkan konfigurasi terbaru
    ///   dengan menghentikan browser lama sebelum menjalankan yang baru.
    /// - Jika belum berjalan, meluncurkan browser terisolasi dengan direktori data sesi khusus profil.
    pub fn activate_profile(
        &mut self,
        id: &ProfileId,
    ) -> Result<ActivationResult, ApplicationError> {
        let profile = self
            .get_profile(id)
            .ok_or_else(|| ApplicationError::ProfileNotFound { id: id.clone() })?
            .clone();

        if profile.target_monitor().is_none() {
            return Err(ApplicationError::DraftNotReady { id: id.clone() });
        }

        // Verifikasi kapabilitas browser
        let adapter = match crate::browsers::detect_browser_support(profile.browser_path()) {
            crate::browsers::BrowserSupportStatus::Verified(adapter) => adapter,
            crate::browsers::BrowserSupportStatus::Unverified => {
                return Err(ApplicationError::BrowserNotVerified(
                    profile.browser_path().display().to_string(),
                    format!(
                        "Dukungan pemisahan sesi dan kepemilikan instance untuk browser '{}' belum diverifikasi.",
                        profile.browser_path().display()
                    ),
                ));
            }
        };

        // Verifikasi parameter kustom: tolak argumen yang berbenturan sebelum aktivasi
        adapter.validate_custom_parameters(profile.custom_parameters())?;

        let monitors = self.available_monitors()?;
        let target_desc = match profile.evaluate_target_monitor(&monitors) {
            TargetMonitorStatus::Unset => {
                return Err(ApplicationError::DraftNotReady { id: id.clone() });
            }
            TargetMonitorStatus::Unavailable { recorded_identity } => {
                // Monitor tujuan belum tersedia: jika ada instance lama yang sedang berjalan (karena konfigurasi diubah),
                // hentikan instance lama tersebut terlebih dahulu sebelum masuk status menunggu.
                if self.is_profile_running(id) {
                    self.stop_profile(id)?;
                }
                self.lifecycle_states.insert(
                    id.clone(),
                    ProfileLifecycleState::WaitingForMonitor {
                        target_monitor: recorded_identity.clone(),
                    },
                );
                let _ = self.update_tray_status();
                return Ok(ActivationResult::WaitingForMonitor {
                    target_monitor: recorded_identity,
                });
            }
            TargetMonitorStatus::Ambiguous {
                recorded_identity,
                match_count,
            } => {
                return Err(ApplicationError::TargetMonitorAmbiguous {
                    id: id.clone(),
                    recorded_identity,
                    match_count,
                });
            }
            TargetMonitorStatus::Available(desc) => desc,
        };

        // Periksa apakah instance profil sudah berjalan (reuse window)
        if let Some(existing) = self.active_instances.get(id) {
            if existing.process.is_alive() {
                if existing.matches_config(&profile, &target_desc) {
                    existing
                        .process
                        .reposition(&target_desc)
                        .map_err(ApplicationError::Platform)?;
                    self.lifecycle_states.insert(
                        id.clone(),
                        ProfileLifecycleState::Running {
                            pid: existing.process.pid(),
                            target_monitor: target_desc.clone(),
                        },
                    );
                    let _ = self.update_tray_status();
                    return Ok(ActivationResult::Reused {
                        pid: existing.process.pid(),
                        target_monitor: target_desc,
                        login_retention: existing.login_retention.clone(),
                    });
                } else {
                    // Konfigurasi profil telah berubah: hentikan instance lama terlebih dahulu
                    self.stop_profile(id)?;
                }
            } else {
                self.active_instances.remove(id);
            }
        }

        // Eksklusivitas per-monitor: Hanya satu profil aktif menguasai satu monitor tujuan.
        // Jika ada profil lain yang sedang berjalan pada monitor tujuan yang sama,
        // hentikan profil lama tersebut terlebih dahulu secara berurutan sebelum meluncurkan profil baru.
        let conflicting_active_profiles: Vec<ProfileId> = self
            .active_instances
            .iter()
            .filter(|(other_id, inst)| {
                *other_id != id
                    && inst.target_monitor.identity == target_desc.identity
                    && inst.process.is_alive()
            })
            .map(|(other_id, _)| other_id.clone())
            .collect();

        for other_id in conflicting_active_profiles {
            if let Err(err) = self.stop_profile(&other_id) {
                let prev_name = self
                    .get_profile(&other_id)
                    .map(|p| p.name().to_string())
                    .unwrap_or_else(|| other_id.to_string());
                let switch_err = ApplicationError::SwitchProfileFailed {
                    previous_profile_id: other_id,
                    previous_profile_name: prev_name,
                    target_monitor: target_desc.identity.clone(),
                    reason: err.to_string(),
                };
                self.last_error = Some(switch_err.to_string());
                return Err(switch_err);
            }
        }

        let (pid, login_retention) = self.launch_profile_instance(&profile, &target_desc)?;
        let _ = self.update_tray_status();

        Ok(ActivationResult::Launched {
            pid,
            target_monitor: target_desc,
            login_retention,
        })
    }

    /// Menjalankan ulang (restart) profil dengan konfigurasi tersimpan terbaru secara sengaja.
    ///
    /// Aturan:
    /// - Memvalidasi konfigurasi terlebih dahulu (browser, parameter kustom, status monitor).
    ///   Jika validasi gagal, instance yang sedang berjalan tetap aman dan tidak dihentikan secara prematur.
    /// - Jika profil sedang berjalan aktif, menghentikan instance yang sedang berjalan terlebih dahulu
    ///   dengan penutupan normal dan batas tenggat 3 detik.
    /// - Meluncurkan instance proses baru dengan konfigurasi tersimpan terbaru pada monitor tujuan.
    /// - Jika monitor tujuan belum tersedia, profil masuk ke status `WaitingForMonitor`.
    pub fn restart_profile(
        &mut self,
        id: &ProfileId,
    ) -> Result<ActivationResult, ApplicationError> {
        let profile = self
            .get_profile(id)
            .ok_or_else(|| ApplicationError::ProfileNotFound { id: id.clone() })?
            .clone();

        if profile.target_monitor().is_none() {
            return Err(ApplicationError::DraftNotReady { id: id.clone() });
        }

        // 1. Validasi kapabilitas browser dan parameter sebelum mematikan proses aktif
        let adapter = match crate::browsers::detect_browser_support(profile.browser_path()) {
            crate::browsers::BrowserSupportStatus::Verified(adapter) => adapter,
            crate::browsers::BrowserSupportStatus::Unverified => {
                return Err(ApplicationError::BrowserNotVerified(
                    profile.browser_path().display().to_string(),
                    format!(
                        "Dukungan pemisahan sesi dan kepemilikan instance untuk browser '{}' belum diverifikasi.",
                        profile.browser_path().display()
                    ),
                ));
            }
        };
        adapter.validate_custom_parameters(profile.custom_parameters())?;

        // 2. Validasi evaluasi monitor: jika ambigu, tolak tanpa mematikan tayangan aktif
        let monitors = self.available_monitors()?;
        match profile.evaluate_target_monitor(&monitors) {
            TargetMonitorStatus::Unset => {
                return Err(ApplicationError::DraftNotReady { id: id.clone() });
            }
            TargetMonitorStatus::Ambiguous {
                recorded_identity,
                match_count,
            } => {
                return Err(ApplicationError::TargetMonitorAmbiguous {
                    id: id.clone(),
                    recorded_identity,
                    match_count,
                });
            }
            _ => {}
        }

        // 3. Setelah validasi lolos, hentikan instance lama jika sedang berjalan
        if self.is_profile_running(id) {
            self.stop_profile(id)?;
        }

        self.activate_profile(id)
    }

    /// Menghapus profil dari konfigurasi dan penyimpanan secara permanen.
    ///
    /// Aturan:
    /// - Menyimpan perubahan penghapusan ke penyimpanan (store) terlebih dahulu agar jika terjadi kegagalan I/O,
    ///   instance aktif tidak dimatikan secara sia-sia.
    /// - Jika profil sedang berjalan aktif, menghentikan instance proses milik profil
    ///   dengan penutupan normal dan batas tenggat 3 detik (force stop hanya jika tidak responsif).
    /// - Menghapus seluruh status siklus hidup runtime profil sehingga tidak memicu pemulihan reconnect.
    /// - Mengembalikan StopOutcome untuk memberi tahu status penghentian proses (Graceful, Forced, atau NotRunning).
    pub fn delete_profile(&mut self, id: &ProfileId) -> Result<StopOutcome, ApplicationError> {
        if self.load_failed {
            let reason = self
                .last_error
                .clone()
                .unwrap_or_else(|| "Gagal memuat konfigurasi awal".to_string());
            return Err(ApplicationError::BlockedByCorruptState(reason));
        }

        if self.get_profile(id).is_none() {
            return Err(ApplicationError::ProfileNotFound { id: id.clone() });
        }

        // 1. Buat kandidat konfigurasi tanpa profil yang dihapus dan simpan ke store terlebih dahulu
        let mut candidate_config = self.config.clone();
        candidate_config.profiles_mut().retain(|p| p.id() != id);
        self.store.save(&candidate_config)?;

        // 2. Setelah penyimpanan terkonfirmasi berhasil, hentikan proses yang sedang aktif jika ada
        let stop_outcome = if self.is_profile_running(id) {
            self.stop_profile(id)?
        } else {
            StopOutcome::NotRunning
        };

        // 3. Komit ke in-memory state
        self.config = candidate_config;
        self.active_instances.remove(id);
        self.lifecycle_states.remove(id);
        self.last_error = None;
        let _ = self.update_tray_status();
        Ok(stop_outcome)
    }

    /// Meluncurkan instance proses baru untuk profil pada monitor tujuan yang diberikan.
    fn launch_profile_instance(
        &mut self,
        profile: &Profile,
        target_desc: &MonitorDescriptor,
    ) -> Result<(ProcessId, LoginRetentionNote), ApplicationError> {
        let adapter = match crate::browsers::detect_browser_support(profile.browser_path()) {
            crate::browsers::BrowserSupportStatus::Verified(adapter) => adapter,
            crate::browsers::BrowserSupportStatus::Unverified => {
                return Err(ApplicationError::BrowserNotVerified(
                    profile.browser_path().display().to_string(),
                    format!(
                        "Dukungan pemisahan sesi dan kepemilikan instance untuk browser '{}' belum diverifikasi.",
                        profile.browser_path().display()
                    ),
                ));
            }
        };

        adapter.validate_custom_parameters(profile.custom_parameters())?;

        let profile_data_dir = self
            .base_data_dir
            .join("profiles")
            .join(profile.id().as_str())
            .join("browser_session");

        let launch_plan = adapter.build_launch_plan(
            profile.browser_path(),
            &profile_data_dir,
            profile.custom_parameters(),
        )?;

        let managed_proc = self
            .process_launcher
            .launch(&launch_plan, target_desc)
            .map_err(ApplicationError::Platform)?;

        let pid = managed_proc.pid();
        let login_retention = launch_plan.login_retention.clone();

        self.active_instances.insert(
            profile.id().clone(),
            ActiveProfileInstance {
                process: managed_proc,
                launch_browser_path: profile.browser_path().to_path_buf(),
                launch_parameters: profile.custom_parameters().to_string(),
                target_monitor: target_desc.clone(),
                login_retention: login_retention.clone(),
                stopping: false,
            },
        );

        self.lifecycle_states.insert(
            profile.id().clone(),
            ProfileLifecycleState::Running {
                pid,
                target_monitor: target_desc.clone(),
            },
        );

        Ok((pid, login_retention))
    }

    /// Menangani perubahan daftar monitor yang terhubung ke sistem.
    pub fn handle_monitors_changed(
        &mut self,
    ) -> Result<Vec<LifecycleTransitionOutcome>, ApplicationError> {
        let monitors = self.available_monitors()?;
        self.process_monitors_changed(&monitors)
    }

    /// Memproses daftar monitor aktif untuk memulihkan profil yang menunggu / terputus
    /// atau menghentikan profil yang kehilangan koneksi monitor.
    pub fn process_monitors_changed(
        &mut self,
        available_monitors: &[MonitorDescriptor],
    ) -> Result<Vec<LifecycleTransitionOutcome>, ApplicationError> {
        let mut transitions = Vec::new();

        // 0. Periksa instance aktif yang keluar secara spontan (crash atau ditutup manual)
        let active_keys: Vec<ProfileId> = self.active_instances.keys().cloned().collect();
        for profile_id in active_keys {
            if let Some(instance) = self.active_instances.get(&profile_id) {
                if !instance.process.is_alive() && !instance.stopping {
                    let pid = instance.process.pid();
                    let reason = instance
                        .process
                        .exit_reason()
                        .unwrap_or(ExitReason::ManualClose);
                    self.active_instances.remove(&profile_id);
                    self.lifecycle_states
                        .insert(profile_id.clone(), ProfileLifecycleState::Exited { reason });
                    transitions.push(LifecycleTransitionOutcome::Exited {
                        profile_id,
                        pid,
                        reason,
                    });
                }
            }
        }

        // 1. Periksa instance aktif yang monitor tujuannya terputus atau menjadi ambigu
        let active_keys: Vec<ProfileId> = self.active_instances.keys().cloned().collect();
        for profile_id in active_keys {
            let Some(mut instance) = self.active_instances.remove(&profile_id) else {
                continue;
            };

            let target_id = instance.target_monitor.identity.clone();
            let eval = TargetMonitorStatus::evaluate(Some(&target_id), available_monitors);

            match eval {
                TargetMonitorStatus::Available(desc) => {
                    // Monitor masih tersedia normal; perbarui descriptor jika ada pembaruan posisi
                    instance.target_monitor = desc;
                    self.active_instances.insert(profile_id, instance);
                }
                TargetMonitorStatus::Unavailable { .. } | TargetMonitorStatus::Ambiguous { .. } => {
                    // Monitor terputus atau ambigu: minta tutup normal dan terapkan deadline 3 detik
                    instance.stopping = true;
                    let outcome = Self::stop_active_instance_with_deadline(&self.clock, &instance)?;

                    let (pid, forced) = match outcome {
                        StopOutcome::Graceful { pid } => (pid, false),
                        StopOutcome::Forced { pid } => (pid, true),
                        StopOutcome::NotRunning => (instance.process.pid(), false),
                    };

                    self.lifecycle_states.insert(
                        profile_id.clone(),
                        ProfileLifecycleState::Disconnected {
                            target_monitor: target_id,
                        },
                    );

                    transitions.push(LifecycleTransitionOutcome::Disconnected {
                        profile_id,
                        pid,
                        forced,
                    });
                }
                TargetMonitorStatus::Unset => {
                    self.active_instances.insert(profile_id, instance);
                }
            }
        }

        // 2. Pulihkan profil yang berstatus WaitingForMonitor atau Disconnected
        let profiles = self.config.profiles().to_vec();
        for profile in profiles {
            let pid = profile.id().clone();
            let state = self
                .lifecycle_states
                .get(&pid)
                .cloned()
                .unwrap_or(ProfileLifecycleState::Idle);

            match state {
                ProfileLifecycleState::WaitingForMonitor { target_monitor }
                | ProfileLifecycleState::Disconnected { target_monitor } => {
                    // Hanya pulihkan jika monitor yang kembali tersambung/menunggu masih sesuai dengan target profil tersimpan
                    if profile.target_monitor() != Some(&target_monitor) {
                        continue;
                    }

                    let eval =
                        TargetMonitorStatus::evaluate(Some(&target_monitor), available_monitors);
                    if let TargetMonitorStatus::Available(desc) = eval {
                        match self.launch_profile_instance(&profile, &desc) {
                            Ok((launched_pid, _)) => {
                                transitions.push(LifecycleTransitionOutcome::Restored {
                                    profile_id: pid,
                                    pid: launched_pid,
                                    target_monitor: desc,
                                });
                            }
                            Err(err) => {
                                self.last_error = Some(err.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let _ = self.update_tray_status();
        Ok(transitions)
    }

    /// Menghentikan instance proses yang sedang berjalan dengan penutupan anggun dan batas tenggat 3 detik.
    fn stop_active_instance_with_deadline(
        clock: &Arc<dyn Clock>,
        instance: &ActiveProfileInstance,
    ) -> Result<StopOutcome, ApplicationError> {
        let pid = instance.process.pid();
        if !instance.process.is_alive() {
            return Ok(StopOutcome::NotRunning);
        }

        // 1. Minta penutupan anggun (graceful close)
        instance
            .process
            .request_close()
            .map_err(ApplicationError::Platform)?;

        // 2. Tunggu hingga proses berhenti atau batas 3 detik terlampaui menggunakan Clock
        let start_time = clock.now();
        let timeout = std::time::Duration::from_secs(3);
        let poll_interval = std::time::Duration::from_millis(100);

        while instance.process.is_alive() && clock.now().duration_since(start_time) < timeout {
            clock.sleep(poll_interval);
        }

        // 3. Evaluasi apakah sudah berhenti atau perlu force stop
        if !instance.process.is_alive() {
            Ok(StopOutcome::Graceful { pid })
        } else {
            // Force stop hanya pada instance profil terverifikasi
            instance
                .process
                .force_stop()
                .map_err(ApplicationError::Platform)?;
            if instance.process.is_alive() {
                return Err(ApplicationError::Platform(PlatformError::Os(format!(
                    "Gagal menghentikan proses {} (PID: {}) setelah batas waktu 3 detik",
                    pid,
                    pid.as_u32()
                ))));
            }
            Ok(StopOutcome::Forced { pid })
        }
    }

    /// Memeriksa apakah profil sedang berjalan aktif dengan instance proses yang masih hidup.
    pub fn is_profile_running(&self, id: &ProfileId) -> bool {
        self.active_instances
            .get(id)
            .map(|inst| inst.process.is_alive())
            .unwrap_or(false)
    }

    /// Memeriksa apakah konfigurasi tersimpan saat ini memiliki perbedaan dibanding konfigurasi
    /// yang sedang berjalan pada instance aktif (misal: parameter, browser executable, atau monitor tujuan diubah).
    /// Mengembalikan false jika profil tidak sedang berjalan aktif.
    pub fn has_running_config_changes(&self, id: &ProfileId) -> bool {
        let Some(profile) = self.get_profile(id) else {
            return false;
        };
        let Some(active) = self.active_instances.get(id) else {
            return false;
        };
        if !active.process.is_alive() {
            return false;
        }

        !active.matches_saved_profile(profile)
    }

    /// Menemukan profil lain yang saat ini sedang aktif berjalan pada monitor tertentu.
    pub fn find_conflicting_active_profile(
        &self,
        exclude_id: Option<&ProfileId>,
        monitor_id: &MonitorIdentity,
    ) -> Option<&Profile> {
        let active_profile_id = self.active_instances.iter().find_map(|(id, inst)| {
            if exclude_id != Some(id)
                && &inst.target_monitor.identity == monitor_id
                && inst.process.is_alive()
            {
                Some(id)
            } else {
                None
            }
        })?;
        self.get_profile(active_profile_id)
    }

    /// Mengambil referensi ke instance proses yang sedang aktif untuk profil tertentu (jika ada).
    pub fn active_instance(&self, id: &ProfileId) -> Option<Arc<dyn ManagedProcess>> {
        self.active_instances
            .get(id)
            .map(|inst| inst.process.clone())
    }

    /// Menghentikan profil yang sedang berjalan.
    ///
    /// Aturan:
    /// - Mengirimkan permintaan penutupan normal (graceful close) terlebih dahulu.
    /// - Menunggu hingga proses berhenti dengan batas tenggat 3 detik.
    /// - Jika belum berhenti setelah 3 detik, menghentikan paksa (force stop) hanya instance milik profil yang terverifikasi.
    /// - Proses lain (termasuk browser kerja perawat) tidak disentuh sama sekali.
    pub fn stop_profile(&mut self, id: &ProfileId) -> Result<StopOutcome, ApplicationError> {
        let mut instance = match self.active_instances.remove(id) {
            Some(mut inst) => {
                if !inst.process.is_alive() {
                    self.lifecycle_states
                        .insert(id.clone(), ProfileLifecycleState::Idle);
                    return Ok(StopOutcome::NotRunning);
                }
                inst.stopping = true;
                inst
            }
            None => {
                self.lifecycle_states
                    .insert(id.clone(), ProfileLifecycleState::Idle);
                return Ok(StopOutcome::NotRunning);
            }
        };

        match Self::stop_active_instance_with_deadline(&self.clock, &instance) {
            Ok(outcome) => {
                self.lifecycle_states
                    .insert(id.clone(), ProfileLifecycleState::Idle);
                let _ = self.update_tray_status();
                Ok(outcome)
            }
            Err(err) => {
                // Jika gagal menghentikan dan proses masih hidup, pulihkan status instance dan running state
                if instance.process.is_alive() {
                    instance.stopping = false;
                    self.lifecycle_states.insert(
                        id.clone(),
                        ProfileLifecycleState::Running {
                            pid: instance.process.pid(),
                            target_monitor: instance.target_monitor.clone(),
                        },
                    );
                    self.active_instances.insert(id.clone(), instance);
                } else {
                    self.lifecycle_states
                        .insert(id.clone(), ProfileLifecycleState::Idle);
                }
                self.last_error = Some(err.to_string());
                let _ = self.update_tray_status();
                Err(err)
            }
        }
    }

    /// Menghentikan seluruh instance browser yang dikelola sebelum aplikasi keluar.
    ///
    /// Aturan:
    /// - Mengirimkan permintaan penutupan normal (graceful close) ke setiap instance aktif.
    /// - Menunggu hingga proses berhenti dengan batas tenggat 3 detik.
    /// - Jika belum berhenti setelah 3 detik, menghentikan paksa (force stop) hanya instance
    ///   milik profil yang terverifikasi.
    /// - Browser kerja perawat dan proses di luar kepemilikan terverifikasi tidak disentuh sama sekali.
    /// - Membersihkan seluruh instance aktif dan mengatur status menjadi Idle.
    /// - Menghapus ikon tray sebelum aplikasi berhenti.
    pub fn shutdown(&mut self) -> Result<Vec<(ProfileId, StopOutcome)>, ApplicationError> {
        let active_keys: Vec<ProfileId> = self.active_instances.keys().cloned().collect();
        let mut outcomes = Vec::new();
        let mut first_error = None;

        for pid in active_keys {
            match self.stop_profile(&pid) {
                Ok(outcome) => {
                    outcomes.push((pid, outcome));
                }
                Err(err) => {
                    if first_error.is_none() {
                        first_error = Some(err);
                    }
                }
            }
        }

        self.active_instances.clear();
        let _ = self.update_tray_status();
        let _ = self.tray_provider.remove_tray();

        if let Some(err) = first_error {
            self.last_error = Some(err.to_string());
            return Err(err);
        }

        Ok(outcomes)
    }

    /// Menangani keluarnya proses browser yang dikelola.
    ///
    /// Jika proses keluar secara spontan (bukan karena permintaan penutupan oleh aplikasi),
    /// profil beralih ke status `Exited { reason }`, menonaktifkan pemulihan reconnect otomatis
    /// tanpa retry loop.
    /// Event terlambat dari instance lama (stale PID) diabaikan dengan aman tanpa mengubah status profil.
    pub fn handle_process_exit(
        &mut self,
        pid: ProcessId,
        reason: ExitReason,
    ) -> Option<LifecycleTransitionOutcome> {
        let matching_profile_id = self
            .active_instances
            .iter()
            .find(|(_, inst)| inst.process.pid() == pid)
            .map(|(id, _)| id.clone());

        let Some(profile_id) = matching_profile_id else {
            // Event terlambat dari instance lama atau PID tidak dikenal: abaikan dengan aman
            return None;
        };

        let instance = self.active_instances.remove(&profile_id)?;
        if instance.stopping {
            // Proses keluar karena permintaan penutupan aplikasi (misal monitor terputus);
            // penanganannya sudah/sedang diurus oleh koordinator.
            return None;
        }

        self.lifecycle_states
            .insert(profile_id.clone(), ProfileLifecycleState::Exited { reason });

        let _ = self.update_tray_status();

        Some(LifecycleTransitionOutcome::Exited {
            profile_id,
            pid,
            reason,
        })
    }

    /// Mengambil pesan error terakhir yang terjadi (jika ada) untuk ditampilkan pada UI.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Membersihkan status pesan error.
    pub fn clear_error(&mut self) {
        self.last_error = None;
        let _ = self.update_tray_status();
    }

    /// Mengambil konfigurasi penuh saat ini.
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Mengambil referensi store yang mendasari.
    pub fn store(&self) -> &Arc<dyn ProfileStore> {
        &self.store
    }
}
