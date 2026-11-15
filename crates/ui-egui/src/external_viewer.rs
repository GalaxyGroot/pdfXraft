//! File ▸ Open in external viewer: hand the saved file to another PDF program.
//!
//! Preferences ▸ Documents and view ▸ External PDF viewer holds the program's name or path (for
//! example MuPDF's `mupdf-gl`). PdfCraft starts it only when the user chooses the command, passes
//! the file as its only argument without a shell, and never starts anything while no program is
//! set. The viewer is a separate process: nothing of it is linked into or shipped with PdfCraft.

/// The longest program name or path kept for Preferences ▸ External PDF viewer.
pub(crate) const MAX_VIEWER_CHARS: usize = 1024;

/// Whether `program` can be kept as the external viewer: empty (none), or a name or path of sane
/// length without control characters. Settings files and the control channel are untrusted.
pub(crate) fn valid_viewer(program: &str) -> bool {
    program.chars().count() <= MAX_VIEWER_CHARS && !program.chars().any(char::is_control)
}

/// The command that opens `path` in `program`: the file is the only argument, no shell is
/// involved, and a path starting with `-` can't become an option (it is made absolute first).
#[cfg(not(target_arch = "wasm32"))]
fn external_command(program: &str, path: &std::path::Path) -> Result<std::process::Command, String> {
    let program = program.trim();
    if program.is_empty() {
        return Err("Set an external viewer in Preferences first".into());
    }
    if !valid_viewer(program) {
        return Err("The external viewer setting is not a program name or path".into());
    }
    let path = std::path::absolute(path).map_err(|e| e.to_string())?;
    if !path.try_exists().map_err(|e| e.to_string())? {
        return Err("The file no longer exists".into());
    }
    let mut command = std::process::Command::new(program);
    command.arg(&path).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    Ok(command)
}

/// Start the external viewer on `path` and leave it running on its own.
#[cfg(not(target_arch = "wasm32"))]
fn open_external(program: &str, path: &std::path::Path) -> Result<(), String> {
    let mut command = external_command(program, path)?;
    let mut child = command.spawn().map_err(|e| format!("Could not start {}: {e}", program.trim()))?;
    // Reaped when it ends, so it doesn't linger as a zombie process.
    let _ = std::thread::Builder::new().name("pdfcraft-external-viewer".into()).spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

impl crate::PdfCraftApp {
    /// File ▸ Open in external viewer, for the document in tab `active`.
    pub(crate) fn open_in_external_viewer(&mut self, active: Option<usize>) {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = active;
            self.notify("The external viewer is not available in the web version");
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(doc) = active.and_then(|i| self.views.get(i)).and_then(|v| self.session.get(v.id)) else { return };
            let (path, dirty) = (doc.path.clone(), doc.dirty);
            let Some(path) = path else {
                self.notify("Save the file first: the external viewer opens the saved file");
                return;
            };
            match open_external(&self.external_viewer, std::path::Path::new(&path)) {
                Ok(()) if dirty => self.notify("The external viewer shows the saved file, without your unsaved changes"),
                Ok(()) => {}
                Err(e) => self.notify_error(e),
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn a_file() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("pdfcraft-viewer-{}.pdf", std::process::id()));
        std::fs::write(&path, b"%PDF-1.4\n").unwrap();
        path
    }

    #[test]
    fn no_viewer_set_starts_nothing() {
        let path = a_file();
        for none in ["", "   "] {
            let err = external_command(none, &path).unwrap_err();
            assert!(err.contains("Preferences"), "{err}");
        }
    }

    #[test]
    fn the_file_is_the_only_argument_and_cannot_become_an_option() {
        let path = a_file();
        let command = external_command("  mupdf-gl ", &path).unwrap();
        assert_eq!(command.get_program(), "mupdf-gl", "the setting is trimmed");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args.len(), 1);
        assert!(std::path::Path::new(args[0]).is_absolute(), "an absolute path never starts with '-'");
    }

    #[test]
    fn a_missing_file_or_odd_setting_is_refused() {
        let gone = std::env::temp_dir().join("pdfcraft-viewer-does-not-exist.pdf");
        assert!(external_command("viewer", &gone).unwrap_err().contains("no longer exists"));
        let path = a_file();
        assert!(external_command("view\u{0}er", &path).is_err());
        assert!(external_command(&"x".repeat(MAX_VIEWER_CHARS + 1), &path).is_err());
    }

    #[test]
    fn settings_are_checked_when_restored() {
        assert!(valid_viewer(""));
        assert!(valid_viewer(r"C:\Program Files\MuPDF\mupdf.exe"));
        assert!(!valid_viewer("a\nb"));
        assert!(!valid_viewer(&"x".repeat(MAX_VIEWER_CHARS + 1)));
    }

    #[test]
    fn a_viewer_that_does_not_exist_is_reported_not_a_crash() {
        let path = a_file();
        let err = open_external("pdfcraft-no-such-viewer-program", &path).unwrap_err();
        assert!(err.starts_with("Could not start pdfcraft-no-such-viewer-program"), "{err}");
    }
}
