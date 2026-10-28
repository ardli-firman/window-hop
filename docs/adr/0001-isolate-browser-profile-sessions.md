---
status: accepted
---

# Pisahkan sesi browser untuk setiap profil Window Hop

Setiap profil Window Hop menggunakan data dan sesi browser tersendiri, terpisah dari browser kerja perawat dan profil lainnya. Pengguna menyepakati pemisahan ini agar aktivasi ulang serta penutupan saat monitor terputus hanya memengaruhi instance milik profil; memakai sesi browser biasa dapat meneruskan peluncuran ke instance kerja perawat yang sudah berjalan. Konsekuensinya, cookie dan login tidak diwarisi dari browser kerja perawat, sehingga login yang diperlukan dilakukan tersendiri pada sesi antrian.

Mekanisme pemisahan perlu mengikuti browser yang digunakan. Untuk Chromium, direktori data pengguna merupakan batas pemisahan instance. Dukungan browser lain harus diverifikasi sebelum mengandalkan perilaku yang sama. Sumber: [Chromium ProcessSingleton](https://chromium.googlesource.com/chromium/src/+/HEAD/chrome/browser/process_singleton.h) dan [user data directory](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/user_data_dir.md).

Pemisahan sesi tidak dengan sendirinya membuktikan kepemilikan setiap proses. Implementasi tetap harus memverifikasi kepemilikan sebelum menutup atau menghentikan proses.
