use papercraft::app::Start;
use papercraft::editor::Editor;
use papercraft::io::{export, file, import, png, svg};
use papercraft::model::Color;
use papercraft::project::{MANIFEST, Store};
use papercraft::render::{frame, gpu::Headless};
use papercraft::session::Session;
use papercraft::ui::UiState;
use papercraft::ui::home::HomeView;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
papercraft — infinite canvas for planning, working and sketching

USAGE:
  papercraft                                 open the projects home screen
  papercraft PROJECT_DIR                     open a project folder
  papercraft FILE.papercraft                 open a PaperCraft document (not autosaved)
  papercraft --demo                          open the app with a sample board
  papercraft --screenshot OUT.png [--home | PROJECT|FILE|--demo] [--size WxH] [--scale S] [--chat]
                                           render the full UI headlessly to a PNG
  papercraft --export-png OUT.png PROJECT|FILE [--page N] [--scale S] [--transparent]
  papercraft --export-svg OUT.svg PROJECT|FILE [--page N]
  papercraft --export-pdf OUT.pdf PROJECT|FILE [--page N] [--frames | --all-pages]
  papercraft --export OUT PROJECT|FILE --format png|svg|pdf [--scope page|frames|all]
                                           [--page N] [--scale S] [--transparent]
                                           (frames/all with PNG/SVG write a folder)
  papercraft --import FILE                   create a project from .excalidraw, .svg or
                                           .papercraft and print its folder
  papercraft --help

Projects live in ~/Documents/papercraft (override with PAPERCRAFT_HOME).
";

fn is_project(p: &std::path::Path) -> bool {
    p.join(MANIFEST).is_file()
}

