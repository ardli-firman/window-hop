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
