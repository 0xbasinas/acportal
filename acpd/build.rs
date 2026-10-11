// Gives acpd.exe the ACP Portal icon on Windows, only when ACPD_EMBED_ICON=1 is set (the Release
// workflow does). Ordinary builds and cross-checks never need a resource compiler. A failure only
// warns: the program still builds, it just shows the default icon.
fn main() {
    println!("cargo:rerun-if-env-changed=ACPD_EMBED_ICON");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || std::env::var("ACPD_EMBED_ICON").as_deref() != Ok("1")
    {
        return;
    }
    println!("cargo:rerun-if-changed=../assets/logo/logo.ico");
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("../assets/logo/logo.ico");
    resource.set("FileDescription", "ACP Portal host");
    resource.set("ProductName", "ACP Portal");
    if let Err(error) = resource.compile() {
        println!("cargo:warning=could not embed the Windows icon: {error}");
    }
}
