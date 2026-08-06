fn main() {
    pdfium::copy(std::path::Path::new(
        &std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"),
    ));
}
