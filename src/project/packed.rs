//! Pages and saved versions are stored as zstd-compressed JSON (`<name>.json.zst`): a 34 MB
//! page becomes about 0.6 MB and reads faster. A plain `<name>.json` from older builds still
//! reads, and is removed once the packed file is written.

use std::io;
use std::path::{Path, PathBuf};

/// Extension of a packed file, after its stem.
pub const EXT: &str = "json.zst";

/// zstd level on native builds: 3 packs a large page in ~7 ms; higher levels gain little.
#[cfg(not(target_arch = "wasm32"))]
const LEVEL: i32 = 3;

/// `<dir>/<stem>.json.zst`.
pub fn path(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{stem}.{EXT}"))
}

/// The plain `.json` an older build wrote in place of `packed`.
pub fn legacy(packed: &Path) -> PathBuf {
    packed.with_extension("")
}

/// The stem of a packed or legacy file name (`a.json.zst` or `a.json` give `a`).
pub fn stem(file_name: &str) -> Option<&str> {
    file_name.strip_suffix(".json.zst").or_else(|| file_name.strip_suffix(".json"))
}

pub fn pack(json: &[u8]) -> Vec<u8> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        zstd::encode_all(json, LEVEL).expect("zstd compresses from memory")
    }
    // ponytail: ruzstd's fastest level only (~5x larger than level 3), the browser has no C zstd.
    #[cfg(target_arch = "wasm32")]
    {
        ruzstd::encoding::compress_to_vec(json, ruzstd::encoding::CompressionLevel::Fastest)
    }
}

pub fn unpack(bytes: &[u8]) -> io::Result<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        zstd::decode_all(bytes)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let mut out = Vec::new();
        let mut r = ruzstd::decoding::StreamingDecoder::new(bytes).map_err(io::Error::other)?;
        io::Read::read_to_end(&mut r, &mut out)?;
        Ok(out)
    }
}

/// The JSON text of `packed`, or of its legacy `.json` when only that exists.
pub fn read(packed: &Path) -> io::Result<String> {
    let bytes = match crate::vfs::read(packed) {
        Ok(b) => unpack(&b)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => crate::vfs::read(legacy(packed))?,
        Err(e) => return Err(e),
    };
    String::from_utf8(bytes).map_err(io::Error::other)
}

/// Writes `json` packed (atomically), then removes the legacy `.json`, which would otherwise
/// be stale beside it.
pub fn write(packed: &Path, json: &str) -> io::Result<()> {
    crate::io::atomic::write(packed, &pack(json.as_bytes()))?;
    remove_legacy(packed);
    Ok(())
}

fn remove_legacy(packed: &Path) {
    let _ = crate::vfs::remove_file(legacy(packed));
}

/// Removes `packed` and its legacy `.json`.
pub fn remove(packed: &Path) {
    let _ = crate::vfs::remove_file(packed);
    remove_legacy(packed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_packed_then_legacy_and_drops_legacy_on_write() {
        let dir = std::env::temp_dir().join(format!("pc-packed-{}", crate::project::new_id()));
        crate::vfs::create_dir_all(&dir).unwrap();
        let p = path(&dir, "a");
        crate::vfs::write(legacy(&p), "{\"old\":1}").unwrap();
        assert_eq!(read(&p).unwrap(), "{\"old\":1}");
        write(&p, "{\"new\":2}").unwrap();
        assert!(!legacy(&p).exists());
        assert_eq!(read(&p).unwrap(), "{\"new\":2}");
        assert_eq!(stem("a.json.zst"), Some("a"));
        assert_eq!(stem("a.json"), Some("a"));
        remove(&p);
        assert!(!p.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
