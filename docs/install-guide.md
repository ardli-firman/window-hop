# Panduan Pemasangan Window Hop (Installation Guide)

Dokumen ini mendefinisikan batas sistem yang diverifikasi dan prosedur instalasi Window Hop untuk petugas pemeliharaan workstation poliklinik.

## 1. Batasan Sistem Operasi dan Runtime yang Diverifikasi

Sesuai [ADR 0002](adr/0002-target-windows-10-and-11.md) dan spesifikasi produk versi 1:

| Komponen | Persyaratan Minimum yang Diverifikasi | Catatan |
|---|---|---|
| **Sistem Operasi** | Windows 10 64-bit (Build 19041 / 2004 atau lebih baru)<br>Windows 11 64-bit | Windows 7, Windows 8/8.1, dan Windows 32-bit **tidak didukung**. Validasi fisik lengkap dijadwalkan pada ticket #16 dan #17. |
| **Arsitektur Aplikasi** | x64 (64-bit) | Window Hop dibangun sebagai biner x64. |
| **Arsitektur Browser** | x86 (32-bit) dan x64 (64-bit) | Browser poliklinik 32-bit maupun 64-bit didukung pada OS 64-bit. |
| **Akselerasi Grafis & GPUI** | Direct3D 11 (Feature Level 11_0+) / Direct3D 12 | GPUI memanfaatkan rendering DirectX/Direct3D dan DirectWrite font. Resource/asset UI dan font disertakan langsung (ter-embed) di dalam biner aplikasi via `gpui-kit`. |
| **Runtime C++** | Microsoft Visual C++ 2015–2022 Redistributable (x64) | Umumnya telah terpasang pada Windows 10/11 terbarui. |
| **Hak Akses Pengguna** | Standard User (Tanpa Hak Administrator / UAC) | Lokasi instalasi per-pengguna tidak memerlukan hak administrator. |

## 2. Lokasi Instalasi yang Konsisten

Untuk memastikan jalan pintas (shortcut) dan startup otomatis berfungsi konsisten di semua komputer poliklinik:

- **Lokasi Default Resmi**:
  `%LOCALAPPDATA%\Programs\Window Hop\window-hop.exe`
  *(contoh: `C:\Users\Perawat\AppData\Local\Programs\Window Hop\window-hop.exe`)*
- **Berkas yang Tersedia pada Hasil Instalasi**:
  1. `window-hop.exe` — executable utama aplikasi dan koordinator.
  2. `uninstall.exe` — executable uninstaller untuk pencopotan bersih.
  3. `INSTALL-GUIDE.txt` — ringkasan batasan OS/runtime dan petunjuk pemasangan.
  4. `install-manifest.json` — catatan integritas versi, berkas terpasang, shortcut yang dibuat, dan stempel waktu instalasi.
- **Integrasi Windows**:
  1. **Start Menu**: Shortcut dibuat di `Start Menu\Programs\Window Hop\Window Hop.lnk`.
  2. **Desktop**: Shortcut dibuat di Desktop perawat (`Window Hop.lnk`).
  3. **Installed Apps (Add/Remove Programs)**: Terdaftar di `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop` sehingga aplikasi tercatat di Windows Settings dan dapat di-uninstall sewaktu-waktu.

## 3. Cara Pemasangan

### Pilihan 1: Bundel Setup Standar (Wizard GUI Modern)

Unduh `WindowHop-Setup.exe` (atau `WindowHop-Setup-v<versi>.exe`) dari GitHub Artifacts / Releases. Cukup klik ganda berkas tersebut untuk membuka wizard pemasangan:
1. Ikuti petunjuk wizard (lokasi default: `%LOCALAPPDATA%\Programs\Window Hop`).
2. Wizard akan memasang berkas, membuat shortcut Start Menu dan Desktop, serta mendaftarkan uninstaller Windows.
3. Di halaman akhir, centang "Jalankan Window Hop sekarang" untuk langsung menggunakan aplikasi.

Untuk instalasi senyap tanpa interaksi pengguna (unattended deployment):
```powershell
.\WindowHop-Setup.exe /S
```

### Pilihan 2: Installer Mandiri (CLI & Dialog Konfirmasi)

Petugas juga dapat menjalankan `window-hop-installer.exe` (berisi payload aplikasi tersemat mandiri):

```powershell
# Pemasangan interaktif ke %LOCALAPPDATA%\Programs\Window Hop
.\window-hop-installer.exe

# Pemasangan senyap (unattended / silent) untuk deployment otomatis
.\window-hop-installer.exe --silent

# Pemasangan ke direktori khusus
.\window-hop-installer.exe --dir "D:\Aplikasi\Window Hop"

# Pencopotan instalasi (uninstallation)
.\window-hop-installer.exe --uninstall
```

