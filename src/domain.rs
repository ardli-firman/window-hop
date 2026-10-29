//! Domain models and pure rules for Window Hop.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

/// Identitas konsisten untuk suatu profil Window Hop.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(String);

impl ProfileId {
    /// Membuat ProfileId dari nilai string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Menghasilkan ProfileId unik baru menggunakan UUID v4.
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Mengambil representasi string dari ProfileId.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identitas proses sistem operasi yang dikelola.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProcessId(u32);

impl ProcessId {
    /// Membuat ProcessId dari nilai numerik PID.
    pub fn new(pid: u32) -> Self {
        Self(pid)
    }

    /// Mengambil representasi integer u32 dari PID.
    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

impl fmt::Display for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PID:{}", self.0)
    }
}

/// Identitas konsisten untuk monitor tujuan.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MonitorIdentity(String);

impl MonitorIdentity {
    /// Membuat MonitorIdentity dari nilai string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Mengambil representasi string dari MonitorIdentity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MonitorIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Deskripsi monitor fisik yang terdeteksi oleh sistem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorDescriptor {
    pub identity: MonitorIdentity,
    pub name: String,
    pub is_primary: bool,
}

/// Status evaluasi monitor tujuan terhadap monitor yang terdeteksi saat ini.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetMonitorStatus {
    /// Monitor tujuan belum dipilih (draft).
    Unset,
    /// Monitor tujuan ditemukan secara unik dan tersedia.
    Available(MonitorDescriptor),
    /// Monitor tujuan tidak ditemukan di antara monitor yang terdeteksi saat ini.
    Unavailable { recorded_identity: MonitorIdentity },
    /// Identitas monitor cocok dengan lebih dari satu monitor yang aktif (ambigu),
    /// membutuhkan pemilihan ulang oleh pengguna.
    Ambiguous {
        recorded_identity: MonitorIdentity,
        match_count: usize,
    },
}

impl TargetMonitorStatus {
    /// Mengevaluasi status kecocokan monitor tujuan terhadap daftar monitor yang tersedia saat ini.
    pub fn evaluate(
        target_monitor: Option<&MonitorIdentity>,
        available_monitors: &[MonitorDescriptor],
    ) -> Self {
        let Some(target_id) = target_monitor else {
            return Self::Unset;
        };

        let matches: Vec<&MonitorDescriptor> = available_monitors
            .iter()
            .filter(|m| &m.identity == target_id)
            .collect();

        match matches.len() {
            0 => Self::Unavailable {
                recorded_identity: target_id.clone(),
            },
            1 => Self::Available((*matches[0]).clone()),
            count => Self::Ambiguous {
                recorded_identity: target_id.clone(),
                match_count: count,
            },
        }
    }
}

impl fmt::Display for TargetMonitorStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unset => write!(f, "Belum Dipilih"),
            Self::Available(m) => write!(f, "Tersedia: {}", m.name),
            Self::Unavailable { recorded_identity } => {
                write!(f, "Tidak Tersedia ({})", recorded_identity)
            }
            Self::Ambiguous {
                recorded_identity,
                match_count,
            } => write!(
                f,
                "Ambigu: {} monitor terdeteksi dengan identitas '{}'",
                match_count, recorded_identity
            ),
        }
    }
}

/// Alasan penghentian proses browser profil.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExitReason {
    /// Browser crash (terminasi abnormal / kode keluar non-nol).
    Crash,
    /// Browser ditutup secara manual oleh pengguna.
    ManualClose,
}

impl fmt::Display for ExitReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Crash => write!(f, "Crash"),
            Self::ManualClose => write!(f, "Ditutup Manual"),
        }
    }
}

/// Status siklus hidup runtime suatu profil yang dikelola Window Hop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileLifecycleState {
    /// Profil tidak sedang aktif atau berjalan.
    Idle,
    /// Profil diminta aktif namun monitor tujuannya belum tersedia;
    /// menunggu monitor yang cocok tanpa meluncurkan browser.
    WaitingForMonitor { target_monitor: MonitorIdentity },
    /// Profil sedang berjalan aktif pada monitor tujuan.
    Running {
        pid: ProcessId,
        target_monitor: MonitorDescriptor,
    },
    /// Profil dihentikan karena monitor tujuannya terputus;
    /// memenuhi syarat pemulihan otomatis saat monitor dengan identitas yang sama tersambung kembali.
    Disconnected { target_monitor: MonitorIdentity },
    /// Profil berhenti karena browser crash atau ditutup manual;
    /// pemulihan otomatis reconnect dinonaktifkan (tidak ada retry loop).
    Exited { reason: ExitReason },
}

