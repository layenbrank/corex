#[path = "../build/pdfium.rs"]
mod pdfium;

fn main() {
    pdfium::copy_pdfium_dll(
        std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")),
    );
}
