fn write_windows_common_controls_manifest() -> std::path::PathBuf {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").ok();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").ok();
    if target_os.as_deref() != Some("windows") || target_env.as_deref() != Some("msvc") {
        return std::path::PathBuf::new();
    }

    // Cargo's test/example binaries do not get the Tauri app manifest by default.
    // That leaves the executable without a resource section, so desktop dependencies such as
    // comctl32 may resolve the legacy Common Controls DLL and fail at process startup
    // (STATUS_ENTRYPOINT_NOT_FOUND) before any Rust code runs.
    let manifest = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>
"#;

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest_path = out_dir.join("windows-test-manifest.xml");
    std::fs::write(&manifest_path, manifest).expect("write windows test manifest");
    manifest_path
}

fn embed_windows_test_manifest() {
    let manifest_path = write_windows_common_controls_manifest();
    if manifest_path.as_os_str().is_empty() {
        return;
    }

    println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
        manifest_path.display()
    );
}

fn embed_windows_example_manifest() {
    let manifest_path = write_windows_common_controls_manifest();
    if manifest_path.as_os_str().is_empty() {
        return;
    }

    println!("cargo:rustc-link-arg-examples=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-examples=/MANIFESTINPUT:{}",
        manifest_path.display()
    );
}

fn main() {
    embed_windows_test_manifest();
    embed_windows_example_manifest();
    tauri_build::build()
}
