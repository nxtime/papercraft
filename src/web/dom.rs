//! Small browser helpers: file download and pick, local storage, and the clipboard mirror.
//! Raw JS (like `vfs/mem.rs` does for IndexedDB) where web-sys would need many feature flags
//! or unstable APIs (`ClipboardItem`).

use crate::io::clipboard::Provider;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
export function download(name, bytes, mime) {
  const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10000);
}
export async function read_files(files) {
  const out = [];
  for (const f of files) out.push([f.name, new Uint8Array(await f.arrayBuffer())]);
  return out;
}
export function pick_files(accept, multiple) {
  return new Promise((res) => {
    const i = document.createElement('input');
    i.type = 'file';
    i.accept = accept;
    i.multiple = multiple;
    i.style.display = 'none';
    i.onchange = () => { res(read_files(i.files)); i.remove(); };
    i.oncancel = () => { res([]); i.remove(); };
    document.body.appendChild(i);
    i.click();
  });
}
export async function copy_png(bytes) {
  await navigator.clipboard.write([
    new ClipboardItem({ 'image/png': new Blob([bytes], { type: 'image/png' }) }),
  ]);
}
export function relay_global() {
  return typeof window.PAPERCRAFT_RELAY === 'string' ? window.PAPERCRAFT_RELAY : '';
}
"#)]
extern "C" {
    fn download(name: &str, bytes: &[u8], mime: &str);
    fn read_files(files: &JsValue) -> js_sys::Promise;
    fn pick_files(accept: &str, multiple: bool) -> js_sys::Promise;
    #[wasm_bindgen(catch)]
    async fn copy_png(bytes: &[u8]) -> Result<JsValue, JsValue>;
    fn relay_global() -> String;
}

/// `window.PAPERCRAFT_RELAY`, set by the page that hosts the app.
pub fn relay_override() -> Option<String> {
    Some(relay_global()).filter(|s| !s.is_empty())
}

/// A file the user chose or dropped.
pub struct Picked {
    pub name: String,
    pub bytes: Vec<u8>,
}

fn picked(rows: JsValue) -> Vec<Picked> {
    js_sys::Array::from(&rows)
        .iter()
        .filter_map(|row| {
            let row = js_sys::Array::from(&row);
            Some(Picked {
                name: row.get(0).as_string()?,
                bytes: js_sys::Uint8Array::new(&row.get(1)).to_vec(),
            })
        })
        .collect()
}

/// Ask for files with a hidden `<input type=file>`. Must start from a user gesture; resolves
/// to nothing when the dialog is cancelled.
pub async fn pick(accept: &str, multiple: bool) -> Result<Vec<Picked>, String> {
    wasm_bindgen_futures::JsFuture::from(pick_files(accept, multiple))
        .await
        .map(picked)
        .map_err(|e| format!("could not read the chosen file: {e:?}"))
}

/// Read the contents of a DOM `FileList` (dropped or pasted files).
pub async fn read_list(files: &web_sys::FileList) -> Result<Vec<Picked>, String> {
    wasm_bindgen_futures::JsFuture::from(read_files(files))
        .await
        .map(picked)
        .map_err(|e| format!("could not read the dropped file: {e:?}"))
}

/// Offer `bytes` as a browser download.
pub fn save_as(name: &str, bytes: &[u8], mime: &str) {
    download(name, bytes, mime);
}

pub fn mime_for(name: &str) -> &'static str {
    match crate::io::import::extension(name).as_str() {
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

/// Put a PNG on the system clipboard (needs a user gesture and permission; reports failure).
pub async fn copy_png_to_clipboard(png: Vec<u8>) -> Result<(), String> {
    copy_png(&png)
        .await
        .map(|_| ())
        .map_err(|e| format!("the browser refused the clipboard write: {e:?}"))
}

/// A page-lifetime `localStorage` value.
pub fn local_get(key: &str) -> Option<String> {
    super::window().local_storage().ok()??.get_item(key).ok()?
}

pub fn local_set(key: &str, value: &str) {
    if let Some(Ok(Some(s))) = Some(super::window().local_storage()) {
        let _ = s.set_item(key, value);
    }
}

/// What the last copy or paste carried. The editor's clipboard provider is synchronous but the
/// browser's is not, so the editor reads and writes this mirror; the shell fills it from DOM
/// `paste` events and pushes copies out to `navigator.clipboard`.
#[derive(Default)]
pub struct Mirror {
    pub text: Option<String>,
    pub image: Option<image::RgbaImage>,
}

pub type SharedMirror = Rc<RefCell<Mirror>>;

pub struct WebClipboard(pub SharedMirror);

impl Provider for WebClipboard {
    fn read_text(&mut self) -> Option<String> {
        self.0.borrow().text.clone()
    }

    fn write_text(&mut self, s: &str) {
        let mut m = self.0.borrow_mut();
        m.text = Some(s.into());
        m.image = None;
        // Fire and forget: failure only means other apps do not see the copy.
        let _ = super::window().navigator().clipboard().write_text(s);
    }

    fn read_image(&mut self) -> Option<image::RgbaImage> {
        self.0.borrow().image.clone()
    }

    fn write_image(&mut self, img: &image::RgbaImage) {
        self.0.borrow_mut().image = Some(img.clone());
        let Ok(png) = crate::io::export::encode_png(img) else {
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = copy_png_to_clipboard(png).await {
                log::warn!("{e}");
            }
        });
    }
}