impl ProfileLifecycleState {
    /// Apakah profil sedang berjalan aktif.
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    /// Apakah profil memenuhi syarat untuk pemulihan otomatis saat monitor tersambung kembali.
    pub fn can_reconnect_restore(&self, monitor_id: &MonitorIdentity) -> bool {
        match self {
            Self::WaitingForMonitor { target_monitor } => target_monitor == monitor_id,
            Self::Disconnected { target_monitor } => target_monitor == monitor_id,
            Self::Idle | Self::Running { .. } | Self::Exited { .. } => false,
        }
    }
}

impl fmt::Display for ProfileLifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "Tidak Aktif"),
            Self::WaitingForMonitor { target_monitor } => {
                write!(f, "Menunggu Monitor ({})", target_monitor)
            }
            Self::Running {
                pid,
                target_monitor,
            } => {
                write!(f, "Berjalan di {} ({})", target_monitor.name, pid)
            }
            Self::Disconnected { target_monitor } => {
                write!(f, "Monitor Terputus ({})", target_monitor)
            }
            Self::Exited { reason } => write!(f, "Berhenti ({})", reason),
        }
    }
}

/// Status kesiapan atau siklus hidup suatu profil.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileStatus {
    /// Profil draft belum memiliki monitor tujuan sehingga belum siap diaktifkan
    /// dan tidak boleh meluncurkan browser.
    DraftNotReady,
    /// Monitor tujuan dipilih dan tersedia; siap diaktifkan.
    Ready,
    /// Monitor tujuan hilang/terputus; tidak tersedia.
    TargetUnavailable,
    /// Monitor tujuan ambigu atau berubah; meminta pemilihan ulang.
    TargetAmbiguous,
}

impl fmt::Display for ProfileStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DraftNotReady => write!(f, "Draft (Belum Siap Diaktifkan)"),
            Self::Ready => write!(f, "Siap Diaktifkan"),
            Self::TargetUnavailable => write!(f, "Monitor Tujuan Tidak Tersedia"),
            Self::TargetAmbiguous => write!(f, "Monitor Tujuan Ambigu (Perlu Pemilihan Ulang)"),
        }
    }
}

/// Data input untuk pembuatan atau pembaruan profil draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftProfileInput {
    pub name: String,
    pub browser_path: PathBuf,
    pub custom_parameters: String,
    pub target_monitor: Option<MonitorIdentity>,
}

impl DraftProfileInput {
    /// Membuat data input baru untuk profil draft.
    pub fn new(
        name: impl Into<String>,
        browser_path: PathBuf,
        custom_parameters: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            browser_path,
            custom_parameters: custom_parameters.into(),
            target_monitor: None,
        }
    }

    /// Menambahkan pilihan monitor tujuan ke data input profil.
    pub fn with_target_monitor(mut self, target_monitor: Option<MonitorIdentity>) -> Self {
        self.target_monitor = target_monitor;
        self
    }
}

/// Konfigurasi profil satu aplikasi antrian.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    id: ProfileId,
    name: String,
    browser_path: PathBuf,
    custom_parameters: String,
    target_monitor: Option<MonitorIdentity>,
}

