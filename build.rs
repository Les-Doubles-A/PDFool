fn main() {
    println!("cargo:rerun-if-changed=PDFool.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("PDFool.ico");
        res.set("ProductName", "PDFool");
        res.set("FileDescription", "PDFool - outils PDF hors ligne");
        res.set("CompanyName", "Les-Doubles-A");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icone Windows non integree : {e}");
        }
    }
}
