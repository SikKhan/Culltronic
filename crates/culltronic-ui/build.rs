//! Embeds the native Windows application icon in release executables.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=assets/branding/culltronic.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winres::WindowsResource::new()
            .set_icon("assets/branding/culltronic.ico")
            .set("FileDescription", "Culltronic photo culling")
            .set("ProductName", "Culltronic")
            .compile()?;
    }
    Ok(())
}
