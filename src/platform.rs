//! Platform abstraction boundaries for OS display, process lifecycle, and integration.

use crate::browsers::LaunchPlan;
use crate::domain::{MonitorDescriptor, ProcessId};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Kesalahan pada operasi platform OS.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("Kesalahan platform OS: {0}")]
    Os(String),

    #[error("Operasi platform tidak didukung pada sistem ini: {0}")]
    Unsupported(String),
}

/// Seam antarmuka penyedia informasi monitor/display dari sistem operasi.
pub trait MonitorProvider: Send + Sync {
    /// Mengambil daftar semua monitor yang terhubung dan aktif saat ini.
    fn available_monitors(&self) -> Result<Vec<MonitorDescriptor>, PlatformError>;
}

/// Implementasi mock in-memory untuk pengujian monitor tanpa bergantung pada hardware fisik.
pub struct MockMonitorProvider {
    monitors: Mutex<Vec<MonitorDescriptor>>,
    error: Mutex<Option<String>>,
}

impl MockMonitorProvider {
    /// Membuat instance baru MockMonitorProvider dengan daftar monitor awal.
    pub fn new(monitors: Vec<MonitorDescriptor>) -> Self {
        Self {
            monitors: Mutex::new(monitors),
            error: Mutex::new(None),
        }
    }

    /// Mengubah daftar monitor yang tersedia (misal simulasi cabut/pasang atau perubahan urutan).
    pub fn set_monitors(&self, monitors: Vec<MonitorDescriptor>) {
        let mut guard = self.monitors.lock().unwrap();
        *guard = monitors;
    }

    /// Mengatur simulasi error OS pada pemanggilan enumerasi berikutnya.
    pub fn set_error(&self, error: Option<String>) {
        let mut guard = self.error.lock().unwrap();
        *guard = error;
    }
}

impl Default for MockMonitorProvider {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl MonitorProvider for MockMonitorProvider {
    fn available_monitors(&self) -> Result<Vec<MonitorDescriptor>, PlatformError> {
        let error_guard = self.error.lock().unwrap();
        if let Some(ref err_msg) = *error_guard {
            return Err(PlatformError::Os(err_msg.clone()));
        }

        let guard = self.monitors.lock().unwrap();
        Ok(guard.clone())
    }
}

/// Seam antarmuka abstraksi waktu untuk pengujian deterministik tanpa sleep nyata.
pub trait Clock: Send + Sync {
    /// Waktu saat ini menurut clock.
    fn now(&self) -> Instant;

    /// Jeda tidur selama durasi tertentu (pada mock, langsung memajukan waktu).
    fn sleep(&self, duration: Duration);
}

/// Jam berbasis waktu sistem operasi untuk lingkungan produksi.
#[derive(Debug, Default)]
pub struct SystemClock;

impl SystemClock {
    pub fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// Jam tiruan (mock) yang waktunya dapat dimajukan secara terkontrol dalam pengujian tanpa sleep nyata.
pub struct MockClock {
    current: Mutex<Instant>,
}

impl MockClock {
    pub fn new() -> Self {
        Self {
            current: Mutex::new(Instant::now()),
        }
    }

    /// Memajukan waktu simulasi.
    pub fn advance(&self, duration: Duration) {
        let mut guard = self.current.lock().unwrap();
        *guard += duration;
    }
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MockClock {
    fn now(&self) -> Instant {
        *self.current.lock().unwrap()
    }

    fn sleep(&self, duration: Duration) {
        self.advance(duration);
    }
}

/// Handle proses yang dikelola oleh Window Hop.
pub trait ManagedProcess: Send + Sync {
    /// Mengambil PID proses.
    fn pid(&self) -> ProcessId;

    /// Memeriksa apakah proses masih hidup.
    fn is_alive(&self) -> bool;

    /// Mengirim sinyal permintaan penutupan anggun (graceful close).
    fn request_close(&self) -> Result<(), PlatformError>;

    /// Menghentikan proses secara paksa (force kill).
    fn force_stop(&self) -> Result<(), PlatformError>;

    /// Memindahkan atau memastikan penempatan jendela pada monitor tujuan.
    fn reposition(&self, target_monitor: &MonitorDescriptor) -> Result<(), PlatformError>;

