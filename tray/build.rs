// Gives acpd-tray.exe the ACP Portal icon on Windows. A missing resource compiler only warns:
// the program still builds, it just shows the default icon.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo:rerun-if-changed=../assets/logo/logo.ico");
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("../assets/logo/logo.ico");
    resource.set("FileDescription", "ACP Portal tray");
    resource.set("ProductName", "ACP Portal");
    if let Err(error) = resource.compile() {
        println!("cargo:warning=could not embed the Windows icon: {error}");
    }
}
