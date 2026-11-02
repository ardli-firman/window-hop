---
status: accepted
---

# Eliminasi pemanggilan PowerShell demi Win32 API native untuk mencegah deteksi karantina antivirus

Window Hop sebelumnya memanfaatkan subprocess `powershell.exe` dengan flag `CREATE_NO_WINDOW` untuk sejumlah tugas integrasi sistem operasi Windows:
1. Pembuatan shortcut `.lnk` profil dan uninstaller via WScript.Shell COM wrapper.
2. Penambahan dan penghapusan registry startup (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`) dan uninstaller (`HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop`).
3. Pemindaian instalasi browser terpasang pada `SOFTWARE\Clients\StartMenuInternet`.
4. Deteksi versi dan arsitektur OS Windows pada binary installer.

## Masalah

Pemanggilan `powershell.exe` secara background dengan jendela tersembunyi (`CREATE_NO_WINDOW`) saat aplikasi desktop beroperasi memicu sistem Antimalware Scan Interface (AMSI) dan analisis perilaku Windows Defender (`Trojan:Script/Wacatac`, `Behavior:Win32/Persistence`). Perilaku ini menyebabkan aplikasi Window Hop dikarantina dan dihapus otomatis dari desktop sesaat setelah profil aktif dibuat. Selain masalah keamanan semu ini, pemanggilan proses eksternal PowerShell memperkenalkan latensi proses yang signifikan (200-500ms per pemanggilan).

## Keputusan

Mengeliminasi seluruh dependensi terhadap subprocess `powershell.exe`, `wscript.exe`, maupun script shell eksternal lainnya di seluruh codebase Window Hop (`window-hop` dan `window-hop-installer`).

Menggantikannya dengan pemanggilan native Win32 API:
1. **Pembuatan Shortcut:** Menggunakan native Win32 COM API (`IShellLinkW` dan `IPersistFile`).
2. **Manajemen Registri:** Menggunakan Win32 Registry API (`RegOpenKeyExW`, `RegCreateKeyExW`, `RegSetValueExW`, `RegDeleteValueW`, `RegQueryValueExW`, `RegEnumKeyExW`, `RegDeleteTreeW`) yang dibungkus dalam RAII guard (`RegKey`) untuk memastikan handle kunci registri tertutup secara otomatis tanpa kebocoran resource.
3. **Deteksi Browser:** Membaca registri `SOFTWARE\Clients\StartMenuInternet` langsung dari `HKEY_LOCAL_MACHINE` dan `HKEY_CURRENT_USER` via Win32 API serta menangani ekspansi environment string (`REG_EXPAND_SZ`).
4. **Deteksi Versi OS:** Membaca kunci registri `SOFTWARE\Microsoft\Windows NT\CurrentVersion` dan memanggil `GetNativeSystemInfo` untuk arsitektur hardware.

## Konsekuensi

1. Biner beralih sepenuhnya ke instruksi mesin native tanpa ada string skrip PowerShell yang dapat memicu heuristik AMSI.
2. Waktu eksekusi pembuatan shortcut, registrasi startup, dan pemindaian browser meningkat drastis (hampir instan, tanpa overhead proses baru).
3. Penggunaan blok `unsafe` pada FFI Windows diisolasi secara ketat dalam fungsi pembantu RAII dengan pengujian statis regresi otomatis yang memastikan ketiadaan pemanggilan skrip eksternal di masa depan.
