use crate::{microphone, sensors::Sensors, tnoodle::Tnoodle, trace::Tracer, ui::App};
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
    let tnoodle = Tnoodle::android(&app);
    let (scrambles, drawer) = (tnoodle.scrambles("THREE"), tnoodle.drawer("THREE"));
    // External so adb can read it without a debuggable build. Internal if it's unmounted.
    let data = app
        .external_data_path()
        .or_else(|| app.internal_data_path())
        .expect("no data path");
    let tracer = Tracer::new(data.join("traces"));
    microphone::start(tracer.clone());
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    eframe::run_native(
        "cube-rs",
        options,
        Box::new(|cc| {
            let ctx = cc.egui_ctx.clone();
            let sensors = Sensors::start(move || ctx.request_repaint(), tracer.clone());
            let app = App::new(&cc.egui_ctx, scrambles, drawer, sensors, tracer, data);
            Ok(Box::new(app))
        }),
    )
    .unwrap();
}
