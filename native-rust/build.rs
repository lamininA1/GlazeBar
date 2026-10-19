use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=glazebar.rc");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let resource = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("glazebar.res");
    let status = Command::new("rc.exe")
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg("glazebar.rc")
        .status()
        .expect("Windows SDK resource compiler (rc.exe) must be on PATH");
    assert!(
        status.success(),
        "Windows version resource compilation failed"
    );
    println!("cargo:rustc-link-arg={}", resource.display());
}
