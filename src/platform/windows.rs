//! Windows display and monitor enumeration using Win32 APIs.

use crate::browsers::{read_executable_architecture, LaunchPlan};
use crate::domain::{MonitorDescriptor, MonitorIdentity, ProcessId, TrayStatusSummary};
use crate::platform::{
    BrowserDetector, DetectedBrowser, ManagedProcess, MonitorProvider, PlatformError,
    ProcessLauncher, StartupManager, TrayAction, TrayActionHandler, TrayProvider,
};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use windows::core::{BOOL, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW,
    DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICE_PRIMARY_DEVICE, HDC, HMONITOR,
    MONITORINFOEXW,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, EnumWindows, GetCursorPos, GetMessageW, GetWindowThreadProcessId, LoadIconW,
    PostMessageW, PostQuitMessage, RegisterClassW, SetForegroundWindow, SetWindowPos,
    TrackPopupMenu, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDI_APPLICATION,
    MF_DISABLED, MF_SEPARATOR, MF_STRING, SWP_NOACTIVATE, SWP_NOZORDER, TPM_NONOTIFY,
    TPM_RETURNCMD, WM_APP, WM_CLOSE, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP,
    WNDCLASSW, WS_OVERLAPPEDWINDOW,
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
            if !adapter_device
                .StateFlags
                .contains(DISPLAY_DEVICE_ATTACHED_TO_DESKTOP)
            {
                continue;
            }

            let is_primary = adapter_device
                .StateFlags
                .contains(DISPLAY_DEVICE_PRIMARY_DEVICE);

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
                if monitor_device
                    .StateFlags
                    .contains(DISPLAY_DEVICE_ATTACHED_TO_DESKTOP)
                {
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

            if monitor_device
                .StateFlags
                .contains(DISPLAY_DEVICE_ATTACHED_TO_DESKTOP)
            {
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
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
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
            PlatformError::Os(format!(
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

/// Membuat file shortcut (.lnk) Windows secara native menggunakan Win32 COM API (IShellLinkW & IPersistFile).
/// Tidak memanggil external script host sehingga bebas dari alarm AMSI dan antivirus.
pub fn create_windows_shortcut(
    shortcut_path: &std::path::Path,
    target_exe: &std::path::Path,
    args: &str,
    working_dir: Option<&std::path::Path>,
) -> Result<(), PlatformError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{Interface, HSTRING, PCWSTR};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    if let Some(parent) = shortcut_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            PlatformError::Os(format!("Gagal membuat folder tujuan shortcut: {}", e))
        })?;
    }

    // SAFETY: Inisialisasi COM apartemen pada thread pemanggil.
    let coinit = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let need_uninit = coinit.is_ok();

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: CoUninitialize dipanggil hanya jika CoInitializeEx berhasil pada thread ini.
                unsafe { CoUninitialize() };
            }
        }
    }
    let _guard = ComGuard(need_uninit);

    // SAFETY: CoCreateInstance membuat objek ShellLink native in-process.
    let shell_link: IShellLinkW = unsafe {
        CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| {
            PlatformError::Os(format!("Gagal membuat instance COM ShellLink: {}", e))
        })?
    };

    let target_str = target_exe.to_string_lossy();
    let target_h = HSTRING::from(target_str.as_ref());
    // SAFETY: Mengatur jalur executable target shortcut.
    unsafe {
        shell_link.SetPath(&target_h).map_err(|e| {
            PlatformError::Os(format!("Gagal mengatur target executable shortcut: {}", e))
        })?;
    }

    if !args.is_empty() {
        let args_h = HSTRING::from(args);
        // SAFETY: Mengatur argumen baris perintah shortcut.
        unsafe {
            shell_link.SetArguments(&args_h).map_err(|e| {
                PlatformError::Os(format!("Gagal mengatur argumen shortcut: {}", e))
            })?;
        }
    }

    let effective_workdir = working_dir.or_else(|| target_exe.parent());
    if let Some(dir) = effective_workdir {
        let dir_str = dir.to_string_lossy();
        let dir_h = HSTRING::from(dir_str.as_ref());
        // SAFETY: Mengatur direktori kerja awal shortcut.
        unsafe {
            shell_link.SetWorkingDirectory(&dir_h).map_err(|e| {
                PlatformError::Os(format!("Gagal mengatur direktori kerja shortcut: {}", e))
            })?;
        }
    }

    let persist_file: IPersistFile = shell_link
        .cast()
        .map_err(|e| PlatformError::Os(format!("Gagal cast ShellLink ke IPersistFile: {}", e)))?;

    let mut wide_path: Vec<u16> = shortcut_path.as_os_str().encode_wide().collect();
    wide_path.push(0);

    // SAFETY: Menyimpan representasi shortcut biner (.lnk) langsung ke sistem berkas.
    unsafe {
        persist_file
            .Save(PCWSTR::from_raw(wide_path.as_ptr()), true)
            .map_err(|e| PlatformError::Os(format!("Gagal menyimpan file shortcut: {}", e)))?;
    }

    Ok(())
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
        let shortcut_path = spec.shortcut_path();
        let args_str = spec.arguments.join(" ");
        let workdir = match spec.app_executable.parent() {
            Some(p) if !p.as_os_str().is_empty() => Some(p),
            _ => None,
        };

        create_windows_shortcut(&shortcut_path, &spec.app_executable, &args_str, workdir)?;
        Ok(shortcut_path)
    }
}

