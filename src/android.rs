use crate::{
    microphone, sensors::Sensors, session::Session, tnoodle::Tnoodle, trace::Tracer, ui::App,
};
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
    // External so `adb pull` can read it without a debuggable build.
    let tracer = app
        .external_data_path()
        .map_or_else(Tracer::default, |dir| Tracer::new(dir.join("traces")));
    microphone::start(tracer.clone());
    let data = app.internal_data_path();
    let session = data.as_ref().map_or_else(Session::default, |dir| {
        Session::open(&dir.join("solves.jsonl"))
    });
    let settings = data.map(|dir| dir.join("settings.json"));
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
            let app = App::new(&cc.egui_ctx, scrambles, sensors, tracer, session, settings);
            Ok(Box::new(app))
        }),
    )
    .unwrap();
}
