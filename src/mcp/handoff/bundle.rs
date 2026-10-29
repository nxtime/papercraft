//! `export_handoff`: the folder a code base builds from, written in one call. Files are read
//! on demand by agents billed per token, so the bundle is many small files behind a one-line-
//! per-screen index, every file's size in tokens is in the manifest, and a run that changes
//! nothing writes nothing (a diff of the folder is the change).

use super::super::exec::{self, Host};
use super::super::{Out, ToolResult};
use super::{markdown, report_since, str_arg, text};
use crate::model::{Color, Document, NodeId, NodeKind};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

const MANIFEST: &str = "manifest.json";
/// Bumped when a file's layout changes, so a reader can refuse a bundle it does not know.
const FORMAT: u32 = 1;
/// Characters per token, near enough to budget a brief.
const CHARS_PER_TOKEN: usize = 4;
/// The page holding the team's question table, by name.
const HANDOFF_PAGE: &str = "handoff";

pub fn export(host: &mut dyn Host, a: &Value) -> ToolResult {
    let dir = Path::new(str_arg(a, "dir").ok_or("`dir` is required: the folder to write the bundle into")?).to_path_buf();
    let images = a.get("images").and_then(Value::as_bool).unwrap_or(true);
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    let mut inv: Value = serde_json::from_str(&call(host, "get_inventory", json!({}))?).map_err(|e| e.to_string())?;
    let project = host.project().map_or_else(|| "Untitled".into(), |p| p.manifest.name.clone());
    let questions = handoff_rows(host);
    inv["project"] = json!(project);
    inv["handoffQuestions"] = json!(questions);
    files.insert("inventory.json".into(), pretty(&inv)?);
    files.insert("tokens.json".into(), call(host, "get_variables", json!({"format": "json"}))?.into_bytes());
    files.insert("tokens.css".into(), call(host, "get_variables", json!({"format": "css"}))?.into_bytes());
    files.insert("tokens.tailwind.css".into(), call(host, "get_variables", json!({"format": "tailwind"}))?.into_bytes());
    files.insert("components.json".into(), call(host, "get_components", json!({}))?.into_bytes());
    files.insert("open-questions.md".into(), open_questions(&inv, &questions).into_bytes());

    let gpu = if images { host.gpu().map(Ok).unwrap_or_else(crate::render::gpu::Headless::new) } else { Err("images: false".into()) };
    let mut index = Vec::new();
    let mut shots = 0;
    for page in inv["pages"].as_array().into_iter().flatten() {
        let (page_id, page_name) = (page["id"].as_str().unwrap_or_default(), page["name"].as_str().unwrap_or_default());
        let screens = page["screens"].as_array().into_iter().flatten().collect::<Vec<_>>();
        if screens.is_empty() {
            continue;
        }
        index.push(format!("\n## {page_name} (screens/{}/)", slug(page_name)));
        let mut used = Vec::new();
        for s in screens {
            let (key, name) = (s["id"].as_str().unwrap_or_default(), s["name"].as_str().unwrap_or_default());
            let mut stem = format!("screens/{}/{}", slug(page_name), slug(name));
            if used.contains(&stem) {
                stem = format!("{stem}-{}", key.rsplit(':').next().unwrap_or_default());
            }
            used.push(stem.clone());
            let spec = call(host, "get_design_spec", json!({"pageId": page_id, "nodeId": key, "format": "compact"}))?;
            let (w, h, class) = (s["width"].as_f64().unwrap_or(0.0), s["height"].as_f64().unwrap_or(0.0), s["class"].as_str().unwrap_or_default());
            let spec = format!("<!-- {key} {w}x{h} {class} -->\n{spec}");
            index.push(format!("- {name} · {class} {w}×{h} · `{key}` · {}.txt ~{}", stem.rsplit('/').next().unwrap_or_default(), tokens(spec.len())));
            files.insert(format!("{stem}.txt"), spec.into_bytes());
            if let Ok(g) = &gpu {
                let ed = host.editor();
                let Some(p) = ed.pages.iter().position(|p| p.id == page_id) else { continue };
                let id = key.rsplit(':').next().and_then(|n| n.parse().ok()).map(NodeId).ok_or("bad screen key")?;
                files.insert(format!("{stem}.png"), shot(g, ed.page_doc(p), id)?);
                shots += 1;
            }
        }
    }
    let changes = match str_arg(a, "since") {
        Some(since) => {
            let r = report_since(host, since)?;
            files.insert("changes.md".into(), markdown(&r, None).into_bytes());
            files.insert("changes.json".into(), pretty(&serde_json::to_value(&r).map_err(|e| e.to_string())?)?);
            Some(json!({"tokens": r.tokens.len(), "components": r.components.len(), "screens": r.screens.len()}))
        }
        None => None,
    };
    let screens = inv["screens"].as_u64().unwrap_or(0);
    let head = format!(
        "# {project}: design handoff\n{screens} screens · {} components. Build from these files, not the live app. Tokens: tokens.json (DTCG, modes), tokens.css, tokens.tailwind.css. Components: components.json. Questions: open-questions.md.{}\nEach screen: compact spec (.txt, ~tokens) and a reference image (.png) beside it; a scrolling screen's image shows its whole content.\n",
        inv["components"].as_array().map_or(0, Vec::len),
        if changes.is_some() { " Changes since the handoff point: changes.md (index), changes.json." } else { "" },
    );
    files.insert("index.md".into(), (head + &index.join("\n") + "\n").into_bytes());

    let (written, removed) = write(&dir, files.clone())?;
    let mut out = json!({"dir": dir, "files": files.len() + 1, "written": written, "removed": removed, "screens": screens, "images": shots});
    if let Err(e) = gpu {
        out["imagesSkipped"] = json!(e);
    }
    if let Some(c) = changes {
        out["changes"] = c;
    }
    text(out)
}

