//! Figma `.fig` (Kiwi binary) and REST API JSON importer.

use crate::geom::{Rect, V2, v2};
use crate::model::{
    Align, AutoLayout, BlendMode, Cap, Color, Constraint, Constraints, Direction, Document,
    FillRule, ImageMode, Join, LayoutChild, Node, NodeId, NodeKind, Paint, Shadow, Sizing, Stop,
    StrokeAlign, Style, SubPath, TextAlign, TextStyle,
};
use crate::project::PageData;
use crate::vector::{self, VPath};
use serde_json::Value as Json;
use std::collections::{BTreeMap, HashMap};

const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const END_SIG: u32 = 0x0605_4b50;
const LOCAL_LEN: usize = 30;
const CENTRAL_LEN: usize = 46;
const END_LEN: usize = 22;
const METHOD_STORED: u16 = 0;
const METHOD_DEFLATE: u16 = 8;
const MAX_ENTRY: usize = 256 << 20;
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

// ---------------------------------------------------------------------------------- detection

/// Returns true if `bytes` is a bare `fig-kiwi` / `fig-jam.` binary or a ZIP archive containing
/// `"canvas.fig"`.
pub fn is_fig(bytes: &[u8]) -> bool {
    bytes.starts_with(b"fig-kiwi")
        || bytes.starts_with(b"fig-jam.")
        || (bytes.starts_with(b"PK\x03\x04") && zip_has_entry(bytes, "canvas.fig"))
}

/// Returns true if `text` is a Figma REST API file JSON response (`{"document": {"type": "DOCUMENT", "children": [...]}}`).
pub fn is_figma_rest_json(text: &str) -> bool {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('{')
        || !trimmed.contains("\"document\"")
        || !trimmed.contains("\"DOCUMENT\"")
    {
        return false;
    }
    let Ok(v) = serde_json::from_str::<Json>(trimmed) else {
        return false;
    };
    let Some(doc) = v.get("document").and_then(Json::as_object) else {
        return false;
    };
    doc.get("type").and_then(Json::as_str) == Some("DOCUMENT")
        && doc.get("children").is_some_and(Json::is_array)
}

