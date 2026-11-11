{
  description = "Rubik's cube timer for Android";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    utils,
    crane,
    rust-overlay,
    ...
  }:
  # The NDK only ships linux-x86_64 binaries.
    utils.lib.eachSystem ["x86_64-linux"] (
      system: let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [rust-overlay.overlays.default];
          config = {
            allowUnfree = true;
            android_sdk.accept_license = true;
          };
        };
        inherit (pkgs) lib jdk;

        craneLib = (crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml);

        version = lib.fileContents ./VERSION;
        versionCode = lib.foldl (code: n: code * 100 + lib.toInt n) 0 (lib.splitString "." version);

        minSdk = "26";
        targetSdk = "36";
        buildTools = "36.0.0";
        ndk = "29.0.14206865";
        sdk =
          (pkgs.androidenv.composeAndroidPackages {
            platformVersions = [targetSdk];
            buildToolsVersions = [buildTools];
            includeNDK = true;
            ndkVersions = [ndk];
          }).androidsdk
          + "/libexec/android-sdk";
        bin = "${sdk}/build-tools/${buildTools}";
        androidJar = "${sdk}/platforms/android-${targetSdk}/android.jar";

        # Shared by the Nix build and the devShell (`cargo build --target aarch64-linux-android`).
        androidEnv = {
          ANDROID_HOME = sdk;
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
            // androidEnv
            // {
              CARGO_BUILD_TARGET = "aarch64-linux-android";
              doCheck = false;
            };
        in
          craneLib.buildPackage (args // {cargoArtifacts = craneLib.buildDepsOnly args;});

        # Unsigned. Sign with `nix run .#install` or in CI.
        apk = pkgs.runCommand "cube-rs-${version}" {nativeBuildInputs = [jdk pkgs.zip];} ''
          ${bin}/aapt2 link -o app.apk -I ${androidJar} --manifest ${./AndroidManifest.xml} \
            --min-sdk-version ${minSdk} --target-sdk-version ${targetSdk} \
            --version-code ${toString versionCode} --version-name ${version}
          ${bin}/d8 --release --min-api ${minSdk} --lib ${androidJar} --output . ${toString tnoodleJars}
          install -D ${native}/lib/libcube_rs.so lib/arm64-v8a/libcube_rs.so
          zip -qr app.apk classes*.dex lib
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

        install = pkgs.writeShellApplication {
          name = "install";
          runtimeInputs = [jdk sign pkgs.android-tools];
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

        hostEnv = {
          JAVA_HOME = jdk.home;
          TNOODLE_CLASSPATH = lib.concatStringsSep ":" tnoodleJars;
        };

        test = craneLib.cargoTest (common
          // hostEnv
          // {
            cargoArtifacts = craneLib.buildDepsOnly (common // hostEnv);
          });

        # Keeps every input's source alive through CI's store GC.
        flake-inputs = pkgs.linkFarm "flake-inputs" self.inputs;
      in {
        packages = {
          default = apk;
          inherit apk native flake-inputs;
        };

        apps = {
          install = utils.lib.mkApp {drv = install;};
          sign = utils.lib.mkApp {drv = sign;};
        };

        checks = {inherit test;};

        devShells.default = craneLib.devShell (androidEnv
          // hostEnv
          // {
            packages = [jdk sign pkgs.android-tools];
            RUST_LOG = "info";
          });
      }
    );
}
