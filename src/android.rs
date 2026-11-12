use crate::{sensors::Sensors, tnoodle::Tnoodle, ui::App};
use winit::platform::android::activity::{AndroidApp, WindowManagerFlags};

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("cube_rs"),
    );
    std::panic::set_hook(Box::new(|info| log::error!("{info}")));

    app.set_window_flags(
        WindowManagerFlags::KEEP_SCREEN_ON,
        WindowManagerFlags::empty(),
    );
    let scrambles = Tnoodle::android().scrambles("THREE");
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    eframe::run_native(
        "cube-rs",
        options,
        Box::new(|cc| {
            let ctx = cc.egui_ctx.clone();
            let sensors = Sensors::start(move || ctx.request_repaint());
            Ok(Box::new(App::new(&cc.egui_ctx, scrambles, sensors)))
        }),
    )
    .unwrap();
}
