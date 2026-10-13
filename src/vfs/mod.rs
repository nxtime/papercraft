//! The file system the project store runs on.
//!
//! Native builds use `std::fs` directly. The browser has none, so there a whole tree lives in
//! memory and every change is mirrored into IndexedDB (loaded back by [`load`] at startup).
//! Only the calls the app makes are provided.

#[cfg(not(target_arch = "wasm32"))]
pub use std::fs::*;

#[cfg(not(target_arch = "wasm32"))]
pub fn is_file(p: &std::path::Path) -> bool {
    p.is_file()
}
#[cfg(not(target_arch = "wasm32"))]
pub fn is_dir(p: &std::path::Path) -> bool {
    p.is_dir()
}
#[cfg(not(target_arch = "wasm32"))]
pub fn exists(p: &std::path::Path) -> bool {
    p.exists()
}

#[cfg(target_arch = "wasm32")]
mod mem;
#[cfg(target_arch = "wasm32")]
pub use mem::*;
