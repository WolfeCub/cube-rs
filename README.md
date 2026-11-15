<div align="center">

# cube-rs

**A Rubik's cube timer for Android, written in Rust.**

[![CI](https://github.com/WolfeCub/cube-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/WolfeCub/cube-rs/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/WolfeCub/cube-rs)](https://github.com/WolfeCub/cube-rs/releases/latest)
![Android 8.0+](https://img.shields.io/badge/Android-8.0%2B-3DDC84?logo=android&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)

</div>

## Features

- **Touch timing**: hold until the time turns green, release to start, touch to stop
- **Slam to stop**: with the phone flat on a table, slam the table next to it
- **WCA inspection**, optional: 15 s with +2 and DNF penalties, vibrating at 8 s and 12 s
- **Official WCA scrambles**, with a picture of the scrambled cube
- **Sessions**: named, saved across restarts, each with a list of its solves and scrambles
- **Stats**: mean, best and current ao5 to ao1000, and a graph of times
- **Penalties**: mark the last solve OK, +2 or DNF, or discard it
- **Settings**: slam sensitivity, hold time, inspection and solve traces

### Tech stack

- **Rust** for the whole app, on `NativeActivity` (no Java or Kotlin)
- **[egui](https://github.com/emilk/egui)** for the UI
- **[tnoodle-lib](https://github.com/thewca/tnoodle-lib)**, the WCA's scrambler, over JNI
- **NDK sensors**: the accelerometer at its fastest rate (~400 Hz)
- **Nix** builds the APK, no Gradle:
  - [crane](https://github.com/ipetkov/crane) cross-compiles the Rust library
  - aapt2, d8 and zipalign package it
- **GitHub Actions** builds, signs and releases

## Development

```sh
direnv allow                                  # or: nix develop
cargo test                                    # host tests, tnoodle on a real JVM
cargo build --target aarch64-linux-android    # the app library
nix build .#apk                               # unsigned APK in result/
nix run .#install                             # sign with the debug key, adb install
adb logcat -s cube_rs                         # logs
```

## Releasing

Bump `VERSION` and push. CI builds every push. On the default branch, a `VERSION` without
a GitHub release is signed and released as `v$VERSION`. It needs two secrets:

```sh
gh secret set ANDROID_KEYSTORE_BASE64 < <(base64 -w0 release.jks)
gh secret set ANDROID_KEYSTORE_PASSWORD
```

### Signing locally

`nix build .#apk` is unsigned. Sign it with the release keystore (prompts for the password):

```sh
nix run .#sign -- release.jks cube-rs.apk
```

> [!IMPORTANT]
> Android only installs updates signed with the same key, so keep `release.jks` and its
> password in a password manager. Debug and release builds can't be installed over each
> other; uninstall first.

### Keystore backup

Keep the keystore as base64 in the password manager entry, next to its password:

```sh
base64 -w0 release.jks | wl-copy              # back up: paste into the entry
wl-paste | base64 -d > release.jks            # restore
```
