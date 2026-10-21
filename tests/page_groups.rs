//! Page groups: creating, renaming, membership and surviving a reopen.

use papercraft::editor::Editor;
use papercraft::project::{MANIFEST, Manifest, Project, Store};
use papercraft::session::Session;

fn temp_store() -> Store {
    let root = std::env::temp_dir().join(format!("papercraft-groups-{:016x}", fastrand::u64(..)));
    Store::open(root).unwrap()
}

fn ids(ed: &Editor) -> Vec<String> {
    ed.pages.iter().map(|p| p.id.clone()).collect()
}

#[test]
fn groups_are_created_renamed_and_removed() {
    let mut ed = Editor::default();
    assert!(ed.create_page_group("Design"));
    assert!(ed.meta_dirty, "the manifest has to be written again");
    assert!(!ed.create_page_group("Design"), "names are unique");
    assert!(!ed.create_page_group("  "), "names are not empty");
    assert!(ed.create_page_group("UX"));
    assert!(
        !ed.rename_page_group("Design", "UX"),
        "the new name is taken"
    );
    assert!(ed.rename_page_group("Design", "Product"));
    assert!(
        !ed.rename_page_group("Design", "Other"),
        "the old name is gone"
    );
    assert!(
        !ed.rename_page_group("Product", "Product"),
        "nothing changes"
    );
    assert!(ed.remove_page_group("Product"));
    assert!(!ed.remove_page_group("Product"));
    assert!(ed.page_groups.contains_key("UX"));
}

#[test]
fn pages_join_a_group_and_can_leave_it() {
    let mut ed = Editor::default();
    ed.add_page();
    ed.create_page_group("A");
    ed.create_page_group("B");
    let p0 = ed.pages[0].id.clone();
    let p1 = ed.pages[1].id.clone();

    assert!(ed.assign_page_group(&p0, Some("A")));
    assert_eq!(ed.page_group(&p0), Some("A"));
    assert!(!ed.assign_page_group(&p0, Some("A")), "nothing changes");
    assert!(ed.assign_page_group(&p0, Some("B")), "reassigned");
    assert_eq!(ed.page_group(&p0), Some("B"));
    assert!(ed.page_groups["A"].is_empty(), "the old group stays, empty");
    assert!(ed.assign_page_group(&p0, None), "out of every group");
    assert_eq!(ed.page_group(&p0), None);
    assert!(!ed.assign_page_group(&p0, None), "nothing to do");
    assert!(!ed.assign_page_group("ghost", Some("A")), "unknown page");
    assert!(!ed.assign_page_group(&p1, Some("Ghost")), "unknown group");
    assert_eq!(ed.page_group(&p1), None);
}

#[test]
fn added_and_duplicated_pages_inherit_the_group() {
    let mut ed = Editor::default();
    ed.create_page_group("Flows");
    let p0 = ed.pages[0].id.clone();
    ed.assign_page_group(&p0, Some("Flows"));

    // A new page joins the group of the page that was active.
    ed.add_page();
    let p1 = ed.pages[1].id.clone();
    assert_eq!(ed.page_group(&p1), Some("Flows"));

    // A copy joins the group of the page it was made from.
    let dup = ed.duplicate_page(0);
    let copy = ed.pages[dup].id.clone();
    assert_eq!(ed.page_group(&copy), Some("Flows"));
    assert_eq!(ed.page_groups["Flows"], vec![p0, copy, p1]);
}

#[test]
fn deleting_a_page_keeps_the_group() {
    let mut ed = Editor::default();
    ed.add_page();
    ed.add_page();
    ed.create_page_group("A");
    let p0 = ed.pages[0].id.clone();
    let p1 = ed.pages[1].id.clone();
    ed.assign_page_group(&p0, Some("A"));
    ed.assign_page_group(&p1, Some("A"));

    // An inactive page leaves the group; the group stays.
    ed.switch_page(0);
    ed.delete_page(1);
    assert!(ed.page_groups.contains_key("A"));
    assert_eq!(ed.page_groups["A"], vec![p0.clone()]);

    // The active page leaves the group too; the group stays, empty.
    ed.delete_page(0);
    assert!(ed.page_groups.contains_key("A"));
    assert!(ed.page_groups["A"].is_empty());
    assert_eq!(ed.pages.len(), 1);
}

#[test]
fn group_order_follows_the_page_list() {
    let mut ed = Editor::default();
    ed.add_page();
    ed.add_page();
    ed.create_page_group("A");
    for id in ids(&ed) {
        ed.assign_page_group(&id, Some("A"));
    }
    assert_eq!(ed.page_groups["A"], ids(&ed));

    // Moving a page moves it inside the group as well.
    ed.move_page(0, 2);
    assert_eq!(ed.page_groups["A"], ids(&ed));
}

#[test]
fn groups_survive_a_save_and_reopen() {
    let store = temp_store();
    let mut sess = Session::new(store.clone());
    let mut ed = Editor::default();
    sess.create(&mut ed, "Groups").unwrap();
    ed.add_page();
    ed.create_page_group("Design");
    ed.create_page_group("Empty");
    let p0 = ed.pages[0].id.clone();
    let p1 = ed.pages[1].id.clone();
    ed.assign_page_group(&p0, Some("Design"));
    ed.assign_page_group(&p1, Some("Design"));
    sess.save(&mut ed).unwrap();

    let dir = sess.project.as_ref().unwrap().dir.clone();
    let p = Project::open(&dir).unwrap();
    assert_eq!(
        p.manifest.page_groups["Design"],
        vec![p0.clone(), p1.clone()]
    );
    assert!(p.manifest.page_groups["Empty"].is_empty());

    let mut ed2 = Editor::default();
    Session::new(store.clone()).open(&mut ed2, &dir).unwrap();
    assert_eq!(ed2.page_group(&p0), Some("Design"));
    assert_eq!(ed2.page_group(&p1), Some("Design"));
    assert_eq!(ed2.page_groups["Design"], vec![p0, p1]);
    assert!(
        ed2.page_groups.contains_key("Empty"),
        "an empty group is kept"
    );
    assert!(ed2.page_groups["Empty"].is_empty());
    assert_eq!(ed2.pages.len(), 2);
    std::fs::remove_dir_all(&store.root).ok();
}

#[test]
fn old_manifests_without_groups_load() {
    let legacy = r#"{"format":"paper-rs-project","version":1,"id":"a","name":"Old",
        "created":0,"modified":0,"pages":[{"id":"p1","name":"Page 1"}]}"#;
    let m = Manifest::from_json(legacy).unwrap();
    assert!(m.page_groups.is_empty());

    // A project on disk whose manifest predates groups still opens.
    let store = temp_store();
    let mut sess = Session::new(store.clone());
    let mut ed = Editor::default();
    sess.create(&mut ed, "Old").unwrap();
    ed.create_page_group("Design");
    let id = ed.pages[0].id.clone();
    ed.assign_page_group(&id, Some("Design"));
    sess.save(&mut ed).unwrap();

    let dir = sess.project.as_ref().unwrap().dir.clone();
    let path = dir.join(MANIFEST);
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v.as_object_mut().unwrap().remove("page_groups");
    std::fs::write(&path, v.to_string()).unwrap();

    let mut ed2 = Editor::default();
    Session::new(store.clone()).open(&mut ed2, &dir).unwrap();
    assert!(ed2.page_groups.is_empty());
    assert_eq!(ed2.page_group(&id), None);
    assert_eq!(ed2.pages.len(), 1);
    std::fs::remove_dir_all(&store.root).ok();
}