const WM_TRAY_CALLBACK: u32 = WM_APP + 10;
const WM_TRAY_UPDATE: u32 = WM_APP + 11;
const ID_MENU_OPEN_SETTINGS: usize = 1001;
const ID_MENU_STATUS_INFO: usize = 1002;
const ID_MENU_EXIT: usize = 1003;

struct TraySharedState {
    summary: Mutex<Option<TrayStatusSummary>>,
    action_handler: Mutex<Option<TrayActionHandler>>,
}

/// Implementasi TrayProvider native untuk Windows menggunakan Win32 Shell_NotifyIcon.
pub struct WindowsTrayProvider {
    shared_state: Arc<TraySharedState>,
    hwnd_raw: Mutex<Option<isize>>,
    removed: AtomicBool,
}

impl WindowsTrayProvider {
    pub fn new() -> Self {
        let shared_state = Arc::new(TraySharedState {
            summary: Mutex::new(None),
            action_handler: Mutex::new(None),
        });

        let (tx, rx) = std::sync::mpsc::channel();
        let thread_state = shared_state.clone();

        std::thread::spawn(move || {
            // Jalankan loop pesan Win32 pada thread terpisah agar tidak membekukan UI GPUI
            Self::run_tray_message_loop(thread_state, tx);
        });

        let hwnd_raw = match rx.recv_timeout(std::time::Duration::from_secs(3)) {
            Ok(Ok(hwnd_val)) => Some(hwnd_val),
            _ => None,
        };

        Self {
            shared_state,
            hwnd_raw: Mutex::new(hwnd_raw),
            removed: AtomicBool::new(false),
        }
    }

