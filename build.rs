use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=jniLibs");

    if env::var_os("CARGO_FEATURE_MPV").is_none() {
        return;
    }

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let target = env::var("TARGET").unwrap_or_default();
    let abi = if target == "aarch64-linux-android" {
        "arm64-v8a"
    } else {
        return;
    };

    let lib_dir = manifest_dir.join("jniLibs").join(abi);
    let mpv_library = lib_dir.join("libmpv.so");

    if !mpv_library.is_file() {
        println!(
            "cargo:warning=MPV habilitado, pero falta {}",
            mpv_library.display()
        );
        return;
    }

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
}