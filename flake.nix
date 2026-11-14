{
  description = "Rubik's cube timer for Android";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    # tnoodle scramblers and their deps in one jar: dexed into the APK, and run in a host JVM by the tests.
    tnoodle = {
      url = "file+https://repo1.maven.org/maven2/org/worldcubeassociation/tnoodle/lib-scrambles/0.20.0/lib-scrambles-0.20.0-all.jar";
      flake = false;
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    rust-overlay,
    tnoodle,
  }: let
    # The NDK only ships linux-x86_64 binaries.
    system = "x86_64-linux";
    pkgs = import nixpkgs {
      inherit system;
      overlays = [rust-overlay.overlays.default];
      config = {
        allowUnfree = true;
        android_sdk.accept_license = true;
      };
    };
    inherit (pkgs) lib jdk;

    version = lib.fileContents ./VERSION;
    # 1.2.3 -> 10203
    versionCode = lib.foldl (code: n: code * 100 + lib.toInt n) 0 (lib.splitString "." version);

    minSdk = "26";
    targetSdk = "36";
    buildTools = "36.0.0";
    ndk = "29.0.14206865";
    # Its bin/ has Google's adb, which supports wireless pairing (nixpkgs' android-tools doesn't).
    androidsdk =
      (pkgs.androidenv.composeAndroidPackages {
        platformVersions = [targetSdk];
        buildToolsVersions = [buildTools];
        includeNDK = true;
        ndkVersion = ndk;
        includeCmake = false;
        toolsVersion = null;
      }).androidsdk;
    sdk = "${androidsdk}/libexec/android-sdk";
    bin = "${sdk}/build-tools/${buildTools}";
    androidJar = "${sdk}/platforms/android-${targetSdk}/android.jar";

    # Minimal; the dev shell adds the tools.
    toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
    craneLib = (crane.mkLib pkgs).overrideToolchain (_: toolchain);
    common = {
      src = craneLib.cleanCargoSource ./.;
      strictDeps = true;
    };
    linker = {
      CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "${sdk}/ndk/${ndk}/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android${minSdk}-clang";
    };

    jvmEnv = {
      JAVA_HOME = jdk.home;
      TNOODLE_CLASSPATH = "${tnoodle}";
    };

    # libcube_rs.so
    native = let
      args =
        common
        // linker
        // {
          CARGO_BUILD_TARGET = "aarch64-linux-android";
          doCheck = false;
        };
      # Skips crane's `cargo check` pass: its .rmeta only serves clippy, which isn't run here.
      deps = craneLib.buildDepsOnly (args // {buildPhaseCargoCommand = "cargoWithProfile build --locked";});
    in
      craneLib.buildPackage (args // {cargoArtifacts = deps;});

    # Unsigned; see `sign`.
    apk = pkgs.runCommand "cube-rs-${version}" {nativeBuildInputs = [jdk pkgs.zip];} ''
      PATH=${bin}:$PATH
      aapt2 compile --dir ${./res} -o res.zip
      aapt2 link -o app.apk -I ${androidJar} --manifest ${./AndroidManifest.xml} -R res.zip \
        --min-sdk-version ${minSdk} --target-sdk-version ${targetSdk} \
        --version-code ${toString versionCode} --version-name ${version}
      # d8 needs the .jar extension.
      ln -s ${tnoodle} tnoodle.jar
      d8 --release --min-api ${minSdk} --lib ${androidJar} --output . tnoodle.jar
      install -D ${native}/lib/libcube_rs.so lib/arm64-v8a/libcube_rs.so
      find classes*.dex lib -exec touch -d @$SOURCE_DATE_EPOCH {} +
      zip -qrX app.apk classes*.dex lib
      mkdir $out
      zipalign -P 16 -f 4 app.apk $out/cube-rs.apk
    '';

    # Dev profile: no LTO or optimisation for tests.
    test = let
      args = common // jvmEnv // {CARGO_PROFILE = "dev";};
    in
      craneLib.cargoTest (args // {cargoArtifacts = craneLib.buildDepsOnly args;});

    # sign KEYSTORE OUT [apksigner options]. Prompts for the password unless --ks-pass is given.
    sign = pkgs.writeShellApplication {
      name = "sign";
      runtimeInputs = [jdk];
      text = ''
        ks=$1 out=$2
        shift 2
        ${bin}/apksigner sign --ks "$ks" --out "$out" "$@" ${apk}/cube-rs.apk
      '';
    };

    # Signs with the debug key and installs over adb.
    install = pkgs.writeShellApplication {
      name = "install";
      runtimeInputs = [jdk sign androidsdk];
      text = ''
        ks=~/.android/debug.keystore
        if [ ! -f "$ks" ]; then
          mkdir -p ~/.android
          keytool -genkeypair -keystore "$ks" -storepass android -alias androiddebugkey \
            -keyalg RSA -validity 10000 -dname "CN=Android Debug,O=Android,C=US"
        fi
        out=$(mktemp -d)/cube-rs.apk
        sign "$ks" "$out" --ks-pass pass:android
        adb install -r "$out"
      '';
    };

    # traces pull [DIR] | traces clear: the solve traces on the device (src/trace.rs).
    traces = pkgs.writeShellApplication {
      name = "traces";
      runtimeInputs = [androidsdk];
      text = ''
        dir=/sdcard/Android/data/io.github.wolfecub.cube_rs/files/traces
        case ''${1-} in
          pull) adb pull "$dir" "''${2:-.}" ;;
          clear) adb shell rm -rf "$dir" ;;
          *) echo "usage: traces pull [DIR] | traces clear" >&2 && exit 1 ;;
        esac
      '';
    };

    # Everything CI builds, as one GC root for its cache. `inputs` keeps nixpkgs and the other sources.
    ci = pkgs.linkFarm "ci" {
      inherit apk test;
      inputs = pkgs.linkFarm "inputs" self.inputs;
    };
  in {
    packages.${system} = {
      default = apk;
      inherit apk native sign install traces ci;
    };

    checks.${system} = {inherit test;};

    devShells.${system}.default = pkgs.mkShell (linker
      // jvmEnv
      // {
        ANDROID_HOME = sdk;
        packages = [
          (toolchain.override {extensions = ["clippy" "rustfmt" "rust-src" "rust-analyzer"];})
          jdk
          androidsdk
        ];
      });
  };
}