    fn run_tray_message_loop(
        shared_state: Arc<TraySharedState>,
        tx: std::sync::mpsc::Sender<Result<isize, PlatformError>>,
    ) {
        // Daftarkan window class unik untuk hidden window penampung tray icon
        let class_name: Vec<u16> = "WindowHopTrayMessageWindowClass\0".encode_utf16().collect();

        // SAFETY: WNDCLASSW diinisialisasi nol terlebih dahulu sesuai kontrak Win32 API.
        let mut wc: WNDCLASSW = unsafe { std::mem::zeroed() };
        wc.style = CS_HREDRAW | CS_VREDRAW;
        wc.lpfnWndProc = Some(Self::tray_wndproc);
        wc.lpszClassName = PCWSTR(class_name.as_ptr());
        // SAFETY: LoadIconW dipanggil dengan IDI_APPLICATION standar Windows.
        wc.hIcon = unsafe { LoadIconW(None, IDI_APPLICATION).unwrap_or_default() };

        // SAFETY: RegisterClassW dipanggil dengan class name yang valid dan null-terminated.
        unsafe { RegisterClassW(&wc) };

        // Buat jendela tersembunyi (hidden window) khusus untuk menangkap pesan tray
        // SAFETY: CreateWindowExW dipanggil dengan class_name yang baru saja didaftarkan.
        let hwnd = match unsafe {
            CreateWindowExW(
                windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                PCWSTR(class_name.as_ptr()),
                PCWSTR(class_name.as_ptr()),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                None,
                None,
            )
        } {
            Ok(h) if !h.0.is_null() => h,
            _ => {
                let _ = tx.send(Err(PlatformError::Os(
                    "Gagal membuat window penampung notifikasi system tray".to_string(),
                )));
                return;
            }
        };

        // Daftarkan ikon tray ke Shell Windows
        let nid = Self::build_notify_icon_data(hwnd, "Window Hop - Siap");
        // SAFETY: Shell_NotifyIconW NIM_ADD dipanggil dengan NOTIFYICONDATAW yang telah diinisialisasi.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        };

        // Simpan pointer ke shared_state pada GWLP_USERDATA agar wndproc dapat mengaksesnya
        let state_ptr = Arc::into_raw(shared_state) as isize;
        // SAFETY: SetWindowLongPtrW mengatur GWLP_USERDATA window handle milik thread ini.
        #[cfg(target_pointer_width = "64")]
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                state_ptr,
            );
        }

        let hwnd_raw_val = hwnd.0 as isize;
        // Kirimkan HWND ke thread pemanggil segera setelah window dibuat sebelum memasuki blocking message loop
        let _ = tx.send(Ok(hwnd_raw_val));

        // Loop penanganan pesan Win32 standar
        // SAFETY: MSG struct diinisialisasi nol sebelum pemanggilan GetMessageW.
        unsafe {
            let mut msg = std::mem::zeroed();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    fn build_notify_icon_data(hwnd: HWND, tooltip: &str) -> NOTIFYICONDATAW {
        // SAFETY: NOTIFYICONDATAW diinisialisasi nol sebelum mengatur ukuran struct dan properti.
        let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY_CALLBACK;
        // SAFETY: LoadIconW mengambil icon aplikasi standar.
        nid.hIcon = unsafe { LoadIconW(None, IDI_APPLICATION).unwrap_or_default() };

        let wide_tip: Vec<u16> = tooltip.encode_utf16().collect();
        let len = wide_tip.len().min(127);
        nid.szTip[..len].copy_from_slice(&wide_tip[..len]);
        nid.szTip[len] = 0;

        nid
    }

    unsafe extern "system" fn tray_wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        // Ambil pointer state dari GWLP_USERDATA
        let raw_ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
        );

        if raw_ptr != 0 {
            let state = &*(raw_ptr as *const TraySharedState);

            match msg {
                WM_TRAY_UPDATE => {
                    let tooltip_text = {
                        let guard = state.summary.lock().unwrap();
                        guard
                            .as_ref()
                            .map(|s| s.tooltip.clone())
                            .unwrap_or_else(|| "Window Hop".to_string())
                    };
                    let nid = Self::build_notify_icon_data(hwnd, &tooltip_text);
                    let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
                    return LRESULT(0);
                }
                WM_TRAY_CALLBACK => {
                    let event = (lparam.0 & 0xffff) as u32;
                    if event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK {
                        let handler = state.action_handler.lock().unwrap().clone();
                        if let Some(h) = handler {
                            h(TrayAction::OpenSettings);
                        }
                        return LRESULT(0);
                    } else if event == WM_RBUTTONUP {
                        let mut pt = POINT { x: 0, y: 0 };
                        let _ = GetCursorPos(&mut pt);

                        let hmenu = CreatePopupMenu().unwrap_or_default();
                        if !hmenu.0.is_null() {
                            let open_label: Vec<u16> = "Buka Pengaturan\0".encode_utf16().collect();
                            let _ = AppendMenuW(
                                hmenu,
                                MF_STRING,
                                ID_MENU_OPEN_SETTINGS,
                                PCWSTR(open_label.as_ptr()),
                            );

                            let status_text = {
                                let guard = state.summary.lock().unwrap();
                                guard
                                    .as_ref()
                                    .map(|s| format!("Status: {}", s.status_text))
                                    .unwrap_or_else(|| "Status: Siap".to_string())
                            };
                            let status_label: Vec<u16> =
                                format!("{}\0", status_text).encode_utf16().collect();
                            let _ = AppendMenuW(
                                hmenu,
                                MF_STRING | MF_DISABLED,
                                ID_MENU_STATUS_INFO,
                                PCWSTR(status_label.as_ptr()),
                            );

                            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());

                            let exit_label: Vec<u16> =
                                "Keluar dari Window Hop\0".encode_utf16().collect();
                            let _ = AppendMenuW(
                                hmenu,
                                MF_STRING,
                                ID_MENU_EXIT,
                                PCWSTR(exit_label.as_ptr()),
                            );

                            let _ = SetForegroundWindow(hwnd);
                            let cmd = TrackPopupMenu(
                                hmenu,
                                TPM_RETURNCMD | TPM_NONOTIFY,
                                pt.x,
                                pt.y,
                                Some(0),
                                hwnd,
                                None,
                            );
                            let _ = DestroyMenu(hmenu);
                            // Sesuai panduan Microsoft KB135788 untuk TrackPopupMenu notifikasi tray:
                            // kirimkan WM_NULL agar popup menu menutup otomatis saat pengguna mengklik jendela lain
                            unsafe {
                                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                                    Some(hwnd),
                                    windows::Win32::UI::WindowsAndMessaging::WM_NULL,
                                    WPARAM(0),
                                    LPARAM(0),
                                );
                            }

                            if cmd.0 as usize == ID_MENU_OPEN_SETTINGS {
                                let handler = state.action_handler.lock().unwrap().clone();
                                if let Some(h) = handler {
                                    h(TrayAction::OpenSettings);
                                }
                            } else if cmd.0 as usize == ID_MENU_EXIT {
                                let handler = state.action_handler.lock().unwrap().clone();
                                if let Some(h) = handler {
                                    h(TrayAction::Exit);
                                }
                            }
                        }
                        return LRESULT(0);
                    }
                }
                WM_CLOSE => {
                    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
                    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
                    nid.hWnd = hwnd;
                    nid.uID = 1;
                    let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    return LRESULT(0);
                }
                _ => {}
            }
        }

        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

