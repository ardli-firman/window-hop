//! Platform abstraction boundaries for OS display, process lifecycle, and integration.

use crate::browsers::LaunchPlan;
use crate::domain::{MonitorDescriptor, ProcessId, ProfileId};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
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
    fail_force_stop: AtomicBool,
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
            fail_force_stop: AtomicBool::new(false),
            exit_reason: Mutex::new(None),
        }
    }

    /// Mengatur apakah proses menolak penutupan anggun (misal jendela konfirmasi dialog yang macet).
    pub fn set_refuse_graceful(&self, refuse: bool) {
        self.refuse_graceful.store(refuse, Ordering::SeqCst);
    }

    /// Mengatur apakah proses gagal dihentikan secara paksa (misal kendala hak akses OS).
    pub fn set_fail_force_stop(&self, fail: bool) {
        self.fail_force_stop.store(fail, Ordering::SeqCst);
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
        if self.fail_force_stop.load(Ordering::SeqCst) {
            return Err(PlatformError::Os(
                "Akses ditolak saat mencoba menghentikan proses".to_string(),
            ));
        }
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

/// Flag argumen baris perintah untuk aktivasi profil langsung lewat shortcut.
pub const ACTIVATE_PROFILE_FLAG: &str = "--activate-profile";

/// Membaca flag `--activate-profile <PROFILE_ID>` dari argumen baris perintah jika ada.
pub fn parse_activation_flag<I>(args: I) -> Option<ProfileId>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == ACTIVATE_PROFILE_FLAG && i + 1 < args.len() {
            return Some(ProfileId::new(&args[i + 1]));
        }
        i += 1;
    }
    None
}

/// Spesifikasi pembuatan shortcut profil Window Hop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutSpec {
    pub profile_id: ProfileId,
    pub profile_name: String,
    pub target_dir: PathBuf,
    pub app_executable: PathBuf,
    pub arguments: Vec<String>,
}

impl ShortcutSpec {
    /// Menghapus atau mengganti karakter ilegal pada nama file shortcut untuk platform Windows.
    pub fn sanitize_name(name: &str) -> String {
        let sanitized: String = name
            .chars()
            .map(|c| match c {
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
                other => other,
            })
            .collect();
        let trimmed = sanitized.trim();
        if trimmed.is_empty() {
            "Profil Window Hop".to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Menghasilkan nama file shortcut berekstensi .lnk yang aman dari karakter ilegal.
    pub fn shortcut_filename(&self) -> String {
        format!("{}.lnk", Self::sanitize_name(&self.profile_name))
    }

    /// Menghasilkan path lengkap file shortcut pada target_dir.
    pub fn shortcut_path(&self) -> PathBuf {
        self.target_dir.join(self.shortcut_filename())
    }
}

/// Menghapus atau mengganti karakter ilegal pada nama file shortcut untuk platform Windows.
pub fn sanitize_shortcut_filename(name: &str) -> String {
    ShortcutSpec::sanitize_name(name)
}

/// Seam antarmuka pembuatan shortcut pada platform OS.
pub trait ShortcutCreator: Send + Sync {
    /// Mengambil path folder Desktop default pengguna.
    fn default_desktop_dir(&self) -> Result<PathBuf, PlatformError>;

    /// Membuat file shortcut berdasarkan spesifikasi yang diberikan.
    /// Mengembalikan path file shortcut yang berhasil dibuat.
    fn create_shortcut(&self, spec: &ShortcutSpec) -> Result<PathBuf, PlatformError>;
}

/// Implementasi tiruan (mock) dari ShortcutCreator untuk pengujian unit deterministik.
pub struct MockShortcutCreator {
    default_desktop: PathBuf,
    created_shortcuts: Mutex<Vec<ShortcutSpec>>,
    error: Mutex<Option<String>>,
}

impl MockShortcutCreator {
    pub fn new(default_desktop: PathBuf) -> Self {
        Self {
            default_desktop,
            created_shortcuts: Mutex::new(Vec::new()),
            error: Mutex::new(None),
        }
    }

    pub fn set_error(&self, error: Option<String>) {
        *self.error.lock().unwrap() = error;
    }

    pub fn created_shortcuts(&self) -> Vec<ShortcutSpec> {
        self.created_shortcuts.lock().unwrap().clone()
    }
}

impl ShortcutCreator for MockShortcutCreator {
    fn default_desktop_dir(&self) -> Result<PathBuf, PlatformError> {
        if let Some(ref err) = *self.error.lock().unwrap() {
            return Err(PlatformError::Os(err.clone()));
        }
        Ok(self.default_desktop.clone())
    }

    fn create_shortcut(&self, spec: &ShortcutSpec) -> Result<PathBuf, PlatformError> {
        if let Some(ref err) = *self.error.lock().unwrap() {
            return Err(PlatformError::Os(err.clone()));
        }
        self.created_shortcuts.lock().unwrap().push(spec.clone());
        Ok(spec.shortcut_path())
    }
}

/// Perintah aktivasi antar-proses (IPC) Window Hop.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ActivationCommand {
    ActivateProfile(ProfileId),
}

/// Seam antarmuka koordinasi single-instance dan transmisi aktivasi antar-proses.
/// Handler fungsi penerima perintah aktivasi IPC.
pub type ActivationCommandHandler = Arc<dyn Fn(ActivationCommand) + Send + Sync>;

pub trait SingleInstanceCoordinator: Send + Sync {
    /// Mencoba mengirim perintah aktivasi ke primary instance yang sedang berjalan.
    /// Mengembalikan Ok(true) jika berhasil terkirim dan direspons.
    /// Mengembalikan Ok(false) jika belum ada instance yang berjalan.
    fn send_activation(&self, command: &ActivationCommand) -> Result<bool, PlatformError>;

