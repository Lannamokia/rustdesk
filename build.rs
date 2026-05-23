#[cfg(windows)]
fn build_windows() {
    let file = "src/platform/windows.cc";
    let file2 = "src/platform/windows_delete_test_cert.cc";
    cc::Build::new().file(file).file(file2).compile("windows");
    println!("cargo:rustc-link-lib=WtsApi32");
    println!("cargo:rerun-if-changed={}", file);
    println!("cargo:rerun-if-changed={}", file2);
}

#[cfg(target_os = "macos")]
fn build_mac() {
    let file = "src/platform/macos.mm";
    let mut b = cc::Build::new();
    if let Ok(os_version::OsVersion::MacOS(v)) = os_version::detect() {
        let v = v.version;
        if v.contains("10.14") {
            b.flag("-DNO_InputMonitoringAuthStatus=1");
        }
    }
    b.flag("-std=c++17").file(file).compile("macos");
    println!("cargo:rerun-if-changed={}", file);
}

#[cfg(all(windows, feature = "inline"))]
fn build_manifest() {
    use std::io::Write;
    if std::env::var("PROFILE").unwrap() == "release" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("res/icon.ico")
            .set_language(winapi::um::winnt::MAKELANGID(
                winapi::um::winnt::LANG_ENGLISH,
                winapi::um::winnt::SUBLANG_ENGLISH_US,
            ))
            .set_manifest_file("res/manifest.xml");
        match res.compile() {
            Err(e) => {
                write!(std::io::stderr(), "{}", e).unwrap();
                std::process::exit(1);
            }
            Ok(_) => {}
        }
    }
}

fn install_android_deps() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os != "android" {
        return;
    }
    let mut target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if target_arch == "x86_64" {
        target_arch = "x64".to_owned();
    } else if target_arch == "x86" {
        target_arch = "x86".to_owned();
    } else if target_arch == "aarch64" {
        target_arch = "arm64".to_owned();
    } else {
        target_arch = "arm".to_owned();
    }
    let target = format!("{}-android", target_arch);
    let vcpkg_root = std::env::var("VCPKG_ROOT").unwrap();
    let mut path: std::path::PathBuf = vcpkg_root.into();
    if let Ok(vcpkg_root) = std::env::var("VCPKG_INSTALLED_ROOT") {
        path = vcpkg_root.into();
    } else {
        path.push("installed");
    }
    path.push(target);
    println!(
        "cargo:rustc-link-search={}",
        path.join("lib").to_str().unwrap()
    );
    println!("cargo:rustc-link-lib=ndk_compat");
    println!("cargo:rustc-link-lib=oboe");
    println!("cargo:rustc-link-lib=c++");
    println!("cargo:rustc-link-lib=OpenSLES");
}

// =============================================================================
// vhd-machine-auth-bridge: compile-time injection of RustDeskClientSharedSecret
// and Bridge_Config.secret_version.
//
// Pure helpers (parser, decoders, resolver) live in the `build_support`
// workspace crate (see `libs/build_support/`) so both this build script and
// the integration test in `libs/build_support/tests/build_script_tests.rs`
// can consume them without duplication.  This file owns the side-effecting
// glue: env var lookups, `cargo:` directives, OUT_DIR writes, and process exit
// on resolver errors.
// =============================================================================

#[cfg(feature = "vhd-bridge")]
fn die(msg: &str) -> ! {
    eprintln!("vhd-bridge build error: {}", msg);
    std::process::exit(1);
}

#[cfg(feature = "vhd-bridge")]
fn inject_vhd_bridge_secret() {
    use build_support::{parse_secret_sec, resolve_secret_version, resolve_shared_secret, SecretInputs};
    use std::path::Path;

    // Cargo build-script directives — register all sources so the build
    // re-runs whenever any of them changes.
    println!("cargo:rerun-if-env-changed=VHD_BRIDGE_SECRET_HEX");
    println!("cargo:rerun-if-env-changed=VHD_BRIDGE_SECRET_B64");
    println!("cargo:rerun-if-env-changed=VHD_BRIDGE_SECRET_VERSION");
    println!("cargo:rerun-if-changed=vhd_bridge_secret.bin");
    println!("cargo:rerun-if-changed=secret.sec");

    let hex_env = std::env::var("VHD_BRIDGE_SECRET_HEX").ok();
    let b64_env = std::env::var("VHD_BRIDGE_SECRET_B64").ok();

    let secret_bin_path = Path::new("vhd_bridge_secret.bin");
    let secret_sec_path = Path::new("secret.sec");
    let sec_map = parse_secret_sec(secret_sec_path);

    let bin_owned: Option<Vec<u8>> = if secret_bin_path.exists() {
        match std::fs::read(secret_bin_path) {
            Ok(b) => Some(b),
            Err(_) => die("vhd_bridge_secret.bin exists but could not be read"),
        }
    } else {
        None
    };

    let inputs = SecretInputs {
        hex_env: hex_env.as_deref(),
        b64_env: b64_env.as_deref(),
        bin_bytes: bin_owned.as_deref(),
        sec_map: &sec_map,
    };

    let (bytes, _source) = match resolve_shared_secret(&inputs) {
        Ok(v) => v,
        Err(e) => die(&e),
    };

    // Defensive — every decode_* path already validates length, but keep the
    // contract obvious.
    if bytes.len() != 32 {
        die("resolved shared secret did not have the required length of 32 bytes");
    }

    // Build the literal `[0x00, 0x01, ..., 0x1F]` — 32 explicit byte literals
    // (NOT a `[0xNN; 32]` repeat-fill), so `secret.rs` can `include!()` it as
    // the body of a `[u8; 32]` constant.
    let mut literal = String::with_capacity(2 + 32 * 6);
    literal.push('[');
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            literal.push_str(", ");
        }
        literal.push_str(&format!("0x{:02x}", b));
    }
    literal.push(']');

    let out_dir = std::env::var("OUT_DIR")
        .unwrap_or_else(|_| die("OUT_DIR environment variable was not set by cargo"));
    let secret_path = Path::new(&out_dir).join("vhd_bridge_secret.rs");
    if let Err(_) = std::fs::write(&secret_path, &literal) {
        die("failed to write generated shared-secret include file under OUT_DIR");
    }

    // Resolve secret_version: env > secret.sec line > default 1.
    let version_env = std::env::var("VHD_BRIDGE_SECRET_VERSION").ok();
    let version: u32 = match resolve_secret_version(version_env.as_deref(), &sec_map) {
        Ok(v) => v,
        Err(e) => die(&e),
    };

    let version_path = Path::new(&out_dir).join("vhd_bridge_secret_version.rs");
    if let Err(_) = std::fs::write(&version_path, format!("{}u32", version)) {
        die("failed to write generated secret-version include file under OUT_DIR");
    }
}