fn load_editor(path: Option<&str>, demo: bool) -> Result<Editor, String> {
    if demo {
        return Ok(papercraft::demo::showcase());
    }
    let Some(p) = path else {
        return Ok(Editor::default());
    };
    let p = PathBuf::from(p);
    if is_project(&p) {
        let mut ed = Editor::default();
        let store = Store::open(p.parent().unwrap_or(&p)).map_err(|e| e.to_string())?;
        Session::new(store)
            .open(&mut ed, &p)
            .map_err(|e| format!("{}: {e}", p.display()))?;
        return Ok(ed);
    }
    let (doc, cam) = file::load(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let mut ed = Editor::default();
    ed.load(doc, cam, Some(p));
    Ok(ed)
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let positional: Vec<&String> = {
        let mut out = Vec::new();
        let mut skip = false;
        for a in &args {
            if skip {
                skip = false;
                continue;
            }
            if matches!(
                a.as_str(),
                "--screenshot"
                    | "--export-png"
                    | "--export-svg"
                    | "--export-pdf"
                    | "--export"
                    | "--format"
                    | "--scope"
                    | "--import"
                    | "--size"
                    | "--scale"
                    | "--page"
            ) {
                skip = true;
                continue;
            }
            if !a.starts_with("--") {
                out.push(a);
            }
        }
        out
    };
    let input = positional.first().map(|s| s.as_str());
    let scale: f64 = value("--scale")
        .map(|s| s.parse().map_err(|_| "bad --scale"))
        .transpose()?
        .unwrap_or(2.0);

    if flag("--help") || flag("-h") {
        print!("{USAGE}");
        return Ok(());
    }
    let page: usize = value("--page")
        .map(|s| s.parse().map_err(|_| "bad --page"))
        .transpose()?
        .unwrap_or(0);
    if let Some(out) = value("--screenshot") {
        let mut ed = load_editor(input, flag("--demo"))?;
        let mut st = UiState::default();
        if flag("--chat") {
            st.chat_mode = true;
            st.project_dir = input.map(std::path::PathBuf::from).filter(|p| p.is_dir());
            // As if the project was already seen, so `draw` keeps Chat mode on.
            st.chat.project.clone_from(&st.project_dir);
            st.chat.picker_open = flag("--chat-picker");
            if let Some(dir) = &st.project_dir {
                st.runs = papercraft::runs::list(dir);
                st.runs_key.clone_from(&st.project_dir);
            }
        }
        if flag("--dark") {
            papercraft::theme::apply_choice(&egui::Context::default(), papercraft::theme::Choice::Dark);
        }
        if flag("--home") {
            let store = Store::open(Store::default_root()).map_err(|e| e.to_string())?;
            st.home = Some(HomeView::load(&store, papercraft::project::now_ms()));
        }
        let (w, h) = match value("--size") {
            Some(s) => {
                let (w, h) = s.split_once('x').ok_or("--size must be WxH")?;
                (
                    w.parse::<f32>().map_err(|_| "bad width")?,
                    h.parse::<f32>().map_err(|_| "bad height")?,
                )
            }
            None => (1440.0, 900.0),
        };
        let gpu = Headless::new()?;
        let img = frame::capture(&gpu, &mut ed, &mut st, [w, h], scale as f32, |ed| {
            ed.zoom_to_fit()
        });
        img.save(&out).map_err(|e| e.to_string())?;
        println!(
            "wrote {out} ({}x{}) via {}",
            img.width(),
            img.height(),
            gpu.adapter_info.name
        );
        return Ok(());
    }
    let page_doc = |input: Option<&str>| -> Result<papercraft::model::Document, String> {
        if flag("--demo") {
            return Ok(papercraft::demo::showcase().doc);
        }
        let path = input.ok_or("missing input file")?;
        let pages = import::load_any(path.as_ref())?;
        pages
            .into_iter()
            .nth(page)
            .map(|p| p.doc)
            .ok_or_else(|| format!("no page {page}"))
    };
    if let Some(out) = value("--export-png") {
        let doc = page_doc(input)?;
        let bg = (!flag("--transparent")).then_some(Color::WHITE);
        let (w, h) = png::export(out.as_ref(), &doc, None, scale, bg)?;
        println!("wrote {out} ({w}x{h})");
        return Ok(());
    }
    if let Some(out) = value("--export-svg") {
        let doc = page_doc(input)?;
        let s = svg::export(&doc, None, None).ok_or("nothing to export")?;
        std::fs::write(&out, s).map_err(|e| e.to_string())?;
        println!("wrote {out}");
        return Ok(());
    }
    let export_with =
        |out: &str, format: export::Format, scope: export::Scope| -> Result<(), String> {
            let pages: Vec<papercraft::project::PageData> = if flag("--demo") {
                vec![papercraft::project::PageData {
                    id: "demo".into(),
                    name: "Launch plan".into(),
                    doc: papercraft::demo::showcase().doc,
                    camera: None,
                }]
            } else {
                import::load_any(input.ok_or("missing input file")?.as_ref())?
            };
            if page >= pages.len() {
                return Err(format!("no page {page}"));
            }
            let refs: Vec<(&str, &papercraft::model::Document)> =
                pages.iter().map(|p| (p.name.as_str(), &p.doc)).collect();
            let spec = export::Spec {
                format,
                scope,
                scale,
                transparent: flag("--transparent"),
            };
            let items = export::items(&spec, &refs, page, &[]);
            let files = export::write(&spec, &items, out.as_ref(), None)?;
            for f in files {
                println!("wrote {}", f.display());
            }
            Ok(())
        };
    if let Some(out) = value("--export-pdf") {
        let scope = if flag("--all-pages") {
            export::Scope::AllPages
        } else if flag("--frames") {
            export::Scope::Frames
        } else {
            export::Scope::Page
        };
        return export_with(&out, export::Format::Pdf, scope);
    }
    if let Some(out) = value("--export") {
        let format = match value("--format").as_deref().unwrap_or("png") {
            "png" => export::Format::Png,
            "svg" => export::Format::Svg,
            "pdf" => export::Format::Pdf,
            f => return Err(format!("unknown --format {f}")),
        };
        let scope = match value("--scope").as_deref().unwrap_or("page") {
            "page" => export::Scope::Page,
            "frames" => export::Scope::Frames,
            "all" => export::Scope::AllPages,
            s => return Err(format!("unknown --scope {s}")),
        };
        return export_with(&out, format, scope);
    }
    if let Some(src) = value("--import") {
        let p = PathBuf::from(&src);
        let store = Store::open(Store::default_root()).map_err(|e| e.to_string())?;
        let name = p
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Imported".into());
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let pages = match ext.as_str() {
            "excalidraw" | "json" => {
                let t = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
                vec![papercraft::project::PageData {
                    id: papercraft::project::new_id(),
                    name: "Page 1".into(),
                    doc: papercraft::io::excalidraw::import_scene(&t)?,
                    camera: None,
                }]
            }
            "svg" => {
                let t = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
                vec![papercraft::project::PageData {
                    id: papercraft::project::new_id(),
                    name: "Page 1".into(),
                    doc: papercraft::io::svg_import::import_page(&t, &name)?,
                    camera: None,
                }]
            }
            _ => import::import_as_pages(&p)?.1,
        };
        let proj = store
            .create_with_pages(&name, pages)
            .map_err(|e| e.to_string())?;
        for pg in proj.load_pages().map_err(|e| e.to_string())? {
            papercraft::assets::save_to_dir(
                &proj.dir.join(papercraft::project::ASSETS_DIR),
                papercraft::assets::used_by(&pg.doc),
            )
            .map_err(|e| e.to_string())?;
        }
        println!("{}", proj.dir.display());
        return Ok(());
    }
    let start = match input {
        _ if flag("--demo") => Start::Scratch(Box::new(load_editor(None, true)?)),
        None => Start::Home,
        Some(p) if is_project(p.as_ref()) => Start::Project(PathBuf::from(p)),
        Some(p) => Start::Scratch(Box::new(load_editor(Some(p), false)?)),
    };
    papercraft::app::run(start)
}
