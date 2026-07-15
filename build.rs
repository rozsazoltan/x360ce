fn main() {
    println!("cargo:rerun-if-changed=assets/x360ce.ico");
    println!("cargo:rerun-if-changed=src/x360ce.exe.manifest");
    println!("cargo:rerun-if-changed=assets/third-party/ViGEmBus_1.22.0_x64_x86_arm64.exe");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut resource = winresource::WindowsResource::new();
    resource.set_manifest(include_str!("src/x360ce.exe.manifest"));
    resource.set("FileDescription", "x360ce Controller Mapper");
    resource.set("ProductName", "x360ce");
    resource.set("OriginalFilename", "x360ce.exe");
    resource.set_icon("assets/x360ce.ico");

    if let Err(error) = resource.compile() {
        panic!("failed to compile Windows resources: {error}");
    }
}
