//! WCA scrambles from tnoodle-lib over JNI.

use jni::errors::Result;
use jni::objects::{JClassLoader, JMap, JString, JThread};
use jni::refs::Global;
use jni::{Env, JavaVM, bind_java_type};
use std::sync::mpsc::{Receiver, Sender, channel, sync_channel};
use std::thread;

bind_java_type! {
    PuzzleRegistry => org.worldcubeassociation.tnoodle.scrambles.PuzzleRegistry,
    type_map = { Puzzle => org.worldcubeassociation.tnoodle.scrambles.Puzzle },
    methods {
        static fn value_of(name: JString) -> PuzzleRegistry,
        fn get_scrambler() -> Puzzle,
    },
}

bind_java_type! {
    Puzzle => org.worldcubeassociation.tnoodle.scrambles.Puzzle,
    type_map = { Svg => org.worldcubeassociation.tnoodle.svglite.Svg },
    methods {
        fn generate_scramble() -> JString,
        fn draw_scramble(moves: JString, colors: java.util.Map) -> Svg,
    },
}

bind_java_type! {
    Svg => org.worldcubeassociation.tnoodle.svglite.Svg,
    methods { fn text { sig = () -> JString, name = "toString" } },
}

pub struct Scramble {
    pub moves: String,
    /// The scrambled puzzle as an SVG net. Empty if tnoodle couldn't draw it.
    pub svg: String,
}

/// tnoodle-lib in the process's JVM.
#[derive(Clone, Copy)]
pub struct Tnoodle {
    /// The loader that sees tnoodle. Threads started from Rust only see system classes until
    /// it's made their context class loader. Leaked, like the JVM it belongs to.
    class_loader: &'static Global<JClassLoader<'static>>,
}

impl Tnoodle {
    /// The app's JVM, with tnoodle in classes.dex.
    #[cfg(target_os = "android")]
    pub fn android(app: &winit::platform::android::activity::AndroidApp) -> Self {
        Self::new(|env| crate::android::context(env, app)?.get_class_loader(env))
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
    fn spawn(self, puzzle: &'static str, work: impl FnOnce(&Puzzle) + Send + 'static) {
        thread::spawn(move || {
            let puzzle = vm()
                .attach_current_thread(|env| {
                    JThread::current_thread(env)?
                        .set_context_class_loader(env, self.class_loader)?;
                    let name = JString::from_str(env, puzzle)?;
                    let puzzle = PuzzleRegistry::value_of(env, &name)?.get_scrambler(env)?;
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

fn scramble(env: &mut Env, puzzle: &Puzzle) -> Result<Scramble> {
    let moves = puzzle.generate_scramble(env)?.try_to_string(env)?;
    let svg = draw(env, puzzle, &moves)?;
    Ok(Scramble { moves, svg })
}

/// `moves` applied to `puzzle`, as an SVG in tnoodle's default colors.
fn draw(env: &mut Env, puzzle: &Puzzle, moves: &str) -> Result<String> {
    let moves = JString::from_str(env, moves)?;
    let svg = puzzle.draw_scramble(env, &moves, JMap::null())?;
    svg.text(env)?.try_to_string(env)
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
