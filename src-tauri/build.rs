fn main() {
    tauri_build::build();

    // The Windows taskbar balance runs in an isolated helper process so that
    // Explorer's DPI-awareness changes cannot affect the main Tauri process.
    // Windows ships the .NET Framework compiler on the native build hosts.
    #[cfg(target_os = "windows")]
    build_windows_wallet_display();

    // Windows: Embed Common Controls v6 manifest for test binaries
    //
    // When running `cargo test`, the generated test executables don't include
    // the standard Tauri application manifest. Without Common Controls v6,
    // `tauri::test` calls fail with STATUS_ENTRYPOINT_NOT_FOUND.
    //
    // This workaround:
    // 1. Embeds the manifest into test binaries via /MANIFEST:EMBED
    // 2. Uses /MANIFEST:NO for the main binary to avoid duplicate resources
    //    (Tauri already handles manifest embedding for the app binary)
    #[cfg(target_os = "windows")]
    {
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

#[cfg(target_os = "windows")]
fn build_windows_wallet_display() {
    use std::path::PathBuf;
    use std::process::Command;

    let project =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    let source = project.join("resources").join("windows_wallet_display.cs");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("missing output dir"))
        .join("aihubmix-wallet-display.exe");
    let windows = PathBuf::from(std::env::var_os("WINDIR").expect("missing Windows directory"));
    let framework = windows.join("Microsoft.NET").join("Framework");
    let compiler = framework.join("v4.0.30319").join("csc.exe");
    let wpf = framework.join("v4.0.30319").join("WPF");
    let result = Command::new(&compiler)
        .arg("/nologo")
        .arg("/target:winexe")
        .arg("/platform:anycpu")
        .arg("/optimize+")
        .arg("/reference:System.Drawing.dll")
        .arg("/reference:System.Windows.Forms.dll")
        .arg(format!(
            "/reference:{}",
            wpf.join("UIAutomationClient.dll").display()
        ))
        .arg(format!(
            "/reference:{}",
            wpf.join("UIAutomationTypes.dll").display()
        ))
        .arg(format!(
            "/reference:{}",
            wpf.join("WindowsBase.dll").display()
        ))
        .arg(format!("/out:{}", output.display()))
        .arg(&source)
        .status()
        .expect("failed to start the Windows .NET Framework compiler");
    assert!(
        result.success(),
        "could not compile the Windows wallet display"
    );
    println!("cargo:rerun-if-changed={}", source.display());
}
