use jni::JavaVM;
use jni::objects::{GlobalRef, JObject};
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
    let scrambles = crate::tnoodle::scrambles(vm(), class_loader(), "THREE");
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    let app = crate::ui::App::new(scrambles);
    eframe::run_native("cube-rs", options, Box::new(|_| Ok(Box::new(app)))).unwrap();
}

fn vm() -> JavaVM {
    unsafe { JavaVM::from_raw(ndk_context::android_context().vm().cast()) }.unwrap()
}

/// The activity's class loader, which can see classes.dex.
fn class_loader() -> GlobalRef {
    let vm = vm();
    let mut env = vm.attach_current_thread().unwrap();
    let activity = unsafe { JObject::from_raw(ndk_context::android_context().context().cast()) };
    let loader = env
        .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .and_then(|l| l.l())
        .unwrap();
    env.new_global_ref(loader).unwrap()
}
