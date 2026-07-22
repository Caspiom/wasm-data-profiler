//! Wasm shell around `mirante-core`.
//!
//! This crate does three things and nothing else: install the panic hook,
//! hand the byte slice to the core, and serialise the result to a JS object.
//! No profiling logic lives here.
//!
//! There is deliberately no timing API. `std::time::Instant` does not work on
//! `wasm32-unknown-unknown`, so the caller measures with `performance.now()`.

use mirante_core::profile_csv;
use wasm_bindgen::prelude::*;

/// Runs on module instantiation.
///
/// Without the hook a panic surfaces in the console as `unreachable executed`,
/// with no message and no location.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Profiles a CSV file.
///
/// `bytes` is the raw file, exactly as read from disk: undecoded, BOM and all.
/// Returns the profile as a plain JS object, or throws with the reason the file
/// could not be read.
#[wasm_bindgen]
pub fn profile(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let profile = profile_csv(bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&profile).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// The crate version, so the UI can show what it is actually running.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