// =============================================================================
// vhd-machine-auth-bridge §1.2b: Build_Prereq_Vars gate.
//
// Validates HBBS_KEY / HBBS_HOST / HBBR_HOST (env > secret.sec) UNCONDITIONALLY
// (NOT gated by `cfg(feature = "vhd-bridge")`).  The gate runs in all three
// deployment forms (Controlled / Controller / Relay) per Requirement 22.1.
//
// Compatibility: this gate is run-but-lenient when nothing has been provided.
// Preserves existing builds (legacy `RS_PUB_KEY = "OeVuKk..."` etc. continue
// to apply when no env / no secret.sec).  Gate is *strict* — Err ⇒ exit(1) —
// only when ops have actually attempted injection (any of the three envs
// set OR `secret.sec` present).  This matches the intent of Req 22.10:
// "missing iff env unset AND secret.sec line absent for that item".
//
// The actual cargo:rustc-env injection lives in `libs/hbb_common/build.rs`
// (since `RS_PUB_KEY` / `RENDEZVOUS_SERVERS` are defined there and
// `cargo:rustc-env` is scoped per crate).  Running the same resolver in the
// root build script gives the operator a single, unified failure surface
// before sub-crate build scripts trip on the same inputs.
// =============================================================================

fn build_prereq_gate() {
    use build_support::{parse_secret_sec, resolve_build_prereq_vars, BuildPrereqInputs};
    use std::path::Path;

    println!("cargo:rerun-if-env-changed=HBBS_KEY");
    println!("cargo:rerun-if-env-changed=HBBS_HOST");
    println!("cargo:rerun-if-env-changed=HBBR_HOST");
    println!("cargo:rerun-if-changed=secret.sec");

    let hbbs_key_env = std::env::var("HBBS_KEY").ok();
    let hbbs_host_env = std::env::var("HBBS_HOST").ok();
    let hbbr_host_env = std::env::var("HBBR_HOST").ok();
    let secret_sec_path = Path::new("secret.sec");
    let secret_sec_present = secret_sec_path.exists();
    let sec_map = parse_secret_sec(secret_sec_path);

    // Activation policy — see header comment.  We treat the gate as
    // "engaged" if anyone has actually given us something to validate.
    let any_env_set = hbbs_key_env.is_some()
        || hbbs_host_env.is_some()
        || hbbr_host_env.is_some();
    let active = any_env_set || secret_sec_present;
    if !active {
        // No injection attempt.  Preserve legacy behavior — config.rs
        // continues to use its hard-coded `RS_PUB_KEY` / `RENDEZVOUS_SERVERS`
        // defaults via `option_env!` returning `None`.
        return;
    }

    let inputs = BuildPrereqInputs {
        hbbs_key_env: hbbs_key_env.as_deref(),
        hbbs_host_env: hbbs_host_env.as_deref(),
        hbbr_host_env: hbbr_host_env.as_deref(),
        sec_map: &sec_map,
    };
    if let Err(reason) = resolve_build_prereq_vars(&inputs) {
        // Reason-only error per Requirement 22.4 / 22.9 — no value or file
        // content leaks through.  The resolver guarantees this contract;
        // we only forward its message.
        eprintln!("Build_Prereq_Vars gate failed: {}", reason);
        std::process::exit(1);
    }
}

fn main() {
    hbb_common::gen_version();
    install_android_deps();
    #[cfg(all(windows, feature = "inline"))]
    build_manifest();
    #[cfg(windows)]
    build_windows();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os == "macos" {
        #[cfg(target_os = "macos")]
        build_mac();
        println!("cargo:rustc-link-lib=framework=ApplicationServices");
    }
    build_prereq_gate();
    #[cfg(feature = "vhd-bridge")]
    inject_vhd_bridge_secret();
    println!("cargo:rerun-if-changed=build.rs");
}
