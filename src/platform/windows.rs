//! Windows display and monitor enumeration using Win32 APIs.

use crate::browsers::LaunchPlan;
use crate::domain::{MonitorDescriptor, MonitorIdentity, ProcessId};
use crate::platform::{ManagedProcess, MonitorProvider, PlatformError, ProcessLauncher};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW,
    DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICE_PRIMARY_DEVICE, HDC, HMONITOR,
    MONITORINFOEXW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, PostMessageW, SetWindowPos, SWP_NOACTIVATE,
    SWP_NOZORDER, WM_CLOSE,
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

// Callback enumerasi monitor Win32 untuk menemukan koordinat fisik monitor berdasarkan MonitorIdentity
unsafe extern "system" fn enum_monitors_for_rect(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let state = &mut *(lparam.0 as *mut (String, Option<RECT>));
    let mut info: MONITORINFOEXW = std::mem::zeroed();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    if GetMonitorInfoW(hmonitor, &mut info.monitorInfo as *mut _).as_bool() {
        let mut monitor_index = 0;
        loop {
            let mut monitor_device: DISPLAY_DEVICEW = std::mem::zeroed();
            monitor_device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
            let ok = EnumDisplayDevicesW(
                PCWSTR(info.szDevice.as_ptr()),
                monitor_index,
                &mut monitor_device,
                0,
            );
            if !ok.as_bool() {
                break;
            }
            monitor_index += 1;

            if (monitor_device.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP) != 0 {
                let id = utf16_slice_to_string(&monitor_device.DeviceID);
                if id.trim() == state.0.as_str() {
                    state.1 = Some(info.monitorInfo.rcMonitor);
                    return BOOL(0); // Selesai, monitor tujuan ditemukan
                }
            }
        }
    }
    BOOL(1)
}

/// Menemukan batas koordinat fisik monitor pada desktop berdasarkan identitas perangkat kerasnya.
fn find_monitor_rect(identity: &MonitorIdentity) -> Option<RECT> {
    let mut state = (identity.as_str().to_string(), None);
    // SAFETY: EnumDisplayMonitors dipanggil dengan callback aman `enum_monitors_for_rect`
    // dan pointer mutable valid ke `state`.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitors_for_rect),
            LPARAM(&mut state as *mut _ as isize),
        );
    }
    state.1
}

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

struct RepositionContext {
    target_pid: u32,
    rect: RECT,
    windows_moved: usize,
}

// Callback enumerasi jendela Win32 untuk menempatkan jendela milik target_pid pada koordinat fisik monitor tujuan
unsafe extern "system" fn enum_windows_reposition_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam.0 as *mut RepositionContext);
    let mut window_pid = 0u32;
    // SAFETY: GetWindowThreadProcessId dipanggil dengan pointer valid ke `window_pid`.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut window_pid)) };
    if window_pid == ctx.target_pid {
        let x = ctx.rect.left;
        let y = ctx.rect.top;
        let width = ctx.rect.right - ctx.rect.left;
        let height = ctx.rect.bottom - ctx.rect.top;

        // SAFETY: SetWindowPos dipanggil untuk memposisikan jendela milik target_pid ke koordinat
        // dan ukuran monitor tujuan fisik (tanpa SWP_NOMOVE).
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                None,
                x,
                y,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        };
        ctx.windows_moved += 1;
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

    fn reposition(&self, target_monitor: &MonitorDescriptor) -> Result<(), PlatformError> {
        let target_pid = self.pid.as_u32();
        let rect = find_monitor_rect(&target_monitor.identity).ok_or_else(|| {
            PlatformError::DeviceUnavailable(format!(
                "Monitor '{}' ({}) tidak ditemukan pada desktop",
                target_monitor.name,
                target_monitor.identity.as_str()
            ))
        })?;

        let mut ctx = RepositionContext {
            target_pid,
            rect,
            windows_moved: 0,
        };

        // SAFETY: EnumWindows memanggil `enum_windows_reposition_callback` dengan konteks pointer valid.
        unsafe {
            let _ = EnumWindows(
                Some(enum_windows_reposition_callback),
                LPARAM(&mut ctx as *mut _ as isize),
            );
        }

        if ctx.windows_moved == 0 {
            return Err(PlatformError::Os(format!(
                "Belum ada window aktif yang ditemukan untuk proses {}",
                target_pid
            )));
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

        // Tunggu hingga UI thread browser selesai membuat HWND melalui retry loop polling singkat
        let start_time = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(2000);
        let poll_interval = std::time::Duration::from_millis(100);

        while managed.is_alive() && start_time.elapsed() < timeout {
            if managed.reposition(target_monitor).is_ok() {
                break;
            }
            std::thread::sleep(poll_interval);
        }

        Ok(managed)
    }
}

/// Implementasi pembuat shortcut pada platform Windows.
pub struct WindowsShortcutCreator;

impl WindowsShortcutCreator {
    /// Membuat instance baru WindowsShortcutCreator.
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsShortcutCreator {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::platform::ShortcutCreator for WindowsShortcutCreator {
    fn default_desktop_dir(&self) -> Result<std::path::PathBuf, PlatformError> {
        if let Some(onedrive) = std::env::var_os("OneDrive") {
            let desktop = std::path::PathBuf::from(onedrive).join("Desktop");
            if desktop.exists() {
                return Ok(desktop);
            }
        }
        if let Some(userprofile) = std::env::var_os("USERPROFILE") {
            let desktop = std::path::PathBuf::from(userprofile).join("Desktop");
            return Ok(desktop);
        }
        Err(PlatformError::Os(
            "Tidak dapat menemukan direktori Desktop pengguna Windows".to_string(),
        ))
    }

    fn create_shortcut(
        &self,
        spec: &crate::platform::ShortcutSpec,
    ) -> Result<std::path::PathBuf, PlatformError> {
        std::fs::create_dir_all(&spec.target_dir).map_err(|e| {
            PlatformError::Os(format!("Gagal membuat folder tujuan shortcut: {}", e))
        })?;

        let shortcut_path = spec.shortcut_path();
        let app_exe_str = spec.app_executable.to_string_lossy().to_string();
        let args_str = spec.arguments.join(" ");
        let workdir = match spec.app_executable.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => std::env::current_dir().unwrap_or_else(|_| spec.target_dir.clone()),
        };

        // Menggunakan script block berparameter (& { param($lnk, $target, $arg, $dir) ... })
        // agar parameter posisi terikat dengan benar dan bebas dari masalah shell injection atau kutip/spasi.
        let ps_script = "& { param($lnk, $target, $arg, $dir) $ws = New-Object -ComObject WScript.Shell; $s = $ws.CreateShortcut($lnk); $s.TargetPath = $target; $s.Arguments = $arg; $s.WorkingDirectory = $dir; $s.Save() }";

        let output = std::process::Command::new("powershell.exe")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-Command")
            .arg(ps_script)
            .arg(shortcut_path.to_string_lossy().as_ref())
            .arg(&app_exe_str)
            .arg(&args_str)
            .arg(workdir.to_string_lossy().as_ref())
            .output()
            .map_err(|e| {
                PlatformError::Os(format!(
                    "Gagal menjalankan powershell untuk membuat shortcut: {}",
                    e
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PlatformError::Os(format!(
                "Pembuatan shortcut gagal: {}",
                stderr
            )));
        }

        Ok(shortcut_path)
    }
}
