fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_claude_resources().expect("Unable to embed Claude localization resources");
    }
    tauri_build::build();

    // Windows (MSVC): Embed Common Controls v6 manifest for test binaries
    //
    // When running `cargo test`, the generated test executables don't include
    // the standard Tauri application manifest. Without Common Controls v6,
    // `tauri::test` calls fail with STATUS_ENTRYPOINT_NOT_FOUND.
    //
    // This workaround:
    // 1. Embeds the manifest into test binaries via /MANIFEST:EMBED
    // 2. Uses /MANIFEST:NO for the main binary to avoid duplicate resources
    //    (Tauri already handles manifest embedding for the app binary)
    //
    // The `/MANIFEST*` flags are MSVC link.exe specific. On the GNU toolchain
    // (x86_64-pc-windows-gnu, used by local development environments without
    // MSVC Build Tools) they are rejected by ld, so skip embedding there.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let manifest_path = std::path::PathBuf::from(
            std::env::var("CARGO_MANIFEST_DIR").expect("missing CARGO_MANIFEST_DIR"),
        )
        .join("common-controls.manifest");
        let manifest_arg = format!("/MANIFESTINPUT:{}", manifest_path.display());

        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg={}", manifest_arg);
        // Avoid duplicate manifest resources in binary builds.
        println!("cargo:rustc-link-arg-bins=/MANIFEST:NO");
        println!("cargo:rerun-if-changed={}", manifest_path.display());
    }
}

fn embed_claude_resources() -> std::io::Result<()> {
    use std::{fs, path::PathBuf};
    let root =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("resources/claude_zh");
    let mut files = vec![
        PathBuf::from("LICENSE"),
        PathBuf::from("NOTICE.md"),
        PathBuf::from("scripts/install_windows.ps1"),
    ];
    for entry in fs::read_dir(root.join("resources"))? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            files.push(PathBuf::from("resources").join(entry.file_name()));
        }
    }
    files.sort();
    println!("cargo:rerun-if-changed={}", root.display());
    let mut source = String::from("const CLAUDE_RESOURCES: &[(&str, &[u8])] = &[\n");
    for relative in files {
        source.push_str(&format!(
            "({:?}, include_bytes!({:?})),\n",
            relative.to_string_lossy().replace('\\', "/"),
            root.join(relative).to_string_lossy(),
        ));
    }
    source.push_str("];\n");
    fs::write(
        PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("claude_resources.rs"),
        source,
    )
}
