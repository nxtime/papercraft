//! Reads the pages of a project in the background, so a large project opens as soon as the page
//! in view is read. The editor installs what arrives (`Editor::poll_loading`) and, when something
//! needs a page that is still on its way, reads it itself.

use super::{FormatError, PageData, PageRef};
use crate::variables::PageModes;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

/// A page read from disk, not yet finalized against the design system.
pub type ReadPage = (PageData, PageModes);

/// Most threads reading pages at once.
#[cfg(not(target_arch = "wasm32"))]
const MAX_WORKERS: usize = 3;

#[derive(Debug)]
pub struct PageLoader {
    /// The project folder, for reads the editor does itself.
    pub dir: PathBuf,
    pub rx: Receiver<(String, Result<ReadPage, FormatError>)>,
}

impl PageLoader {
    /// Start reading `pages` (in this order). The browser has no threads: there `Project`
    /// never leaves pages pending, so this is not reached.
    pub fn spawn(dir: PathBuf, pages: Vec<PageRef>) -> PageLoader {
        let (tx, rx) = std::sync::mpsc::channel();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let queue = std::sync::Arc::new(std::sync::Mutex::new(
                pages.into_iter().collect::<std::collections::VecDeque<_>>(),
            ));
            let workers = queue.lock().map_or(1, |q| q.len()).clamp(1, MAX_WORKERS);
            for _ in 0..workers {
                let (queue, tx, dir) = (queue.clone(), tx.clone(), dir.clone());
                std::thread::spawn(move || {
                    loop {
                        let Some(r) = queue.lock().ok().and_then(|mut q| q.pop_front()) else {
                            return;
                        };
                        let path = super::page_file(&dir, &r.id);
                        // The editor is gone (closed or another project opened): stop reading.
                        if tx.send((r.id.clone(), super::read_page_file(&path, &r))).is_err() {
                            return;
                        }
                    }
                });
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (&tx, pages);
        }
        PageLoader { dir, rx }
    }
}
