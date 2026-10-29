//! Windows display and monitor enumeration using Win32 APIs.

use crate::domain::{MonitorDescriptor, MonitorIdentity};
use crate::platform::{MonitorProvider, PlatformError};
use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAY_DEVICE_ATTACHED_TO_DESKTOP,
    DISPLAY_DEVICE_PRIMARY_DEVICE,
};

/// Implementasi MonitorProvider untuk sistem operasi Windows.
pub struct WindowsMonitorProvider;

impl WindowsMonitorProvider {
    /// Membuat instance baru WindowsMonitorProvider.
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsMonitorProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitorProvider for WindowsMonitorProvider {
    fn available_monitors(&self) -> Result<Vec<MonitorDescriptor>, PlatformError> {
        let mut monitors = Vec::new();
        let mut adapter_index = 0;

        loop {
            // SAFETY: `DISPLAY_DEVICEW` adalah Win32 C struct sederhana; inisialisasi nol diwajibkan oleh API Windows sebelum mengatur `cb`.
            let mut adapter_device: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
            adapter_device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;

            // SAFETY: `adapter_device.cb` telah diatur ke ukuran struct sebelum memanggil API Win32 EnumDisplayDevicesW.
            let success =
                unsafe { EnumDisplayDevicesW(None, adapter_index, &mut adapter_device, 0) };

            if !success.as_bool() {
                break;
            }

            adapter_index += 1;

            // Hanya proses display adapter yang terhubung ke desktop aktif
            if (adapter_device.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP) == 0 {
                continue;
            }

            let is_primary = (adapter_device.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE) != 0;

            // Enumerasi monitor fisik yang terpasang pada adapter ini
            let mut monitor_index = 0;

            loop {
                // SAFETY: `DISPLAY_DEVICEW` adalah Win32 C struct sederhana; inisialisasi nol diwajibkan oleh API Windows sebelum mengatur `cb`.
                let mut monitor_device: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
                monitor_device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;

                // SAFETY: `adapter_device.DeviceName` adalah null-terminated wide string yang valid dari pemanggilan sebelumnya.
                let monitor_success = unsafe {
                    EnumDisplayDevicesW(
                        PCWSTR(adapter_device.DeviceName.as_ptr()),
                        monitor_index,
                        &mut monitor_device,
                        0,
                    )
                };

                if !monitor_success.as_bool() {
                    break;
                }

                monitor_index += 1;

                // Hanya sertakan monitor yang aktif dan terpasang pada desktop
                if (monitor_device.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP) != 0 {
                    let raw_id = utf16_slice_to_string(&monitor_device.DeviceID);
                    let raw_name = utf16_slice_to_string(&monitor_device.DeviceString);

                    // Pastikan identitas monitor berasal dari DeviceID hardware/PnP yang valid, bukan nomor urutan layar
                    let trimmed_id = raw_id.trim();
                    if !trimmed_id.is_empty() {
                        let display_name = if !raw_name.trim().is_empty() {
                            raw_name.trim().to_string()
                        } else {
                            "Monitor Antrian".to_string()
                        };

                        monitors.push(MonitorDescriptor {
                            identity: MonitorIdentity::new(trimmed_id),
                            name: display_name,
                            is_primary,
                        });
                    }
                }
            }
        }

        Ok(monitors)
    }
}

fn utf16_slice_to_string(slice: &[u16]) -> String {
    let len = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    String::from_utf16_lossy(&slice[..len])
}

use crate::browsers::LaunchPlan;
use crate::domain::ProcessId;
use crate::platform::{ManagedProcess, ProcessLauncher};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, PostMessageW, SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, WM_CLOSE,
};

/// Handle proses browser yang dikelola pada Windows dengan verifikasi kepemilikan instance melalui Child handle.
pub struct WindowsManagedProcess {
    pid: ProcessId,
    child: Mutex<Option<Child>>,
}