    /// Memulai listener di background untuk menerima pesan aktivasi dari instance lain.
    fn start_listener(
        &self,
        on_command: Box<dyn Fn(ActivationCommand) + Send + Sync>,
    ) -> Result<(), PlatformError>;
}

/// Implementasi tiruan (mock) dari SingleInstanceCoordinator untuk pengujian deterministik.
pub struct MockSingleInstanceCoordinator {
    listener: Mutex<Option<ActivationCommandHandler>>,
}

impl MockSingleInstanceCoordinator {
    pub fn new() -> Self {
        Self {
            listener: Mutex::new(None),
        }
    }
}

impl Default for MockSingleInstanceCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl SingleInstanceCoordinator for MockSingleInstanceCoordinator {
    fn send_activation(&self, command: &ActivationCommand) -> Result<bool, PlatformError> {
        let guard = self.listener.lock().unwrap();
        if let Some(ref callback) = *guard {
            callback(command.clone());
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn start_listener(
        &self,
        on_command: Box<dyn Fn(ActivationCommand) + Send + Sync>,
    ) -> Result<(), PlatformError> {
        let mut guard = self.listener.lock().unwrap();
        *guard = Some(Arc::from(on_command));
        Ok(())
    }
}

/// Implementasi SingleInstanceCoordinator menggunakan loopback socket TCP lokal dan file port.
/// Bekerja secara portabel di Windows 10/11 maupun Linux tanpa memerlukan library eksternal.
pub struct LocalSingleInstanceCoordinator {
    base_data_dir: PathBuf,
}

impl LocalSingleInstanceCoordinator {
    pub fn new(base_data_dir: PathBuf) -> Self {
        Self { base_data_dir }
    }

    fn port_file_path(&self) -> PathBuf {
        self.base_data_dir.join("window_hop_ipc.port")
    }

    fn lock_file_path(&self) -> PathBuf {
        self.base_data_dir.join("window_hop_ipc.lock")
    }
}

impl SingleInstanceCoordinator for LocalSingleInstanceCoordinator {
    fn send_activation(&self, command: &ActivationCommand) -> Result<bool, PlatformError> {
        let port_file = self.port_file_path();
        let lock_file = self.lock_file_path();

        // Jika file port belum ada namun file lock ada (primary sedang dalam proses inisialisasi / startup race),
        // tunggu beberapa saat agar listener primary selesai melakukan bind dan menulis port.
        let start_wait = Instant::now();
        while !port_file.exists()
            && lock_file.exists()
            && start_wait.elapsed() < Duration::from_millis(1500)
        {
            std::thread::sleep(Duration::from_millis(50));
        }

        if !port_file.exists() {
            return Ok(false);
        }

        let port_str = match std::fs::read_to_string(&port_file) {
            Ok(s) => s,
            Err(_) => return Ok(false),
        };

        let port: u16 = match port_str.trim().parse() {
            Ok(p) => p,
            Err(_) => return Ok(false),
        };

        let addr = format!("127.0.0.1:{}", port);
        let socket_addr = match addr.parse() {
            Ok(sa) => sa,
            Err(e) => return Err(PlatformError::Os(format!("Alamat IP tidak valid: {}", e))),
        };

        let stream = match TcpStream::connect_timeout(&socket_addr, Duration::from_millis(500)) {
            Ok(s) => s,
            Err(_) => {
                let _ = std::fs::remove_file(&port_file);
                let _ = std::fs::remove_file(&lock_file);
                return Ok(false);
            }
        };

        let _ = stream.set_read_timeout(Some(Duration::from_millis(1000)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(1000)));

        let mut writer = match stream.try_clone() {
            Ok(w) => w,
            Err(e) => return Err(PlatformError::Os(e.to_string())),
        };

        let json_data = serde_json::to_string(command)
            .map_err(|e| PlatformError::Os(format!("Gagal serialisasi perintah IPC: {}", e)))?;

        writer
            .write_all(format!("{}\n", json_data).as_bytes())
            .map_err(|e| PlatformError::Os(format!("Gagal mengirim pesan IPC: {}", e)))?;
        writer
            .flush()
            .map_err(|e| PlatformError::Os(e.to_string()))?;

        let mut reader = BufReader::new(stream);
        let mut response = String::new();
        match reader.read_line(&mut response) {
            Ok(_) if response.trim() == "OK" => Ok(true),
            _ => Ok(true),
        }
    }

    fn start_listener(
        &self,
        on_command: Box<dyn Fn(ActivationCommand) + Send + Sync>,
    ) -> Result<(), PlatformError> {
        let _ = std::fs::create_dir_all(&self.base_data_dir);
        let lock_file = self.lock_file_path();
        let _ = std::fs::write(&lock_file, std::process::id().to_string());

        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| PlatformError::Os(format!("Gagal bind IPC listener: {}", e)))?;

        let port = listener
            .local_addr()
            .map_err(|e| PlatformError::Os(e.to_string()))?
            .port();

        let port_file = self.port_file_path();
        std::fs::write(&port_file, port.to_string())
            .map_err(|e| PlatformError::Os(format!("Gagal menulis file port IPC: {}", e)))?;

        let port_file_cleanup = port_file.clone();
        let lock_file_cleanup = lock_file.clone();
        let on_command: Arc<dyn Fn(ActivationCommand) + Send + Sync> = Arc::from(on_command);

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(mut stream) => {
                        let on_command_clone = on_command.clone();
                        std::thread::spawn(move || {
                            let _ = stream.set_read_timeout(Some(Duration::from_millis(2000)));
                            if let Ok(stream_clone) = stream.try_clone() {
                                let mut reader = BufReader::new(stream_clone);
                                let mut line = String::new();
                                if reader.read_line(&mut line).is_ok() {
                                    if let Ok(cmd) =
                                        serde_json::from_str::<ActivationCommand>(line.trim())
                                    {
                                        on_command_clone(cmd);
                                        let _ = stream.write_all(b"OK\n");
                                        let _ = stream.flush();
                                    }
                                }
                            }
                        });
                    }
                    Err(_) => break,
                }
            }
            let _ = std::fs::remove_file(&port_file_cleanup);
            let _ = std::fs::remove_file(&lock_file_cleanup);
        });

        Ok(())
    }
}

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::{WindowsMonitorProvider, WindowsProcessLauncher, WindowsShortcutCreator};

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

#[cfg(not(windows))]
/// Substitut pembuat shortcut untuk lingkungan non-Windows yang melaporkan kapabilitas tidak tersedia.
pub struct NonWindowsShortcutCreator;

#[cfg(not(windows))]
impl ShortcutCreator for NonWindowsShortcutCreator {
    fn default_desktop_dir(&self) -> Result<PathBuf, PlatformError> {
        Err(PlatformError::Unsupported(
            "Deteksi direktori Desktop native hanya tersedia pada platform Windows.".to_string(),
        ))
    }

    fn create_shortcut(&self, _spec: &ShortcutSpec) -> Result<PathBuf, PlatformError> {
        Err(PlatformError::Unsupported(
            "Pembuatan shortcut desktop native hanya tersedia pada platform Windows.".to_string(),
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

/// Mengembalikan pembuat shortcut default untuk platform saat ini.
pub fn default_shortcut_creator() -> Arc<dyn ShortcutCreator> {
    #[cfg(windows)]
    {
        Arc::new(WindowsShortcutCreator::new())
    }

    #[cfg(not(windows))]
    {
        Arc::new(NonWindowsShortcutCreator)
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