## 4. Perilaku Shortcut Profil dan Startup

1. **Shortcut Profil**:
   - Dibuat melalui antarmuka Window Hop (UI pengaturan profil).
   - Shortcut yang dihasilkan selalu menunjuk ke executable aplikasi yang telah terpasang (`%LOCALAPPDATA%\Programs\Window Hop\window-hop.exe`), bukan ke folder kompilasi pengembang (`target/debug` atau `target/release`).
   - Default diletakkan di Desktop pengguna poliklinik, dengan opsi memilih folder alternatif.
2. **Startup Otomatis**:
   - Jika startup diaktifkan melalui UI, Window Hop mendaftarkan nilai registry `HKCU:\Software\Microsoft\Windows\CurrentVersion\Run` bernama `WindowHop` dengan nilai `"<lokasi_terpasang>\window-hop.exe" --startup`.
   - Saat login Windows, Window Hop langsung berjalan di system tray tanpa membuka jendela pengaturan, dan mengaktifkan profil startup yang dipilih ke monitor tujuan.

## 5. Pemeriksaan Kegagalan dan Diagnosis

- **Kegagalan Arsitektur (OS 32-bit)**:
  Installer akan langsung berhenti dengan pesan:
  `Arsitektur OS x86 (32-bit) tidak didukung. Window Hop memerlukan Windows 64-bit (x64).`
- **Kegagalan Versi Windows (Windows 7 / Windows 10 lama)**:
  Installer akan menolak dengan rincian nomor build OS yang ditemukan dan batas minimum (Windows 10 build 19041).
- **Pemeriksaan Integritas Berkas**:
  Integritas instalasi dapat diverifikasi sewaktu-waktu melalui `install-manifest.json`.

## 6. Build Otomatis, Makefile, dan GitHub Actions

### Kompilasi Lokal dengan Makefile

Proyek menyediakan `Makefile` untuk mempermudah kompilasi installer dan aplikasi baik dalam mode pengembangan (*dev*) maupun produksi (*release*):

```bash
# Menampilkan bantuan dan daftar perintah build
make help

# Menjalankan aplikasi Window Hop mode pengembangan (dev)
make run
# atau dengan argumen khusus:
make run ARGS="--startup"

# Menjalankan installer dalam mode pengembangan
make run-installer ARGS="--help"

# Membangun executable dev (unoptimized + debuginfo)
make dev

# Membangun executable installer saja (dev)
make dev-installer

# Membangun executable release (teroptimasi)
make release

# Membangun executable installer saja (release)
make installer

# Membuat paket distribusi di folder dist/ beserta arsip .zip
make package

# Menjalankan seluruh pengujian dan pengecekan kualitas
make test
make check
```

### Manajemen Versi (Versioning)

Nomor versi dikelola secara konsisten mengikuti Semantic Versioning (SemVer) dan disinkronkan ke `Cargo.toml`, manifest instalasi, serta metadata biner. Tooling versioning dibangun dengan pure Rust (`scripts/version.rs`, otomatis dikompilasi via `rustc`) dan alternatif Node.js (`scripts/version.js`), sehingga tidak memerlukan Python:

```bash
# Melihat versi aktif saat ini
make version

# Melihat versi aktif beserta hash commit git
make version-full

# Mengubah versi ke nilai spesifik
make set-version VERSION=0.2.0

# Menaikkan versi otomatis (patch: 0.1.0 -> 0.1.1, minor: 0.1.0 -> 0.2.0, major: 0.1.0 -> 1.0.0)
make bump-patch
make bump-minor
make bump-major

# Membuat tag Git beranotasi sesuai versi aktif
make tag
```

### Integrasi CI/CD GitHub Actions

Alur kerja GitHub Actions (`.github/workflows/build-installer.yml`) berjalan otomatis untuk:
1. Menjalankan linter (`cargo fmt`, `cargo clippy -D warnings`) dan pengujian (`cargo test`) pada Linux.
2. Membangun executable `window-hop.exe`, installer mandiri tersemat `window-hop-installer.exe`, serta wizard setup Windows modern `WindowHop-Setup.exe` menggunakan `make` pada runner Windows (`windows-latest`, target `x86_64-pc-windows-msvc`).
3. Menguji eksekusi `window-hop-installer.exe --help` langsung pada lingkungan Windows runner.
4. Menghasilkan checksum SHA-256 dan mengunggah artefak build yang siap unduh dan pakai (`WindowHop-Setup-v*.exe`, `window-hop-installer.exe`, `window-hop-v*-portable.zip`).
5. Saat push tag `v*` (contoh: `v0.1.0`), alur kerja otomatis membuat GitHub Release dan melampirkan berkas installer setup `.exe` serta arsip `.zip` portable.
