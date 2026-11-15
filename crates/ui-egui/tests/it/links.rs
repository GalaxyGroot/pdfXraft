//! Community links: the Discord button is one click away everywhere; Help menu, About dialog and
//! home screen open the ArtCraft and PdfCraft pages.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_engine::links;
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
fn discord_button_in_the_top_bar_opens_discord() {
    let mut h = harness(|_| {});
    h.get_by_label("Discord").click();
    h.run_steps(2);
    assert_eq!(h.state().last_opened_url.as_deref(), Some(links::DISCORD));
    assert_eq!(links::DISCORD, "https://discord.gg/artcraft");
}

#[test]
fn home_screen_links() {
    for (label, url) in [
        ("Join our Discord", links::DISCORD),
        ("PdfCraft web page", "https://getartcraft.com/apps/pdfcraft"),
        ("PdfCraft on GitHub", "https://github.com/storytold/pdfcraft"),
        ("ArtCraft website", "https://getartcraft.com"),
    ] {
        let mut h = harness(|_| {});
        h.get_by_label("Join the ArtCraft community");
        h.get_by_label(label).click();
        h.run_steps(2);
        assert_eq!(h.state().last_opened_url.as_deref(), Some(url), "{label}");
    }
}

#[test]
fn home_community_card_closes_for_this_run() {
    let mut h = harness(|_| {});
    h.get_by_label("Join the ArtCraft community");
    h.get_by_label("Close").click();
    h.run_steps(2);
    assert!(h.state().community_card_closed);
    assert!(h.query_by_label("Join the ArtCraft community").is_none());
    assert!(h.query_by_label("Join our Discord").is_none());
    // Not remembered: a fresh app shows the card again.
    assert!(!PdfCraftApp::new().community_card_closed);
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
    assert!(h.query_all_by_label("ArtCraft").count() >= 1, "the mark (alt text)");
    h.get_by_label("PdfCraft");
    assert!(h.query_by_label_contains("Version ").is_some(), "the version shows");
    h.get_by_label("Credits");
    for gone in ["Join our Discord", "PdfCraft web page", "ArtCraft website", "PdfCraft on GitHub", "Contributors", "Models"] {
        assert!(h.query_by_label(gone).is_none(), "{gone}: no links or tabs in About any more");
    }
}

#[test]
fn help_commands_open_each_link() {
    for l in links::LINKS {
        let mut h = harness(|_| {});
        assert!(h.state_mut().execute(l.command), "{}", l.command);
        assert_eq!(h.state().last_opened_url.as_deref(), Some(l.url));
        assert_eq!(pdfcraft_engine::commands::command(l.command).unwrap().menu, Some("Help"));
    }
}