impl Profile {
    /// Membuat profil draft baru (tanpa monitor tujuan).
    pub fn new_draft(
        id: ProfileId,
        name: impl Into<String>,
        browser_path: PathBuf,
        custom_parameters: impl Into<String>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            browser_path,
            custom_parameters: custom_parameters.into(),
            target_monitor: None,
        }
    }

    /// Mengambil ID profil.
    pub fn id(&self) -> &ProfileId {
        &self.id
    }

    /// Mengambil nama profil.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Mengambil jalur file executable browser.
    pub fn browser_path(&self) -> &Path {
        &self.browser_path
    }

    /// Mengambil parameter peluncuran kustom.
    pub fn custom_parameters(&self) -> &str {
        &self.custom_parameters
    }

    /// Mengambil monitor tujuan jika sudah dipilih.
    pub fn target_monitor(&self) -> Option<&MonitorIdentity> {
        self.target_monitor.as_ref()
    }

    /// Status profil statis (tanpa konteks monitor OS).
    /// Profil tanpa monitor tujuan selalu `DraftNotReady`.
    /// Untuk evaluasi status kesiapan aktual berdasarkan deteksi monitor aktif,
    /// gunakan `evaluate_status(&monitors)` atau `LifecycleCoordinator::get_profile_status`.
    pub fn status(&self) -> ProfileStatus {
        ProfileStatus::DraftNotReady
    }

    /// Memperbarui monitor tujuan.
    pub fn set_target_monitor(&mut self, monitor: Option<MonitorIdentity>) {
        self.target_monitor = monitor;
    }

    /// Mengevaluasi status kecocokan monitor tujuan terhadap daftar monitor yang tersedia saat ini.
    pub fn evaluate_target_monitor(
        &self,
        available_monitors: &[MonitorDescriptor],
    ) -> TargetMonitorStatus {
        TargetMonitorStatus::evaluate(self.target_monitor.as_ref(), available_monitors)
    }

    /// Mengevaluasi status kesiapan profil berdasarkan ketersediaan monitor tujuan saat ini.
    pub fn evaluate_status(&self, available_monitors: &[MonitorDescriptor]) -> ProfileStatus {
        match self.evaluate_target_monitor(available_monitors) {
            TargetMonitorStatus::Unset => ProfileStatus::DraftNotReady,
            TargetMonitorStatus::Available(_) => ProfileStatus::Ready,
            TargetMonitorStatus::Unavailable { .. } => ProfileStatus::TargetUnavailable,
            TargetMonitorStatus::Ambiguous { .. } => ProfileStatus::TargetAmbiguous,
        }
    }

    /// Memperbarui nama profil.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Memperbarui jalur browser.
    pub fn set_browser_path(&mut self, path: PathBuf) {
        self.browser_path = path;
    }

    /// Memperbarui parameter kustom.
    pub fn set_custom_parameters(&mut self, params: impl Into<String>) {
        self.custom_parameters = params.into();
    }
}

/// Konfigurasi aplikasi Window Hop yang dipersistensikan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    schema_version: u32,
    profiles: Vec<Profile>,
}

impl AppConfig {
    /// Membuat konfigurasi baru dengan versi skema dan daftar profil.
    pub fn new(schema_version: u32, profiles: Vec<Profile>) -> Self {
        Self {
            schema_version,
            profiles,
        }
    }

    /// Mengambil versi skema konfigurasi.
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Mengambil referensi ke daftar profil.
    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    /// Mengambil referensi mutable ke daftar profil untuk modul internal.
    pub(crate) fn profiles_mut(&mut self) -> &mut Vec<Profile> {
        &mut self.profiles
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            profiles: Vec::new(),
        }
    }
}

/// Ringkasan status keseluruhan Window Hop untuk ditampilkan pada system tray dan indikator ringkas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayStatusSummary {
    /// Teks ringkas status (misal: "1 profil berjalan", "Menunggu monitor tujuan", "Siap").
    pub status_text: String,
    /// Tooltip lengkap untuk ikon system tray.
    pub tooltip: String,
    /// Jumlah profil yang sedang berjalan aktif.
    pub running_count: usize,
    /// Jumlah profil yang sedang menunggu monitor tujuan atau terputus.
    pub waiting_count: usize,
    /// Apakah terdapat kesalahan otomatis terakhir.
    pub has_error: bool,
    /// Pesan kesalahan terakhir jika ada.
    pub last_error: Option<String>,
}

impl TrayStatusSummary {
    /// Membuat instance baru TrayStatusSummary.
    pub fn new(
        status_text: impl Into<String>,
        tooltip: impl Into<String>,
        running_count: usize,
        waiting_count: usize,
        has_error: bool,
        last_error: Option<String>,
    ) -> Self {
        Self {
            status_text: status_text.into(),
            tooltip: tooltip.into(),
            running_count,
            waiting_count,
            has_error,
            last_error,
        }
    }

    /// Status siap default (idle).
    pub fn idle() -> Self {
        Self {
            status_text: "Siap (tidak ada profil aktif)".to_string(),
            tooltip: "Window Hop: Siap".to_string(),
            running_count: 0,
            waiting_count: 0,
            has_error: false,
            last_error: None,
        }
    }
}