fn call(host: &mut dyn Host, tool: &str, args: Value) -> Result<String, String> {
    exec::call(host, tool, &args)?
        .into_iter()
        .find_map(|o| match o {
            Out::Text(t) => Some(t),
            Out::Image { .. } => None,
        })
        .ok_or_else(|| format!("{tool} returned no text"))
}

fn pretty(v: &Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(v).map_err(|e| e.to_string())
}

fn tokens(bytes: usize) -> usize {
    bytes.div_ceil(CHARS_PER_TOKEN)
}

/// FNV-1a: stable across builds, which `DefaultHasher` is not.
fn hash(bytes: &[u8]) -> String {
    let h = bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3));
    format!("{h:016x}")
}

/// The screen at scale 1 on white, unrolled when its content scrolls (as the deck shows it).
fn shot(g: &crate::render::gpu::Headless, doc: &Document, id: NodeId) -> Result<Vec<u8>, String> {
    let unrolled = crate::io::deck::unroll::unrolled(doc, &[id]);
    let doc = unrolled.as_ref().unwrap_or(doc);
    let roots = crate::io::export::with_layers_on_top(doc, &[id]);
    crate::io::export::encode_png(&crate::io::png::render(g, doc, Some(&roots), 1.0, Some(Color::WHITE))?)
}

