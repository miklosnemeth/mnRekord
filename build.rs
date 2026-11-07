fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "mnRekord");
        res.set("FileDescription", "mnRekord screen recorder");
        res.set("LegalCopyright", "Copyright (c) 2026 Miklós Németh");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
