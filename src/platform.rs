//! Platform abstraction boundaries for OS display and integration.

use crate::domain::MonitorDescriptor;
use std::sync::{Arc, Mutex};

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

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::WindowsMonitorProvider;

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