impl Default for WindowsTrayProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl TrayProvider for WindowsTrayProvider {
    fn update_status(&self, summary: &TrayStatusSummary) -> Result<(), PlatformError> {
        *self.shared_state.summary.lock().unwrap() = Some(summary.clone());
        let hwnd_guard = self.hwnd_raw.lock().unwrap();
        if let Some(raw) = *hwnd_guard {
            let hwnd = HWND(raw as *mut std::ffi::c_void);
            // SAFETY: PostMessageW memicu pembaruan tooltip tray pada thread pesan Win32.
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_TRAY_UPDATE, WPARAM(0), LPARAM(0));
            }
        }
        Ok(())
    }

    fn set_action_handler(
        &self,
        handler: Box<dyn Fn(TrayAction) + Send + Sync>,
    ) -> Result<(), PlatformError> {
        *self.shared_state.action_handler.lock().unwrap() = Some(Arc::from(handler));
        Ok(())
    }

    fn remove_tray(&self) -> Result<(), PlatformError> {
        if self.removed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let hwnd_guard = self.hwnd_raw.lock().unwrap();
        if let Some(raw) = *hwnd_guard {
            let hwnd = HWND(raw as *mut std::ffi::c_void);
            // SAFETY: PostMessageW mengirim WM_CLOSE agar thread tray menghapus ikon dan berhenti dengan bersih.
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        Ok(())
    }
}

impl Drop for WindowsTrayProvider {
    fn drop(&mut self) {
        let _ = self.remove_tray();
    }
}

pub(crate) struct RegKey(pub(crate) windows::Win32::System::Registry::HKEY);

impl Drop for RegKey {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = windows::Win32::System::Registry::RegCloseKey(self.0);
            }
        }
    }
}

