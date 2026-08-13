// Embed a Windows .ico into the exe on `*-windows-gnu` builds via `windres`.
//
// The icon is committed under `build-assets/icon.ico`, generated from
// `static/favicon.svg` so the desktop icon matches the webUI tab favicon.
// To re-generate:
//     python3 -c "import cairosvg,io; from PIL import Image; \
//       imgs=[Image.open(io.BytesIO(cairosvg.svg2png(url='static/favicon.svg', \
//       output_width=s, output_height=s))).convert('RGBA') for s in [16,32,48,64,128,256]]; \
//       imgs[-1].save('build-assets/icon.ico', format='ICO', \
//       sizes=[(i.width,i.height) for i in imgs])"
//
// On non-Windows targets this script is a no-op.

fn main() {
    // Always pin fingerprint to this build script itself so cargo does not
    // fall back to scanning the entire source tree (which on this host
    // triggers EACCES on .git/info/exclude under cargo 1.96).
    println!("cargo:rerun-if-changed=build.rs");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env == "msvc" {
        return;
    }

    let manifest_dir = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo"),
    );
    let icon = manifest_dir.join("build-assets").join("icon.ico");
    if !icon.exists() {
        // Missing icon = fall back to the default Windows exe icon rather
        // than failing the build.
        return;
    }

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR set"));
    let rc = out_dir.join("webclx-icon.rc");
    let res = out_dir.join("webclx-icon.o");

    std::fs::write(
        &rc,
        format!("1 ICON \"{}\"\n", icon.display().to_string().replace('\\', "\\\\")),
    )
    .expect("write .rc");

    let windres =
        std::env::var("WINDRES").unwrap_or_else(|_| "x86_64-w64-mingw32-windres".to_string());
    let status = std::process::Command::new(&windres)
        .arg(format!("-i{}", rc.display()))
        .arg("-O")
        .arg("coff")
        .arg(format!("-o{}", res.display()))
        .status()
        .unwrap_or_else(|e| panic!("failed to run windres `{}`: {}", windres, e));
    assert!(status.success(), "windres failed to compile icon resource");

    println!("cargo:rustc-link-arg={}", res.display());
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-env-changed=WINDRES");
}
