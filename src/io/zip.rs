//! Minimal zip files: a store-only writer (project backups, multi-file exports in the browser)
//! and a reader that understands what the writer and common tools produce (stored or deflated
//! entries, no encryption, no zip64).

use std::collections::HashSet;

const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const END_SIG: u32 = 0x0605_4b50;
const LOCAL_LEN: usize = 30;
const CENTRAL_LEN: usize = 46;
const END_LEN: usize = 22;
/// Zip version needed to extract (2.0: deflate, folders).
const VERSION: u16 = 20;
/// General purpose flag bit 11: names are UTF-8.
const UTF8_FLAG: u16 = 1 << 11;
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATE: u16 = 8;
/// 1980-01-01, the earliest DOS date; entries carry no real timestamp.
const DOS_DATE: u16 = 0x0021;
/// Largest entry the reader inflates, against zip bombs.
const MAX_ENTRY: usize = 512 << 20;
/// Largest total the reader inflates across all entries: entries may overlap in the file, so
/// per-entry limits alone let a small archive expand to many times its size.
const MAX_TOTAL: usize = 1 << 30;

pub type Entry = (String, Vec<u8>);

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

/// A store-only zip of `entries` (`name` uses `/` separators).
pub fn write(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let name = name.as_bytes();
        out.extend(LOCAL_SIG.to_le_bytes());
        out.extend(VERSION.to_le_bytes());
        out.extend(UTF8_FLAG.to_le_bytes());
        out.extend(METHOD_STORED.to_le_bytes());
        out.extend(0u16.to_le_bytes()); // time
        out.extend(DOS_DATE.to_le_bytes());
        out.extend(crc.to_le_bytes());
        out.extend(size.to_le_bytes()); // compressed
        out.extend(size.to_le_bytes()); // uncompressed
        out.extend((name.len() as u16).to_le_bytes());
        out.extend(0u16.to_le_bytes()); // extra
        out.extend(name);
        out.extend(data);

        central.extend(CENTRAL_SIG.to_le_bytes());
        central.extend(VERSION.to_le_bytes()); // made by
        central.extend(VERSION.to_le_bytes()); // needed
        central.extend(UTF8_FLAG.to_le_bytes());
        central.extend(METHOD_STORED.to_le_bytes());
        central.extend(0u16.to_le_bytes());
        central.extend(DOS_DATE.to_le_bytes());
        central.extend(crc.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend((name.len() as u16).to_le_bytes());
        central.extend([0u8; 12]); // extra len, comment len, disk, internal attrs, external attrs
        central.extend(offset.to_le_bytes());
        central.extend(name);
    }
    let central_offset = out.len() as u32;
    let central_len = central.len() as u32;
    out.extend(central);
    out.extend(END_SIG.to_le_bytes());
    out.extend([0u8; 4]); // disk numbers
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend(central_len.to_le_bytes());
    out.extend(central_offset.to_le_bytes());
    out.extend(0u16.to_le_bytes()); // comment
    out
}

fn u16_at(b: &[u8], i: usize) -> Result<u16, String> {
    b.get(i..i + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| "truncated zip".to_string())
}

fn u32_at(b: &[u8], i: usize) -> Result<u32, String> {
    b.get(i..i + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| "truncated zip".to_string())
}

/// Every file in the archive (folders are skipped), verifying CRCs.
pub fn read(bytes: &[u8]) -> Result<Vec<Entry>, String> {
    read_capped(bytes, MAX_TOTAL)
}

