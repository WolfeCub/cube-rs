//! WCA scrambles from tnoodle-lib over JNI.

use jni::errors::Result;
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};
use std::sync::mpsc::{Receiver, sync_channel};
use std::thread;

pub struct Scramble {
    pub moves: String,
    /// The scrambled puzzle as an SVG net.
    pub svg: String,
}

/// A JVM with tnoodle-lib on its class path.
pub struct Tnoodle {
    vm: JavaVM,
    /// Used instead of `FindClass`, which on Android native threads only sees system classes.
    class_loader: GlobalRef,
}

impl Tnoodle {
    /// The app's JVM, with tnoodle in classes.dex.
    #[cfg(target_os = "android")]
    pub fn android() -> Self {
        let ctx = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.unwrap();
        let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
        Self::new(vm, |env| {
            object(
                env,
                &activity,
                "getClassLoader",
                "()Ljava/lang/ClassLoader;",
                &[],
            )
        })
    }

    /// A new JVM with `TNOODLE_CLASSPATH`.
    #[cfg(not(target_os = "android"))]
    pub fn host() -> Self {
        let classpath = std::env::var("TNOODLE_CLASSPATH").expect("TNOODLE_CLASSPATH");
        let args = jni::InitArgsBuilder::new()
            .option(format!("-Djava.class.path={classpath}"))
            .build()
            .unwrap();
        Self::new(JavaVM::new(args).unwrap(), |env| {
            env.call_static_method(
                "java/lang/ClassLoader",
                "getSystemClassLoader",
                "()Ljava/lang/ClassLoader;",
                &[],
            )?
            .l()
        })
    }

    fn new(
        vm: JavaVM,
        class_loader: impl for<'a> FnOnce(&mut JNIEnv<'a>) -> Result<JObject<'a>>,
    ) -> Self {
        let class_loader = {
            let mut env = vm.attach_current_thread().unwrap();
            let loader = class_loader(&mut env).unwrap();
            env.new_global_ref(loader).unwrap()
        };
        Self { vm, class_loader }
    }

    /// Generates scrambles for `puzzle` (a `PuzzleRegistry` name such as `"THREE"`)
    /// on a background thread, keeping the next one ready.
    pub fn scrambles(self, puzzle: &'static str) -> Receiver<Scramble> {
        let (tx, rx) = sync_channel(1);
        thread::spawn(move || {
            let mut env = self.vm.attach_current_thread_permanently().unwrap();
            let puzzle = scrambler(&mut env, &self.class_loader, puzzle).unwrap();
            while tx
                .send(
                    env.with_local_frame(8, |env| scramble(env, &puzzle))
                        .unwrap(),
                )
                .is_ok()
            {}
        });
        rx
    }
}

fn scrambler<'a>(
    env: &mut JNIEnv<'a>,
    class_loader: &JObject,
    puzzle: &str,
) -> Result<JObject<'a>> {
    let name = env.new_string("org.worldcubeassociation.tnoodle.scrambles.PuzzleRegistry")?;
    let registry = object(
        env,
        class_loader,
        "loadClass",
        "(Ljava/lang/String;)Ljava/lang/Class;",
        &[JValue::Object(&name)],
    )?;
    let puzzle = env.new_string(puzzle)?;
    let entry = env
        .call_static_method(
            JClass::from(registry),
            "valueOf",
            "(Ljava/lang/String;)Lorg/worldcubeassociation/tnoodle/scrambles/PuzzleRegistry;",
            &[JValue::Object(&puzzle)],
        )?
        .l()?;
    object(
        env,
        &entry,
        "getScrambler",
        "()Lorg/worldcubeassociation/tnoodle/scrambles/Puzzle;",
        &[],
    )
}

fn scramble(env: &mut JNIEnv, puzzle: &JObject) -> Result<Scramble> {
    let moves = object(env, puzzle, "generateScramble", "()Ljava/lang/String;", &[])?;
    let svg = object(
        env,
        puzzle,
        "drawScramble",
        "(Ljava/lang/String;Ljava/util/Map;)Lorg/worldcubeassociation/tnoodle/svglite/Svg;",
        &[JValue::Object(&moves), JValue::Object(&JObject::null())],
    )?;
    let svg = object(env, &svg, "toString", "()Ljava/lang/String;", &[])?;
    Ok(Scramble {
        moves: env.get_string(&JString::from(moves))?.into(),
        svg: env.get_string(&JString::from(svg))?.into(),
    })
}

/// Calls a method that returns an object.
fn object<'a>(
    env: &mut JNIEnv<'a>,
    obj: &JObject,
    method: &str,
    sig: &str,
    args: &[JValue],
) -> Result<JObject<'a>> {
    env.call_method(obj, method, sig, args)?.l()
}

#[cfg(test)]
mod tests {
    #[test]
    fn three_by_three() {
        let scramble = super::Tnoodle::host().scrambles("THREE").recv().unwrap();
        let moves: Vec<&str> = scramble.moves.split_whitespace().collect();
        assert!((15..=25).contains(&moves.len()), "{}", scramble.moves);
        let face = |m: &&str| "URFDLB".contains(&m[..1]) && matches!(&m[1..], "" | "'" | "2");
        assert!(moves.iter().all(face), "{}", scramble.moves);
        assert!(scramble.svg.starts_with("<svg"), "{}", scramble.svg);
    }
}
