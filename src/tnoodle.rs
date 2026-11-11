//! WCA scrambles from tnoodle-lib over JNI.
//!
//! Classes are loaded through a `ClassLoader` because `FindClass` on a native
//! thread only sees system classes on Android, not the app's dex.

use jni::errors::Result;
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};
use std::sync::mpsc::{Receiver, sync_channel};
use std::thread;

/// Generates scrambles for `puzzle` (a `PuzzleRegistry` name such as `"THREE"`)
/// on a background thread, keeping the next one ready.
pub fn scrambles(vm: JavaVM, class_loader: GlobalRef, puzzle: &'static str) -> Receiver<String> {
    let (tx, rx) = sync_channel(1);
    thread::spawn(move || {
        let mut env = vm.attach_current_thread_permanently().expect("attach JVM");
        let puzzle = scrambler(&mut env, &class_loader, puzzle).expect("load tnoodle");
        loop {
            let scramble = env
                .with_local_frame(4, |env| -> Result<String> {
                    let s = env
                        .call_method(&puzzle, "generateScramble", "()Ljava/lang/String;", &[])?
                        .l()?;
                    Ok(env.get_string(&JString::from(s))?.into())
                })
                .expect("generate scramble");
            if tx.send(scramble).is_err() {
                return;
            }
        }
    });
    rx
}

fn scrambler<'a>(
    env: &mut JNIEnv<'a>,
    class_loader: &JObject,
    puzzle: &str,
) -> Result<JObject<'a>> {
    let name = env.new_string("org.worldcubeassociation.tnoodle.scrambles.PuzzleRegistry")?;
    let registry = env
        .call_method(
            class_loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&name)],
        )?
        .l()?;
    let puzzle = env.new_string(puzzle)?;
    let entry = env
        .call_static_method(
            JClass::from(registry),
            "valueOf",
            "(Ljava/lang/String;)Lorg/worldcubeassociation/tnoodle/scrambles/PuzzleRegistry;",
            &[JValue::Object(&puzzle)],
        )?
        .l()?;
    env.call_method(
        &entry,
        "getScrambler",
        "()Lorg/worldcubeassociation/tnoodle/scrambles/Puzzle;",
        &[],
    )?
    .l()
}

#[cfg(test)]
mod tests {
    use jni::{InitArgsBuilder, JavaVM};

    #[test]
    fn three_by_three() {
        let classpath = std::env::var("TNOODLE_CLASSPATH").expect("TNOODLE_CLASSPATH");
        let args = InitArgsBuilder::new()
            .option(format!("-Djava.class.path={classpath}"))
            .build()
            .unwrap();
        let vm = JavaVM::new(args).unwrap();
        let loader = {
            let mut env = vm.attach_current_thread().unwrap();
            let loader = env
                .call_static_method(
                    "java/lang/ClassLoader",
                    "getSystemClassLoader",
                    "()Ljava/lang/ClassLoader;",
                    &[],
                )
                .and_then(|l| l.l())
                .unwrap();
            env.new_global_ref(loader).unwrap()
        };

        let scramble = super::scrambles(vm, loader, "THREE").recv().unwrap();
        let moves: Vec<&str> = scramble.split_whitespace().collect();
        assert!((15..=25).contains(&moves.len()), "{scramble}");
        let face = |m: &&str| "URFDLB".contains(&m[..1]) && matches!(&m[1..], "" | "'" | "2");
        assert!(moves.iter().all(face), "{scramble}");
    }
}
