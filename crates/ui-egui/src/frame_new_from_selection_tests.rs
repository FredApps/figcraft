//! The Frame tool's panel starts with New from selection (#1683): with layers selected it wraps
//! them in a frame, as Object ▸ Frame selection does, in one undo step, and returns to the Move
//! tool; with nothing selected it does nothing.

use egui::accesskit::Role;
use egui::{Pos2, pos2};
use figcraft_doc::{NodeId, NodeKind};
use figcraft_engine::{Session, ToolId};
use serde_json::json;

use crate::layers_drag_tests::Ui;

/// Two rectangles, both selected when `select`, with the Frame tool active.
fn scene(select: bool) -> (Ui, [NodeId; 2]) {
    let mut s = Session::new();
    let mut rect =
        |x: i32| NodeId(s.execute("shape.rectangle", &json!({"x": x, "y": 0, "width": 50, "height": 50})).unwrap()["id"].as_u64().unwrap());
    let ids = [rect(0), rect(80)];
    let selected: &[NodeId] = if select { &ids } else { &[] };
    s.execute("select.set", &json!({"ids": selected})).unwrap();
    s.execute("tool.set", &json!({"tool": "frame"})).unwrap();
    (Ui::new(s), ids)
}

/// The middle of the button named `label`, found in the AccessKit tree.
fn named_button(u: &mut Ui, label: &str) -> Pos2 {
    u.ctx.enable_accesskit();
    u.frame(vec![]);
    let out = u.frame_output(vec![]);
    let nodes = out.platform_output.accesskit_update.expect("an AccessKit update").nodes;
    let n = nodes.iter().map(|(_, n)| n).find(|n| n.role() == Role::Button && n.label() == Some(label)).expect(label);
    let b = n.bounds().expect("bounds");
    pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)
}

fn click(u: &mut Ui, p: Pos2) {
    u.button(p, true);
    u.button(p, false);
    u.frame(vec![]);
}

#[test]
fn new_from_selection_frames_the_selected_layers_and_returns_to_move() {
    let (mut u, [a, b]) = scene(true);
    let history = u.app.session.undo_depth().0;
    let at = named_button(&mut u, "New from selection");
    click(&mut u, at);
    let doc = u.app.session.doc().unwrap();
    let parent = doc.node(a).unwrap().parent.expect("a parent");
    assert_eq!(doc.node(b).unwrap().parent, Some(parent), "both layers in the same frame");
    assert!(matches!(doc.node(parent).unwrap().kind, NodeKind::Frame), "the parent is a new frame");
    assert_eq!(u.app.session.undo_depth().0, history + 1, "one undo step");
    assert_eq!(u.app.session.tool.tool, ToolId::Move);
}

#[test]
fn new_from_selection_does_nothing_without_a_selection() {
    let (mut u, [a, _]) = scene(false);
    let page = u.app.session.doc().unwrap().node(a).unwrap().parent;
    let history = u.app.session.undo_depth().0;
    let at = named_button(&mut u, "New from selection");
    click(&mut u, at);
    assert_eq!(u.app.session.doc().unwrap().node(a).unwrap().parent, page, "nothing was framed");
    assert_eq!(u.app.session.undo_depth().0, history);
    assert_eq!(u.app.session.tool.tool, ToolId::Frame, "the Frame tool stays");
}