pub(crate) fn to_wide(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

/// Helper untuk membaca nilai string (REG_SZ / REG_EXPAND_SZ) dari registry Win32.
/// Jika `value_name` adalah None, akan membaca nilai default key.
#[cfg(windows)]
pub(crate) fn query_registry_string(
    hkey: windows::Win32::System::Registry::HKEY,
    value_name: Option<&str>,
) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegQueryValueExW, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
    };

    let wide_name = value_name.map(to_wide);
    let lp_value = match &wide_name {
        Some(v) => PCWSTR(v.as_ptr()),
        None => PCWSTR::null(),
    };

    let mut val_type = REG_VALUE_TYPE::default();
    let mut data_len = 0u32;

    let status = unsafe {
        RegQueryValueExW(
            hkey,
            lp_value,
            None,
            Some(&mut val_type),
            None,
            Some(&mut data_len),
        )
    };
    if status != ERROR_SUCCESS || (val_type != REG_SZ && val_type != REG_EXPAND_SZ) || data_len < 2
    {
        return None;
    }

    let u16_count = (data_len as usize).div_ceil(2);
    let mut buffer: Vec<u16> = vec![0; u16_count];
    let mut bytes_capacity = (u16_count * std::mem::size_of::<u16>()) as u32;
    let status = unsafe {
        RegQueryValueExW(
            hkey,
            lp_value,
            None,
            Some(&mut val_type),
            Some(buffer.as_mut_ptr() as *mut u8),
            Some(&mut bytes_capacity),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }

    // Pastikan buffer memiliki null-terminator jika terpotong
    if !buffer.contains(&0) {
        buffer.push(0);
    }

    if val_type == REG_EXPAND_SZ {
        use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
        let req_len = unsafe { ExpandEnvironmentStringsW(PCWSTR(buffer.as_ptr()), None) };
        if req_len > 0 {
            let mut expanded: Vec<u16> = vec![0; req_len as usize];
            let res =
                unsafe { ExpandEnvironmentStringsW(PCWSTR(buffer.as_ptr()), Some(&mut expanded)) };
            if res > 0 {
                buffer = expanded;
            }
        }
    }

    while let Some(&0) = buffer.last() {
        buffer.pop();
    }
    let s = String::from_utf16_lossy(&buffer);
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Helper untuk membaca nilai DWORD (REG_DWORD / u32) dari registry Win32.
#[cfg(windows)]
pub(crate) fn query_registry_dword(
    hkey: windows::Win32::System::Registry::HKEY,
    value_name: &str,
) -> Option<u32> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegQueryValueExW, REG_DWORD, REG_VALUE_TYPE};

    let wide_name = to_wide(value_name);
    let mut val_type = REG_VALUE_TYPE::default();
    let mut val: u32 = 0;
    let mut data_len = std::mem::size_of::<u32>() as u32;

    let status = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(wide_name.as_ptr()),
            None,
            Some(&mut val_type),
            Some(&mut val as *mut u32 as *mut u8),
            Some(&mut data_len),
        )
    };
    if status == ERROR_SUCCESS && val_type == REG_DWORD {
        Some(val)
    } else {
        None
    }
}

/// Memeriksa apakah aplikasi terdaftar di registry startup Windows Run key.
pub fn is_windows_startup_registered() -> Result<bool, PlatformError> {
    use windows::core::w;
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE,
    };

    let mut hkey = HKEY::default();
    let res = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            None,
            KEY_QUERY_VALUE,
            &mut hkey,
        )
    };
    if res == ERROR_FILE_NOT_FOUND {
        return Ok(false);
    }
    if res != ERROR_SUCCESS {
        return Err(PlatformError::Os(format!(
            "Gagal membuka registry startup key: {}",
            res.0
        )));
    }
    let _guard = RegKey(hkey);

    let query_res = unsafe { RegQueryValueExW(hkey, w!("WindowHop"), None, None, None, None) };
    if query_res == ERROR_SUCCESS {
        Ok(true)
    } else if query_res == ERROR_FILE_NOT_FOUND {
        Ok(false)
    } else {
        Err(PlatformError::Os(format!(
            "Gagal membaca nilai registry startup WindowHop: {}",
            query_res.0
        )))
    }
}

/// Mendaftarkan executable aplikasi ke registry startup Windows Run key secara native.
pub fn register_windows_startup(app_exe: &std::path::Path) -> Result<(), PlatformError> {
    use windows::core::w;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCreateKeyW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, REG_SZ,
    };

    let val = format!("\"{}\" --startup", app_exe.to_string_lossy());
    let val_w = to_wide(&val);
    let bytes = unsafe { std::slice::from_raw_parts(val_w.as_ptr() as *const u8, val_w.len() * 2) };

    let mut hkey = HKEY::default();
    let res = unsafe {
        RegCreateKeyW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            &mut hkey,
        )
    };
    if res != ERROR_SUCCESS {
        return Err(PlatformError::Os(format!(
            "Gagal membuat atau membuka registry startup key: {}",
            res.0
        )));
    }
    let _guard = RegKey(hkey);

    let set_res = unsafe { RegSetValueExW(hkey, w!("WindowHop"), None, REG_SZ, Some(bytes)) };
    if set_res != ERROR_SUCCESS {
        return Err(PlatformError::Os(format!(
            "Gagal menulis nilai registry startup WindowHop: {}",
            set_res.0
        )));
    }
    Ok(())
}