    /// Mengambil alasan penghentian proses jika diketahui (misal crash atau ditutup manual).
    fn exit_reason(&self) -> Option<crate::domain::ExitReason> {
        None
    }
}

/// Seam antarmuka peluncuran proses terisolasi.
pub trait ProcessLauncher: Send + Sync {
    /// Meluncurkan proses browser dengan rencana peluncuran terisolasi dan menempatkannya di monitor tujuan.
    fn launch(
        &self,
        plan: &LaunchPlan,
        target_monitor: &MonitorDescriptor,
    ) -> Result<Arc<dyn ManagedProcess>, PlatformError>;
}

/// Implementasi tiruan (mock) dari proses yang dikelola untuk pengujian tanpa proses OS nyata.
pub struct MockManagedProcess {
    pid: ProcessId,
    plan: LaunchPlan,
    current_monitor: Mutex<MonitorDescriptor>,
    is_alive: AtomicBool,
    close_requested: AtomicBool,
    force_stopped: AtomicBool,
    reposition_count: AtomicUsize,
    refuse_graceful: AtomicBool,
    exit_reason: Mutex<Option<crate::domain::ExitReason>>,
}

impl MockManagedProcess {
    pub fn new(pid: ProcessId, plan: LaunchPlan, target_monitor: MonitorDescriptor) -> Self {
        Self {
            pid,
            plan,
            current_monitor: Mutex::new(target_monitor),
            is_alive: AtomicBool::new(true),
            close_requested: AtomicBool::new(false),
            force_stopped: AtomicBool::new(false),
            reposition_count: AtomicUsize::new(0),
            refuse_graceful: AtomicBool::new(false),
            exit_reason: Mutex::new(None),
        }
    }

    /// Mengatur apakah proses menolak penutupan anggun (misal jendela konfirmasi dialog yang macet).
    pub fn set_refuse_graceful(&self, refuse: bool) {
        self.refuse_graceful.store(refuse, Ordering::SeqCst);
    }

    /// Mengambil rencana peluncuran proses ini.
    pub fn plan(&self) -> &LaunchPlan {
        &self.plan
    }

    /// Mengambil monitor saat ini tempat proses berada.
    pub fn current_monitor(&self) -> MonitorDescriptor {
        self.current_monitor.lock().unwrap().clone()
    }

    /// Memeriksa apakah penutupan normal telah diminta.
    pub fn close_requested(&self) -> bool {
        self.close_requested.load(Ordering::SeqCst)
    }

    /// Memeriksa apakah penutupan paksa telah dilakukan.
    pub fn force_stopped(&self) -> bool {
        self.force_stopped.load(Ordering::SeqCst)
    }

    /// Mengambil jumlah pemanggilan reposisi.
    pub fn reposition_count(&self) -> usize {
        self.reposition_count.load(Ordering::SeqCst)
    }

    /// Menyimulasikan proses keluar karena crash.
    pub fn simulate_crash(&self) {
        *self.exit_reason.lock().unwrap() = Some(crate::domain::ExitReason::Crash);
        self.is_alive.store(false, Ordering::SeqCst);
    }

    /// Menyimulasikan proses keluar karena ditutup manual oleh pengguna.
    pub fn simulate_manual_close(&self) {
        *self.exit_reason.lock().unwrap() = Some(crate::domain::ExitReason::ManualClose);
        self.is_alive.store(false, Ordering::SeqCst);
    }

    /// Menyimulasikan proses keluar secara mandiri.
    pub fn simulate_exit(&self) {
        self.simulate_manual_close();
    }
}

impl ManagedProcess for MockManagedProcess {
    fn pid(&self) -> ProcessId {
        self.pid
    }

    fn is_alive(&self) -> bool {
        self.is_alive.load(Ordering::SeqCst)
    }

    fn request_close(&self) -> Result<(), PlatformError> {
        self.close_requested.store(true, Ordering::SeqCst);
        if !self.refuse_graceful.load(Ordering::SeqCst) {
            self.is_alive.store(false, Ordering::SeqCst);
        }
        Ok(())
    }

