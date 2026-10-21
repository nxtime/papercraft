//! Saving a version of a page and restoring it brings the saved content back, as one undo step.

use papercraft::{demo, versions};

#[test]
fn restoring_a_version_brings_back_the_saved_page_and_can_be_undone() {
    let dir = std::env::temp_dir().join(format!("papercraft-restore-{:016x}", fastrand::u64(..)));
    let mut e = demo::showcase();
    let saved_nodes = e.doc.nodes.len();
    let saved_roots = e.doc.root.clone();
    let page = e.page_data(e.active_page);
    let v = versions::save(&dir, &page.id, "Launch plan v1", false, &page.doc).unwrap();

    // Change the page: delete the first top-level layer.
    let first = e.doc.root[0];
    e.edit("Delete", |tx| tx.remove(first));
    assert!(e.doc.nodes.len() < saved_nodes);

    let snap = versions::load(&dir, &v.id).unwrap();
    e.restore_snapshot(&snap);
    assert_eq!(e.doc.nodes.len(), saved_nodes);
    assert_eq!(e.doc.root, saved_roots);
    e.doc.validate().unwrap();

    // One undo step goes back to the edited page.
    e.undo();
    assert!(e.doc.nodes.len() < saved_nodes);
    std::fs::remove_dir_all(dir).ok();
}
