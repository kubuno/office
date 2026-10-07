//! The window's handlers for the Propriétés panel (`pages/format_panel`): what the panel shows of the
//! selection, and the edits it raises turned into the editor's commands.

use kubuno_office_diagrams_core::editor::{Align, Editor, Order};
use kubuno_office_diagrams_core::model::Obj;
use serde_json::Value;

use super::DiagramWindow;
use crate::pages::format_panel::{ConnFormat, FormatCommand, FormatEditEventArgs, FormatState, ShapeFormat};

/// What the panel shows of `ed`'s selection: the shape of a selection of exactly one (`selectedShape`),
/// else the connector of a selection of exactly one (`selectedConn`), else the hint.
pub(crate) fn format_state(ed: &Editor) -> FormatState {
    if let Some(s) = ed.selected_shape() {
        return FormatState::Shape(Box::new(ShapeFormat {
            style: s.style(),
            label_style: s.label_style(),
            label: s.label().to_string(),
            x: s.x(),
            y: s.y(),
            w: s.w(),
            h: s.h(),
            rotation: s.rotation(),
            selected: ed.selected().len(),
            has_group: ed.selection_has_group(),
        }));
    }
    if let Some(c) = ed.selected_conn() {
        return FormatState::Connector(Box::new(ConnFormat { style: c.style(), label: c.label().to_string() }));
    }
    FormatState::None
}

/// `{ key: value }`.
fn patch(key: &str, value: Value) -> Obj {
    let mut o = Obj::new();
    o.insert(key.to_string(), value);
    o
}

/// Runs `cmd` on `ed`, whose selected shape is `sid` and selected connector `cid`. « Modifier le style… » is
/// not an edit: the window opens its dialog.
pub(crate) fn apply(ed: &mut Editor, cmd: FormatCommand, sid: Option<&str>, cid: Option<&str>) {
    match cmd {
        FormatCommand::ShapeStyle(k, v) => {
            if let Some(sid) = sid {
                ed.update_shape_style(sid, &patch(&k, v));
            }
        }
        FormatCommand::LabelStyle(k, v) => {
            if let Some(sid) = sid {
                ed.update_shape_label_style(sid, &patch(&k, v));
            }
        }
        FormatCommand::Label(text) => {
            if let Some(sid) = sid {
                ed.set_shape_label(sid, &text);
            }
        }
        FormatCommand::Geometry(k, v) => {
            if let Some(sid) = sid {
                ed.set_shape_geometry(sid, &k, v);
            }
        }
        FormatCommand::Rotation(v) => {
            if let Some(sid) = sid {
                ed.set_shape_rotation(sid, v);
            }
        }
        FormatCommand::Flip(horizontal) => ed.flip(horizontal),
        FormatCommand::Order(mode) => {
            let order = match mode.as_str() {
                "front" => Order::Front,
                "forward" => Order::Forward,
                "backward" => Order::Backward,
                "back" => Order::Back,
                _ => return,
            };
            ed.reorder(order);
        }
        FormatCommand::Align(axis) => {
            let axis = match axis.as_str() {
                "left" => Align::Left,
                "center" => Align::Center,
                "right" => Align::Right,
                "top" => Align::Top,
                "middle" => Align::Middle,
                "bottom" => Align::Bottom,
                _ => return,
            };
            ed.align(axis);
        }
        FormatCommand::Group => ed.group(),
        FormatCommand::Ungroup => ed.ungroup(),
        FormatCommand::Preset(fill, stroke) => {
            let mut p = patch("fillColor", Value::String(fill));
            p.insert("strokeColor".into(), Value::String(stroke));
            ed.apply_style_to_selection(&p);
        }
        FormatCommand::ConnStyle(k, v) => {
            if let Some(cid) = cid {
                ed.update_conn_style(cid, &patch(&k, v));
            }
        }
        FormatCommand::ConnLabel(text) => {
            if let Some(cid) = cid {
                ed.set_conn_label(cid, &text);
            }
        }
        FormatCommand::Delete => ed.delete_selected(),
        FormatCommand::EditStyle => {}
    }
}

impl DiagramWindow {
    /// Called after every state change of the canvas: the panel shows the selection.
    pub(super) fn refresh_format(&mut self) {
        let Some(state) = self.canvas.with(|c| format_state(c.editor())) else { return };
        self.format_panel.with(|f| f.set_state(state));
    }

