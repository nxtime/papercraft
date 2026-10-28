//! Opening a large project reads the page in view first; the other pages are read in the
//! background and must never be saved empty, stale or half read.

use papercraft::editor::Editor;
use papercraft::geom::Rect;
use papercraft::mcp::{Out, exec};
use papercraft::model::{Color, Document, Node, NodeId, NodeKind};
use papercraft::project::{PageData, Project, Store};
use papercraft::session::Session;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const BIG_PAGE_NODES: usize = 3000;

fn store() -> Store {
    Store::open(std::env::temp_dir().join(format!("papercraft-lazy-{:016x}", fastrand::u64(..))))
        .unwrap()
}

fn page(id: &str, name: &str, nodes: usize) -> PageData {
    let mut doc = Document {
        name: name.into(),
        ..Default::default()
    };
    for i in 0..nodes {
        let id = doc.alloc_id();
        let mut n = Node::new(
            id,
            NodeKind::Rect,
            Rect::new((i % 60) as f64 * 30.0, (i / 60) as f64 * 30.0, 24.0, 24.0),
        );
        n.name = format!("{name} {i}");
        n.style.set_fill(Some(Color::rgb((i % 255) as u8, 90, 160)));
        doc.nodes.insert(id, n);
        doc.root.push(id);
    }
    PageData {
        id: id.into(),
        name: name.into(),
        doc,
        camera: None,
    }
}

/// A saved project (design.json included, so it opens lazily): a one-node first page followed
/// by `big` pages of `BIG_PAGE_NODES` nodes.
fn project(big: usize) -> (Store, PathBuf) {
    let s = store();
    let mut pages = vec![page("p0", "Small", 1)];
    pages.extend((1..=big).map(|i| page(&format!("p{i}"), &format!("Big {i}"), BIG_PAGE_NODES)));
    let dir = s.create_with_pages("Lazy", pages).unwrap().dir;
    // The first open builds design.json from the pages (eagerly); saving writes it.
    let mut ed = Editor::default();
    let mut sess = Session::new(s.clone());
    sess.open(&mut ed, &dir).unwrap();
    sess.save(&mut ed).unwrap();
    assert!(dir.join("design.json").is_file());
    (s, dir)
}

fn open(s: &Store, dir: &PathBuf) -> (Session, Editor) {
    let mut ed = Editor::default();
    ed.headless = true;
    let mut sess = Session::new(s.clone());
    sess.open(&mut ed, dir).unwrap();
    (sess, ed)
}

fn page_files(dir: &PathBuf) -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<_> = std::fs::read_dir(dir.join("pages"))
        .unwrap()
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().to_string(), std::fs::read(e.path()).unwrap()))
        .collect();
    v.sort();
    v
}

#[test]
fn background_load_gives_the_same_documents_as_a_full_load() {
    let (s, dir) = project(3);
    let all = Project::open(&dir).unwrap().load_all().unwrap();
    let (_sess, mut ed) = open(&s, &dir);
    assert!(ed.pages.iter().any(|p| p.pending), "the other pages are not read at open");
    assert!(!ed.pages[0].pending, "the page in view is");

    // Without any blocking call, the loader's pages arrive on their own.
    let t0 = Instant::now();
    while ed.loading() {
        ed.poll_loading();
        assert!(t0.elapsed() < Duration::from_secs(120), "pages never arrived");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!ed.project_dirty(), "reading a page does not make it unsaved");
    for (i, p) in all.pages.iter().enumerate() {
        let got = ed.page_data(i);
        assert_eq!(got.id, p.id);
        assert_eq!(got.name, p.name);
        assert_eq!(got.doc, p.doc, "page {i}");
        assert_eq!(ed.pages[i].modes, all.modes[i], "modes of page {i}");
        assert!(!ed.pages[i].pending);
    }
}

#[test]
fn open_returns_before_the_large_pages_are_read() {
    let (s, dir) = project(3);
    let t = Instant::now();
    Project::open(&dir).unwrap().load_all().unwrap();
    let full = t.elapsed();

    let t = Instant::now();
    let (_sess, mut ed) = open(&s, &dir);
    let lazy = t.elapsed();
    eprintln!("open: {lazy:?} lazy, {full:?} full");
    assert!(ed.loading());
    assert!(lazy * 2 < full, "lazy open {lazy:?} vs full load {full:?}");
    ed.finish_loading();
    assert!(!ed.loading());
}

