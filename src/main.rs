use gpui_kit::*;
use std::sync::Arc;
use window_hop::application::LifecycleCoordinator;
use window_hop::persistence::{default_config_path, FileProfileStore};
use window_hop::ui::WindowHopView;

fn main() {
    let config_path = default_config_path();
    let store = Arc::new(FileProfileStore::new(config_path));
    let mut coordinator = LifecycleCoordinator::new(store);
    let _ = coordinator.load();

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Window Hop".into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(900.0), px(600.0)),
                cx,
            ))),
            ..Default::default()
        };

        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| WindowHopView::new(coordinator, window, cx))
        })
        .expect("gagal membuka jendela Window Hop");
    });
}