    /// An edit of the panel: the editor's command, or « Modifier le style… »'s dialog.
    pub(super) fn format_panel_format_edited(&mut self, e: &FormatEditEventArgs) {
        let Some(cmd) = FormatCommand::of(e) else { return };
        if cmd == FormatCommand::EditStyle {
            self.edit_style();
            return;
        }
        self.canvas.with(|c| {
            c.edit(|ed| {
                let sid = ed.selected_shape().map(|s| s.id().to_string());
                let cid = ed.selected_conn().map(|c| c.id().to_string());
                apply(ed, cmd, sid.as_deref(), cid.as_deref());
            })
        });
    }

    /// « Modifier le style… »: the selected shape's raw style, as JSON (`JSON.stringify(style, null, 2)`).
    fn edit_style(&mut self) {
        let Some(Some((sid, text))) = self.canvas.with(|c| {
            c.editor().selected_shape().map(|s| (s.id().to_string(), serde_json::to_string_pretty(&Value::Object(s.style_obj())).unwrap_or_else(|_| "{}".into())))
        }) else {
            return;
        };
        if let Some(style) = crate::views::style_dialog::StyleDialog::ask(self, &text) {
            self.canvas.with(|c| c.edit(|ed| ed.set_shape_style_obj(&sid, style)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubuno_office_diagrams_core::file::Page;
    use kubuno_office_diagrams_core::model::{Connector, PageData, Shape};
    use serde_json::json;

    fn editor() -> Editor {
        let a = Shape::new("a", "rect", 10.0, 20.0, 120.0, 60.0, "A", Obj::new(), 1.0, "default");
        let b = Shape::new("b", "rect", 200.0, 20.0, 120.0, 60.0, "B", Obj::new(), 2.0, "default");
        let c = Connector::between("c", "a", "b", "default");
        let data = PageData { shapes: vec![a, b], connectors: vec![c], layers: None, rest: Obj::new() };
        Editor::new(vec![Page::new("p1", "Page 1", data)], 1)
    }

    #[test]
    fn the_state_follows_the_web_s_selected_shape_and_conn() {
        let mut ed = editor();
        assert_eq!(format_state(&ed), FormatState::None);
        ed.set_selection(vec!["a".into()], vec![]);
        let FormatState::Shape(s) = format_state(&ed) else { panic!("a shape") };
        assert_eq!((s.x, s.label.as_str(), s.selected), (10.0, "A", 1));
        ed.set_selection(vec!["a".into(), "b".into()], vec![]);
        assert_eq!(format_state(&ed), FormatState::None, "two shapes: no selectedShape");
        ed.set_selection(vec![], vec!["c".into()]);
        assert!(matches!(format_state(&ed), FormatState::Connector(_)));
    }

    #[test]
    fn the_panel_s_edits_change_the_selection() {
        let mut ed = editor();
        ed.set_selection(vec!["a".into()], vec![]);
        apply(&mut ed, FormatCommand::ShapeStyle("strokeWidth".into(), json!(3)), Some("a"), None);
        apply(&mut ed, FormatCommand::LabelStyle("bold".into(), json!(true)), Some("a"), None);
        apply(&mut ed, FormatCommand::Geometry("w".into(), 150.7), Some("a"), None);
        apply(&mut ed, FormatCommand::Rotation(-90.0), Some("a"), None);
        apply(&mut ed, FormatCommand::Label("Début".into()), Some("a"), None);
        let s = ed.data().shape("a").cloned().expect("a");
        assert_eq!(s.style().stroke_width, 3.0);
        assert!(s.label_style().bold);
        assert_eq!(s.w(), 150.0, "parseInt");
        assert_eq!(s.rotation(), 270.0, "((v % 360) + 360) % 360");
        assert_eq!(s.label(), "Début");
        apply(&mut ed, FormatCommand::Preset("none".into(), "#000000".into()), Some("a"), None);
        let st = ed.data().shape("a").map(|s| s.style()).expect("a");
        assert_eq!((st.fill_color.as_str(), st.stroke_color.as_str()), ("none", "#000000"));
        ed.set_selection(vec![], vec!["c".into()]);
        apply(&mut ed, FormatCommand::ConnStyle("routing".into(), json!("curved")), None, Some("c"));
        apply(&mut ed, FormatCommand::ConnLabel("oui".into()), None, Some("c"));
        let c = ed.data().connector("c").cloned().expect("c");
        assert_eq!(c.style().routing.as_str(), "curved");
        assert_eq!(c.label(), "oui");
        apply(&mut ed, FormatCommand::Delete, None, Some("c"));
        assert!(ed.data().connector("c").is_none());
    }
}
