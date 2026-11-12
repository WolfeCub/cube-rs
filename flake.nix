{
  description = "Rubik's cube timer for Android";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    rust-overlay,
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

    # Minimal; the dev shell adds the tools.
    rust = p: p.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
    craneLib = (crane.mkLib pkgs).overrideToolchain rust;

    version = lib.fileContents ./VERSION;
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
        ndkVersions = [ndk];
      }).androidsdk;
    sdk = "${androidsdk}/libexec/android-sdk";
    bin = "${sdk}/build-tools/${buildTools}";
    androidJar = "${sdk}/platforms/android-${targetSdk}/android.jar";
    linker = {
      CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "${sdk}/ndk/${ndk}/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android${minSdk}-clang";
    };

    maven = path: hash:
      pkgs.fetchurl {
        url = "https://repo1.maven.org/maven2/${path}";
        inherit hash;
      };
    tnoodle = name: maven "org/worldcubeassociation/tnoodle/${name}/0.20.0/${name}-0.20.0.jar";
    tnoodleJars = [
      (tnoodle "lib-scrambles" "sha256-HgU8zmErJnxm2zAmUKVyX/sBgrFOZaw4C+pv1zGuBik=")
      (tnoodle "lib-svglite" "sha256-N92xW/BmOz9aw30FH7oXFRtTG2yXKSVmllsfd3Y6Ezc=")
      (tnoodle "scrambler-min2phase" "sha256-CrK4Lk7n/qyoeif3+BNDM0xzYTzlprdXkzQi64cURNs=")
      (tnoodle "scrambler-threephase" "sha256-K36iph1XH6kbzjfJXIuFK5yjMdFmV9yAAi2OV4eXCMM=")
      (tnoodle "scrambler-sq12phase" "sha256-39sSwbjv/Z5Kq35k6p2DQrXvbhzsjSKWQ03SZBQckd4=")
      (tnoodle "scrambler-fto3phase" "sha256-Kt3pKiaQI/bH4YpHbrbLDKkOfhH1+FSOKxub5uhFJQE=")
      (maven "org/timepedia/exporter/gwtexporter/2.5.1/gwtexporter-2.5.1.jar" "sha256-tHWD3QLUH0wDdlnXVzt5bdw+3cRMnv/FySXMkg3cPGQ=")
      (maven "org/slf4j/slf4j-api/2.0.20/slf4j-api-2.0.20.jar" "sha256-fhRGSZs1lnXYqr5q8t6GuF5yrooqkywLQNDlulcJdDk=")
    ];

    common = {
      src = craneLib.cleanCargoSource ./.;
      strictDeps = true;
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
      ${bin}/aapt2 compile --dir ${./res} -o res.zip
      ${bin}/aapt2 link -o app.apk -I ${androidJar} --manifest ${./AndroidManifest.xml} -R res.zip \
        --min-sdk-version ${minSdk} --target-sdk-version ${targetSdk} \
        --version-code ${toString versionCode} --version-name ${version}
      ${bin}/d8 --release --min-api ${minSdk} --lib ${androidJar} --output . ${toString tnoodleJars}
      install -D ${native}/lib/libcube_rs.so lib/arm64-v8a/libcube_rs.so
      find classes*.dex lib -exec touch -d @$SOURCE_DATE_EPOCH {} +
      zip -qrX app.apk classes*.dex lib
      mkdir $out
      ${bin}/zipalign -P 16 -f 4 app.apk $out/cube-rs.apk
    '';

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

    hostEnv = {
      JAVA_HOME = jdk.home;
      TNOODLE_CLASSPATH = lib.concatStringsSep ":" tnoodleJars;
    };

    # Dev profile: no LTO or optimisation for tests.
    test = let
      args = common // hostEnv // {CARGO_PROFILE = "dev";};
    in
      craneLib.cargoTest (args // {cargoArtifacts = craneLib.buildDepsOnly args;});

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
      // hostEnv
      // {
        ANDROID_HOME = sdk;
        packages = [
          ((rust pkgs).override {extensions = ["clippy" "rustfmt" "rust-src" "rust-analyzer"];})
          jdk
          androidsdk
        ];
      });
  };
}
