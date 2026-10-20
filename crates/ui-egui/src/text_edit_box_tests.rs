//! The text box you type in, driven through egui headless: a double-click into text that is too
//! small to read starts the edit with a comfortable zoom (the caret stays on screen), and the box
//! being edited draws a visible card (a surface with padding and an accent border) so it is easy
//! to find. A double-click into already-legible text leaves the view alone.

use egui::{Event, Key, Modifiers, Shape};
use figcraft_doc::NodeId;
use figcraft_engine::Session;
use serde_json::json;

use crate::canvas::{TextOnScreen, edit_card};
use crate::layers_drag_tests::Ui;

fn id(v: &serde_json::Value) -> NodeId {
    NodeId(v["id"].as_u64().unwrap())
}

fn setup(text: &str, font: f64, zoom: f64) -> (Ui, NodeId) {
    let mut s = Session::new();
    let t = id(&s.execute("text.create", &json!({"text": text, "x": 200, "y": 150, "props": {"fontSize": font}})).unwrap());
    s.execute("view.set", &json!({"zoom": zoom, "cx": 300, "cy": 250, "vw": 1440, "vh": 900})).unwrap();
    (Ui::new(s), t)
}

fn double_click(u: &mut Ui, p: egui::Pos2) {
    for _ in 0..2 {
        u.frame(vec![
            Event::PointerMoved(p),
            Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
        ]);
        u.frame(vec![Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
    }
}

/// The editing card, when both its translucent tint and its accent border are drawn for layer
/// `t`. The tint must stay translucent: the canvas (and the edited text) is painted beneath this
/// overlay, so an opaque fill would hide what is being typed.
fn card(out: &egui::FullOutput, u: &Ui, t: NodeId) -> Option<[egui::Pos2; 4]> {
    let doc = u.app.session.doc().unwrap();
    let want = edit_card(&u.app, doc, t)?;
    let border = crate::theme::canvas_accent(&u.ctx);
    let tint = border.gamma_multiply(0.12);
    let (mut fill, mut stroke) = (false, false);
    for s in &out.shapes {
        let egui::epaint::ClippedShape { shape: Shape::Path(p), .. } = s else { continue };
        if !p.closed || p.points.len() != 4 || p.points.iter().zip(&want).any(|(a, b)| a.distance(*b) > 0.01) {
            continue;
        }
        if p.fill == tint && p.fill.a() < 255 {
            fill = true;
        }
        if p.stroke.color == egui::epaint::ColorMode::Solid(border) && (p.stroke.width - 1.5).abs() < 1e-6 {
            stroke = true;
        }
    }
    (fill && stroke).then_some(want)
}

#[test]
fn double_click_into_small_text_starts_the_edit_at_a_comfortable_zoom_with_a_card() {
    // 12pt at 25% is 3px on screen: far too small to read.
    let (mut u, t) = setup("jjjj", 12.0, 0.25);
    let p = u.app.to_screen(205.0, 155.0);
    double_click(&mut u, p);
    let ds = u.app.session.ds().unwrap();
    assert_eq!(ds.text_edit.as_ref().map(|e| e.id), Some(t), "the double-click starts editing");
    let z = ds.view.zoom;
    assert!((z - 2.0).abs() < 1e-9, "12pt zooms in to 24px on screen (200%), got {z}");
    // The caret stays on screen (the exact anchor math is covered by the engine tests); the text
    // is laid out at the new zoom and the card marks the box.
    let doc = u.app.session.doc().unwrap();
    assert!(TextOnScreen::of(doc, t).is_some(), "the text lays out at the comfortable zoom");
    // The card: a padded surface around the box with an accent border, drawn this frame.
    let out = u.frame_output(vec![]);
    let Some(r) = card(&out, &u, t) else {
        panic!("no editing card around the text box");
    };
    let w = r[1].x - r[0].x;
    let h = r[3].y - r[0].y;
    assert!(w > 0.0 && h > 0.0, "the card has size: {r:?}");
}

#[test]
fn double_click_into_legible_text_keeps_the_view_and_still_drawing_the_card() {
    // 24pt at 100% is 24px: already legible, nothing moves.
    let (mut u, t) = setup("Hello", 24.0, 1.0);
    let before = (u.app.session.ds().unwrap().view.zoom, u.app.session.ds().unwrap().view.cx, u.app.session.ds().unwrap().view.cy);
    let p = u.app.to_screen(205.0, 155.0);
    double_click(&mut u, p);
    let ds = u.app.session.ds().unwrap();
    assert_eq!(ds.text_edit.as_ref().map(|e| e.id), Some(t));
    assert_eq!((ds.view.zoom, ds.view.cx, ds.view.cy), before, "the view is untouched");
    let out = u.frame_output(vec![]);
    assert!(card(&out, &u, t).is_some(), "the card draws while editing");
}

#[test]
fn the_editing_card_follows_a_rotated_box() {
    // A 45° box gets a card along its edges (each side the box's side plus 2×8px), not an
    // axis-aligned bounding box that would swallow the space around it.
    let (mut u, t) = setup("Hi", 12.0, 1.0);
    u.app.session.execute("node.set", &json!({"id": t, "rotation": 45})).unwrap();
    let doc = u.app.session.doc().unwrap();
    let n = doc.node(t).unwrap();
    let (w, h) = (n.width as f32, n.height as f32);
    let q = edit_card(&u.app, doc, t).unwrap();
    assert!(((q[1] - q[0]).length() - (w + 16.0)).abs() < 0.1, "the top side is padded: {q:?}");
    assert!(((q[2] - q[1]).length() - (h + 16.0)).abs() < 0.1, "the right side is padded: {q:?}");
    assert!((q[1] - q[0]).y.abs() > 1.0, "the card is turned with the box: {q:?}");
}

#[test]
fn the_text_tool_creates_a_legible_new_box_at_a_low_zoom() {
    // The other way into a text box: click with the Text tool. The box is empty (no glyphs to
    // aim at), so its fallback anchor must still zoom in and draw the card.
    let mut s = Session::new();
    s.execute("view.set", &json!({"zoom": 0.25, "cx": 100, "cy": 80, "vw": 1440, "vh": 900})).unwrap();
    let mut u = Ui::new(s);
    u.app.run("tool.set", json!({"tool": "text"}));
    let p = u.app.to_screen(100.0, 80.0);
    u.frame(vec![
        Event::PointerMoved(p),
        Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
    ]);
    u.frame(vec![Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
    let ds = u.app.session.ds().unwrap();
    let te = ds.text_edit.clone().expect("the Text tool starts editing the new box");
    assert_eq!(ds.doc.node(te.id).unwrap().kind.text().unwrap().characters, "");
    assert!((ds.view.zoom - 2.0).abs() < 1e-9, "a new 12pt box zooms in to 24px (200%), got {}", ds.view.zoom);
    let out = u.frame_output(vec![]);
    assert!(card(&out, &u, te.id).is_some(), "the new box draws its card");
    u.frame(vec![Event::Text("Hi".into())]);
    assert_eq!(u.app.session.ds().unwrap().doc.node(te.id).unwrap().kind.text().unwrap().characters, "Hi");
}

#[test]
fn typing_in_the_comfortable_edit_shows_on_the_canvas() {
    // The full loop: small text, double-click (selects all), type replaces it, the End key then
    // appends, and Escape ends the edit with the card gone.
    let (mut u, t) = setup("jj", 12.0, 0.25);
    let p = u.app.to_screen(203.0, 155.0);
    double_click(&mut u, p);
    u.frame(vec![Event::Text("jj".into())]);
    let doc = u.app.session.doc().unwrap();
    assert_eq!(doc.node(t).unwrap().kind.text().unwrap().characters, "jj", "typing replaces the double-click selection");
    // The laid-out text the canvas will paint includes what was typed.
    assert!(TextOnScreen::of(doc, t).is_some());
    // To the end, then append.
    u.frame(vec![
        Event::Key { key: Key::End, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE },
        Event::Key { key: Key::End, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE },
    ]);
    u.frame(vec![Event::Text("jj".into())]);
    let doc = u.app.session.doc().unwrap();
    assert_eq!(doc.node(t).unwrap().kind.text().unwrap().characters, "jjjj");
    // Escape ends the edit; the card is gone on the next frame.
    u.frame(vec![
        Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE },
        Event::Key { key: Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE },
    ]);
    assert!(u.app.session.ds().unwrap().text_edit.is_none());
    let out = u.frame_output(vec![]);
    assert!(card(&out, &u, t).is_none(), "the card is gone when the edit ends");
}
