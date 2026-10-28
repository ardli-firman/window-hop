# Kebutuhan Window Hop

Status: spesifikasi versi pertama disetujui pengguna pada 28 Oktober 2026. Implementasi belum dimulai.

## Kebutuhan yang sudah disepakati

- Aplikasi desktop Windows menggunakan Rust, GPUI, dan gpui-kit sesuai permintaan pengguna. Identitas paket dan versinya belum diverifikasi.
- Dipasang pada komputer perawat poliklinik yang terhubung ke monitor antrian.
- Aktivasi dipicu setelah pengguna login Windows dan saat shortcut profil diklik.
- Satu profil pilihan menjadi profil startup.
- Satu profil berisi satu aplikasi, parameter peluncuran kustom, dan satu monitor tujuan.
- Parameter browser `--kiosk` adalah contoh parameter; opsi ukuran jendela tambahan belum menjadi kebutuhan yang disepakati.
- Jendela aplikasi harus menuju monitor tujuan yang dipilih pengguna.
- Aktivasi ulang menggunakan kembali window milik profil dan mengembalikannya ke monitor tujuan; aplikasi diluncurkan jika belum berjalan.
- Window aplikasi kerja perawat yang bukan milik profil tidak boleh ikut dipindahkan.
- Jika monitor tujuan belum tersedia, aplikasi tidak boleh dijalankan.
- Jika monitor tujuan terputus, aplikasi profil ditutup agar tidak mengganggu monitor primary.
- Konfigurasi profil dapat diatur melalui UI oleh pengguna sesi Windows tersebut, tanpa PIN atau akun tambahan Window Hop.
- Profil yang sebelumnya aktif atau tertunda karena monitor tidak tersedia dijalankan otomatis ketika monitor tujuannya kembali tersambung.
- Satu monitor hanya memiliki satu profil aktif. Aktivasi profil berbeda pada monitor yang sama menutup aplikasi profil sebelumnya, lalu menjalankan aplikasi profil baru.
- Saat monitor terputus, aplikasi diminta menutup secara normal. Jika belum tertutup setelah 3 detik, hanya instance milik profil yang ditutup paksa. Pengguna menyetujui risiko hilangnya data yang belum disimpan pada instance tersebut.
- Cakupan awal adalah semua browser. Dukungan aplikasi non-browser ditunda. Batas kompatibilitas dan daftar browser yang diuji belum ditentukan.
- Setiap profil memiliki sesi dan data browser sendiri, terpisah dari browser kerja perawat dan profil lainnya. Login yang diperlukan dilakukan dalam sesi browser profil tersebut.
- Penempatan memakai perilaku yang direkomendasikan: periksa ketersediaan monitor sebelum peluncuran, segera tempatkan window pada monitor tujuan, dan tutup ketika monitor terputus. Kemunculan sesaat pada primary perlu diuji per browser; tidak ada jaminan generik nol kemunculan.
- Ketika identitas monitor tidak cocok atau ambigu, jangan jalankan browser. Tampilkan status monitor tujuan tidak ditemukan dan minta pemilihan ulang melalui UI.
- Window Hop berjalan di system tray setelah login tanpa otomatis membuka halaman pengaturan. UI dapat dibuka dari tray untuk mengatur profil dan melihat status.
- Browser yang crash tidak dijalankan ulang otomatis. Pengguna memulihkannya melalui shortcut profil atau aktivasi setelah login Windows berikutnya.
- Parameter kustom yang bertentangan dengan pemisahan sesi browser ditolak dengan penjelasan melalui UI.
- Versi Windows yang wajib didukung adalah Windows 10 dan 11. Windows 7 dikeluarkan dari cakupan setelah verifikasi ketidakcocokan stack GPUI saat ini.
- OS yang didukung adalah Windows 10 dan 11 64-bit. Browser 32-bit dan 64-bit masuk cakupan. Pengguna memilih tidak mewajibkan Window Hop berjalan pada OS 32-bit.
- Setiap profil memiliki tombol membuat shortcut, default ke Desktop dengan pilihan folder lain. Shortcut mengaktifkan profil tanpa membuka halaman pengaturan.
- Perubahan profil aktif dapat disimpan tanpa langsung mengganggu tayangan. UI menyediakan tindakan menjalankan ulang dengan perubahan; aktivasi berikutnya juga menggunakan konfigurasi terbaru.
- Setelah browser crash atau ditutup manual, reconnect monitor tidak menjalankan browser kembali. Profil hanya diluncurkan kembali lewat shortcut atau login Windows berikutnya. Reconnect otomatis hanya memulihkan profil yang berhenti karena monitor terputus.
- Keluar dari Window Hop melalui tray menutup browser milik profil sebelum Window Hop berhenti.
- Aturan tutup normal lalu paksa setelah 3 detik berlaku juga saat mengganti atau menghapus profil aktif. Browser profil lama harus berhenti sebelum browser profil pengganti dijalankan.
- Penghapusan profil aktif meminta konfirmasi melalui UI, lalu menutup browser milik profil tersebut.
- UI mendeteksi browser terpasang dan menyediakan pilihan file `.exe` manual untuk browser portable atau lokasi khusus.
- Profil boleh disimpan, tetapi aktivasi ditolak dengan alasan yang jelas jika pemisahan sesi dan kepemilikan instance belum bisa diverifikasi. Cakupan semua browser adalah target kompatibilitas, bukan klaim semua browser sudah teruji.
- Distribusi menggunakan installer `.exe` agar lokasi aplikasi, shortcut, dan startup konsisten.
- UI menyediakan pengaturan mengaktifkan/menonaktifkan startup dan memilih satu profil startup.
- Kegagalan otomatis ditampilkan melalui status tray dan detail di UI, tanpa membuka window atau mengambil fokus. Kesalahan tindakan yang dilakukan lewat UI ditampilkan langsung pada UI tersebut.

