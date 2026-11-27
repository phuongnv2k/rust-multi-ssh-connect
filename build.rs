use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.ico");

    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("windows") {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let res_obj = out_dir.join("app_icon.o");
        let windres = if Command::new("x86_64-w64-mingw32-windres.exe")
            .arg("--version")
            .output()
            .is_ok()
        {
            "x86_64-w64-mingw32-windres.exe"
        } else {
            "windres.exe"
        };

        let status = Command::new(windres)
            .args(["assets/app.rc", "-O", "coff", "-o"])
            .arg(&res_obj)
            .status();

        if let Ok(s) = status {
            if s.success() {
                println!("cargo:rustc-link-arg={}", res_obj.display());
            }
        }
    }
}
