//! Files in the browser: importing (pick / drop / paste), exporting as downloads, project
//! backups and home-screen thumbnails. Everything that touches the GPU is asynchronous here
//! (see `Headless::read_async`); the finished work re-enters the shell through `Web::me`.

use super::Web;
use super::dom::{self, Picked};
use crate::geom::V2;
use crate::io::export::{self, Format, Spec};
use crate::io::file;
use crate::io::import::{self, Imported};
use crate::library::LibItem;
use crate::model::{ImageMode, Node, NodeId, Paint};
use crate::project::format::PageData;

const BACKUP_NAME: &str = "PaperCraft backup.zip";
/// Pixel scale for "Copy as PNG" (matches the desktop app).
const COPY_PNG_SCALE: f64 = 2.0;
const IMAGE_ACCEPT: &str = ".png,.jpg,.jpeg,.gif,.webp";

fn import_accept() -> String {
    format!(
        ".{},.{},.excalidraw,.excalidrawlib,.json,.svg,.zip,{IMAGE_ACCEPT}",
        file::EXTENSION,
        file::LEGACY_EXTENSION
    )
}

impl Web {
    /// Run `f` on the shell once the future finishes (the shell is not borrowed across awaits).
    pub(super) fn finish<T: 'static>(
        &self,
        fut: impl std::future::Future<Output = T> + 'static,
        f: impl FnOnce(&mut Web, T) + 'static,
    ) {
        let me = self.me.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let out = fut.await;
            if let Some(w) = me.upgrade() {
                let mut w = w.borrow_mut();
                f(&mut w, out);
                w.dirty = true;
            }
        });
    }

    /// Open the file chooser and hand what was picked to `then`.
    fn pick_then(
        &self,
        accept: String,
        multiple: bool,
        then: impl FnOnce(&mut Web, Vec<Picked>) + 'static,
    ) {
        self.finish(
            async move { dom::pick(&accept, multiple).await },
            |w, r| match r {
                Ok(files) if files.is_empty() => {}
                Ok(files) => then(w, files),
                Err(e) => w.error(e),
            },
        );
    }

    // ---------------------------------------------------------------- import

    /// Home screen "Import…" and File > Open.
    pub(super) fn request_import(&self) {
        self.pick_then(import_accept(), true, |w, files| {
            for f in files {
                w.import_file(&f.name, &f.bytes, None);
            }
        });
    }

    /// Insert > Image.
    pub(super) fn request_place_image(&self) {
        self.pick_then(IMAGE_ACCEPT.into(), false, |w, files| {
            for f in files {
                w.import_file(&f.name, &f.bytes, None);
            }
        });
    }

    /// Image fill for fill `i` of `ids`.
    pub(super) fn request_image_fill(&self, ids: Vec<NodeId>, i: usize) {
        self.pick_then(IMAGE_ACCEPT.into(), false, move |w, files| {
            let Some(f) = files.into_iter().next() else {
                return;
            };
            match crate::assets::register(&f.bytes) {
                Ok(asset) => {
                    crate::ui::inspector::apply(&mut w.ed, "fills", &ids, |n| {
                        let paint = Paint::Image {
                            asset: asset.clone(),
                            mode: ImageMode::Fill,
                            opacity: 1.0,
                        };
                        match n.style.fills.get_mut(i) {
                            Some(slot) => *slot = paint,
                            None => n.style.fills.push(paint),
                        }
                    });
                }
                Err(e) => w.error(format!("Could not load {}: {e}", f.name)),
            }
        });
    }

    /// Files dropped on the canvas at `screen` (canvas px), or pasted.
    pub(super) fn import_files(&mut self, files: Vec<Picked>, screen: Option<egui::Pos2>) {
        let at = screen.map(|p| self.ed.camera.screen_to_world(self.to_canvas(p)));
        for f in files {
            self.import_file(&f.name, &f.bytes, at);
        }
    }

    fn in_project(&self) -> bool {
        self.session.project.is_some() && self.st.home.is_none()
    }

    fn view_center(&self) -> V2 {
        self.ed.camera.screen_to_world(self.ed.viewport * 0.5)
    }

    /// Bring one file in: SVG / Excalidraw / images go on the open page (a new project is made
    /// when none is open), documents become pages or a new project, zips restore a backup.
    pub(super) fn import_file(&mut self, name: &str, bytes: &[u8], at: Option<V2>) {
        let imported = match import::import_bytes(name, bytes) {
            Ok(i) => i,
            Err(e) => return self.error(format!("Could not import {name}: {e}")),
        };
        match imported {
            Imported::Backup => self.restore_backup(bytes),
            Imported::Library(items) => self.add_to_library(items),
            Imported::Pages(_pname, pages) if self.in_project() => self.add_pages(pages),
            Imported::Pages(pname, pages) => self.new_project_from(&pname, pages),
            Imported::Image(_) | Imported::Scene { .. } => {
                if !self.in_project() {
                    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
                    let r = self.session.create(&mut self.ed, stem);
                    self.enter_project(r);
                    if !self.in_project() {
                        return;
                    }
                }
                let at = at.unwrap_or_else(|| self.view_center());
                match imported {
                    Imported::Image(b) => self.place_image(name, &b, at),
                    Imported::Scene { nodes, group } => self.place_scene(name, nodes, group, at),
                    _ => unreachable!(),
                }
            }
        }
    }

    fn place_image(&mut self, name: &str, bytes: &[u8], at: V2) {
        match crate::assets::register(bytes) {
            Ok(id) => {
                self.ed.insert_image(&id, at);
                self.ed.status = format!("Placed {name}");
            }
            Err(e) => self.error(format!("Could not place {name}: {e}")),
        }
    }

    fn place_scene(&mut self, name: &str, nodes: Vec<Node>, group: Option<String>, at: V2) {
        use crate::io::clipboard::{bounds, center_offset, roots_of};
        let (roots, world) = roots_of(&nodes);
        let off = center_offset(&world, at);
        let label = format!("Import {name}");
        let ids = match group {
            // Keep an SVG together as one group.
            Some(group) => {
                let b = bounds(&world).unwrap_or_default();
                vec![self.ed.edit(&label, |tx| {
                    crate::io::svg_import::insert_group(tx, nodes, &group, b.min() + off)
                })]
            }
            None => self
                .ed
                .insert_subtrees(&label, &nodes, &roots, &world, off, false),
        };
        self.ed.selection = ids;
        self.ed.status = format!("Imported {name}");
    }

    fn add_pages(&mut self, pages: Vec<PageData>) {
        use crate::io::clipboard::roots_of;
        let n = pages.len();
        for pg in pages {
            let i = self.ed.add_page();
            self.ed.rename_page(i, &pg.name);
            let nodes: Vec<Node> = pg
                .doc
                .paint_order()
                .into_iter()
                .filter_map(|id| pg.doc.get(id).cloned())
                .collect();
            let (roots, world) = roots_of(&nodes);
            self.ed
                .insert_subtrees("Import page", &nodes, &roots, &world, V2::ZERO, false);
        }
        self.ed.status = format!("Imported {n} page(s)");
    }

    fn new_project_from(&mut self, name: &str, pages: Vec<PageData>) {
        match self.session.store.create_with_pages(name, pages) {
            Ok(proj) => {
                let dir = proj.dir.clone();
                let r = self.session.open(&mut self.ed, &dir);
                self.enter_project(r);
                self.ed.status = format!("Imported {name}");
            }
            Err(e) => self.error(format!("Could not create a project: {e}")),
        }
    }

    fn add_to_library(&mut self, items: Vec<LibItem>) {
        let n = items.len();
        self.st.library.library.items.extend(items);
        self.st.library.dirty = true;
        self.st.show_library = true;
        self.ed.status = format!("Added {n} item(s) to the library");
    }

    // ---------------------------------------------------------------- backup

    /// "Download backup": a zip of the whole project folder.
    pub(super) fn download_backup(&mut self) {
        if self.session.project.is_some() {
            self.save_project(true);
        }
        match crate::io::backup::pack(&self.session.store.root) {
            Ok(zip) => {
                dom::save_as(BACKUP_NAME, &zip, dom::mime_for(BACKUP_NAME));
                self.ed.status = "Backup downloaded".into();
            }
            Err(e) => self.error(format!("Could not back up: {e}")),
        }
    }

    fn restore_backup(&mut self, bytes: &[u8]) {
        match crate::io::backup::unpack(bytes, &self.session.store.root) {
            Ok(dirs) => {
                self.ed.status = format!("Restored {} project(s) from the backup", dirs.len());
                if self.st.home.is_some() {
                    self.show_home();
                }
            }
            Err(e) => self.error(format!("Could not restore the backup: {e}")),
        }
    }

    // ---------------------------------------------------------------- export

    /// Export as a download: one file, or a zip when the spec yields several.
    pub(super) fn export(&mut self, spec: Spec) {
        let names: Vec<String> = self.ed.pages.iter().map(|p| p.name.clone()).collect();
        let docs: Vec<_> = (0..names.len())
            .map(|i| self.ed.page_doc(i).clone())
            .collect();
        let (active, sel) = (self.ed.active_page, self.ed.top_selection());
        let project = if self.st.project_name.is_empty() {
            "PaperCraft".to_string()
        } else {
            self.st.project_name.clone()
        };
        let gpu = self.gpu_handle();
        self.finish(
            async move {
                let pages: Vec<_> = names.iter().map(String::as_str).zip(docs.iter()).collect();
                let items = export::items(&spec, &pages, active, &sel);
                if items.is_empty() {
                    return Err("Nothing to export".to_string());
                }
                let base = if items.len() == 1 {
                    items[0].name.clone()
                } else {
                    project
                };
                let files = export::render_files(&spec, &items, &base, &gpu).await?;
                if let [(name, bytes)] = files.as_slice() {
                    return Ok((name.clone(), bytes.clone()));
                }
                let zip = crate::io::zip::write(&files);
                Ok((format!("{}.zip", crate::project::folder_name(&base)), zip))
            },
            |w, r| match r {
                Ok((name, bytes)) => {
                    dom::save_as(&name, &bytes, dom::mime_for(&name));
                    w.ed.status = format!("Exported {name}");
                }
                Err(e) => w.error(format!("Export failed: {e}")),
            },
        );
    }

    /// Render the selection (or the page) and put the PNG on the clipboard.
    pub(super) fn copy_as_png(&mut self) {
        let roots = Some(self.ed.top_selection()).filter(|s| !s.is_empty());
        let doc = self.ed.doc.clone();
        let gpu = self.gpu_handle();
        self.finish(
            async move {
                crate::io::png::render_async(&gpu, &doc, roots.as_deref(), COPY_PNG_SCALE, None)
                    .await
            },
            |w, r| match r {
                Ok(img) => {
                    if let Some(io) = &mut w.ed.clipboard_io {
                        io.write_image(&img);
                        w.ed.status = format!("Copied {}×{} PNG", img.width(), img.height());
                    }
                }
                Err(e) => w.error(format!("Could not copy as PNG: {e}")),
            },
        );
    }

    // ---------------------------------------------------------------- thumbnails

    /// Render the open project's thumbnail off the main flow; the home screen refreshes when
    /// it lands.
    pub(super) fn start_thumbnail(&mut self) {
        let Some(job) = self.session.start_thumbnail(&self.ed) else {
            return;
        };
        let gpu = self.gpu_handle();
        self.finish(
            async move {
                let png = crate::session::render_thumbnail_async(&gpu, &job.doc).await?;
                crate::session::finish_thumbnail(&job.path, png)
            },
            |w, r| match r {
                Ok(()) if w.st.home.is_some() => w.show_home(),
                Ok(()) => {}
                Err(e) => log::warn!("thumbnail: {e}"),
            },
        );
    }

    /// Format picked by the inspector (kept next to `export` so both stay in sync).
    pub(super) fn export_format(&mut self, f: Format) {
        let spec = Spec {
            format: f,
            ..self.st.export
        };
        self.export(spec);
    }
}
