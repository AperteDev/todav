//! UniFFI's generator for Kotlin and Swift: `cargo run --features bindgen --bin uniffi-bindgen --
//! generate --library <the built library> --language kotlin --out-dir <dir>`.

fn main() {
    uniffi::uniffi_bindgen_main()
}
