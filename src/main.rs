use gpui_kit::component::TitleBar;
use gpui_kit::*;
use std::sync::{Arc, Mutex};
use window_hop::application::LifecycleCoordinator;
use window_hop::persistence::{default_config_path, FileProfileStore};
use window_hop::platform::{
    parse_activation_flag, parse_silent_or_startup_flag, ActivationCommand,
    LocalSingleInstanceCoordinator, SingleInstanceCoordinator, TrayAction,
};
use window_hop::ui::WindowHopView;

enum AppEvent {
    OpenSettings,
    Exit,
}

fn open_or_focus_settings(
    coord: Arc<Mutex<LifecycleCoordinator>>,
    window_slot: Arc<Mutex<Option<AnyWindowHandle>>>,
    cx: &mut App,
) {
    let mut slot = window_slot.lock().unwrap();
    if let Some(handle) = *slot {
        let activated = handle.update(cx, |_view, window, _cx| {
            window.activate_window();
        });
        if activated.is_ok() {
            return;
        }
    }

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

    if let Ok((handle, _view)) = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| WindowHopView::with_shared_coordinator(coord, window, cx))
    }) {
        *slot = Some(handle);
    }
}

fn main() {
    let activate_profile_id = parse_activation_flag(std::env::args());
    let is_silent_or_startup = parse_silent_or_startup_flag(std::env::args());

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
    } else if is_silent_or_startup {
        // Jika dipanggil silent/startup dan sudah ada instance primer yang berjalan di tray,
        // kirim Startup agar instance primer mengaktifkan profil startup jika belum aktif,
        // lalu keluar segera agar tidak membuat instance ganda
        if let Ok(true) = ipc.send_activation(&ActivationCommand::Startup) {
            return;
        }
    } else {
        // Jika dipanggil secara normal tanpa argumen dan sudah ada instance berjalan di tray, minta buka pengaturan
        if let Ok(true) = ipc.send_activation(&ActivationCommand::OpenSettings) {
            return;
        }
    }

    // Jika belum ada instance berjalan, inisialisasi koordinator primary
    let store = Arc::new(FileProfileStore::new(config_path));
    let mut coordinator = LifecycleCoordinator::from_store(store);
    let _ = coordinator.load();

    let coordinator_arc = Arc::new(Mutex::new(coordinator));
    let window_slot: Arc<Mutex<Option<AnyWindowHandle>>> = Arc::new(Mutex::new(None));

    let should_open_settings = activate_profile_id.is_none() && !is_silent_or_startup;

    let (event_tx, event_rx) = async_channel::unbounded::<AppEvent>();

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        // Menjaga aplikasi tetap berjalan di system tray meskipun jendela pengaturan belum dibuka atau ditutup
        cx.set_quit_mode(QuitMode::Explicit);

        // 1. Daftarkan IPC listener untuk menerima aktivasi shortcut dan perintah buka pengaturan dari instance sekunder
        let coord_for_ipc = coordinator_arc.clone();
        let tx_for_ipc = event_tx.clone();
        let _ = ipc.start_listener(Box::new(move |command| match command {
            ActivationCommand::ActivateProfile(pid) => {
                if let Ok(mut coord) = coord_for_ipc.lock() {
                    let _ = coord.handle_shortcut_activation(&pid);
                }
            }
            ActivationCommand::Startup => {
                if let Ok(mut coord) = coord_for_ipc.lock() {
                    let _ = coord.handle_startup_activation();
                }
            }
            ActivationCommand::OpenSettings => {
                let _ = tx_for_ipc.send_blocking(AppEvent::OpenSettings);
            }
            ActivationCommand::Ping => {
                // Instance primer merespons Ping (ACK OK otomatis dikirimkan ke stream IPC)
            }
        }));

        // 2. Hubungkan aksi system tray (Buka Pengaturan, Keluar dari Window Hop)
        let tray_provider = {
            let coord = coordinator_arc.lock().unwrap();
            coord.tray_provider().clone()
        };

        let tx_for_tray = event_tx.clone();
        let _ = tray_provider.set_action_handler(Box::new(move |action| match action {
            TrayAction::OpenSettings => {
                let _ = tx_for_tray.send_blocking(AppEvent::OpenSettings);
            }
            TrayAction::Exit => {
                let _ = tx_for_tray.send_blocking(AppEvent::Exit);
            }
        }));

        // 3. Tangani event aplikasi (Buka Pengaturan, Keluar) dari channel asinkron tanpa memblokir thread UI
        let coord_for_events = coordinator_arc.clone();
        let slot_for_events = window_slot.clone();
        let tray_for_events = tray_provider.clone();

        cx.spawn(move |cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                while let Ok(event) = event_rx.recv().await {
                    match event {
                        AppEvent::OpenSettings => {
                            let coord = coord_for_events.clone();
                            let slot = slot_for_events.clone();
                            cx.update(|cx| {
                                open_or_focus_settings(coord, slot, cx);
                            });
                        }
                        AppEvent::Exit => {
                            let coord = coord_for_events.clone();
                            let tray = tray_for_events.clone();
                            // Shutdown processes on background thread off the UI event loop
                            cx.background_executor()
                                .spawn(async move {
                                    if let Ok(mut c) = coord.lock() {
                                        let _ = c.shutdown();
                                    }
                                    let _ = tray.remove_tray();
                                })
                                .await;

                            cx.update(|cx| {
                                cx.quit();
                            });
                        }
                    }
                }
            }
        })
        .detach();

        // 4. Perbarui status tray awal
        if let Ok(coord) = coordinator_arc.lock() {
            let _ = coord.update_tray_status();
        }

        // 5. Jika dimulai dengan flag aktivasi shortcut, jalankan aktivasi profil di latar belakang
        if let Some(ref profile_id) = activate_profile_id {
            if let Ok(mut coord) = coordinator_arc.lock() {
                let _ = coord.handle_shortcut_activation(profile_id);
            }
        }

        // Jika dimulai dengan flag startup/silent/tray, jalankan aktivasi profil startup di latar belakang
        if is_silent_or_startup {
            if let Ok(mut coord) = coordinator_arc.lock() {
                let _ = coord.handle_startup_activation();
            }
        }

        // 6. Buka jendela pengaturan jika dipanggil secara normal (bukan startup/silent/shortcut)
        if should_open_settings {
            open_or_focus_settings(coordinator_arc.clone(), window_slot.clone(), cx);
        }
    });
}
