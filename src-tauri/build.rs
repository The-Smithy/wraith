fn main() {
    println!("cargo:rerun-if-changed=wraith.exe.manifest");
    let mut attributes = tauri_build::Attributes::new();
    // Windows release builds always run as administrator. Debug builds don't, because a
    // process that requires elevation can't be started by `tauri dev` from a normal terminal.
    let is_windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if is_windows && std::env::var("PROFILE").as_deref() == Ok("release") {
        attributes = attributes.windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("wraith.exe.manifest")),
        );
    }
    tauri_build::try_build(attributes).expect("failed to run the Tauri build step");
}
