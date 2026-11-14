//! WCA scrambles from tnoodle-lib over JNI.

use jni::errors::Result;
use jni::objects::{JClassLoader, JObject, JString, JValue};
use jni::refs::Global;
use jni::{Env, JavaVM, jni_sig, jni_str};
use std::sync::mpsc::{Receiver, Sender, channel, sync_channel};
use std::thread;

pub struct Scramble {
    pub moves: String,
    /// The scrambled puzzle as an SVG net. Empty if tnoodle couldn't draw it.
    pub svg: String,
}

/// tnoodle-lib in the process's JVM.
#[derive(Clone, Copy)]
pub struct Tnoodle {
    /// Used instead of `FindClass`, which on Android native threads only sees system classes.
    /// Leaked, like the JVM it belongs to.
    class_loader: &'static Global<JClassLoader<'static>>,
}

impl Tnoodle {
    /// The app's JVM, with tnoodle in classes.dex.
    #[cfg(target_os = "android")]
    pub fn android(app: &winit::platform::android::activity::AndroidApp) -> Self {
        Self::new(|env| {
            let activity = crate::android::activity(env, app);
            let loader = env
                .call_method(
                    &activity,
                    jni_str!("getClassLoader"),
                    jni_sig!(() -> java.lang.ClassLoader),
                    &[],
                )?
                .l()?;
            env.cast_local::<JClassLoader>(loader)
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
        JavaVM::new(args).unwrap();
        Self::new(JClassLoader::get_system_class_loader)
    }

    fn new(class_loader: impl for<'a> FnOnce(&mut Env<'a>) -> Result<JClassLoader<'a>>) -> Self {
        let class_loader = vm()
            .attach_current_thread(|env| {
                let loader = class_loader(env)?;
                env.new_global_ref(loader)
            })
            .unwrap();
        Self {
            class_loader: Box::leak(Box::new(class_loader)),
        }
    }

    /// Generates scrambles for `puzzle` (a `PuzzleRegistry` name such as `"THREE"`)
    /// on a background thread, keeping the next one ready.
    pub fn scrambles(self, puzzle: &'static str) -> Receiver<Scramble> {
        let (tx, rx) = sync_channel(1);
        self.spawn(puzzle, move |puzzle| {
            loop {
                let scramble = vm()
                    .attach_current_thread(|env| scramble(env, puzzle))
                    .unwrap();
                if tx.send(scramble).is_err() {
                    return;
                }
            }
        });
        rx
    }

    /// Draws scrambles of `puzzle` on a background thread, as they're asked for.
    pub fn drawer(self, puzzle: &'static str) -> Drawer {
        let (requests, rx) = channel::<String>();
        let (tx, drawn) = channel();
        self.spawn(puzzle, move |puzzle| {
            for moves in rx {
                let svg = vm()
                    .attach_current_thread(|env| draw(env, puzzle, &moves))
                    .unwrap_or_else(|e| {
                        log::error!("drawing {moves}: {e}");
                        String::new()
                    });
                if tx.send(Scramble { moves, svg }).is_err() {
                    return;
                }
            }
        });
        Drawer { requests, drawn }
    }

    /// Runs `work` on a new thread with `puzzle`'s scrambler.
    fn spawn(self, puzzle: &'static str, work: impl FnOnce(&JObject) + Send + 'static) {
        thread::spawn(move || {
            let puzzle = vm()
                .attach_current_thread(|env| {
                    let puzzle = scrambler(env, self.class_loader, puzzle)?;
                    env.new_global_ref(puzzle)
                })
                .unwrap();
            work(&puzzle);
        });
    }
}

/// Draws scrambles on a background thread.
pub struct Drawer {
    requests: Sender<String>,
    drawn: Receiver<Scramble>,
}

impl Drawer {
    /// Asks for `moves` to be drawn.
    pub fn request(&self, moves: &str) {
        let _ = self.requests.send(moves.to_owned());
    }

    /// A drawn scramble, if one is ready.
    pub fn try_recv(&self) -> Option<Scramble> {
        self.drawn.try_recv().ok()
    }
}

/// The process's one JVM.
fn vm() -> JavaVM {
    JavaVM::singleton().unwrap()
}

fn scrambler<'a>(env: &mut Env<'a>, loader: &JClassLoader, puzzle: &str) -> Result<JObject<'a>> {
    let name = env.new_string("org.worldcubeassociation.tnoodle.scrambles.PuzzleRegistry")?;
    let registry = loader.load_class(env, name)?;
    let puzzle = env.new_string(puzzle)?;
    let entry = env
        .call_static_method(
            registry,
            jni_str!("valueOf"),
            jni_sig!((java.lang.String) -> org.worldcubeassociation.tnoodle.scrambles.PuzzleRegistry),
            &[JValue::Object(&puzzle)],
        )?
        .l()?;
    env.call_method(
        &entry,
        jni_str!("getScrambler"),
        jni_sig!(() -> org.worldcubeassociation.tnoodle.scrambles.Puzzle),
        &[],
    )?
    .l()
}

fn scramble(env: &mut Env, puzzle: &JObject) -> Result<Scramble> {
    let moves = env
        .call_method(
            puzzle,
            jni_str!("generateScramble"),
            jni_sig!(() -> java.lang.String),
            &[],
        )?
        .l()?;
    let moves = env.cast_local::<JString>(moves)?.try_to_string(env)?;
    let svg = draw(env, puzzle, &moves)?;
    Ok(Scramble { moves, svg })
}

/// `moves` applied to `puzzle`, as an SVG in tnoodle's default colors.
fn draw(env: &mut Env, puzzle: &JObject, moves: &str) -> Result<String> {
    let moves = env.new_string(moves)?;
    let svg = env
        .call_method(
            puzzle,
            jni_str!("drawScramble"),
            jni_sig!(
                (java.lang.String, java.util.Map) -> org.worldcubeassociation.tnoodle.svglite.Svg
            ),
            &[JValue::Object(&moves), JValue::Object(&JObject::null())],
        )?
        .l()?;
    let svg = env
        .call_method(
            &svg,
            jni_str!("toString"),
            jni_sig!(() -> java.lang.String),
            &[],
        )?
        .l()?;
    env.cast_local::<JString>(svg)?.try_to_string(env)
}

#[cfg(test)]
mod tests {
    /// One test: a process can only start one JVM.
    #[test]
    fn three_by_three() {
        let tnoodle = super::Tnoodle::host();
        let scramble = tnoodle.scrambles("THREE").recv().unwrap();
        let moves: Vec<&str> = scramble.moves.split_whitespace().collect();
        assert!((15..=25).contains(&moves.len()), "{}", scramble.moves);
        let face = |m: &&str| "URFDLB".contains(&m[..1]) && matches!(&m[1..], "" | "'" | "2");
        assert!(moves.iter().all(face), "{}", scramble.moves);
        assert!(scramble.svg.starts_with("<svg"), "{}", scramble.svg);

        let drawer = tnoodle.drawer("THREE");
        drawer.request(&scramble.moves);
        drawer.request("not a scramble");
        let drawn = drawer.drawn.recv().unwrap();
        assert_eq!((drawn.moves, drawn.svg), (scramble.moves, scramble.svg));
        assert_eq!(drawer.drawn.recv().unwrap().svg, "");
    }
}
