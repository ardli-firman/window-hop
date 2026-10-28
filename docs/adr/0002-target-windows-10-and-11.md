---
status: accepted
---

# Targetkan Windows 10 dan 11

Window Hop menargetkan Windows 10 dan 11 64-bit, tetap dengan Rust, GPUI, dan gpui-kit. Pengguna mengeluarkan Windows 7 dari cakupan setelah verifikasi bahwa backend GPUI yang diperiksa memanggil API yang tidak tersedia pada Windows 7. Dengan cakupan ini, proyek tidak membutuhkan port kompatibilitas GPUI untuk Windows 7 atau antarmuka legacy terpisah.

Pengguna kemudian memilih OS 64-bit saja untuk versi pertama, dengan kebutuhan menjalankan browser 32-bit maupun 64-bit. Window Hop sendiri tidak perlu dibangun untuk OS 32-bit; kompatibilitas pengelolaan browser kedua arsitektur tetap harus diuji.

Rujukan: [backend GPUI](https://docs.rs/crate/gpui-pre-windows/0.3.10/source/src/window.rs) dan [persyaratan GetDpiForWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdpiforwindow).
