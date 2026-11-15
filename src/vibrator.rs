//! The phone's vibration motor, over JNI.

use jni::errors::Result;
use jni::objects::JString;
use jni::refs::Global;
use jni::sys::jlong;
use jni::{JavaVM, bind_java_type};
use winit::platform::android::activity::AndroidApp;

/// A buzz's length, and the gap between buzzes, in ms.
const BUZZ_MS: jlong = 100;

bind_java_type! {
    VibrationEffect => android.os.VibrationEffect,
    methods { static fn create_waveform(timings: jlong[], repeat: jint) -> VibrationEffect },
}

bind_java_type! {
    VibratorService => android.os.Vibrator,
    type_map = { VibrationEffect => android.os.VibrationEffect },
    methods { fn vibrate(effect: VibrationEffect) },
}

/// The system vibrator, or nothing if it couldn't be found.
pub struct Vibrator(Option<Global<VibratorService<'static>>>);

impl Vibrator {
    pub fn new(app: &AndroidApp) -> Self {
        let service = vm().attach_current_thread(|env| -> Result<_> {
            let context = crate::android::context(env, app)?;
            let name = JString::from_str(env, "vibrator")?;
            let service = context.get_system_service(env, &name)?;
            env.new_cast_global_ref::<VibratorService>(&service)
        });
        let service = service.inspect_err(|e| log::error!("finding the vibrator: {e}"));
        Self(service.ok())
    }

    /// Buzzes `n` times.
    pub fn buzz(&self, n: usize) {
        let Some(service) = &self.0 else { return };
        let result = vm().attach_current_thread(|env| -> Result<()> {
            // Off, then on, for each buzz.
            let timings: Vec<jlong> = (0..2 * n)
                .map(|i| if i == 0 { 0 } else { BUZZ_MS })
                .collect();
            let array = env.new_long_array(timings.len())?;
            array.set_region(env, 0, &timings)?;
            let effect = VibrationEffect::create_waveform(env, &array, -1)?;
            service.vibrate(env, &effect)
        });
        if let Err(e) = result {
            log::error!("vibrating: {e}");
        }
    }
}

fn vm() -> JavaVM {
    JavaVM::singleton().unwrap()
}