// ---------------------------------------------------------------------------------- zip helper

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    b.get(i..i + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    b.get(i..i + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn zip_has_entry(bytes: &[u8], target: &str) -> bool {
    let Some(end) = (0..=bytes.len().saturating_sub(END_LEN))
        .rev()
        .find(|&i| u32_at(bytes, i) == Some(END_SIG))
    else {
        return false;
    };
    let Some(count) = u16_at(bytes, end + 10).map(|n| n as usize) else {
        return false;
    };
    let Some(mut at) = u32_at(bytes, end + 16).map(|n| n as usize) else {
        return false;
    };
    for _ in 0..count {
        if u32_at(bytes, at) != Some(CENTRAL_SIG) {
            return false;
        }
        let Some(name_len) = u16_at(bytes, at + 28).map(|n| n as usize) else {
            return false;
        };
        let Some(extra_len) = u16_at(bytes, at + 30).map(|n| n as usize) else {
            return false;
        };
        let Some(comment_len) = u16_at(bytes, at + 32).map(|n| n as usize) else {
            return false;
        };
        let Some(name) = bytes.get(at + CENTRAL_LEN..at + CENTRAL_LEN + name_len) else {
            return false;
        };
        if name == target.as_bytes() {
            return true;
        }
        at += CENTRAL_LEN + name_len + extra_len + comment_len;
    }
    false
}

/// Extract `canvas.fig`, optional `meta.json`, and any `images/*` entries from a `.fig` ZIP.
fn read_fig_zip(
    bytes: &[u8],
) -> Result<(Vec<u8>, Option<String>, HashMap<String, Vec<u8>>), String> {
    let end = (0..=bytes.len().saturating_sub(END_LEN))
        .rev()
        .find(|&i| u32_at(bytes, i) == Some(END_SIG))
        .ok_or_else(|| "not a valid zip archive".to_string())?;
    let count = u16_at(bytes, end + 10).ok_or("truncated zip")? as usize;
    let mut at = u32_at(bytes, end + 16).ok_or("truncated zip")? as usize;
    let mut canvas = None;
    let mut file_name = None;
    let mut images = HashMap::new();
    for _ in 0..count {
        if u32_at(bytes, at) != Some(CENTRAL_SIG) {
            return Err("corrupt zip central directory".into());
        }
        let method = u16_at(bytes, at + 10).ok_or("truncated zip")?;
        let csize = u32_at(bytes, at + 20).ok_or("truncated zip")? as usize;
        let name_len = u16_at(bytes, at + 28).ok_or("truncated zip")? as usize;
        let extra_len = u16_at(bytes, at + 30).ok_or("truncated zip")? as usize;
        let comment_len = u16_at(bytes, at + 32).ok_or("truncated zip")? as usize;
        let local = u32_at(bytes, at + 42).ok_or("truncated zip")? as usize;
        let raw_name = bytes
            .get(at + CENTRAL_LEN..at + CENTRAL_LEN + name_len)
            .ok_or("truncated zip")?;
        let name = String::from_utf8_lossy(raw_name).into_owned();
        at += CENTRAL_LEN + name_len + extra_len + comment_len;
        let wanted = name == "canvas.fig" || name == "meta.json" || name.starts_with("images/");
        if !wanted || name.ends_with('/') {
            continue;
        }
        if u32_at(bytes, local) != Some(LOCAL_SIG) {
            return Err(format!("corrupt zip local header for {name}"));
        }
        let l_name = u16_at(bytes, local + 26).ok_or("truncated zip")? as usize;
        let l_extra = u16_at(bytes, local + 28).ok_or("truncated zip")? as usize;
        let start = local + LOCAL_LEN + l_name + l_extra;
        let raw = bytes.get(start..start + csize).ok_or("truncated zip")?;
        let data = match method {
            METHOD_STORED => raw.to_vec(),
            METHOD_DEFLATE => miniz_oxide::inflate::decompress_to_vec_with_limit(raw, MAX_ENTRY)
                .map_err(|e| format!("cannot inflate {name}: {e:?}"))?,
            m => return Err(format!("{name}: unsupported zip compression method {m}")),
        };
        if name == "canvas.fig" {
            canvas = Some(data);
        } else if name == "meta.json" {
            file_name = serde_json::from_slice::<Json>(&data)
                .ok()
                .and_then(|j| {
                    j.get("file_name")
                        .and_then(Json::as_str)
                        .map(str::to_string)
                })
                .filter(|s| !s.trim().is_empty());
        } else if let Some(hash) = name.strip_prefix("images/")
            && !hash.is_empty()
        {
            images.insert(hash.to_string(), data);
        }
    }
    let canvas = canvas.ok_or_else(|| "zip archive does not contain canvas.fig".to_string())?;
    Ok((canvas, file_name, images))
}

// ---------------------------------------------------------------------------------- Kiwi wire format

const MAX_DEPTH: usize = 64;
const MAX_DEFS: usize = 10_000;
const MAX_FIELDS: usize = 10_000;
const MAX_VALUES: usize = 32 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefKind {
    Enum,
    Struct,
    Message,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldType {
    Bool,
    Byte,
    Int,
    Uint,
    Float,
    Str,
    Int64,
    Uint64,
    Def(usize),
}

const BUILTINS: [FieldType; 8] = [
    FieldType::Bool,
    FieldType::Byte,
    FieldType::Int,
    FieldType::Uint,
    FieldType::Float,
    FieldType::Str,
    FieldType::Int64,
    FieldType::Uint64,
];

#[derive(Clone, Debug)]
pub struct FieldDef {
    pub name: String,
    pub ty: Option<FieldType>,
    pub array: bool,
    pub value: u32,
}

#[derive(Clone, Debug)]
pub struct Def {
    pub name: String,
    pub kind: DefKind,
    pub fields: Vec<FieldDef>,
}

#[derive(Clone, Debug, Default)]
pub struct Schema {
    pub defs: Vec<Def>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Uint(u64),
    Float(f32),
    Str(String),
    Bytes(Vec<u8>),
    Enum(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Object(fields) => fields.iter().find(|(k, _)| k == name).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        let v = match self {
            Value::Float(f) => f64::from(*f),
            Value::Int(i) => *i as f64,
            Value::Uint(u) => *u as f64,
            Value::Bool(b) => f64::from(u8::from(*b)),
            _ => return None,
        };
        v.is_finite().then_some(v)
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Uint(u) => Some(*u),
            Value::Int(i) if *i >= 0 => Some(*i as u64),
            Value::Float(f) if *f >= 0.0 && f.is_finite() => Some(*f as u64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) | Value::Enum(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_list(&self) -> &[Value] {
        match self {
            Value::Array(a) => a,
            _ => &[],
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }
}

pub fn read_var_uint(data: &[u8], pos: &mut usize) -> Result<u32, String> {
    let mut value = 0u32;
    let mut shift = 0u32;
    loop {
        let b = *data
            .get(*pos)
            .ok_or_else(|| "kiwi: unexpected end of data".to_string())?;
        *pos += 1;
        value |= u32::from(b & 127).checked_shl(shift).unwrap_or(0);
        shift += 7;
        if b & 128 == 0 || shift >= 35 {
            return Ok(value);
        }
    }
}

pub fn read_var_int(data: &[u8], pos: &mut usize) -> Result<i32, String> {
    let v = read_var_uint(data, pos)?;
    Ok(if v & 1 != 0 {
        !((v >> 1) as i32)
    } else {
        (v >> 1) as i32
    })
}

pub fn read_var_uint64(data: &[u8], pos: &mut usize) -> Result<u64, String> {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let b = *data
            .get(*pos)
            .ok_or_else(|| "kiwi: unexpected end of data".to_string())?;
        *pos += 1;
        if shift == 56 {
            return Ok(value | (u64::from(b) << 56));
        }
        value |= u64::from(b & 127) << shift;
        shift += 7;
        if b & 128 == 0 {
            return Ok(value);
        }
    }
}

pub fn read_var_int64(data: &[u8], pos: &mut usize) -> Result<i64, String> {
    let v = read_var_uint64(data, pos)?;
    Ok(if v & 1 != 0 {
        !((v >> 1) as i64)
    } else {
        (v >> 1) as i64
    })
}

pub fn read_var_float(data: &[u8], pos: &mut usize) -> Result<f32, String> {
    if data.get(*pos) == Some(&0) {
        *pos += 1;
        return Ok(0.0);
    }
    let end = pos
        .checked_add(4)
        .filter(|e| *e <= data.len())
        .ok_or_else(|| "kiwi: unexpected end of float".to_string())?;
    let b = &data[*pos..end];
    *pos = end;
    let bits = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    Ok(f32::from_bits(bits.rotate_left(23)))
}

pub fn read_string(data: &[u8], pos: &mut usize) -> Result<String, String> {
    let rest = data
        .get(*pos..)
        .ok_or_else(|| "kiwi: unexpected end of string".to_string())?;
    let len = rest
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| "kiwi: unterminated string".to_string())?;
    let s = String::from_utf8_lossy(&rest[..len]).into_owned();
    *pos += len + 1;
    Ok(s)
}

fn read_byte(data: &[u8], pos: &mut usize) -> Result<u8, String> {
    let b = *data
        .get(*pos)
        .ok_or_else(|| "kiwi: unexpected end of data".to_string())?;
    *pos += 1;
    Ok(b)
}

impl Schema {
    pub fn decode(data: &[u8]) -> Result<Schema, String> {
        let mut pos = 0usize;
        let count = read_var_uint(data, &mut pos)? as usize;
        if count > MAX_DEFS || count > data.len().saturating_sub(pos) {
            return Err("kiwi schema: too many definitions".into());
        }
        let mut defs = Vec::with_capacity(count);
        let mut raw_types = Vec::new();
        for _ in 0..count {
            let name = read_string(data, &mut pos)?;
            let kind = match read_byte(data, &mut pos)? {
                0 => DefKind::Enum,
                1 => DefKind::Struct,
                2 => DefKind::Message,
                k => return Err(format!("kiwi schema: unknown definition kind {k}")),
            };
            let n = read_var_uint(data, &mut pos)? as usize;
            if n > MAX_FIELDS || n > data.len().saturating_sub(pos) {
                return Err("kiwi schema: too many fields".into());
            }
            let mut fields = Vec::with_capacity(n);
            for _ in 0..n {
                let fname = read_string(data, &mut pos)?;
                let ty = read_var_int(data, &mut pos)?;
                let array = read_byte(data, &mut pos)? & 1 != 0;
                let value = read_var_uint(data, &mut pos)?;
                raw_types.push(ty);
                fields.push(FieldDef {
                    name: fname,
                    ty: None,
                    array,
                    value,
                });
            }
            defs.push(Def { name, kind, fields });
        }
        let total = defs.len();
        let mut types = raw_types.into_iter();
        for d in &mut defs {
            for f in &mut d.fields {
                let raw = types.next().unwrap_or(0);
                if d.kind == DefKind::Enum {
                    continue;
                }
                f.ty = Some(if raw < 0 {
                    *BUILTINS
                        .get((!raw) as usize)
                        .ok_or_else(|| format!("kiwi schema: unknown builtin type {raw}"))?
                } else {
                    let i = raw as usize;
                    if i >= total {
                        return Err(format!(
                            "kiwi schema: field {} references missing definition {i}",
                            f.name
                        ));
                    }
                    FieldType::Def(i)
                });
            }
        }
        Ok(Schema { defs })
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.defs.iter().position(|d| d.name == name)
    }

    pub fn decode_message(&self, root: usize, data: &[u8]) -> Result<Value, String> {
        decode_message(self, root, data)
    }
}

pub fn decode_message(schema: &Schema, root: usize, data: &[u8]) -> Result<Value, String> {
    let mut pos = 0usize;
    let mut budget = MAX_VALUES;
    decode_def(schema, root, data, &mut pos, 0, &mut budget)
}

fn spend(budget: &mut usize, n: usize) -> Result<(), String> {
    *budget = budget
        .checked_sub(n)
        .ok_or_else(|| "kiwi message: value limit exceeded".to_string())?;
    Ok(())
}

fn decode_def(
    schema: &Schema,
    idx: usize,
    data: &[u8],
    pos: &mut usize,
    depth: usize,
    budget: &mut usize,
) -> Result<Value, String> {
    if depth > MAX_DEPTH {
        return Err("kiwi message: nested too deeply".into());
    }
    spend(budget, 1)?;
    let d = schema
        .defs
        .get(idx)
        .ok_or_else(|| "kiwi message: missing definition".to_string())?;
    match d.kind {
        DefKind::Enum => {
            let v = read_var_uint(data, pos)?;
            Ok(d.fields
                .iter()
                .find(|f| f.value == v)
                .map_or(Value::Uint(u64::from(v)), |f| Value::Enum(f.name.clone())))
        }
        DefKind::Struct => {
            let mut out = Vec::with_capacity(d.fields.len());
            for f in &d.fields {
                out.push((
                    f.name.clone(),
                    decode_field(schema, f, data, pos, depth, budget)?,
                ));
            }
            Ok(Value::Object(out))
        }
        DefKind::Message => {
            let mut out = Vec::new();
            loop {
                let id = read_var_uint(data, pos)?;
                if id == 0 {
                    return Ok(Value::Object(out));
                }
                let f = d
                    .fields
                    .iter()
                    .find(|f| f.value == id)
                    .ok_or_else(|| format!("kiwi message: {} has no field {id}", d.name))?;
                out.push((
                    f.name.clone(),
                    decode_field(schema, f, data, pos, depth, budget)?,
                ));
            }
        }
    }
}

fn decode_field(
    schema: &Schema,
    f: &FieldDef,
    data: &[u8],
    pos: &mut usize,
    depth: usize,
    budget: &mut usize,
) -> Result<Value, String> {
    let ty =
        f.ty.ok_or_else(|| "kiwi message: untyped field".to_string())?;
    if !f.array {
        return decode_value(schema, ty, data, pos, depth, budget);
    }
    let n = read_var_uint(data, pos)? as usize;
    if n > data.len().saturating_sub(*pos) {
        return Err("kiwi message: array longer than remaining data".into());
    }
    if ty == FieldType::Byte {
        spend(budget, 1)?;
        let bytes = data[*pos..*pos + n].to_vec();
        *pos += n;
        return Ok(Value::Bytes(bytes));
    }
    spend(budget, n)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(decode_value(schema, ty, data, pos, depth, budget)?);
    }
    Ok(Value::Array(out))
}

fn decode_value(
    schema: &Schema,
    ty: FieldType,
    data: &[u8],
    pos: &mut usize,
    depth: usize,
    budget: &mut usize,
) -> Result<Value, String> {
    Ok(match ty {
        FieldType::Bool => Value::Bool(read_byte(data, pos)? != 0),
        FieldType::Byte => Value::Uint(u64::from(read_byte(data, pos)?)),
        FieldType::Int => Value::Int(i64::from(read_var_int(data, pos)?)),
        FieldType::Uint => Value::Uint(u64::from(read_var_uint(data, pos)?)),
        FieldType::Float => Value::Float(read_var_float(data, pos)?),
        FieldType::Str => Value::Str(read_string(data, pos)?),
        FieldType::Int64 => Value::Int(read_var_int64(data, pos)?),
        FieldType::Uint64 => Value::Uint(read_var_uint64(data, pos)?),
        FieldType::Def(i) => decode_def(schema, i, data, pos, depth + 1, budget)?,
    })
}

// ---------------------------------------------------------------------------------- .fig binary import

fn inflate_chunk(chunk: &[u8]) -> Result<Vec<u8>, String> {
    if chunk.starts_with(&ZSTD_MAGIC) {
        return Err(
            "zstd-compressed .fig chunk: export as Figma JSON or standard deflate .fig".into(),
        );
    }
    miniz_oxide::inflate::decompress_to_vec_with_limit(chunk, MAX_ENTRY)
        .or_else(|_| miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(chunk, MAX_ENTRY))
        .map_err(|e| format!("corrupt deflate .fig chunk: {e:?}"))
}

fn decode_fig_canvas(canvas: &[u8]) -> Result<(Schema, Value), String> {
    if canvas.len() < 12 || (!canvas.starts_with(b"fig-kiwi") && !canvas.starts_with(b"fig-jam.")) {
        return Err("invalid .fig header (expected fig-kiwi or fig-jam.)".into());
    }
    let mut pos = 12usize;
    let mut chunks = Vec::new();
    while pos < canvas.len() && chunks.len() < 16 {
        let len = u32_at(canvas, pos).ok_or("truncated .fig chunk length")? as usize;
        let start = pos + 4;
        let end = start
            .checked_add(len)
            .filter(|e| *e <= canvas.len())
            .ok_or("truncated .fig chunk payload")?;
        chunks.push(&canvas[start..end]);
        pos = end;
    }
    if chunks.len() < 2 {
        return Err(".fig file requires schema and message chunks".into());
    }
    let schema_bytes = inflate_chunk(chunks[0])?;
    let msg_bytes = inflate_chunk(chunks[1])?;
    let schema = Schema::decode(&schema_bytes)?;
    let root = schema
        .find("Message")
        .or_else(|| schema.defs.iter().rposition(|d| d.kind == DefKind::Message))
        .ok_or_else(|| "kiwi schema has no Message definition".to_string())?;
    let msg = schema.decode_message(root, &msg_bytes)?;
    Ok((schema, msg))
}

fn kiwi_guid(v: &Value) -> Option<(u64, u64)> {
    Some((v.get("sessionID")?.as_u64()?, v.get("localID")?.as_u64()?))
}

/// Import a Figma `.fig` file (raw `fig-kiwi` / `fig-jam.` or ZIP-wrapped `canvas.fig`) into pages.
pub fn import_fig_bytes(stem: &str, bytes: &[u8]) -> Result<(String, Vec<PageData>), String> {
    let (canvas, zip_name, mut images) = if bytes.starts_with(b"PK\x03\x04") {
        read_fig_zip(bytes)?
    } else {
        (bytes.to_vec(), None, HashMap::new())
    };
    let (_schema, msg) = decode_fig_canvas(&canvas)?;
    let blobs: Vec<&[u8]> = msg
        .get("blobs")
        .map(Value::as_list)
        .unwrap_or_default()
        .iter()
        .map(|b| b.get("bytes").and_then(Value::as_bytes).unwrap_or_default())
        .collect();

    let changes = msg
        .get("nodeChanges")
        .map(Value::as_list)
        .unwrap_or_default();
    let mut nodes: BTreeMap<(u64, u64), &Value> = BTreeMap::new();
    for ch in changes {
        let Some(id) = ch.get("guid").and_then(kiwi_guid) else {
            continue;
        };
        if ch.get("phase").and_then(Value::as_str) == Some("REMOVED") {
            nodes.remove(&id);
        } else {
            nodes.insert(id, ch);
        }
    }

    let mut children_map: BTreeMap<(u64, u64), Vec<(&str, (u64, u64))>> = BTreeMap::new();
    let mut doc_id = None;
    let mut doc_name = None;
    for (&id, &ch) in &nodes {
        let ty = ch.get("type").and_then(Value::as_str).unwrap_or("");
        if ty == "DOCUMENT" {
            doc_id = Some(id);
            if let Some(n) = ch.get("name").and_then(Value::as_str)
                && !n.trim().is_empty()
                && n != "Document"
            {
                doc_name = Some(n.to_string());
            }
        }
        if let Some(pi) = ch.get("parentIndex")
            && let Some(pid) = pi.get("guid").and_then(kiwi_guid)
        {
            let pos = pi.get("position").and_then(Value::as_str).unwrap_or("");
            children_map.entry(pid).or_default().push((pos, id));
        }
    }
    for list in children_map.values_mut() {
        list.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.cmp(&b.1)));
        list.dedup_by(|a, b| a.1 == b.1);
    }

    let canvas_ids: Vec<(u64, u64)> = match doc_id.and_then(|d| children_map.get(&d)) {
        Some(kids) => kids
            .iter()
            .map(|(_, id)| *id)
            .filter(|id| {
                nodes
                    .get(id)
                    .and_then(|n| n.get("type"))
                    .and_then(Value::as_str)
                    == Some("CANVAS")
            })
            .collect(),
        None => nodes
            .iter()
            .filter(|(_, n)| n.get("type").and_then(Value::as_str) == Some("CANVAS"))
            .map(|(&id, _)| id)
            .collect(),
    };

    let mut pages = Vec::new();
    for cid in canvas_ids {
        let Some(&cnode) = nodes.get(&cid) else {
            continue;
        };
        let cname = cnode
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Page 1");
        let internal = cnode
            .get("internalOnly")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || cname == "Internal Only Canvas";
        if internal {
            continue;
        }
        let page_name = if cname.trim().is_empty() {
            format!("Page {}", pages.len() + 1)
        } else {
            cname.to_string()
        };
        let mut doc = Document {
            name: page_name.clone(),
            ..Default::default()
        };
        if let Some(kids) = children_map.get(&cid) {
            for &(_, kid) in kids {
                build_kiwi_node(
                    kid,
                    None,
                    &nodes,
                    &children_map,
                    &blobs,
                    &mut images,
                    &mut doc,
                    0,
                );
            }
        }
        pages.push(PageData {
            id: format!("page-{}", pages.len() + 1),
            name: page_name,
            doc,
            camera: None,
        });
    }

    if pages.is_empty() {
        let mut doc = Document {
            name: "Page 1".into(),
            ..Default::default()
        };
        for (&id, &ch) in &nodes {
            let ty = ch.get("type").and_then(Value::as_str).unwrap_or("");
            if !matches!(ty, "DOCUMENT" | "CANVAS" | "VARIABLE" | "VARIABLE_SET")
                && ch.get("parentIndex").is_none()
            {
                build_kiwi_node(
                    id,
                    None,
                    &nodes,
                    &children_map,
                    &blobs,
                    &mut images,
                    &mut doc,
                    0,
                );
            }
        }
        pages.push(PageData {
            id: "page-1".into(),
            name: "Page 1".into(),
            doc,
            camera: None,
        });
    }

    let project_name = zip_name.or(doc_name).unwrap_or_else(|| stem.to_string());
    Ok((project_name, pages))
}

#[allow(clippy::too_many_arguments)]
fn build_kiwi_node(
    guid: (u64, u64),
    parent: Option<NodeId>,
    nodes: &BTreeMap<(u64, u64), &Value>,
    children_map: &BTreeMap<(u64, u64), Vec<(&str, (u64, u64))>>,
    blobs: &[&[u8]],
    images: &mut HashMap<String, Vec<u8>>,
    doc: &mut Document,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some(&n) = nodes.get(&guid) else {
        return;
    };
    let ty = n.get("type").and_then(Value::as_str).unwrap_or("");
    let Some((kind, text_style)) = kiwi_node_kind(ty, n, blobs) else {
        return;
    };
    let (w, h) = n
        .get("size")
        .and_then(|s| Some((s.get("x")?.as_f64()?, s.get("y")?.as_f64()?)))
        .unwrap_or((100.0, 100.0));
    let (x, y) = n
        .get("transform")
        .map(|t| {
            (
                t.get("m02").and_then(Value::as_f64).unwrap_or(0.0),
                t.get("m12").and_then(Value::as_f64).unwrap_or(0.0),
            )
        })
        .unwrap_or((0.0, 0.0));
    let rect = Rect::new(x, y, w.max(0.0), h.max(0.0));
    let id = doc.alloc_id();
    let mut node = Node::new(id, kind, rect);
    if let Some(name) = n.get("name").and_then(Value::as_str)
        && !name.is_empty()
    {
        node.name = name.to_string();
    }
    node.parent = parent;
    node.visible = n.get("visible").and_then(Value::as_bool).unwrap_or(true);
    node.locked = n.get("locked").and_then(Value::as_bool).unwrap_or(false);
    node.text_style = text_style;
    node.component = matches!(ty, "COMPONENT" | "COMPONENT_SET" | "SYMBOL")
        || (ty == "FRAME" && n.get("isStateGroup").and_then(Value::as_bool) == Some(true));

    apply_kiwi_style(&mut node, n, blobs, images);
    if node.is_frame() {
        node.layout = kiwi_auto_layout(n);
    }
    node.layout_child = kiwi_layout_child(n);

    let is_container = node.is_container();
    if let Some(list) = doc.children_mut(parent) {
        list.push(id);
    }
    doc.nodes.insert(id, node);

    if is_container && let Some(kids) = children_map.get(&guid) {
        for &(_, kid) in kids {
            build_kiwi_node(
                kid,
                Some(id),
                nodes,
                children_map,
                blobs,
                images,
                doc,
                depth + 1,
            );
        }
    }
}

fn kiwi_node_kind(ty: &str, n: &Value, blobs: &[&[u8]]) -> Option<(NodeKind, TextStyle)> {
    let mut ts = TextStyle::default();
    let kind = match ty {
        "FRAME" | "COMPONENT" | "COMPONENT_SET" | "INSTANCE" | "SECTION" | "SYMBOL" => {
            let clip = !n
                .get("frameMaskDisabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            NodeKind::Frame { clip }
        }
        "GROUP" => NodeKind::Group,
        "RECTANGLE" | "ROUNDED_RECTANGLE" => NodeKind::Rect,
        "ELLIPSE" => NodeKind::Ellipse,
        "STAR" => {
            let points = n
                .get("count")
                .and_then(Value::as_f64)
                .map(|c| (c as u32).max(3))
                .unwrap_or(5);
            let inner = n
                .get("starInnerScale")
                .and_then(Value::as_f64)
                .unwrap_or(0.382)
                .clamp(0.01, 1.0);
            NodeKind::Star { points, inner }
        }
        "REGULAR_POLYGON" => {
            let sides = n
                .get("count")
                .and_then(Value::as_f64)
                .map(|c| (c as u32).max(3))
                .unwrap_or(3);
            NodeKind::Polygon { sides }
        }
        "TEXT" => {
            let text = n
                .get("textData")
                .and_then(|td| td.get("characters"))
                .and_then(Value::as_str)
                .or_else(|| n.get("characters").and_then(Value::as_str))
                .unwrap_or("")
                .to_string();
            let font_size = n
                .get("fontSize")
                .and_then(Value::as_f64)
                .unwrap_or(16.0)
                .max(1.0);
            let auto_width = match n.get("textAutoResize").and_then(Value::as_str) {
                Some("HEIGHT" | "NONE" | "TRUNCATE") => false,
                _ => true,
            };
            let align = match n.get("textAlignHorizontal").and_then(Value::as_str) {
                Some("CENTER") => TextAlign::Center,
                Some("RIGHT") => TextAlign::Right,
                _ => TextAlign::Left,
            };
            if let Some(style_str) = n
                .get("fontName")
                .and_then(|f| f.get("style"))
                .and_then(Value::as_str)
            {
                let lower = style_str.to_ascii_lowercase();
                ts.bold =
                    lower.contains("bold") || lower.contains("black") || lower.contains("heavy");
                ts.italic = lower.contains("italic") || lower.contains("oblique");
            }
            if n.get("fontWeight")
                .and_then(Value::as_f64)
                .is_some_and(|w| w >= 600.0)
            {
                ts.bold = true;
            }
            NodeKind::Text {
                text,
                font_size,
                auto_width,
                align,
            }
        }
        "LINE" | "VECTOR" | "BOOLEAN_OPERATION" => {
            if let Some((subpaths, fill_rule)) = kiwi_extract_path(n, blobs) {
                NodeKind::Path {
                    subpaths,
                    fill_rule,
                }
            } else {
                NodeKind::Rect
            }
        }
        "STICKY" => {
            let text = n
                .get("textData")
                .and_then(|td| td.get("characters"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let font_size = n
                .get("fontSize")
                .and_then(Value::as_f64)
                .unwrap_or(16.0)
                .max(1.0);
            NodeKind::Sticky { text, font_size }
        }
        "DOCUMENT" | "CANVAS" | "VARIABLE" | "VARIABLE_SET" | "SLICE" => return None,
        _ => NodeKind::Rect,
    };
    Some((kind, ts))
}

fn kiwi_extract_path(n: &Value, blobs: &[&[u8]]) -> Option<(Vec<SubPath>, FillRule)> {
    for key in ["fillGeometry", "strokeGeometry"] {
        let mut path = VPath::new();
        let mut fill_rule = FillRule::NonZero;
        for g in n.get(key).map(Value::as_list).unwrap_or_default() {
            if g.get("windingRule").and_then(Value::as_str) == Some("ODD")
                || g.get("windingRule").and_then(Value::as_str) == Some("EVENODD")
            {
                fill_rule = FillRule::EvenOdd;
            }
            if let Some(idx) = g.get("commandsBlob").and_then(Value::as_u64)
                && let Some(&blob) = blobs.get(idx as usize)
                && let Some(p) = decode_commands_blob(blob)
            {
                path.append(&p);
            } else if let Some(d) = g.get("path").and_then(Value::as_str)
                && let Some(p) = parse_svg_path_data(d)
            {
                path.append(&p);
            }
        }
        let subpaths = vector::to_subpaths(&path, V2::ZERO);
        if !subpaths.is_empty() {
            return Some((subpaths, fill_rule));
        }
    }
    None
}

fn decode_commands_blob(b: &[u8]) -> Option<VPath> {
    let mut p = VPath::new();
    let mut i = 0usize;
    let f = |b: &[u8], i: &mut usize| -> Option<f64> {
        let bytes: [u8; 4] = b.get(*i..*i + 4)?.try_into().ok()?;
        *i += 4;
        let v = f64::from(f32::from_le_bytes(bytes));
        v.is_finite().then_some(v)
    };
    while i < b.len() {
        let cmd = *b.get(i)?;
        i += 1;
        match cmd {
            0 => {
                p.close();
            }
            1 => {
                let (x, y) = (f(b, &mut i)?, f(b, &mut i)?);
                p.move_to(v2(x, y));
            }
            2 => {
                let (x, y) = (f(b, &mut i)?, f(b, &mut i)?);
                p.line_to(v2(x, y));
            }
            3 => {
                let (cx, cy, x, y) = (f(b, &mut i)?, f(b, &mut i)?, f(b, &mut i)?, f(b, &mut i)?);
                p.quad_to(v2(cx, cy), v2(x, y));
            }
            4 => {
                let (c1x, c1y, c2x, c2y, x, y) = (
                    f(b, &mut i)?,
                    f(b, &mut i)?,
                    f(b, &mut i)?,
                    f(b, &mut i)?,
                    f(b, &mut i)?,
                    f(b, &mut i)?,
                );
                p.cubic_to(v2(c1x, c1y), v2(c2x, c2y), v2(x, y));
            }
            _ => return None,
        }
    }
    (!p.is_empty()).then_some(p)
}

fn apply_kiwi_style(
    node: &mut Node,
    n: &Value,
    blobs: &[&[u8]],
    images: &mut HashMap<String, Vec<u8>>,
) {
    let mut style = Style::default();
    if let Some(fp) = n.get("fillPaints") {
        style.fills = fp
            .as_list()
            .iter()
            .filter_map(|p| kiwi_paint(p, blobs, images))
            .collect();
    } else if matches!(node.kind, NodeKind::Text { .. }) {
        style.fills = vec![Paint::solid(Color::INK)];
    }

    if let Some(sp) = n.get("strokePaints") {
        let strokes: Vec<Paint> = sp
            .as_list()
            .iter()
            .filter_map(|p| kiwi_paint(p, blobs, images))
            .collect();
        if let Some(first) = strokes.first() {
            style.stroke = Some(first.preview_color());
        }
    }
    if let Some(w) = n.get("strokeWeight").and_then(Value::as_f64) {
        style.stroke_width = w.max(0.0);
    }
    style.stroke_align = match n.get("strokeAlign").and_then(Value::as_str) {
        Some("INSIDE") => StrokeAlign::Inside,
        Some("OUTSIDE") => StrokeAlign::Outside,
        _ => StrokeAlign::Center,
    };
    style.cap = match n.get("strokeCap").and_then(Value::as_str) {
        Some("ROUND") => Cap::Round,
        Some("SQUARE") => Cap::Square,
        _ => Cap::Butt,
    };
    style.join = match n.get("strokeJoin").and_then(Value::as_str) {
        Some("ROUND") => Join::Round,
        Some("BEVEL") => Join::Bevel,
        _ => Join::Miter,
    };
    if let Some(dashes) = n.get("dashPattern") {
        style.dash = dashes
            .as_list()
            .iter()
            .filter_map(Value::as_f64)
            .filter(|&d| d > 0.0)
            .collect();
    }
    if let Some(r) = n.get("cornerRadius").and_then(Value::as_f64) {
        style.corner_radius = r.max(0.0);
    }
    let tl = n
        .get("rectangleTopLeftCornerRadius")
        .and_then(Value::as_f64);
    let tr = n
        .get("rectangleTopRightCornerRadius")
        .and_then(Value::as_f64);
    let br = n
        .get("rectangleBottomRightCornerRadius")
        .and_then(Value::as_f64);
    let bl = n
        .get("rectangleBottomLeftCornerRadius")
        .and_then(Value::as_f64);
    if n.get("rectangleCornerRadiiIndependent")
        .and_then(Value::as_bool)
        == Some(true)
        || tl.is_some()
        || tr.is_some()
        || br.is_some()
        || bl.is_some()
    {
        let base = style.corner_radius;
        let radii = [
            tl.unwrap_or(base).max(0.0),
            tr.unwrap_or(base).max(0.0),
            br.unwrap_or(base).max(0.0),
            bl.unwrap_or(base).max(0.0),
        ];
        if radii.iter().any(|&r| (r - base).abs() > 1e-6) {
            style.radii = Some(radii);
        } else {
            style.corner_radius = radii[0];
        }
    }
    style.opacity = n
        .get("opacity")
        .and_then(Value::as_f64)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    if let Some(bm) = n
        .get("blendMode")
        .and_then(Value::as_str)
        .and_then(parse_blend_mode)
    {
        style.blend = bm;
    }
    if let Some(effects) = n.get("effects") {
        for e in effects.as_list() {
            if e.get("visible").and_then(Value::as_bool) == Some(false) {
                continue;
            }
            if e.get("type").and_then(Value::as_str) == Some("DROP_SHADOW") {
                let offset = e.get("offset");
                let x = offset
                    .and_then(|o| o.get("x"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                let y = offset
                    .and_then(|o| o.get("y"))
                    .and_then(Value::as_f64)
                    .unwrap_or(4.0);
                let blur = e
                    .get("radius")
                    .and_then(Value::as_f64)
                    .unwrap_or(4.0)
                    .max(0.0);
                let spread = e.get("spread").and_then(Value::as_f64).unwrap_or(0.0);
                let color = e
                    .get("color")
                    .map(|c| kiwi_color(c, 1.0))
                    .unwrap_or(Color::rgba(0, 0, 0, 64));
                style.shadow = Some(Shadow {
                    x,
                    y,
                    blur,
                    spread,
                    color,
                });
                break;
            }
        }
    }
    node.style = style;
}

fn kiwi_color(c: &Value, opacity: f64) -> Color {
    let ch = |k: &str| {
        c.get(k)
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    };
    let a = c
        .get("a")
        .and_then(Value::as_f64)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0)
        * opacity;
    Color::rgba(
        (ch("r") * 255.0).round() as u8,
        (ch("g") * 255.0).round() as u8,
        (ch("b") * 255.0).round() as u8,
        (a.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

fn kiwi_paint(p: &Value, blobs: &[&[u8]], images: &mut HashMap<String, Vec<u8>>) -> Option<Paint> {
    if p.get("visible").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let opacity = p
        .get("opacity")
        .and_then(Value::as_f64)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let ty = p.get("type").and_then(Value::as_str)?;
    match ty {
        "SOLID" => {
            let color = p
                .get("color")
                .map(|c| kiwi_color(c, opacity))
                .unwrap_or(Color::BLACK);
            Some(Paint::Solid { color, var: None })
        }
        "GRADIENT_LINEAR" => {
            let mut stops: Vec<Stop> = p
                .get("stops")
                .map(Value::as_list)
                .unwrap_or_default()
                .iter()
                .filter_map(|s| {
                    let pos = s.get("position")?.as_f64()?.clamp(0.0, 1.0);
                    let color = kiwi_color(s.get("color")?, opacity);
                    Some(Stop { pos, color })
                })
                .collect();
            if stops.is_empty() {
                stops = vec![
                    Stop {
                        pos: 0.0,
                        color: Color::BLACK,
                    },
                    Stop {
                        pos: 1.0,
                        color: Color::WHITE,
                    },
                ];
            }
            let (from, to) = kiwi_gradient_endpoints(p.get("transform"), false);
            Some(Paint::Linear { stops, from, to })
        }
        "GRADIENT_RADIAL" | "GRADIENT_ANGULAR" | "GRADIENT_DIAMOND" => {
            let mut stops: Vec<Stop> = p
                .get("stops")
                .map(Value::as_list)
                .unwrap_or_default()
                .iter()
                .filter_map(|s| {
                    let pos = s.get("position")?.as_f64()?.clamp(0.0, 1.0);
                    let color = kiwi_color(s.get("color")?, opacity);
                    Some(Stop { pos, color })
                })
                .collect();
            if stops.is_empty() {
                stops = vec![
                    Stop {
                        pos: 0.0,
                        color: Color::BLACK,
                    },
                    Stop {
                        pos: 1.0,
                        color: Color::WHITE,
                    },
                ];
            }
            let (center, to) = kiwi_gradient_endpoints(p.get("transform"), true);
            let r = center.dist(to).max(0.01);
            Some(Paint::Radial {
                stops,
                center,
                radius: v2(r, r),
            })
        }
        "IMAGE" => {
            let img = p.get("image")?;
            let bytes = if let Some(idx) = img.get("dataBlob").and_then(Value::as_u64) {
                blobs.get(idx as usize).map(|b| b.to_vec())
            } else if let Some(hash_bytes) = img.get("hash").and_then(Value::as_bytes) {
                let hex: String = hash_bytes.iter().map(|b| format!("{b:02x}")).collect();
                images.remove(&hex)
            } else {
                None
            }?;
            let asset = crate::assets::register(&bytes).ok()?;
            let mode = match p.get("imageScaleMode").and_then(Value::as_str) {
                Some("FIT") => ImageMode::Fit,
                Some("STRETCH") => ImageMode::Stretch,
                Some("TILE") => ImageMode::Tile,
                _ => ImageMode::Fill,
            };
            Some(Paint::Image {
                asset,
                mode,
                opacity,
            })
        }
        _ => None,
    }
}

fn kiwi_gradient_endpoints(tf: Option<&Value>, radial: bool) -> (V2, V2) {
    let default = if radial {
        (v2(0.5, 0.5), v2(1.0, 0.5))
    } else {
        (v2(0.5, 0.0), v2(0.5, 1.0))
    };
    let Some(m) = tf else {
        return default;
    };
    let c = |k: &str| m.get(k).and_then(Value::as_f64);
    let (Some(a), Some(b), Some(e), Some(c_), Some(d), Some(f)) =
        (c("m00"), c("m01"), c("m02"), c("m10"), c("m11"), c("m12"))
    else {
        return default;
    };
    let det = a * d - b * c_;
    if det.abs() < 1e-9 {
        return default;
    }
    let inv = |x: f64, y: f64| {
        let dx = x - e;
        let dy = y - f;
        v2((d * dx - b * dy) / det, (-c_ * dx + a * dy) / det)
    };
    if radial {
        (inv(0.5, 0.5), inv(1.0, 0.5))
    } else {
        (inv(0.0, 0.5), inv(1.0, 0.5))
    }
}

fn kiwi_auto_layout(n: &Value) -> Option<AutoLayout> {
    let direction = match n.get("stackMode").and_then(Value::as_str)? {
        "HORIZONTAL" => Direction::Horizontal,
        "VERTICAL" => Direction::Vertical,
        _ => return None,
    };
    let wrap = n.get("stackWrap").and_then(Value::as_str) == Some("WRAP");
    let gap = n
        .get("itemSpacing")
        .or_else(|| n.get("stackSpacing"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let cross_gap = n
        .get("stackCounterSpacing")
        .or_else(|| n.get("counterAxisSpacing"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let base_pad = n.get("stackPadding").and_then(Value::as_f64).unwrap_or(0.0);
    let h_pad = n
        .get("stackHorizontalPadding")
        .and_then(Value::as_f64)
        .unwrap_or(base_pad);
    let v_pad = n
        .get("stackVerticalPadding")
        .and_then(Value::as_f64)
        .unwrap_or(base_pad);
    let top = n
        .get("stackPaddingTop")
        .and_then(Value::as_f64)
        .unwrap_or(v_pad);
    let right = n
        .get("stackPaddingRight")
        .and_then(Value::as_f64)
        .unwrap_or(h_pad);
    let bottom = n
        .get("stackPaddingBottom")
        .and_then(Value::as_f64)
        .unwrap_or(v_pad);
    let left = n
        .get("stackPaddingLeft")
        .and_then(Value::as_f64)
        .unwrap_or(h_pad);
    let primary = n
        .get("stackPrimaryAlignItems")
        .or_else(|| n.get("stackJustify"))
        .and_then(Value::as_str)
        .unwrap_or("MIN");
    let gap_auto = matches!(primary, "SPACE_BETWEEN" | "SPACE_EVENLY");
    let align_main = match primary {
        "CENTER" => Align::Center,
        "MAX" => Align::End,
        _ => Align::Start,
    };
    let align_cross = match n
        .get("stackCounterAlignItems")
        .or_else(|| n.get("stackAlign"))
        .and_then(Value::as_str)
    {
        Some("CENTER") => Align::Center,
        Some("MAX") => Align::End,
        _ => Align::Start,
    };
    Some(AutoLayout {
        direction,
        wrap,
        gap,
        gap_auto,
        cross_gap,
        padding: [top, right, bottom, left],
        align_main,
        align_cross,
    })
}

fn kiwi_layout_child(n: &Value) -> LayoutChild {
    let mut lc = LayoutChild::default();
    if n.get("stackPositioning").and_then(Value::as_str) == Some("ABSOLUTE") {
        lc.absolute = true;
    }
    if n.get("stackChildPrimaryGrow")
        .and_then(Value::as_f64)
        .is_some_and(|g| g > 0.0)
    {
        lc.sizing[0] = Sizing::Fill;
    }
    if n.get("stackChildAlignSelf").and_then(Value::as_str) == Some("STRETCH") {
        lc.sizing[1] = Sizing::Fill;
    }
    lc
}

// ---------------------------------------------------------------------------------- Figma REST JSON import

/// Import a Figma REST API file JSON string (`GET /v1/files/:key`) into pages.
pub fn import_rest_json(stem: &str, text: &str) -> Result<(String, Vec<PageData>), String> {
    let root: Json =
        serde_json::from_str(text).map_err(|e| format!("invalid Figma REST JSON: {e}"))?;
    let doc_obj = root
        .get("document")
        .and_then(Json::as_object)
        .ok_or_else(|| "missing top-level `document` object".to_string())?;
    if doc_obj.get("type").and_then(Json::as_str) != Some("DOCUMENT") {
        return Err("top-level `document.type` is not `DOCUMENT`".into());
    }
    let children = doc_obj
        .get("children")
        .and_then(Json::as_array)
        .ok_or_else(|| "missing `document.children` array".to_string())?;
    let project_name = root
        .get("name")
        .and_then(Json::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(stem)
        .to_string();

    let mut pages = Vec::new();
    for canvas in children {
        if canvas.get("type").and_then(Json::as_str) != Some("CANVAS") {
            continue;
        }
        let cname = canvas
            .get("name")
            .and_then(Json::as_str)
            .unwrap_or("Page 1");
        if canvas
            .get("internalOnly")
            .and_then(Json::as_bool)
            .unwrap_or(false)
            || cname == "Internal Only Canvas"
        {
            continue;
        }
        let page_name = if cname.trim().is_empty() {
            format!("Page {}", pages.len() + 1)
        } else {
            cname.to_string()
        };
        let mut doc = Document {
            name: page_name.clone(),
            ..Default::default()
        };
        if let Some(kids) = canvas.get("children").and_then(Json::as_array) {
            for child in kids {
                build_rest_node(child, None, V2::ZERO, &mut doc, 0);
            }
        }
        pages.push(PageData {
            id: format!("page-{}", pages.len() + 1),
            name: page_name,
            doc,
            camera: None,
        });
    }
    if pages.is_empty() {
        pages.push(PageData {
            id: "page-1".into(),
            name: "Page 1".into(),
            doc: Document {
                name: "Page 1".into(),
                ..Default::default()
            },
            camera: None,
        });
    }
    Ok((project_name, pages))
}

fn build_rest_node(
    j: &Json,
    parent: Option<NodeId>,
    parent_abs: V2,
    doc: &mut Document,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some(ty) = j.get("type").and_then(Json::as_str) else {
        return;
    };
    let Some((kind, text_style)) = rest_node_kind(ty, j) else {
        return;
    };
    let (rect, world_origin) = rest_node_rect(j, parent_abs);
    let id = doc.alloc_id();
    let mut node = Node::new(id, kind, rect);
    if let Some(name) = j.get("name").and_then(Json::as_str)
        && !name.is_empty()
    {
        node.name = name.to_string();
    }
    node.parent = parent;
    node.visible = j.get("visible").and_then(Json::as_bool).unwrap_or(true);
    node.locked = j.get("locked").and_then(Json::as_bool).unwrap_or(false);
    node.text_style = text_style;
    node.component = matches!(ty, "COMPONENT" | "COMPONENT_SET");

    apply_rest_style(&mut node, j);
    if node.is_frame() {
        node.layout = rest_auto_layout(j);
    }
    node.layout_child = rest_layout_child(j);

    let is_container = node.is_container();
    if let Some(list) = doc.children_mut(parent) {
        list.push(id);
    }
    doc.nodes.insert(id, node);

    if is_container && let Some(kids) = j.get("children").and_then(Json::as_array) {
        for child in kids {
            build_rest_node(child, Some(id), world_origin, doc, depth + 1);
        }
    }
}

fn rest_node_rect(j: &Json, parent_abs: V2) -> (Rect, V2) {
    if let Some(bb) = j.get("absoluteBoundingBox")
        && let (Some(ax), Some(ay), Some(w), Some(h)) = (
            bb.get("x").and_then(Json::as_f64),
            bb.get("y").and_then(Json::as_f64),
            bb.get("width").and_then(Json::as_f64),
            bb.get("height").and_then(Json::as_f64),
        )
    {
        let rect = Rect::new(ax - parent_abs.x, ay - parent_abs.y, w.max(0.0), h.max(0.0));
        return (rect, v2(ax, ay));
    }
    let (w, h) = j
        .get("size")
        .and_then(|s| Some((s.get("x")?.as_f64()?, s.get("y")?.as_f64()?)))
        .unwrap_or((100.0, 100.0));
    let (lx, ly) = j
        .get("relativeTransform")
        .and_then(Json::as_array)
        .and_then(|rows| {
            let r0 = rows.first()?.as_array()?;
            let r1 = rows.get(1)?.as_array()?;
            Some((r0.get(2)?.as_f64()?, r1.get(2)?.as_f64()?))
        })
        .unwrap_or((0.0, 0.0));
    let rect = Rect::new(lx, ly, w.max(0.0), h.max(0.0));
    (rect, parent_abs + v2(lx, ly))
}

fn rest_node_kind(ty: &str, j: &Json) -> Option<(NodeKind, TextStyle)> {
    let mut ts = TextStyle::default();
    let kind = match ty {
        "FRAME" | "COMPONENT" | "COMPONENT_SET" | "INSTANCE" | "SECTION" => {
            let clip = j
                .get("clipsContent")
                .and_then(Json::as_bool)
                .unwrap_or(ty != "SECTION");
            NodeKind::Frame { clip }
        }
        "GROUP" => NodeKind::Group,
        "RECTANGLE" => NodeKind::Rect,
        "ELLIPSE" => NodeKind::Ellipse,
        "REGULAR_POLYGON" => {
            let sides = j
                .get("pointCount")
                .and_then(Json::as_f64)
                .map(|c| (c as u32).max(3))
                .unwrap_or(3);
            NodeKind::Polygon { sides }
        }
        "STAR" => {
            let points = j
                .get("pointCount")
                .and_then(Json::as_f64)
                .map(|c| (c as u32).max(3))
                .unwrap_or(5);
            let inner = j
                .get("innerRadius")
                .and_then(Json::as_f64)
                .unwrap_or(0.382)
                .clamp(0.01, 1.0);
            NodeKind::Star { points, inner }
        }
        "LINE" | "VECTOR" | "BOOLEAN_OPERATION" => {
            if let Some((subpaths, fill_rule)) = rest_extract_path(j) {
                NodeKind::Path {
                    subpaths,
                    fill_rule,
                }
            } else {
                NodeKind::Rect
            }
        }
        "TEXT" => {
            let text = j
                .get("characters")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            let st = j.get("style");
            let font_size = st
                .and_then(|s| s.get("fontSize"))
                .and_then(Json::as_f64)
                .unwrap_or(16.0)
                .max(1.0);
            let auto_width = match st
                .and_then(|s| s.get("textAutoResize"))
                .and_then(Json::as_str)
            {
                Some("HEIGHT" | "NONE" | "TRUNCATE") => false,
                _ => true,
            };
            let align = match st
                .and_then(|s| s.get("textAlignHorizontal"))
                .and_then(Json::as_str)
            {
                Some("CENTER") => TextAlign::Center,
                Some("RIGHT") => TextAlign::Right,
                _ => TextAlign::Left,
            };
            if let Some(st) = st {
                let weight = st.get("fontWeight").and_then(Json::as_f64).unwrap_or(400.0);
                ts.bold = weight >= 600.0;
                ts.italic = st.get("italic").and_then(Json::as_bool).unwrap_or(false);
                if let Some(ls) = st.get("letterSpacing").and_then(Json::as_f64) {
                    ts.letter_spacing = ls;
                }
                if let Some(lh) = st
                    .get("lineHeightPx")
                    .and_then(Json::as_f64)
                    .filter(|&h| h > 0.0)
                {
                    ts.line_height = Some(lh / font_size);
                }
            }
            NodeKind::Text {
                text,
                font_size,
                auto_width,
                align,
            }
        }
        "SLICE" => return None,
        _ => return None,
    };
    Some((kind, ts))
}

fn rest_extract_path(j: &Json) -> Option<(Vec<SubPath>, FillRule)> {
    for key in ["fillGeometry", "strokeGeometry"] {
        let Some(arr) = j.get(key).and_then(Json::as_array) else {
            continue;
        };
        let mut path = VPath::new();
        let mut fill_rule = FillRule::NonZero;
        for g in arr {
            if g.get("windingRule").and_then(Json::as_str) == Some("EVENODD") {
                fill_rule = FillRule::EvenOdd;
            }
            if let Some(d) = g.get("path").and_then(Json::as_str)
                && let Some(p) = parse_svg_path_data(d)
            {
                path.append(&p);
            }
        }
        let subpaths = vector::to_subpaths(&path, V2::ZERO);
        if !subpaths.is_empty() {
            return Some((subpaths, fill_rule));
        }
    }
    None
}

fn apply_rest_style(node: &mut Node, j: &Json) {
    let mut style = Style::default();
    if let Some(fills) = j.get("fills").and_then(Json::as_array) {
        style.fills = fills.iter().filter_map(rest_paint).collect();
    } else if matches!(node.kind, NodeKind::Text { .. }) {
        style.fills = vec![Paint::solid(Color::INK)];
    }

    if let Some(strokes) = j.get("strokes").and_then(Json::as_array) {
        let list: Vec<Paint> = strokes.iter().filter_map(rest_paint).collect();
        if let Some(first) = list.first() {
            style.stroke = Some(first.preview_color());
        }
    }
    if let Some(w) = j.get("strokeWeight").and_then(Json::as_f64) {
        style.stroke_width = w.max(0.0);
    }
    style.stroke_align = match j.get("strokeAlign").and_then(Json::as_str) {
        Some("INSIDE") => StrokeAlign::Inside,
        Some("OUTSIDE") => StrokeAlign::Outside,
        _ => StrokeAlign::Center,
    };
    style.cap = match j.get("strokeCap").and_then(Json::as_str) {
        Some("ROUND") => Cap::Round,
        Some("SQUARE") => Cap::Square,
        _ => Cap::Butt,
    };
    style.join = match j.get("strokeJoin").and_then(Json::as_str) {
        Some("ROUND") => Join::Round,
        Some("BEVEL") => Join::Bevel,
        _ => Join::Miter,
    };
    if let Some(dashes) = j.get("strokeDashes").and_then(Json::as_array) {
        style.dash = dashes
            .iter()
            .filter_map(Json::as_f64)
            .filter(|&d| d > 0.0)
            .collect();
    }
    if let Some(r) = j.get("cornerRadius").and_then(Json::as_f64) {
        style.corner_radius = r.max(0.0);
    }
    if let Some(arr) = j.get("rectangleCornerRadii").and_then(Json::as_array)
        && arr.len() == 4
    {
        let radii = [
            arr[0].as_f64().unwrap_or(0.0).max(0.0),
            arr[1].as_f64().unwrap_or(0.0).max(0.0),
            arr[2].as_f64().unwrap_or(0.0).max(0.0),
            arr[3].as_f64().unwrap_or(0.0).max(0.0),
        ];
        style.radii = Some(radii);
        style.corner_radius = radii[0];
    }
    style.opacity = j
        .get("opacity")
        .and_then(Json::as_f64)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    if let Some(bm) = j
        .get("blendMode")
        .and_then(Json::as_str)
        .and_then(parse_blend_mode)
    {
        style.blend = bm;
    }
    if let Some(effects) = j.get("effects").and_then(Json::as_array) {
        for e in effects {
            if e.get("visible").and_then(Json::as_bool) == Some(false) {
                continue;
            }
            if e.get("type").and_then(Json::as_str) == Some("DROP_SHADOW") {
                let offset = e.get("offset");
                let x = offset
                    .and_then(|o| o.get("x"))
                    .and_then(Json::as_f64)
                    .unwrap_or(0.0);
                let y = offset
                    .and_then(|o| o.get("y"))
                    .and_then(Json::as_f64)
                    .unwrap_or(4.0);
                let blur = e
                    .get("radius")
                    .and_then(Json::as_f64)
                    .unwrap_or(4.0)
                    .max(0.0);
                let spread = e.get("spread").and_then(Json::as_f64).unwrap_or(0.0);
                let color = e
                    .get("color")
                    .map(|c| rest_color(c, 1.0))
                    .unwrap_or(Color::rgba(0, 0, 0, 64));
                style.shadow = Some(Shadow {
                    x,
                    y,
                    blur,
                    spread,
                    color,
                });
                break;
            }
        }
    }
    node.style = style;
}

fn rest_color(c: &Json, opacity: f64) -> Color {
    let ch = |k: &str| {
        c.get(k)
            .and_then(Json::as_f64)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    };
    let a = c
        .get("a")
        .and_then(Json::as_f64)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0)
        * opacity;
    Color::rgba(
        (ch("r") * 255.0).round() as u8,
        (ch("g") * 255.0).round() as u8,
        (ch("b") * 255.0).round() as u8,
        (a.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

fn rest_paint(p: &Json) -> Option<Paint> {
    if p.get("visible").and_then(Json::as_bool) == Some(false) {
        return None;
    }
    let opacity = p
        .get("opacity")
        .and_then(Json::as_f64)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let ty = p.get("type").and_then(Json::as_str)?;
    let handles: Vec<V2> = p
        .get("gradientHandlePositions")
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| Some(v2(v.get("x")?.as_f64()?, v.get("y")?.as_f64()?)))
                .collect()
        })
        .unwrap_or_default();
    let stops = || -> Vec<Stop> {
        let mut s: Vec<Stop> = p
            .get("gradientStops")
            .and_then(Json::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|st| {
                        let pos = st.get("position")?.as_f64()?.clamp(0.0, 1.0);
                        let color = rest_color(st.get("color")?, opacity);
                        Some(Stop { pos, color })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if s.is_empty() {
            s.push(Stop {
                pos: 0.0,
                color: Color::BLACK,
            });
            s.push(Stop {
                pos: 1.0,
                color: Color::WHITE,
            });
        }
        s
    };
    match ty {
        "SOLID" => {
            let color = p
                .get("color")
                .map(|c| rest_color(c, opacity))
                .unwrap_or(Color::BLACK);
            Some(Paint::Solid { color, var: None })
        }
        "GRADIENT_LINEAR" => {
            let from = handles.first().copied().unwrap_or(v2(0.5, 0.0));
            let to = handles.get(1).copied().unwrap_or(v2(0.5, 1.0));
            Some(Paint::Linear {
                stops: stops(),
                from,
                to,
            })
        }
        "GRADIENT_RADIAL" | "GRADIENT_ANGULAR" | "GRADIENT_DIAMOND" => {
            let center = handles.first().copied().unwrap_or(v2(0.5, 0.5));
            let to = handles.get(1).copied().unwrap_or(v2(1.0, 0.5));
            let rx = center.dist(to).max(0.01);
            let ry = handles
                .get(2)
                .map(|h| center.dist(*h).max(0.01))
                .unwrap_or(rx);
            Some(Paint::Radial {
                stops: stops(),
                center,
                radius: v2(rx, ry),
            })
        }
        _ => None,
    }
}

fn rest_auto_layout(j: &Json) -> Option<AutoLayout> {
    let direction = match j.get("layoutMode").and_then(Json::as_str)? {
        "HORIZONTAL" => Direction::Horizontal,
        "VERTICAL" => Direction::Vertical,
        _ => return None,
    };
    let wrap = j.get("layoutWrap").and_then(Json::as_str) == Some("WRAP");
    let gap = j.get("itemSpacing").and_then(Json::as_f64).unwrap_or(0.0);
    let cross_gap = j
        .get("counterAxisSpacing")
        .and_then(Json::as_f64)
        .unwrap_or(0.0);
    let pad = |k: &str| j.get(k).and_then(Json::as_f64).unwrap_or(0.0).max(0.0);
    let padding = [
        pad("paddingTop"),
        pad("paddingRight"),
        pad("paddingBottom"),
        pad("paddingLeft"),
    ];
    let primary = j
        .get("primaryAxisAlignItems")
        .and_then(Json::as_str)
        .unwrap_or("MIN");
    let gap_auto = matches!(primary, "SPACE_BETWEEN" | "SPACE_EVENLY");
    let align_main = match primary {
        "CENTER" => Align::Center,
        "MAX" => Align::End,
        _ => Align::Start,
    };
    let align_cross = match j.get("counterAxisAlignItems").and_then(Json::as_str) {
        Some("CENTER") => Align::Center,
        Some("MAX") => Align::End,
        _ => Align::Start,
    };
    Some(AutoLayout {
        direction,
        wrap,
        gap,
        gap_auto,
        cross_gap,
        padding,
        align_main,
        align_cross,
    })
}

fn rest_layout_child(j: &Json) -> LayoutChild {
    let parse_sizing = |k: &str| match j.get(k).and_then(Json::as_str) {
        Some("HUG") => Sizing::Hug,
        Some("FILL") => Sizing::Fill,
        _ => Sizing::Fixed,
    };
    let mut sizing = [
        parse_sizing("layoutSizingHorizontal"),
        parse_sizing("layoutSizingVertical"),
    ];
    if sizing[0] == Sizing::Fixed
        && j.get("layoutGrow")
            .and_then(Json::as_f64)
            .is_some_and(|g| g > 0.0)
    {
        sizing[0] = Sizing::Fill;
    }
    let absolute = j.get("layoutPositioning").and_then(Json::as_str) == Some("ABSOLUTE");
    let constraints = j
        .get("constraints")
        .map(|c| {
            let h = match c.get("horizontal").and_then(Json::as_str) {
                Some("RIGHT") => Constraint::Max,
                Some("LEFT_RIGHT" | "STRETCH") => Constraint::Stretch,
                Some("CENTER") => Constraint::Center,
                Some("SCALE") => Constraint::Scale,
                _ => Constraint::Min,
            };
            let v = match c.get("vertical").and_then(Json::as_str) {
                Some("BOTTOM") => Constraint::Max,
                Some("TOP_BOTTOM" | "STRETCH") => Constraint::Stretch,
                Some("CENTER") => Constraint::Center,
                Some("SCALE") => Constraint::Scale,
                _ => Constraint::Min,
            };
            Constraints { h, v }
        })
        .unwrap_or_default();
    LayoutChild {
        sizing,
        absolute,
        constraints,
        ..LayoutChild::default()
    }
}

fn parse_blend_mode(s: &str) -> Option<BlendMode> {
    Some(match s {
        "NORMAL" | "PASS_THROUGH" => BlendMode::Normal,
        "MULTIPLY" => BlendMode::Multiply,
        "SCREEN" => BlendMode::Screen,
        "OVERLAY" => BlendMode::Overlay,
        "DARKEN" => BlendMode::Darken,
        "LIGHTEN" => BlendMode::Lighten,
        "COLOR_DODGE" | "LINEAR_DODGE" => BlendMode::ColorDodge,
        "COLOR_BURN" | "LINEAR_BURN" => BlendMode::ColorBurn,
        "HARD_LIGHT" => BlendMode::HardLight,
        "SOFT_LIGHT" => BlendMode::SoftLight,
        "DIFFERENCE" => BlendMode::Difference,
        "EXCLUSION" => BlendMode::Exclusion,
        "HUE" => BlendMode::Hue,
        "SATURATION" => BlendMode::Saturation,
        "COLOR" => BlendMode::Color,
        "LUMINOSITY" => BlendMode::Luminosity,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------- SVG path parser

fn parse_svg_path_data(d: &str) -> Option<VPath> {
    let mut path = VPath::new();
    let mut chars = d.chars().peekable();
    let mut cur = V2::ZERO;
    let mut start = V2::ZERO;
    let mut cmd = ' ';

    let skip_sep = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| {
        while let Some(&c) = chars.peek() {
            if c.is_ascii_whitespace() || c == ',' {
                chars.next();
            } else {
                break;
            }
        }
    };
    let next_num = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| -> Option<f64> {
        skip_sep(chars);
        let mut s = String::new();
        if let Some(&c) = chars.peek()
            && (c == '+' || c == '-')
        {
            s.push(c);
            chars.next();
        }
        let mut seen_dot = false;
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                chars.next();
            } else if c == '.' && !seen_dot {
                seen_dot = true;
                s.push(c);
                chars.next();
            } else if (c == 'e' || c == 'E') && !s.is_empty() {
                s.push(c);
                chars.next();
                if let Some(&sign) = chars.peek()
                    && (sign == '+' || sign == '-')
                {
                    s.push(sign);
                    chars.next();
                }
            } else {
                break;
            }
        }
        s.parse::<f64>().ok().filter(|x| x.is_finite())
    };

    loop {
        skip_sep(&mut chars);
        let Some(&c) = chars.peek() else { break };
        if c.is_ascii_alphabetic() {
            cmd = c;
            chars.next();
        } else if cmd == ' ' {
            return None;
        }
        match cmd {
            'M' | 'm' => {
                let (x, y) = (next_num(&mut chars)?, next_num(&mut chars)?);
                cur = if cmd == 'm' { cur + v2(x, y) } else { v2(x, y) };
                start = cur;
                path.move_to(cur);
                cmd = if cmd == 'm' { 'l' } else { 'L' };
            }
            'L' | 'l' => {
                let (x, y) = (next_num(&mut chars)?, next_num(&mut chars)?);
                cur = if cmd == 'l' { cur + v2(x, y) } else { v2(x, y) };
                path.line_to(cur);
            }
            'H' | 'h' => {
                let x = next_num(&mut chars)?;
                cur.x = if cmd == 'h' { cur.x + x } else { x };
                path.line_to(cur);
            }
            'V' | 'v' => {
                let y = next_num(&mut chars)?;
                cur.y = if cmd == 'v' { cur.y + y } else { y };
                path.line_to(cur);
            }
            'Q' | 'q' => {
                let (cx, cy, x, y) = (
                    next_num(&mut chars)?,
                    next_num(&mut chars)?,
                    next_num(&mut chars)?,
                    next_num(&mut chars)?,
                );
                let ctrl = if cmd == 'q' {
                    cur + v2(cx, cy)
                } else {
                    v2(cx, cy)
                };
                cur = if cmd == 'q' { cur + v2(x, y) } else { v2(x, y) };
                path.quad_to(ctrl, cur);
            }
            'C' | 'c' => {
                let (c1x, c1y, c2x, c2y, x, y) = (
                    next_num(&mut chars)?,
                    next_num(&mut chars)?,
                    next_num(&mut chars)?,
                    next_num(&mut chars)?,
                    next_num(&mut chars)?,
                    next_num(&mut chars)?,
                );
                let c1 = if cmd == 'c' {
                    cur + v2(c1x, c1y)
                } else {
                    v2(c1x, c1y)
                };
                let c2 = if cmd == 'c' {
                    cur + v2(c2x, c2y)
                } else {
                    v2(c2x, c2y)
                };
                cur = if cmd == 'c' { cur + v2(x, y) } else { v2(x, y) };
                path.cubic_to(c1, c2, cur);
            }
            'Z' | 'z' => {
                path.close();
                cur = start;
                cmd = ' ';
            }
            _ => return None,
        }
    }
    (!path.is_empty()).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_json_imports_frames_autolayout_shapes_and_text() {
        let json = r#"{
            "name": "Design System",
            "document": {
                "id": "0:0",
                "name": "Document",
                "type": "DOCUMENT",
                "children": [
                    {
                        "id": "0:1",
                        "name": "Components",
                        "type": "CANVAS",
                        "children": [
                            {
                                "id": "1:2",
                                "name": "Card",
                                "type": "FRAME",
                                "clipsContent": true,
                                "absoluteBoundingBox": { "x": 100.0, "y": 200.0, "width": 320.0, "height": 180.0 },
                                "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 } }],
                                "layoutMode": "VERTICAL",
                                "layoutWrap": "WRAP",
                                "itemSpacing": 12.0,
                                "counterAxisSpacing": 8.0,
                                "paddingTop": 16.0,
                                "paddingRight": 20.0,
                                "paddingBottom": 16.0,
                                "paddingLeft": 20.0,
                                "primaryAxisAlignItems": "CENTER",
                                "counterAxisAlignItems": "MAX",
                                "children": [
                                    {
                                        "id": "1:3",
                                        "name": "Badge",
                                        "type": "RECTANGLE",
                                        "absoluteBoundingBox": { "x": 120.0, "y": 216.0, "width": 80.0, "height": 32.0 },
                                        "cornerRadius": 8.0,
                                        "layoutSizingHorizontal": "FILL",
                                        "layoutSizingVertical": "HUG",
                                        "fills": [{ "type": "SOLID", "color": { "r": 0.2, "g": 0.4, "b": 0.8, "a": 1.0 } }]
                                    },
                                    {
                                        "id": "1:4",
                                        "name": "Title",
                                        "type": "TEXT",
                                        "absoluteBoundingBox": { "x": 120.0, "y": 260.0, "width": 200.0, "height": 24.0 },
                                        "characters": "Hello Figma",
                                        "fills": [{ "type": "SOLID", "color": { "r": 0.1, "g": 0.1, "b": 0.1, "a": 1.0 } }],
                                        "style": {
                                            "fontSize": 20.0,
                                            "fontWeight": 700.0,
                                            "italic": true,
                                            "textAlignHorizontal": "CENTER",
                                            "textAutoResize": "WIDTH_AND_HEIGHT"
                                        }
                                    }
                                ]
                            }
                        ]
                    }
                ]
            }
        }"#;

        assert!(is_figma_rest_json(json));
        let (name, pages) = import_rest_json("fallback", json).expect("REST JSON should import");
        assert_eq!(name, "Design System");
        assert_eq!(pages.len(), 1);
        let page = &pages[0];
        assert_eq!(page.name, "Components");
        page.doc.validate().expect("document invariants hold");
        assert_eq!(page.doc.root.len(), 1);

        let frame = page.doc.get(page.doc.root[0]).unwrap();
        assert_eq!(frame.name, "Card");
        assert_eq!(frame.kind, NodeKind::Frame { clip: true });
        assert_eq!(frame.rect, Rect::new(100.0, 200.0, 320.0, 180.0));
        let layout = frame.layout.expect("frame has auto-layout");
        assert_eq!(layout.direction, Direction::Vertical);
        assert!(layout.wrap);
        assert_eq!(layout.gap, 12.0);
        assert_eq!(layout.cross_gap, 8.0);
        assert_eq!(layout.padding, [16.0, 20.0, 16.0, 20.0]);
        assert_eq!(layout.align_main, Align::Center);
        assert_eq!(layout.align_cross, Align::End);
        assert_eq!(frame.children.len(), 2);

        let rect = page.doc.get(frame.children[0]).unwrap();
        assert_eq!(rect.name, "Badge");
        assert_eq!(rect.kind, NodeKind::Rect);
        assert_eq!(rect.rect, Rect::new(20.0, 16.0, 80.0, 32.0));
        assert_eq!(rect.style.corner_radius, 8.0);
        assert_eq!(rect.layout_child.sizing, [Sizing::Fill, Sizing::Hug]);
        assert_eq!(
            rect.style.fills,
            vec![Paint::solid(Color::rgba(51, 102, 204, 255))]
        );

        let text = page.doc.get(frame.children[1]).unwrap();
        assert_eq!(
            text.kind,
            NodeKind::Text {
                text: "Hello Figma".into(),
                font_size: 20.0,
                auto_width: true,
                align: TextAlign::Center,
            }
        );
        assert!(text.text_style.bold);
        assert!(text.text_style.italic);
    }

    fn push_var_uint(out: &mut Vec<u8>, mut v: u32) {
        loop {
            let b = (v & 127) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return;
            }
            out.push(b | 128);
        }
    }

    fn push_var_int(out: &mut Vec<u8>, v: i32) {
        push_var_uint(out, ((v << 1) ^ (v >> 31)) as u32);
    }

    fn push_var_float(out: &mut Vec<u8>, f: f32) {
        let bits = f.to_bits().rotate_right(23);
        if bits & 255 == 0 {
            out.push(0);
        } else {
            out.extend_from_slice(&bits.to_le_bytes());
        }
    }

    fn push_str(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(s.as_bytes());
        out.push(0);
    }

    #[test]
    fn fig_kiwi_binary_imports_canvas_frame_and_rectangle() {
        // Build a synthetic Kiwi schema:
        // 0: struct GUID { uint sessionID, uint localID }
        // 1: struct ParentIndex { GUID guid, string position }
        // 2: struct Vector { float x, float y }
        // 3: struct Matrix { float m00, float m01, float m02, float m10, float m11, float m12 }
        // 4: struct Color { float r, float g, float b, float a }
        // 5: message Paint { string type=1, Color color=2 }
        // 6: message NodeChange { GUID guid=1, ParentIndex parentIndex=2, string type=3, string name=4, Vector size=5, Matrix transform=6, Paint[] fillPaints=7, float cornerRadius=8 }
        // 7: message Message { NodeChange[] nodeChanges=1 }
        // Builtin type indices: Bool=-1, Byte=-2, Int=-3, Uint=-4, Float=-5, Str=-6
        let mut schema = Vec::new();
        push_var_uint(&mut schema, 8); // 8 definitions

        // 0: struct GUID
        push_str(&mut schema, "GUID");
        schema.push(1); // Struct
        push_var_uint(&mut schema, 2);
        for (i, f) in ["sessionID", "localID"].iter().enumerate() {
            push_str(&mut schema, f);
            push_var_int(&mut schema, -4); // Uint
            schema.push(0);
            push_var_uint(&mut schema, (i + 1) as u32);
        }

        // 1: struct ParentIndex
        push_str(&mut schema, "ParentIndex");
        schema.push(1);
        push_var_uint(&mut schema, 2);
        push_str(&mut schema, "guid");
        push_var_int(&mut schema, 0); // Def(0)
        schema.push(0);
        push_var_uint(&mut schema, 1);
        push_str(&mut schema, "position");
        push_var_int(&mut schema, -6); // Str
        schema.push(0);
        push_var_uint(&mut schema, 2);

        // 2: struct Vector
        push_str(&mut schema, "Vector");
        schema.push(1);
        push_var_uint(&mut schema, 2);
        for (i, f) in ["x", "y"].iter().enumerate() {
            push_str(&mut schema, f);
            push_var_int(&mut schema, -5); // Float
            schema.push(0);
            push_var_uint(&mut schema, (i + 1) as u32);
        }

        // 3: struct Matrix
        push_str(&mut schema, "Matrix");
        schema.push(1);
        push_var_uint(&mut schema, 6);
        for (i, f) in ["m00", "m01", "m02", "m10", "m11", "m12"]
            .iter()
            .enumerate()
        {
            push_str(&mut schema, f);
            push_var_int(&mut schema, -5); // Float
            schema.push(0);
            push_var_uint(&mut schema, (i + 1) as u32);
        }

        // 4: struct Color
        push_str(&mut schema, "Color");
        schema.push(1);
        push_var_uint(&mut schema, 4);
        for (i, f) in ["r", "g", "b", "a"].iter().enumerate() {
            push_str(&mut schema, f);
            push_var_int(&mut schema, -5); // Float
            schema.push(0);
            push_var_uint(&mut schema, (i + 1) as u32);
        }

        // 5: message Paint
        push_str(&mut schema, "Paint");
        schema.push(2); // Message
        push_var_uint(&mut schema, 2);
        push_str(&mut schema, "type");
        push_var_int(&mut schema, -6); // Str
        schema.push(0);
        push_var_uint(&mut schema, 1);
        push_str(&mut schema, "color");
        push_var_int(&mut schema, 4); // Def(4) Color
        schema.push(0);
        push_var_uint(&mut schema, 2);

        // 6: message NodeChange
        push_str(&mut schema, "NodeChange");
        schema.push(2);
        push_var_uint(&mut schema, 8);
        let nc_fields: [(&str, i32, u8, u32); 8] = [
            ("guid", 0, 0, 1),
            ("parentIndex", 1, 0, 2),
            ("type", -6, 0, 3),
            ("name", -6, 0, 4),
            ("size", 2, 0, 5),
            ("transform", 3, 0, 6),
            ("fillPaints", 5, 1, 7),
            ("cornerRadius", -5, 0, 8),
        ];
        for (name, ty, arr, id) in nc_fields {
            push_str(&mut schema, name);
            push_var_int(&mut schema, ty);
            schema.push(arr);
            push_var_uint(&mut schema, id);
        }

        // 7: message Message
        push_str(&mut schema, "Message");
        schema.push(2);
        push_var_uint(&mut schema, 1);
        push_str(&mut schema, "nodeChanges");
        push_var_int(&mut schema, 6); // Def(6) NodeChange
        schema.push(1); // array
        push_var_uint(&mut schema, 1);

        // Build message payload with 3 NodeChanges: CANVAS (0:1), FRAME (1:1), RECTANGLE (1:2)
        let mut msg = Vec::new();
        push_var_uint(&mut msg, 1); // field 1: nodeChanges
        push_var_uint(&mut msg, 3); // 3 items

        // Node 1: CANVAS (0:1)
        push_var_uint(&mut msg, 1); // guid
        push_var_uint(&mut msg, 0);
        push_var_uint(&mut msg, 1);
        push_var_uint(&mut msg, 3); // type
        push_str(&mut msg, "CANVAS");
        push_var_uint(&mut msg, 4); // name
        push_str(&mut msg, "Main Page");
        push_var_uint(&mut msg, 0); // end NodeChange

        // Node 2: FRAME (1:1) under CANVAS (0:1)
        push_var_uint(&mut msg, 1); // guid
        push_var_uint(&mut msg, 1);
        push_var_uint(&mut msg, 1);
        push_var_uint(&mut msg, 2); // parentIndex
        push_var_uint(&mut msg, 0);
        push_var_uint(&mut msg, 1);
        push_str(&mut msg, "!");
        push_var_uint(&mut msg, 3); // type
        push_str(&mut msg, "FRAME");
        push_var_uint(&mut msg, 4); // name
        push_str(&mut msg, "Hero Frame");
        push_var_uint(&mut msg, 5); // size
        push_var_float(&mut msg, 400.0);
        push_var_float(&mut msg, 300.0);
        push_var_uint(&mut msg, 6); // transform
        for v in [1.0, 0.0, 50.0, 0.0, 1.0, 60.0] {
            push_var_float(&mut msg, v);
        }
        push_var_uint(&mut msg, 0); // end NodeChange

        // Node 3: RECTANGLE (1:2) under FRAME (1:1)
        push_var_uint(&mut msg, 1); // guid
        push_var_uint(&mut msg, 1);
        push_var_uint(&mut msg, 2);
        push_var_uint(&mut msg, 2); // parentIndex
        push_var_uint(&mut msg, 1);
        push_var_uint(&mut msg, 1);
        push_str(&mut msg, "!");
        push_var_uint(&mut msg, 3); // type
        push_str(&mut msg, "RECTANGLE");
        push_var_uint(&mut msg, 4); // name
        push_str(&mut msg, "Box");
        push_var_uint(&mut msg, 5); // size
        push_var_float(&mut msg, 120.0);
        push_var_float(&mut msg, 80.0);
        push_var_uint(&mut msg, 6); // transform
        for v in [1.0, 0.0, 10.0, 0.0, 1.0, 20.0] {
            push_var_float(&mut msg, v);
        }
        push_var_uint(&mut msg, 7); // fillPaints (1 item)
        push_var_uint(&mut msg, 1);
        push_var_uint(&mut msg, 1); // Paint.type
        push_str(&mut msg, "SOLID");
        push_var_uint(&mut msg, 2); // Paint.color
        for c in [1.0f32, 0.0, 0.0, 1.0] {
            push_var_float(&mut msg, c);
        }
        push_var_uint(&mut msg, 0); // end Paint
        push_var_uint(&mut msg, 8); // cornerRadius
        push_var_float(&mut msg, 6.0);
        push_var_uint(&mut msg, 0); // end NodeChange

        push_var_uint(&mut msg, 0); // end Message

        let c_schema = miniz_oxide::deflate::compress_to_vec(&schema, 6);
        let c_msg = miniz_oxide::deflate::compress_to_vec(&msg, 6);
        let mut fig_bytes = Vec::new();
        fig_bytes.extend_from_slice(b"fig-kiwi");
        fig_bytes.extend_from_slice(&20u32.to_le_bytes());
        fig_bytes.extend_from_slice(&(c_schema.len() as u32).to_le_bytes());
        fig_bytes.extend_from_slice(&c_schema);
        fig_bytes.extend_from_slice(&(c_msg.len() as u32).to_le_bytes());
        fig_bytes.extend_from_slice(&c_msg);

        assert!(is_fig(&fig_bytes));
        let (name, pages) =
            import_fig_bytes("Mockup", &fig_bytes).expect("fig-kiwi binary should import");
        assert_eq!(name, "Mockup");
        assert_eq!(pages.len(), 1);
        let page = &pages[0];
        assert_eq!(page.name, "Main Page");
        page.doc.validate().expect("document invariants hold");
        assert_eq!(page.doc.root.len(), 1);

        let frame = page.doc.get(page.doc.root[0]).unwrap();
        assert_eq!(frame.name, "Hero Frame");
        assert_eq!(frame.kind, NodeKind::Frame { clip: true });
        assert_eq!(frame.rect, Rect::new(50.0, 60.0, 400.0, 300.0));
        assert_eq!(frame.children.len(), 1);

        let rect = page.doc.get(frame.children[0]).unwrap();
        assert_eq!(rect.name, "Box");
        assert_eq!(rect.kind, NodeKind::Rect);
        assert_eq!(rect.rect, Rect::new(10.0, 20.0, 120.0, 80.0));
        assert_eq!(rect.style.corner_radius, 6.0);
        assert_eq!(rect.style.fills, vec![Paint::solid(Color::rgb(255, 0, 0))]);
    }
}