/// Menghapus pendaftaran aplikasi dari registry startup Windows Run key secara native.
pub fn remove_windows_startup_registration() -> Result<(), PlatformError> {
    use windows::core::w;
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        RegDeleteValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE,
    };

    let mut hkey = HKEY::default();
    let res = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            None,
            KEY_SET_VALUE,
            &mut hkey,
        )
    };
    if res == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    if res != ERROR_SUCCESS {
        return Err(PlatformError::Os(format!(
            "Gagal membuka registry startup key: {}",
            res.0
        )));
    }
    let _guard = RegKey(hkey);

    let del_res = unsafe { RegDeleteValueW(hkey, w!("WindowHop")) };
    if del_res != ERROR_SUCCESS && del_res != ERROR_FILE_NOT_FOUND {
        return Err(PlatformError::Os(format!(
            "Gagal menghapus nilai registry startup WindowHop: {}",
            del_res.0
        )));
    }
    Ok(())
}

/// Metadata uninstaller aplikasi Window Hop untuk registrasi Add/Remove Programs Windows.
pub struct WindowsUninstallOptions<'a> {
    pub display_name: &'a str,
    pub display_version: &'a str,
    pub publisher: &'a str,
    pub install_location: &'a str,
    pub display_icon: &'a str,
    pub uninstall_string: &'a str,
    pub quiet_uninstall_string: &'a str,
}

/// Mendaftarkan entri uninstaller Window Hop ke Add/Remove Programs secara native menggunakan Win32 Registry API.
pub fn register_windows_uninstaller(opts: &WindowsUninstallOptions) -> Result<(), PlatformError> {
    use windows::core::w;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCreateKeyW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, REG_DWORD, REG_SZ,
    };

    let mut hkey = HKEY::default();
    let res = unsafe {
        RegCreateKeyW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop"),
            &mut hkey,
        )
    };
    if res != ERROR_SUCCESS {
        return Err(PlatformError::Os(format!(
            "Gagal membuat atau membuka registry uninstall key: {}",
            res.0
        )));
    }
    let _guard = RegKey(hkey);

    let set_sz =
        |name_w: windows::core::PCWSTR, val: &str, field_name: &str| -> Result<(), PlatformError> {
            let val_w = to_wide(val);
            let bytes =
                unsafe { std::slice::from_raw_parts(val_w.as_ptr() as *const u8, val_w.len() * 2) };
            let res = unsafe { RegSetValueExW(hkey, name_w, None, REG_SZ, Some(bytes)) };
            if res != ERROR_SUCCESS {
                return Err(PlatformError::Os(format!(
                    "Gagal menulis properti registry uninstaller {}: {}",
                    field_name, res.0
                )));
            }
            Ok(())
        };

    let set_dword =
        |name_w: windows::core::PCWSTR, val: u32, field_name: &str| -> Result<(), PlatformError> {
            let bytes = val.to_ne_bytes();
            let res = unsafe { RegSetValueExW(hkey, name_w, None, REG_DWORD, Some(&bytes)) };
            if res != ERROR_SUCCESS {
                return Err(PlatformError::Os(format!(
                    "Gagal menulis properti DWORD registry uninstaller {}: {}",
                    field_name, res.0
                )));
            }
            Ok(())
        };

    set_sz(w!("DisplayName"), opts.display_name, "DisplayName")?;
    set_sz(w!("DisplayVersion"), opts.display_version, "DisplayVersion")?;
    set_sz(w!("Publisher"), opts.publisher, "Publisher")?;
    set_sz(
        w!("InstallLocation"),
        opts.install_location,
        "InstallLocation",
    )?;
    set_sz(w!("DisplayIcon"), opts.display_icon, "DisplayIcon")?;
    set_sz(
        w!("UninstallString"),
        opts.uninstall_string,
        "UninstallString",
    )?;
    set_sz(
        w!("QuietUninstallString"),
        opts.quiet_uninstall_string,
        "QuietUninstallString",
    )?;
    set_dword(w!("NoModify"), 1, "NoModify")?;
    set_dword(w!("NoRepair"), 1, "NoRepair")?;

    Ok(())
}

