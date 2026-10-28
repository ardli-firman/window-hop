# Kriteria penerimaan Window Hop

Status: kriteria penerimaan untuk spesifikasi versi pertama yang disetujui pengguna pada 28 Oktober 2026; pengujian belum dijalankan. Uji membutuhkan Windows dan monitor eksternal nyata; simulasi tidak membuktikan perilaku penempatan window atau kehilangan koneksi.

| Skenario | Hasil yang diharapkan |
| --- | --- |
| Login dengan startup aktif dan monitor tujuan tersedia | Window Hop ada di tray, UI pengaturan tidak terbuka, profil startup berjalan di monitor tujuan. |
| Login dengan monitor tujuan belum tersedia | Browser profil tidak diluncurkan. Setelah monitor yang sama tersedia, profil berjalan. |
| Startup dinonaktifkan melalui UI | Login berikutnya tidak otomatis menjalankan Window Hop. |
| Shortcut profil diklik berulang dengan konfigurasi sama | Tidak menggandakan instance profil; window yang dikelola kembali ke monitor tujuan. |
| Browser kerja perawat sudah terbuka | Aktivasi dan penutupan profil tidak memindahkan atau menghentikan browser kerja tersebut. |
| Monitor tujuan terputus | Browser profil diminta tutup; setelah 3 detik jika perlu dihentikan paksa, terbatas pada instance milik profil. |
| Monitor tersambung kembali setelah kehilangan koneksi | Profil yang berhenti karena kehilangan koneksi dijalankan kembali. |
| Browser crash atau ditutup manual, lalu monitor reconnect | Browser tetap berhenti sampai shortcut diaktifkan atau login Windows berikutnya. |
| Monitor diganti atau identitasnya ambigu | Tidak ada fallback ke primary atau monitor lain; status meminta pemilihan monitor tujuan ulang. |
| Profil lain diaktifkan pada monitor yang sama | Browser profil lama berhenti sebelum browser profil baru dijalankan. |
| Parameter profil aktif diedit dan disimpan | Tayangan berjalan tetap berlangsung. Tindakan menjalankan ulang atau aktivasi berikutnya menerapkan konfigurasi terbaru. |
| Profil aktif dihapus | UI meminta konfirmasi; setelah dikonfirmasi, instance profil ditutup dengan batas 3 detik. |
| Keluar dari tray | Browser milik profil ditutup sebelum Window Hop berhenti. |
| Parameter melanggar pemisahan sesi | UI memberi alasan penolakan; sesi kerja perawat tidak digunakan. |
| Browser belum bisa diverifikasi pemisahan sesi/kepemilikannya | Profil dapat disimpan, tetapi aktivasi ditolak dengan alasan yang jelas. |
| Membuat shortcut | Default Desktop dan folder pilihan lain bekerja; aktivasi tidak membuka halaman pengaturan. |
| Kegagalan aktivasi otomatis | Tray dan detail UI menunjukkan kegagalan tanpa membuka window atau merebut fokus. |
| Kegagalan tindakan melalui UI | Kesalahan muncul pada UI yang sedang digunakan. |
| Browser x86 dan x64 pada Windows x64 | Jalankan skenario profil yang sama untuk masing-masing browser yang dinyatakan kompatibel. |

## Catatan pengamatan

Untuk setiap kombinasi browser, versi OS, dan GPU, catat kemunculan sesaat di primary serta perubahan fokus saat peluncuran dan kehilangan koneksi. Rancangan tidak menjanjikan nol kemunculan di primary pada semua browser.

Hasil uji harus mencantumkan versi browser, arsitektur browser, versi Windows, serta status lulus/gagal dan batasan. Jangan menyebut browser didukung hanya karena executable-nya terdeteksi atau profilnya bisa disimpan.
