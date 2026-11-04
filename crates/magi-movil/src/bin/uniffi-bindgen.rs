//! Generador de bindings de UniFFI (Kotlin) para el contrato de magi-movil.
//!
//! Se compila solo con la feature bindgen:
//!   cargo run -p magi-movil --features bindgen --bin uniffi-bindgen -- \
//!     generate --library target/debug/libmagi_movil.so --language kotlin --out-dir ... --no-format

fn main() {
    uniffi::uniffi_bindgen_main()
}
