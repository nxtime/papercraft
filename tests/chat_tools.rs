//! Tools made for the in-app chat. `zoom_image`: a region of a chat attachment's full-resolution
//! original, and nothing outside the project's chat folder. `ask_user`: the question shape.

use papercraft::chat::{TMP_DIR, full_image_path};
use papercraft::mcp::{Out, exec};
use papercraft::project::Store;
use serde_json::json;

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, _| image::Rgba([(x % 256) as u8, 0, 0, 255]));
    papercraft::io::export::encode_png(&img).unwrap()
}

#[test]
fn zoom_reads_the_original_region_and_only_chat_attachments() {
    let root = std::env::temp_dir().join(format!("papercraft-zoom-{:016x}", fastrand::u64(..)));
    let mut h = exec::SessionHost::new(Store::open(&root).unwrap());
    h.session.create(&mut h.ed, "Zoom").unwrap();
    let folder = h.session.project.as_ref().unwrap().dir.join(TMP_DIR).join("chat1");
    std::fs::create_dir_all(&folder).unwrap();
    let sent = folder.join("1-shot.png");
    std::fs::write(&sent, png(400, 100)).unwrap();
    std::fs::write(full_image_path(&sent), png(4000, 1000)).unwrap();

    let out = exec::call(&mut h, "zoom_image", &json!({"path": sent, "x": 0.5, "y": 0.0, "width": 0.25, "height": 1.0})).unwrap();
    let (Some(Out::Image { data, .. }), Some(Out::Text(meta))) = (out.first(), out.get(1)) else { panic!("{out:?}") };
    let crop = image::load_from_memory(data).unwrap();
    assert_eq!((crop.width(), crop.height()), (1000, 1000), "a quarter of the 4000 px original, not of the sent copy");
    let meta: serde_json::Value = serde_json::from_str(meta).unwrap();
    assert_eq!(meta["region"], json!([2000, 0, 1000, 1000]));

    let outside = root.join("secret.png");
    std::fs::write(&outside, png(10, 10)).unwrap();
    let err = exec::call(&mut h, "zoom_image", &json!({"path": outside})).unwrap_err();
    assert!(err.contains("only images attached"), "{err}");
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn ask_user_checks_the_question_shape() {
    let root = std::env::temp_dir().join(format!("papercraft-ask-{:016x}", fastrand::u64(..)));
    let mut h = exec::SessionHost::new(Store::open(&root).unwrap());
    h.session.create(&mut h.ed, "Ask").unwrap();
    let ok = json!({"questions": [{"question": "Frame size?", "options": [{"label": "Desktop"}, {"label": "Phone"}]}]});
    assert!(exec::call(&mut h, "ask_user", &ok).is_ok());
    let one_option = json!({"questions": [{"question": "Size?", "options": [{"label": "Desktop"}]}]});
    assert!(exec::call(&mut h, "ask_user", &one_option).is_err());
    assert!(exec::call(&mut h, "ask_user", &json!({"questions": []})).is_err());
    std::fs::remove_dir_all(root).ok();
}
