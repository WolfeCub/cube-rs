# cube-rs

Rubik's cube timer for Android, written in Rust with [egui](https://github.com/emilk/egui).
Scrambles come from the official WCA scrambler, [tnoodle-lib](https://github.com/thewca/tnoodle-lib), via JNI.

## Usage

Hold the screen until the time turns green, release to start, touch to stop.

Hands-free, with the phone flat on the table: rest the cube on the proximity sensor at the
top of the screen until the time turns green, pick it up to start, and slam the table to stop.

## Development

```sh
direnv allow                                       # or: nix develop
cargo test                                         # timer logic and tnoodle on the host JVM
cargo build --target aarch64-linux-android         # the app library
nix build .#apk                                    # unsigned APK in result/
nix run .#install                                  # sign with the debug key, adb install
adb logcat -s cube_rs                              # logs
```

## Release build

`nix build .#apk` is unsigned. Sign it with the release keystore (prompts for the password):

```sh
nix run .#sign -- release.jks cube-rs.apk
```

Android only installs updates signed with the same key, so keep `release.jks` and its
password in a password manager. Debug and release builds can't be installed over each
other; uninstall first.

### Keystore backup

Store the keystore as base64 in the password manager entry alongside its password:

```sh
base64 -w0 release.jks | wl-copy           # paste into the entry
```

Restore it:

```sh
wl-paste | base64 -d > release.jks
```

## CI

CI builds every push. On the default branch, a `VERSION` without a GitHub release is
signed with `nix run .#sign` and released as `v$VERSION`. It needs two secrets:

```sh
gh secret set ANDROID_KEYSTORE_BASE64 < <(base64 -w0 release.jks)
gh secret set ANDROID_KEYSTORE_PASSWORD
```
