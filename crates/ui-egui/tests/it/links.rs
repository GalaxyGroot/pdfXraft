//! The home screen and the About dialog carry no community links.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::{Dialog, PdfCraftApp};

fn harness(setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        setup(&mut app);
        app
    });
    h.run_steps(4);
    h
}

#[test]
fn home_has_no_community_card() {
    let h = harness(|_| {});
    assert!(h.query_by_label("Discord").is_none(), "no Discord button in the title bar");
    for gone in ["Join the ArtCraft community", "Join our Discord", "ArtCraft website"] {
        assert!(h.query_by_label(gone).is_none(), "{gone}");
    }
}

#[test]
fn about_dialog_is_one_simple_page_without_links() {
    let pdf = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >> endobj
trailer << /Root 1 0 R >>
%%EOF";
    // With a document open, so the home screen's own links are not on screen.
    let h = harness(move |app| {
        app.open_bytes("one.pdf", None, pdf.to_vec()).unwrap();
        app.dialog = Some(Dialog::About);
    });
    h.get_by_label("PdfXraft");
    assert!(h.query_by_label_contains("Version ").is_some(), "the version shows");
    assert!(h.query_by_label("Credits").is_none(), "no credits section");
    h.get_by_label_contains("MIT license");
    for gone in ["Join our Discord", "PdfXraft web page", "ArtCraft website", "PdfXraft on GitHub", "Contributors", "Models"] {
        assert!(h.query_by_label(gone).is_none(), "{gone}: no links or tabs in About any more");
    }
}
