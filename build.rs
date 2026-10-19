// Ikon dan metadata berkas kara-dm.exe (hanya saat dibangun di Windows).
fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/kara-dm.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("packaging/windows/kara-dm.ico");
        res.set("ProductName", "Kara Download Manager");
        res.set("FileDescription", "Kara Download Manager");
        res.compile().expect("gagal menyematkan resource Windows");
    }
}
