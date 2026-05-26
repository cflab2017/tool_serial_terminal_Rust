// Build-script: generate a multi-size ICO from the procedural icon module,
// then embed it as the .exe file's icon resource on Windows. No external
// assets needed — `src/icon.rs` is shared between this script and the app
// itself via `#[path]`.

#[path = "src/icon.rs"]
mod icon;

use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    println!("cargo:rerun-if-changed=build.rs");

    // Only Windows benefits from a .rc / .ico chain. Bail early on other OSes
    // to avoid running the resource compiler when there's nothing to embed.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR set by cargo"));

    // 16/32/48 = explorer/taskbar/jumplist, 64/128/256 = high-DPI / shell hover
    let ico_bytes = icon::build_ico(&[16, 32, 48, 64, 128, 256]);
    let ico_path = out_dir.join("app.ico");
    fs::write(&ico_path, &ico_bytes).expect("write generated .ico");

    let rc_path = out_dir.join("app.rc");
    // RC strings are happier with forward slashes on Windows.
    let ico_in_rc = ico_path.display().to_string().replace('\\', "/");
    let rc_content = format!("1 ICON \"{}\"\n", ico_in_rc);
    fs::write(&rc_path, rc_content).expect("write .rc file");

    // `compile` returns a CompilationResult that's #[must_use]; surface any
    // failure as a hard build error so a broken icon doesn't pass silently.
    let result = embed_resource::compile(&rc_path, embed_resource::NONE);
    if let Err(e) = result.manifest_optional() {
        panic!("embed-resource failed: {e}");
    }
}
