// Embeds the app icon and version info into draxbyte.exe (Windows only).
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Draxbyte");
        res.set("FileDescription", "Draxbyte - disk scanner and cleaner");
        if let Err(e) = res.compile() {
            // A missing resource compiler must not break the build; the exe just has no icon.
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}