/// Menghapus entri uninstaller Window Hop dari Add/Remove Programs secara native menggunakan Win32 Registry API.
pub fn unregister_windows_uninstaller() -> Result<(), PlatformError> {
    use windows::core::w;
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{RegDeleteTreeW, HKEY_CURRENT_USER};

    let res = unsafe {
        RegDeleteTreeW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop"),
        )
    };
    if res != ERROR_SUCCESS && res != ERROR_FILE_NOT_FOUND {
        return Err(PlatformError::Os(format!(
            "Gagal menghapus registry uninstaller WindowHop: {}",
            res.0
        )));
    }
    Ok(())
}

/// Implementasi StartupManager untuk platform Windows menggunakan registry Run key.
pub struct WindowsStartupManager {
    app_exe: std::sync::Mutex<Option<std::path::PathBuf>>,
}

impl WindowsStartupManager {
    /// Membuat instance baru WindowsStartupManager.
    pub fn new() -> Self {
        Self {
            app_exe: std::sync::Mutex::new(None),
        }
    }

    /// Membuat instance baru WindowsStartupManager dengan path executable eksplisit.
    pub fn with_app_exe(app_exe: std::path::PathBuf) -> Self {
        Self {
            app_exe: std::sync::Mutex::new(Some(app_exe)),
        }
    }
}

impl Default for WindowsStartupManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StartupManager for WindowsStartupManager {
    fn set_app_executable(&self, app_exe: std::path::PathBuf) {
        *self.app_exe.lock().unwrap() = Some(app_exe);
    }

    fn is_startup_registered(&self) -> Result<bool, PlatformError> {
        is_windows_startup_registered()
    }

    fn set_startup_registered(&self, enabled: bool) -> Result<(), PlatformError> {
        if enabled {
            let app_exe = {
                let guard = self.app_exe.lock().unwrap();
                if let Some(ref p) = *guard {
                    p.clone()
                } else {
                    std::env::current_exe().map_err(|e| {
                        PlatformError::Os(format!(
                            "Gagal mendapatkan path executable aplikasi: {}",
                            e
                        ))
                    })?
                }
            };
            register_windows_startup(&app_exe)
        } else {
            remove_windows_startup_registration()
        }
    }
}

/// Implementasi BrowserDetector untuk platform Windows.
///
/// Memeriksa jalur instalasi standar (Program Files, Program Files (x86), LocalAppData)
/// untuk Edge, Chrome, Firefox, dan Brave, serta memeriksa registry StartMenuInternet.
/// Setiap executable yang ditemukan diperiksa arsitektur PE-nya (x86 vs x64) dan versinya jika ada,
/// lalu dideduplikasi sebelum dikembalikan.
pub struct WindowsBrowserDetector;

impl WindowsBrowserDetector {
    /// Membuat instance baru WindowsBrowserDetector.
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsBrowserDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserDetector for WindowsBrowserDetector {
    fn detect_installed_browsers(&self) -> Result<Vec<DetectedBrowser>, PlatformError> {
        let mut candidates = Vec::new();

        // 1. Path-based scanning
        let prog_files = std::env::var_os("ProgramFiles").map(std::path::PathBuf::from);
        let prog_files_x86 = std::env::var_os("ProgramFiles(x86)").map(std::path::PathBuf::from);
        let local_app_data = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);

        let search_bases = [
            (
                prog_files.as_ref(),
                "Microsoft\\Edge\\Application\\msedge.exe",
                "Microsoft Edge",
            ),
            (
                prog_files_x86.as_ref(),
                "Microsoft\\Edge\\Application\\msedge.exe",
                "Microsoft Edge",
            ),
            (
                local_app_data.as_ref(),
                "Microsoft\\Edge\\Application\\msedge.exe",
                "Microsoft Edge",
            ),
            (
                prog_files.as_ref(),
                "Google\\Chrome\\Application\\chrome.exe",
                "Google Chrome",
            ),
            (
                prog_files_x86.as_ref(),
                "Google\\Chrome\\Application\\chrome.exe",
                "Google Chrome",
            ),
            (
                local_app_data.as_ref(),
                "Google\\Chrome\\Application\\chrome.exe",
                "Google Chrome",
            ),
            (
                prog_files.as_ref(),
                "Mozilla Firefox\\firefox.exe",
                "Mozilla Firefox",
            ),
            (
                prog_files_x86.as_ref(),
                "Mozilla Firefox\\firefox.exe",
                "Mozilla Firefox",
            ),
            (
                prog_files.as_ref(),
                "BraveSoftware\\Brave-Browser\\Application\\brave.exe",
                "Brave Browser",
            ),
            (
                prog_files_x86.as_ref(),
                "BraveSoftware\\Brave-Browser\\Application\\brave.exe",
                "Brave Browser",
            ),
            (
                local_app_data.as_ref(),
                "BraveSoftware\\Brave-Browser\\Application\\brave.exe",
                "Brave Browser",
            ),
        ];

        for (base_opt, rel_path, name) in search_bases {
            if let Some(base) = base_opt {
                let full_path = base.join(rel_path);
                if full_path.exists() && full_path.is_file() {
                    let arch = read_executable_architecture(&full_path);
                    let version = crate::browsers::read_installed_browser_version(&full_path);
                    candidates.push(DetectedBrowser {
                        name: name.to_string(),
                        executable_path: full_path,
                        version,
                        architecture: arch,
                    });
                }
            }
        }

        // 2. Native Win32 Registry scanning for StartMenuInternet clients in HKLM and HKCU
        scan_start_menu_internet_root(
            windows::Win32::System::Registry::HKEY_LOCAL_MACHINE,
            &mut candidates,
        );
        scan_start_menu_internet_root(
            windows::Win32::System::Registry::HKEY_CURRENT_USER,
            &mut candidates,
        );

        Ok(crate::platform::deduplicate_detected_browsers(candidates))
    }
}

