use gpui_kit::component::TitleBar;
use gpui_kit::*;
use std::sync::{Arc, Mutex};
use window_hop::application::LifecycleCoordinator;
use window_hop::persistence::{default_config_path, FileProfileStore};
use window_hop::platform::{
    parse_activation_flag, ActivationCommand, LocalSingleInstanceCoordinator,
    SingleInstanceCoordinator,
};
use window_hop::ui::WindowHopView;

fn main() {
    let activate_profile_id = parse_activation_flag(std::env::args());

    let config_path = default_config_path();
    let base_data_dir = config_path.parent().unwrap_or(&config_path).to_path_buf();
    let ipc = Arc::new(LocalSingleInstanceCoordinator::new(base_data_dir));

    // Jika dipanggil dengan flag aktivasi shortcut, coba kirim ke instance pengelola yang sudah berjalan
    if let Some(ref profile_id) = activate_profile_id {
        let command = ActivationCommand::ActivateProfile(profile_id.clone());
        if let Ok(true) = ipc.send_activation(&command) {
            // Berhasil dikirim ke koordinator yang sedang berjalan; keluar segera tanpa membuka pengaturan
            return;
        }
    }

    // Jika belum ada instance berjalan, inisialisasi koordinator primary
    let store = Arc::new(FileProfileStore::new(config_path));
    let mut coordinator = LifecycleCoordinator::from_store(store);
    let _ = coordinator.load();

    let coordinator_arc = Arc::new(Mutex::new(coordinator));
    let coord_for_ipc = coordinator_arc.clone();
    let _ = ipc.start_listener(Box::new(move |command| {
        let ActivationCommand::ActivateProfile(pid) = command;
        if let Ok(mut coord) = coord_for_ipc.lock() {
            let _ = coord.handle_shortcut_activation(&pid);
        }
    }));

    if let Some(ref profile_id) = activate_profile_id {
        if let Ok(mut coord) = coordinator_arc.lock() {
            let _ = coord.handle_shortcut_activation(profile_id);
        }
    }

    if activate_profile_id.is_some() {
        // Dimulai dari shortcut profil: jangan buka pengaturan, tetap jalankan koordinator di latar belakang
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    } else {
        // Dimulai secara normal: buka jendela antarmuka pengaturan Window Hop
        gpui_kit::application().run(move |cx| {
            gpui_kit::init(cx);
            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Window Hop".into()),
                    ..TitleBar::title_bar_options()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(900.0), px(600.0)),
                    cx,
                ))),
                app_owns_titlebar_drag: true,
                ..Default::default()
            };

            let coord = coordinator_arc.clone();
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| WindowHopView::with_shared_coordinator(coord, window, cx))
            })
            .expect("gagal membuka jendela Window Hop");
        });
    }
}