    fn force_stop(&self) -> Result<(), PlatformError> {
        self.force_stopped.store(true, Ordering::SeqCst);
        self.is_alive.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn reposition(&self, target_monitor: &MonitorDescriptor) -> Result<(), PlatformError> {
        let mut guard = self.current_monitor.lock().unwrap();
        *guard = target_monitor.clone();
        self.reposition_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn exit_reason(&self) -> Option<crate::domain::ExitReason> {
        *self.exit_reason.lock().unwrap()
    }
}

/// Implementasi tiruan (mock) dari ProcessLauncher untuk pengujian unit dan koordinasi siklus hidup.
pub struct MockProcessLauncher {
    next_pid: AtomicU32,
    launched_processes: Mutex<Vec<Arc<MockManagedProcess>>>,
    launch_error: Mutex<Option<String>>,
}

impl MockProcessLauncher {
    pub fn new() -> Self {
        Self {
            next_pid: AtomicU32::new(1001),
            launched_processes: Mutex::new(Vec::new()),
            launch_error: Mutex::new(None),
        }
    }

    pub fn set_launch_error(&self, error: Option<String>) {
        let mut guard = self.launch_error.lock().unwrap();
        *guard = error;
    }

    pub fn launched_processes(&self) -> Vec<Arc<MockManagedProcess>> {
        self.launched_processes.lock().unwrap().clone()
    }
}

impl Default for MockProcessLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessLauncher for MockProcessLauncher {
    fn launch(
        &self,
        plan: &LaunchPlan,
        target_monitor: &MonitorDescriptor,
    ) -> Result<Arc<dyn ManagedProcess>, PlatformError> {
        if let Some(ref err) = *self.launch_error.lock().unwrap() {
            return Err(PlatformError::Os(err.clone()));
        }

        let pid = ProcessId::new(self.next_pid.fetch_add(1, Ordering::SeqCst));
        let process = Arc::new(MockManagedProcess::new(
            pid,
            plan.clone(),
            target_monitor.clone(),
        ));
        self.launched_processes
            .lock()
            .unwrap()
            .push(process.clone());
        Ok(process)
    }
}

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::{WindowsMonitorProvider, WindowsProcessLauncher};

#[cfg(not(windows))]
/// Substitut monitor provider untuk lingkungan non-Windows yang melaporkan kapabilitas tidak tersedia.
pub struct NonWindowsMonitorProvider;

#[cfg(not(windows))]
impl MonitorProvider for NonWindowsMonitorProvider {
    fn available_monitors(&self) -> Result<Vec<MonitorDescriptor>, PlatformError> {
        Err(PlatformError::Unsupported(
            "Deteksi monitor hardware native hanya tersedia pada platform Windows.".to_string(),
        ))
    }
}

#[cfg(not(windows))]
/// Substitut peluncur proses untuk lingkungan non-Windows yang melaporkan kapabilitas tidak tersedia.
pub struct NonWindowsProcessLauncher;

#[cfg(not(windows))]
impl ProcessLauncher for NonWindowsProcessLauncher {
    fn launch(
        &self,
        _plan: &LaunchPlan,
        _target_monitor: &MonitorDescriptor,
    ) -> Result<Arc<dyn ManagedProcess>, PlatformError> {
        Err(PlatformError::Unsupported(
            "Peluncuran dan manajemen proses browser native hanya tersedia pada platform Windows."
                .to_string(),
        ))
    }
}

/// Mengembalikan penyedia monitor default untuk platform saat ini.
pub fn default_monitor_provider() -> Arc<dyn MonitorProvider> {
    #[cfg(windows)]
    {
        Arc::new(WindowsMonitorProvider::new())
    }

    #[cfg(not(windows))]
    {
        Arc::new(NonWindowsMonitorProvider)
    }
}

/// Mengembalikan peluncur proses default untuk platform saat ini.
pub fn default_process_launcher() -> Arc<dyn ProcessLauncher> {
    #[cfg(windows)]
    {
        Arc::new(WindowsProcessLauncher::new())
    }

    #[cfg(not(windows))]
    {
        Arc::new(NonWindowsProcessLauncher)
    }
}

/// Mengembalikan jam default sistem.
pub fn default_clock() -> Arc<dyn Clock> {
    Arc::new(SystemClock::new())
}