/// [`read`] with a smaller cap on the total inflated size.
pub fn read_capped(bytes: &[u8], max_total: usize) -> Result<Vec<Entry>, String> {
    let mut total = 0usize;
    let end = (0..=bytes.len().saturating_sub(END_LEN))
        .rev()
        .find(|&i| u32_at(bytes, i).is_ok_and(|s| s == END_SIG))
        .ok_or("not a zip file")?;
    let count = u16_at(bytes, end + 10)? as usize;
    let mut at = u32_at(bytes, end + 16)? as usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if u32_at(bytes, at)? != CENTRAL_SIG {
            return Err("corrupt zip directory".into());
        }
        let method = u16_at(bytes, at + 10)?;
        let crc = u32_at(bytes, at + 16)?;
        let csize = u32_at(bytes, at + 20)? as usize;
        let usize_ = u32_at(bytes, at + 24)? as usize;
        let name_len = u16_at(bytes, at + 28)? as usize;
        let extra_len = u16_at(bytes, at + 30)? as usize;
        let comment_len = u16_at(bytes, at + 32)? as usize;
        let local = u32_at(bytes, at + 42)? as usize;
        let name = bytes
            .get(at + CENTRAL_LEN..at + CENTRAL_LEN + name_len)
            .ok_or("truncated zip")?;
        let name = String::from_utf8_lossy(name).into_owned();
        at += CENTRAL_LEN + name_len + extra_len + comment_len;
        if name.ends_with('/') {
            continue;
        }
        if u32_at(bytes, local)? != LOCAL_SIG {
            return Err(format!("corrupt zip entry {name}"));
        }
        let start = local
            + LOCAL_LEN
            + u16_at(bytes, local + 26)? as usize
            + u16_at(bytes, local + 28)? as usize;
        let raw = bytes.get(start..start + csize).ok_or("truncated zip")?;
        let budget = max_total - total;
        let too_big = || format!("{name}: archive expands past {} MiB", max_total >> 20);
        let data = match method {
            METHOD_STORED if raw.len() > budget => return Err(too_big()),
            METHOD_STORED => raw.to_vec(),
            METHOD_DEFLATE => {
                miniz_oxide::inflate::decompress_to_vec_with_limit(raw, MAX_ENTRY.min(budget))
                    .map_err(|e| match e.status {
                        miniz_oxide::inflate::TINFLStatus::HasMoreOutput if budget < MAX_ENTRY => {
                            too_big()
                        }
                        _ => format!("cannot inflate {name}: {:?}", e.status),
                    })?
            }
            m => return Err(format!("{name}: unsupported zip method {m}")),
        };
        total += data.len();
        if data.len() != usize_ || crc32(&data) != crc {
            return Err(format!("{name}: checksum mismatch"));
        }
        out.push((name, data));
    }
    Ok(out)
}