/// Writes the files whose bytes differ and the manifest, and removes files the last manifest
/// listed that this one does not (nothing else in the folder is touched). Returns how many
/// files were written and removed.
fn write(dir: &Path, mut files: BTreeMap<String, Vec<u8>>) -> Result<(usize, usize), String> {
    let old: Value = std::fs::read(dir.join(MANIFEST)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    let entries: BTreeMap<&String, Value> = files
        .iter()
        .map(|(k, v)| {
            let mut e = json!({"hash": hash(v)});
            if !k.ends_with(".png") {
                e["tokens"] = json!(tokens(v.len()));
            }
            (k, e)
        })
        .collect();
    let manifest = pretty(&json!({"format": FORMAT, "files": entries}))?;
    files.insert(MANIFEST.into(), manifest);
    let mut written = 0;
    for (rel, bytes) in &files {
        let path = dir.join(rel);
        if std::fs::read(&path).is_ok_and(|b| &b == bytes) {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        crate::io::atomic::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        written += 1;
    }
    let mut removed = 0;
    for rel in old["files"].as_object().into_iter().flat_map(|m| m.keys()).filter(|k| !files.contains_key(*k)) {
        // Only paths this tool wrote: never climb out of the folder.
        if rel.split('/').any(|p| [".", ".."].contains(&p) || p.is_empty()) {
            continue;
        }
        if std::fs::remove_file(dir.join(rel)).is_ok() {
            removed += 1;
        }
    }
    Ok((written, removed))
}

/// A file name from a layer name, as `shipcraft/bin/pc-freeze.py` writes it: word characters
/// kept, runs of spaces, `/` and `·` as one `-`, lower case.
fn slug(s: &str) -> String {
    let kept: String = s.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace() || "_/·.-".contains(*c)).collect();
    let mut out = String::new();
    for part in kept.trim().split(|c: char| c.is_whitespace() || c == '/' || c == '·').filter(|p| !p.is_empty()) {
        if !out.is_empty() {
            out.push('-');
        }
        out.push_str(part);
    }
    let out = out.trim_matches('-').to_lowercase();
    if out.is_empty() { "unnamed".into() } else { out }
}

/// The question table on a page named "Handoff": rows of (id like `C7`, question, where,
/// proposed default) text layers, in layer order.
fn handoff_rows(host: &mut dyn Host) -> Vec<Value> {
    let ed = host.editor();
    let Some(p) = ed.pages.iter().position(|p| p.name.trim().eq_ignore_ascii_case(HANDOFF_PAGE)) else { return Vec::new() };
    let doc = ed.page_doc(p);
    let mut texts = Vec::new();
    let mut stack: Vec<NodeId> = doc.root.iter().rev().copied().collect();
    while let Some(id) = stack.pop() {
        let Some(n) = doc.get(id).filter(|n| n.visible) else { continue };
        if let NodeKind::Text { text, .. } = &n.kind {
            texts.push(text.trim().to_owned());
        }
        stack.extend(n.children.iter().rev());
    }
    let is_id = |s: &str| {
        let letters = s.chars().take_while(char::is_ascii_uppercase).count();
        let digits = s.len() - letters;
        (1..=3).contains(&letters) && (1..=3).contains(&digits) && s[letters..].chars().all(|c| c.is_ascii_digit())
    };
    let mut rows = Vec::new();
    let mut i = 0;
    while i + 3 < texts.len() {
        if is_id(&texts[i]) {
            rows.push(json!({"id": texts[i], "question": texts[i + 1], "where": texts[i + 2], "default": texts[i + 3]}));
            i += 4;
        } else {
            i += 1;
        }
    }
    rows
}

fn open_questions(inv: &Value, table: &[Value]) -> String {
    let mut q = vec!["# Open questions".to_owned(), "Build with the proposed default; change it only when the owner answers otherwise.".into()];
    if !table.is_empty() {
        q.push("\n## Handoff table\n| # | Question | Where | Proposed default |\n|---|---|---|---|".into());
        q.extend(table.iter().map(|r| format!("| {} | {} | {} | {} |", r["id"].as_str().unwrap_or_default(), r["question"].as_str().unwrap_or_default(), r["where"].as_str().unwrap_or_default(), r["default"].as_str().unwrap_or_default())));
    }
    for p in inv["pages"].as_array().into_iter().flatten() {
        let open = p["openQuestions"].as_array().into_iter().flatten().collect::<Vec<_>>();
        if open.is_empty() {
            continue;
        }
        q.push(format!("\n## {}: notes marked `? (team)`", p["name"].as_str().unwrap_or_default()));
        q.extend(open.iter().map(|n| format!("- **{}**: {}", n["layer"].as_str().unwrap_or("detached"), n["text"].as_str().unwrap_or_default().replace('\n', " — "))));
    }
    q.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_match_the_freeze_script() {
        assert_eq!(slug("Consultor / Primeiro acesso · Pronto"), "consultor-primeiro-acesso-pronto");
        assert_eq!(slug("Pedidos (PDVs)!"), "pedidos-pdvs");
        assert_eq!(slug("  "), "unnamed");
    }
}