#[cfg(windows)]
fn scan_start_menu_internet_root(
    root: windows::Win32::System::Registry::HKEY,
    candidates: &mut Vec<DetectedBrowser>,
) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegEnumKeyExW, RegOpenKeyExW, HKEY, KEY_READ};

    let sub_key_wide = to_wide(r"SOFTWARE\Clients\StartMenuInternet");
    let mut h_start_menu = HKEY::default();
    let status = unsafe {
        RegOpenKeyExW(
            root,
            PCWSTR(sub_key_wide.as_ptr()),
            None,
            KEY_READ,
            &mut h_start_menu,
        )
    };
    if status != ERROR_SUCCESS {
        return;
    }
    let start_menu_guard = RegKey(h_start_menu);

    let mut index = 0u32;
    loop {
        let mut subkey_name = [0u16; 256];
        let mut name_len = subkey_name.len() as u32;
        let enum_res = unsafe {
            RegEnumKeyExW(
                start_menu_guard.0,
                index,
                Some(windows::core::PWSTR(subkey_name.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                None,
            )
        };
        if enum_res != ERROR_SUCCESS {
            break;
        }
        index += 1;

        let client_key_name = String::from_utf16_lossy(&subkey_name[..name_len as usize]);

        // Open StartMenuInternet\<client_key_name>
        let client_rel_path = to_wide(&client_key_name);
        let mut h_client = HKEY::default();
        let client_open = unsafe {
            RegOpenKeyExW(
                start_menu_guard.0,
                PCWSTR(client_rel_path.as_ptr()),
                None,
                KEY_READ,
                &mut h_client,
            )
        };
        if client_open != ERROR_SUCCESS {
            continue;
        }
        let client_guard = RegKey(h_client);

        // Read default value of <client_key_name> for display name
        let display_name =
            query_registry_string(client_guard.0, None).unwrap_or_else(|| client_key_name.clone());

        // Open shell\open\command under <client_key_name>
        let cmd_rel_path = to_wide(r"shell\open\command");
        let mut h_cmd = HKEY::default();
        let cmd_open = unsafe {
            RegOpenKeyExW(
                client_guard.0,
                PCWSTR(cmd_rel_path.as_ptr()),
                None,
                KEY_READ,
                &mut h_cmd,
            )
        };
        if cmd_open != ERROR_SUCCESS {
            continue;
        }
        let cmd_guard = RegKey(h_cmd);

        // Read default value of shell\open\command
        if let Some(cmd_str) = query_registry_string(cmd_guard.0, None) {
            if let Some(path) = crate::browsers::extract_browser_executable_path(&cmd_str) {
                if path.exists() && path.is_file() {
                    let arch = read_executable_architecture(&path);
                    let version = crate::browsers::read_installed_browser_version(&path);
                    let name = if display_name.trim().is_empty() {
                        path.file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Browser".to_string())
                    } else {
                        display_name.trim().to_string()
                    };
                    candidates.push(DetectedBrowser {
                        name,
                        executable_path: path,
                        version,
                        architecture: arch,
                    });
                }
            }
        }
    }
}