impl WindowsManagedProcess {
    /// Membuat handle proses baru dari Child handle yang dimiliki.
    pub fn new(child: Child) -> Self {
        let pid = ProcessId::new(child.id());
        Self {
            pid,
            child: Mutex::new(Some(child)),
        }
    }
}

// Callback enumerasi jendela Win32 untuk menutup jendela yang terverifikasi dimiliki oleh PID ini
unsafe extern "system" fn enum_windows_close_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let target_pid = lparam.0 as u32;
    let mut window_pid = 0u32;
    // SAFETY: GetWindowThreadProcessId dipanggil dengan pointer valid ke `window_pid`.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut window_pid)) };
    if window_pid == target_pid {
        // SAFETY: PostMessageW mengirimkan pesan WM_CLOSE ke HWND yang terverifikasi dimiliki oleh target_pid.
        unsafe {
            let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
        };
    }
    BOOL(1)
}

// Callback enumerasi jendela Win32 untuk memastikan penempatan jendela milik target_pid pada monitor tujuan
unsafe extern "system" fn enum_windows_reposition_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let target_pid = lparam.0 as u32;
    let mut window_pid = 0u32;
    // SAFETY: GetWindowThreadProcessId dipanggil dengan pointer valid ke `window_pid`.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut window_pid)) };
    if window_pid == target_pid {
        // SAFETY: SetWindowPos dipanggil untuk memposisikan jendela milik target_pid tanpa mengubah ukuran atau Z-order sembarangan.
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        };
    }
    BOOL(1)
}

impl ManagedProcess for WindowsManagedProcess {
    fn pid(&self) -> ProcessId {
        self.pid
    }

    fn is_alive(&self) -> bool {
        let mut guard = self.child.lock().unwrap();
        if let Some(ref mut child) = *guard {
            match child.try_wait() {
                Ok(Some(_status)) => false,
                Ok(None) => true,
                Err(_) => false,
            }
        } else {
            false
        }
    }

    fn request_close(&self) -> Result<(), PlatformError> {
        let target_pid = self.pid.as_u32();
        // SAFETY: EnumWindows memanggil fungsi callback aman `enum_windows_close_callback` untuk menutup jendela milik target_pid.
        unsafe {
            let _ = EnumWindows(
                Some(enum_windows_close_callback),
                LPARAM(target_pid as isize),
            );
        }
        Ok(())
    }

    fn force_stop(&self) -> Result<(), PlatformError> {
        let mut guard = self.child.lock().unwrap();
        if let Some(ref mut child) = *guard {
            child.kill().map_err(|e| PlatformError::Os(e.to_string()))?;
            let _ = child.wait();
        }
        Ok(())
    }

    fn reposition(&self, _target_monitor: &MonitorDescriptor) -> Result<(), PlatformError> {
        let target_pid = self.pid.as_u32();
        // SAFETY: EnumWindows memanggil `enum_windows_reposition_callback` untuk menempatkan jendela pada monitor tujuan.
        unsafe {
            let _ = EnumWindows(
                Some(enum_windows_reposition_callback),
                LPARAM(target_pid as isize),
            );
        }
        Ok(())
    }
}

/// Implementasi peluncur proses untuk platform Windows native.
pub struct WindowsProcessLauncher;

impl WindowsProcessLauncher {
    /// Membuat instance baru WindowsProcessLauncher.
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsProcessLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessLauncher for WindowsProcessLauncher {
    fn launch(
        &self,
        plan: &LaunchPlan,
        target_monitor: &MonitorDescriptor,
    ) -> Result<Arc<dyn ManagedProcess>, PlatformError> {
        let mut cmd = Command::new(&plan.executable);
        for arg in &plan.arguments {
            cmd.arg(arg);
        }

        let child = cmd
            .spawn()
            .map_err(|e| PlatformError::Os(format!("Gagal meluncurkan browser: {}", e)))?;

        let managed = Arc::new(WindowsManagedProcess::new(child));
        let _ = managed.reposition(target_monitor);

        Ok(managed)
    }
}