## Batas cakupan yang disepakati

- Versi pertama tidak menambah PIN atau akun khusus untuk membuka pengaturan Window Hop. Pengaturan tersedia bagi pengguna sesi Windows tersebut.
- Data sesi browser disimpan terpisah per profil. Kemampuan mempertahankan login mengikuti browser dan parameter yang dipilih; Window Hop tidak memaksakan login persisten dalam mode privat atau kiosk yang tidak mendukungnya.

## Konsistensi perilaku untuk implementasi

- Aktivasi ulang profil dengan konfigurasi yang sama memakai kembali window yang ada. Jika konfigurasi telah diubah dan disimpan, aktivasi berikutnya harus menerapkan konfigurasi terbaru; browser lama perlu ditutup dan diluncurkan kembali bila perubahan memerlukannya.
- Menyimpan perubahan tidak mengubah konfigurasi tayangan yang sedang berjalan. Pemantauan tayangan tersebut harus tetap menggunakan monitor dan instance yang sedang dikelola sampai pengguna menerapkan perubahan atau terjadi aktivasi baru.
- Hilangnya sambungan monitor yang masih dikenali dapat memicu pemulihan otomatis saat tersambung kembali. Monitor baru atau identitas ambigu memerlukan pemilihan ulang melalui UI dan tidak boleh menjadi fallback otomatis.

## Verifikasi teknis yang masih diperlukan

- Build dan uji runtime Rust/GPUI/gpui-kit pada Windows 10 dan 11 x64 serta GPU komputer poliklinik; versi minimum Windows 10 harus dicatat berdasarkan bukti kompatibilitas.
- Matriks browser dan versi yang benar-benar diuji, termasuk browser 32-bit pada OS 64-bit, pemisahan sesi, kepemilikan instance, dan parameter kustom.
- Perilaku window saat peluncuran, monitor terputus, penggantian profil, serta batas waktu penutupan.
- Installer, tray, startup setelah login, shortcut, dan komunikasi aktivasi ke instance Window Hop yang sudah berjalan.
- Kriteria penerimaan operasional dicatat di `acceptance.md`. Belum ada pengujian Windows atau browser yang dijalankan.

## Temuan teknis untuk keputusan berikutnya

- Chromium dapat meneruskan perintah peluncuran ke instance yang sudah berjalan dengan direktori data pengguna yang sama. Pemisahan direktori data per profil disepakati untuk melindungi sesi kerja perawat. Sumber: https://chromium.googlesource.com/chromium/src/+/HEAD/chrome/browser/process_singleton.h dan https://chromium.googlesource.com/chromium/src/+/HEAD/docs/user_data_dir.md.
- Penempatan awal melalui STARTUPINFO bergantung pada perilaku aplikasi saat membuat jendela. Dokumentasi Windows tidak memberi jaminan generik bahwa aplikasi pihak ketiga tidak pernah tampil sesaat di primary. Pengguna menyepakati penempatan dan penutupan segera, dengan kemunculan sesaat serta perubahan fokus diuji per browser. Sumber: https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/ns-processthreadsapi-startupinfow dan https://learn.microsoft.com/en-us/windows/win32/gdi/positioning-objects-on-multiple-display-monitors.
- Dokumentasi Edge menyatakan kedua pengalaman kiosk menggunakan InPrivate. Kebutuhan login persisten tidak boleh diasumsikan berlaku dalam mode tersebut. Sumber: https://learn.microsoft.com/en-us/deployedge/microsoft-edge-configure-kiosk-mode.
- Stack yang diperiksa adalah Longbridge gpui-kit 0.8.1, commit `ec89cfc36404667b7947aeb3d40b1c2859d729dd`, yang memilih gpui-pre/gpui-pre-platform 0.3.10. Backend Windows 0.3.10 memanggil langsung GetDpiForWindow, GetDpiForMonitor, dan CreateDXGIFactory2 tanpa fallback pada jalur yang diperiksa. Microsoft mensyaratkan Windows 10 versi 1607 untuk API pertama dan Windows 8.1 untuk dua API lainnya. Stack tersebut tanpa modifikasi tidak memenuhi Windows 7. Sumber kode: https://docs.rs/crate/gpui-pre-windows/0.3.10/source/src/window.rs, https://docs.rs/crate/gpui-pre-windows/0.3.10/source/src/display.rs, dan https://docs.rs/crate/gpui-pre-windows/0.3.10/source/src/directx_devices.rs. Persyaratan API: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdpiforwindow, https://learn.microsoft.com/en-us/windows/win32/api/shellscalingapi/nf-shellscalingapi-getdpiformonitor, dan https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_3/nf-dxgi1_3-createdxgifactory2.
- Backend GPUI yang diperiksa memiliki jalur kode eksplisit untuk pointer 32-bit, tetapi pipeline build dan runtime i686 belum diverifikasi. Tidak ada kesimpulan bahwa dukungan x86 mustahil. Sumber: https://docs.rs/crate/gpui-pre-windows/0.3.10/source/src/util.rs.
- Rust menyediakan target khusus Windows 7 pada Tier 3 tanpa artefak precompiled; dukungan Rust tidak otomatis membuktikan kompatibilitas GPUI. Sumber: https://doc.rust-lang.org/rustc/platform-support/win7-windows-msvc.html.

## Catatan dokumentasi

Istilah domain dicatat di `CONTEXT.md`. ADR dibuat setelah ada keputusan arsitektur dengan pertukaran manfaat dan biaya yang layak disimpan.