#[test]
fn saving_while_pages_load_keeps_the_unread_pages_as_they_are() {
    let (s, dir) = project(3);
    let before = page_files(&dir);
    let (mut sess, mut ed) = open(&s, &dir);
    assert!(ed.pages.iter().any(|p| p.pending));
    // Edit the page in view and save while the rest is still on its way.
    ed.add_node(Node::new(NodeId(0), NodeKind::Ellipse, Rect::new(0.0, 0.0, 9.0, 9.0)), None);
    assert!(ed.pages.iter().skip(1).all(|p| p.pending) || !ed.loading());
    assert_eq!(sess.save(&mut ed).unwrap(), 1, "only the edited page is written");
    let after = page_files(&dir);
    for (b, a) in before.iter().zip(&after) {
        if b.0 != "p0.json.zst" {
            assert_eq!(b, a, "{} was rewritten", b.0);
        }
    }
    let manifest = Project::open(&dir).unwrap().manifest;
    assert_eq!(manifest.pages.len(), 4, "the manifest still lists every page");

    // Everything survives a reload.
    ed.finish_loading();
    let all = Project::open(&dir).unwrap().load_all().unwrap();
    assert_eq!(all.pages[0].doc.nodes.len(), 2);
    for i in 1..4 {
        assert_eq!(all.pages[i].doc.nodes.len(), BIG_PAGE_NODES);
        assert_eq!(ed.page_doc(i), &all.pages[i].doc);
    }
    assert!(!ed.project_dirty());
}

#[test]
fn rename_and_delete_of_an_unread_page_are_kept() {
    let (s, dir) = project(2);
    let (mut sess, mut ed) = open(&s, &dir);
    ed.rename_page(1, "Renamed");
    ed.delete_page(2);
    sess.save(&mut ed).unwrap();
    ed.finish_loading();
    assert_eq!(ed.pages.len(), 2);
    assert_eq!(ed.pages[1].name, "Renamed");
    assert_eq!(ed.page_doc(1).name, "Renamed");
    assert_eq!(ed.page_doc(1).nodes.len(), BIG_PAGE_NODES, "the real page arrived");
    sess.save(&mut ed).unwrap();
    let p = Project::open(&dir).unwrap();
    assert!(!p.page_path("p2").exists(), "the deleted page's file is gone");
    let pages = p.load_all().unwrap().pages;
    assert_eq!(pages[1].name, "Renamed");
    assert_eq!(pages[1].doc.nodes.len(), BIG_PAGE_NODES);
}

#[test]
fn switching_to_an_unread_page_reads_it_first() {
    let (s, dir) = project(2);
    let (_sess, mut ed) = open(&s, &dir);
    assert!(ed.pages[2].pending);
    ed.switch_page(2);
    assert_eq!(ed.active_page, 2);
    assert_eq!(ed.doc.nodes.len(), BIG_PAGE_NODES);
    assert!(!ed.pages[2].pending);
}

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Value {
    match exec::call(h, name, &args) {
        Ok(outs) => match outs.into_iter().next() {
            Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(Value::String(t)),
            _ => Value::Null,
        },
        Err(e) => panic!("{name} failed: {e}"),
    }
}

#[test]
fn an_mcp_call_on_an_unread_page_waits_for_it() {
    let (s, dir) = project(2);
    let mut h = exec::SessionHost::new(s);
    h.session.open(&mut h.ed, &dir).unwrap();
    assert!(h.ed.pages[2].pending);

    // A read of the last page sees all of it.
    let info = call(&mut h, "get_basic_info", json!({"pageId": "p2"}));
    let counts: Vec<u64> = info["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["nodeCount"].as_u64().unwrap())
        .collect();
    assert_eq!(counts, [1, BIG_PAGE_NODES as u64, BIG_PAGE_NODES as u64]);

    // A write to an unread page lands in the real page, not in an empty stand-in.
    let (s2, dir2) = project(2);
    let mut h = exec::SessionHost::new(s2);
    h.session.open(&mut h.ed, &dir2).unwrap();
    assert!(h.ed.pages[2].pending);
    call(&mut h, "rename_nodes", json!({"updates": [{"nodeId": "p2:1", "name": "Renamed by agent"}]}));
    assert!(!h.ed.pages[2].pending);
    assert_eq!(h.ed.page_doc(2).nodes.len(), BIG_PAGE_NODES);
    assert_eq!(h.ed.page_doc(2).get(NodeId(1)).unwrap().name, "Renamed by agent");
    h.session.save(&mut h.ed).unwrap();
    let saved = Project::open(&dir2).unwrap().load_all().unwrap();
    assert_eq!(saved.pages[2].doc.nodes.len(), BIG_PAGE_NODES);
    assert_eq!(saved.pages[2].doc.get(NodeId(1)).unwrap().name, "Renamed by agent");
}

#[test]
fn a_page_that_cannot_be_read_is_never_written_over() {
    let (s, dir) = project(2);
    let bad = papercraft::project::page_file(&dir, "p2");
    std::fs::write(&bad, "{ not a page").unwrap();
    let (mut sess, mut ed) = open(&s, &dir);
    ed.finish_loading();
    assert!(ed.pages[2].failed.is_some());
    assert!(!ed.loading(), "a failed page is not waited for");
    ed.switch_page(2);
    assert_eq!(ed.active_page, 0, "cannot switch to a page that is not there");
    // An edit through the stand-in is not saved either.
    ed.with_page(2, |e| {
        e.add_node(Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 5.0, 5.0)), None)
    });
    ed.add_node(Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 5.0, 5.0)), None);
    sess.save(&mut ed).unwrap();
    assert_eq!(std::fs::read_to_string(&bad).unwrap(), "{ not a page");
}
