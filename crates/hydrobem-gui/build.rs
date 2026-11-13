//! Windows version information of HydroBEM.exe.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set("FileDescription", "HydroBEM GUI");
        res.set("ProductName", "HydroBEM GUI");
        res.set("InternalName", "HydroBEM");
        res.set("OriginalFilename", "HydroBEM.exe");
        res.compile().expect("Windows resource");
    }
}
