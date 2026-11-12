//! Android has no native file or message dialogs (the `rfd` crate has no backend for it): these
//! stand-ins keep the call sites unchanged and answer as if the person dismissed the dialog.
//! Opening and saving go through the projects home instead; picking a file from the phone's
//! storage is the next step (the Storage Access Framework).

use std::path::PathBuf;

#[derive(Default)]
pub struct FileDialog;

impl FileDialog {
    pub fn new() -> Self {
        Self
    }
    pub fn add_filter(self, _name: impl Into<String>, _extensions: &[impl ToString]) -> Self {
        self
    }
    pub fn set_title(self, _title: impl Into<String>) -> Self {
        self
    }
    pub fn set_file_name(self, _name: impl Into<String>) -> Self {
        self
    }
    pub fn pick_file(self) -> Option<PathBuf> {
        None
    }
    pub fn save_file(self) -> Option<PathBuf> {
        None
    }
    pub fn pick_folder(self) -> Option<PathBuf> {
        None
    }
}

pub enum MessageButtons {
    YesNoCancel,
}

pub enum MessageLevel {
    Error,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MessageDialogResult {
    Yes,
    No,
    Cancel,
}

#[derive(Default)]
pub struct MessageDialog;

impl MessageDialog {
    pub fn new() -> Self {
        Self
    }
    pub fn set_title(self, _title: impl Into<String>) -> Self {
        self
    }
    pub fn set_description(self, _text: impl Into<String>) -> Self {
        self
    }
    pub fn set_level(self, _level: MessageLevel) -> Self {
        self
    }
    pub fn set_buttons(self, _buttons: MessageButtons) -> Self {
        self
    }
    pub fn show(self) -> MessageDialogResult {
        MessageDialogResult::Cancel
    }
}
