pub mod gesture;
pub mod session;
pub mod sessions;
pub mod settings;
pub mod store;
pub mod timer;
pub mod tnoodle;
pub mod trace;

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod microphone;
#[cfg(target_os = "android")]
mod sensors;
#[cfg(target_os = "android")]
mod sys;
#[cfg(target_os = "android")]
mod ui;
#[cfg(target_os = "android")]
mod vibrator;

/// A temp path for test `name` with nothing at it.
#[cfg(test)]
fn test_path(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("cube-rs-{name}-{}", std::process::id()));
    // Left by an earlier run with the same pid.
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::remove_file(&path);
    path
}
