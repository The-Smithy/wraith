fn main() {
    println!("cargo:rerun-if-changed=wraith.exe.manifest");
    let mut attributes = tauri_build::Attributes::new();
    // Release builds always run as administrator. Debug builds don't, because a
    // process that requires elevation can't be started by `tauri dev` from a normal terminal.
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        attributes = attributes.windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("wraith.exe.manifest")),
        );
    }
    tauri_build::try_build(attributes).expect("failed to run the Tauri build step");
}