/// `name` made unique among `used` by appending " 2", " 3"… before the extension.
pub fn unique_name(name: &str, used: &mut HashSet<String>) -> String {
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    let dot = if ext.is_empty() { "" } else { "." };
    let mut candidate = name.to_string();
    let mut i = 2;
    while !used.insert(candidate.clone()) {
        candidate = format!("{stem} {i}{dot}{ext}");
        i += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn round_trip() {
        let entries = vec![
            ("a/manifest.json".to_string(), b"{\"x\":1}".to_vec()),
            ("a/pages/p1.json".to_string(), vec![0u8; 1000]),
            ("empty".to_string(), vec![]),
            ("ünï.txt".to_string(), vec![1, 2, 3]),
        ];
        assert_eq!(read(&write(&entries)).unwrap(), entries);
    }

    #[test]
    fn empty_archive() {
        assert!(read(&write(&[])).unwrap().is_empty());
    }

    #[test]
    fn reads_deflated_entries() {
        let data = b"hello hello hello hello".to_vec();
        let packed = miniz_oxide::deflate::compress_to_vec(&data, 6);
        let mut zip = write(&[("x".into(), data.clone())]);
        // Rewrite the single stored entry as deflated by patching both headers.
        let name_len = 1;
        let local_data = LOCAL_LEN + name_len;
        let mut patched = zip[..local_data].to_vec();
        patched[8..10].copy_from_slice(&METHOD_DEFLATE.to_le_bytes());
        patched[18..22].copy_from_slice(&(packed.len() as u32).to_le_bytes());
        patched.extend(&packed);
        let central_at = local_data + data.len();
        let mut central = zip.split_off(central_at);
        central[10..12].copy_from_slice(&METHOD_DEFLATE.to_le_bytes());
        central[20..24].copy_from_slice(&(packed.len() as u32).to_le_bytes());
        let end = central.len() - END_LEN;
        let dir_offset = patched.len() as u32;
        let dir_len = (end) as u32;
        patched.extend(&central);
        let n = patched.len();
        patched[n - 6..n - 2].copy_from_slice(&dir_offset.to_le_bytes());
        patched[n - 10..n - 6].copy_from_slice(&dir_len.to_le_bytes());
        assert_eq!(read(&patched).unwrap(), vec![("x".to_string(), data)]);
    }

    /// A zip whose `count` central entries all point at one deflated local entry of `data`.
    fn overlapping_zip(data: &[u8], count: u16) -> Vec<u8> {
        let packed = miniz_oxide::deflate::compress_to_vec(data, 6);
        let mut z = Vec::new();
        z.extend(LOCAL_SIG.to_le_bytes());
        z.extend([20, 0, 0, 0]);
        z.extend(METHOD_DEFLATE.to_le_bytes());
        z.extend([0; 4]);
        z.extend(crc32(data).to_le_bytes());
        z.extend((packed.len() as u32).to_le_bytes());
        z.extend((data.len() as u32).to_le_bytes());
        z.extend([1, 0, 0, 0]);
        z.push(b'x');
        z.extend(&packed);
        let dir = z.len() as u32;
        for i in 0..count {
            let mut c = vec![0u8; CENTRAL_LEN];
            c[..4].copy_from_slice(&CENTRAL_SIG.to_le_bytes());
            c[10..12].copy_from_slice(&METHOD_DEFLATE.to_le_bytes());
            c[16..20].copy_from_slice(&crc32(data).to_le_bytes());
            c[20..24].copy_from_slice(&(packed.len() as u32).to_le_bytes());
            c[24..28].copy_from_slice(&(data.len() as u32).to_le_bytes());
            c[28] = 1;
            z.extend(c);
            z.push(b'a' + i as u8);
        }
        let dir_len = z.len() as u32 - dir;
        z.extend(END_SIG.to_le_bytes());
        z.extend([0; 4]);
        z.extend(count.to_le_bytes());
        z.extend(count.to_le_bytes());
        z.extend(dir_len.to_le_bytes());
        z.extend(dir.to_le_bytes());
        z.extend([0; 2]);
        z
    }

    #[test]
    fn refuses_archives_that_inflate_past_the_total_cap() {
        let data = vec![7u8; 600_000];
        let one = overlapping_zip(&data, 1);
        assert_eq!(read_capped(&one, 1 << 20).unwrap().len(), 1);
        // Two entries over the same bytes: each is under the cap, together they are not.
        let two = overlapping_zip(&data, 2);
        let err = read_capped(&two, 1 << 20).unwrap_err();
        assert!(err.contains("expands past 1 MiB"), "{err}");
        // A single entry bigger than the whole cap is refused too.
        assert!(read_capped(&one, 100_000).is_err());
    }

    #[test]
    fn counts_stored_entries_against_the_total_cap() {
        let z = write(&[("a".into(), vec![1; 600]), ("b".into(), vec![2; 600])]);
        assert!(read_capped(&z, 1000).is_err());
        assert_eq!(read_capped(&z, 1200).unwrap().len(), 2);
    }

    #[test]
    fn rejects_garbage_and_corruption() {
        assert!(read(b"nope").is_err());
        let mut z = write(&[("f".into(), b"abc".to_vec())]);
        z[LOCAL_LEN + 1] ^= 0xff; // flip a data byte
        assert!(read(&z).is_err());
    }

    #[test]
    fn unique_names() {
        let mut used = HashSet::new();
        assert_eq!(unique_name("a.png", &mut used), "a.png");
        assert_eq!(unique_name("a.png", &mut used), "a 2.png");
        assert_eq!(unique_name("b", &mut used), "b");
        assert_eq!(unique_name("b", &mut used), "b 2");
    }
}
