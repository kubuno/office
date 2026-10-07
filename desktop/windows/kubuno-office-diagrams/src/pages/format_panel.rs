//! Code-behind of the user control `FormatPanel` (`format_panel.kbcontrol`): the Propriétés panel
//! (`DiagramEditorPage.tsx:3317-3546`), and the two small custom-drawn controls it is made of: the style
//! presets grid ([`StylePresets`]) and the rows of square icon buttons ([`SegmentBar`]: bold / italic, the
//! text alignments, flip, order, align).
//!
//! The window hands the panel the selection ([`FormatPanel::set_state`]): one shape (its tabs Style, Texte
//! and Disposition), one connector, or nothing (the hint — a multi-selection shows the hint too, as on the
//! web, whose `selectedShape` is the shape of a selection of exactly one). What the user changes leaves as
//! ONE event, `FormatEdited` ([`FormatEditEventArgs`]: what, key, JSON value), which the window turns into
//! the editor's command ([`FormatCommand::of`]). A value written by the window (`ChangeSource::Binding`)
//! never comes back as an edit.

use kubuno_desktop::views::events::{ChangeSource, ItemEventArgs};
use kubuno_desktop::views::prelude::*;
use kubuno_office_diagrams_core::model::{js_number, ConnStyle, LabelStyle, ShapeStyle};
use serde_json::Value;

use kubuno_desktop::{Row, Rows, Value as BValue};

use crate::Resources;

pub use parts::{PanelTabs, SegmentBar, StylePresets};

/// The web's style presets (`STYLE_PRESETS`, `:146-150`): `[fillColor, strokeColor]`.
pub const STYLE_PRESETS: [(&str, &str); 12] = [
    ("#dae8fc", "#6c8ebf"),
    ("#d5e8d4", "#82b366"),
    ("#ffe6cc", "#d79b00"),
    ("#fff2cc", "#d6b656"),
    ("#f8cecc", "#b85450"),
    ("#e1d5e7", "#9673a6"),
    ("#f5f5f5", "#666666"),
    ("#ffffff", "#000000"),
    ("none", "#000000"),
    ("#1a73e8", "#1557b0"),
    ("#0b5394", "#073763"),
    ("#202124", "#000000"),
];

/// The single selected shape, as the panel shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeFormat {
    pub style: ShapeStyle,
    pub label_style: LabelStyle,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rotation: f64,
    /// How many shapes are selected (the align block and « Grouper » need two, `selectedIds.size >= 2`).
    pub selected: usize,
    /// `selectionHasGroup` (« Dégrouper »).
    pub has_group: bool,
}

/// The single selected connector.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnFormat {
    pub style: ConnStyle,
    pub label: String,
}

/// What the panel shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum FormatState {
    /// Nothing (or several things) selected: the hint.
    #[default]
    None,
    Shape(Box<ShapeFormat>),
    Connector(Box<ConnFormat>),
}

/// An edit of the panel: `what` names the target, `key` the property, `value` is JSON.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct FormatEditEventArgs {
    /// `style`, `label_style`, `label`, `geom`, `rotation`, `flip`, `order`, `align`, `group`, `ungroup`,
    /// `preset`, `conn_style`, `conn_label`, `delete`, `edit_style`.
    pub what: String,
    pub key: String,
    pub value: String,
}

impl FormatEditEventArgs {
    pub fn new(what: &str, key: &str, value: Value) -> Self {
        Self { what: what.into(), key: key.into(), value: value.to_string() }
    }
}

/// The editor command an edit stands for (pure: unit-tested).
#[derive(Debug, Clone, PartialEq)]
pub enum FormatCommand {
    /// `updateShapeStyle(sid, { key: value })`.
    ShapeStyle(String, Value),
    /// `updateShapeLabelStyle(sid, { key: value })`.
    LabelStyle(String, Value),
    Label(String),
    /// X, Y, W or H.
    Geometry(String, f64),
    Rotation(f64),
    /// `flipSelection('h')` (true) or `'v'`.
    Flip(bool),
    /// `front`, `forward`, `backward`, `back`.
    Order(String),
    /// `left`, `center`, `right`, `top`, `middle`, `bottom`.
    Align(String),
    Group,
    Ungroup,
    /// A style preset: `applyStyleToSelection({ fillColor, strokeColor })`.
    Preset(String, String),
    /// `updateConnStyle(cid, { key: value })`.
    ConnStyle(String, Value),
    ConnLabel(String),
    Delete,
    /// « Modifier le style… »: the raw style JSON dialog.
    EditStyle,
}

impl FormatCommand {
    /// The command of an edit, `None` for an edit that is not understood.
    pub fn of(e: &FormatEditEventArgs) -> Option<FormatCommand> {
        let v: Value = serde_json::from_str(&e.value).unwrap_or(Value::Null);
        Some(match e.what.as_str() {
            "style" => FormatCommand::ShapeStyle(e.key.clone(), v),
            "label_style" => FormatCommand::LabelStyle(e.key.clone(), v),
            "label" => FormatCommand::Label(v.as_str()?.to_string()),
            "geom" => FormatCommand::Geometry(e.key.clone(), v.as_f64()?),
            "rotation" => FormatCommand::Rotation(v.as_f64()?),
            "flip" => FormatCommand::Flip(e.key == "h"),
            "order" => FormatCommand::Order(e.key.clone()),
            "align" => FormatCommand::Align(e.key.clone()),
            "group" => FormatCommand::Group,
            "ungroup" => FormatCommand::Ungroup,
            "preset" => {
                let (fill, stroke) = STYLE_PRESETS.get(e.key.parse::<usize>().ok()?)?;
                FormatCommand::Preset(fill.to_string(), stroke.to_string())
            }
            "conn_style" => FormatCommand::ConnStyle(e.key.clone(), v),
            "conn_label" => FormatCommand::ConnLabel(v.as_str()?.to_string()),
            "delete" => FormatCommand::Delete,
            "edit_style" => FormatCommand::EditStyle,
            _ => return None,
        })
    }
}

/// A number from a field, as the web's `parseFloat` of the field's text writes it (`2.3`, not
/// `2.299999952316284`: the field holds an `f32`).
pub fn field_number(v: f32) -> Value {
    let f: f64 = format!("{v}").parse().unwrap_or(v as f64);
    js_number(f)
}

/// The rows of a list of options: `(value, label)`.
fn options(items: &[(&str, &str)]) -> Rows {
    Rows::from(items.iter().map(|(v, l)| Row::new().with("Value", BValue::Str((*v).into())).with("Label", BValue::Str((*l).into()))).collect::<Vec<_>>())
}

/// A number as a template literal writes it (`12`, `1.5`).
fn num_text(v: f64) -> String {
    js_number(v).to_string()
}

/// `#rrggbb` for a colour swatch: a `none` (or an empty) colour shows `fallback` (`#ffffff` for a fill,
/// `#000000` for a stroke), as the web's `<input type="color">` value does.
fn swatch(color: &str, fallback: &str) -> String {
    if color == "none" || color.is_empty() {
        fallback.to_string()
    } else {
        color.to_string()
    }
}

/// The Propriétés panel (see the module doc).
#[derive(UserControl)]
#[user_control(view = "format_panel.kbcontrol")]
#[category("Diagrams")]
#[default_event("FormatEdited")]
pub struct FormatPanel {
    base: UserControlCore,
    /// Occurs when the user changes a property of the selection.
    #[event]
    #[category("Action")]
    pub format_edited: Event<FormatEditEventArgs>,
    // ── What the view binds ──
    #[property(bindable)]
    #[browsable(false)]
    pub show_hint: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub show_shape: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub show_conn: bool,
    /// The shape's tab (`formatTab`): 0 Style, 1 Texte, 2 Disposition; kept from one selection to the next.
    #[property(bindable)]
    #[browsable(false)]
    pub tab: f32,
    #[property(bindable)]
    #[browsable(false)]
    pub tab_labels: String,
    #[property(bindable)]
    #[browsable(false)]
    pub show_style: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub show_text: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub show_arrange: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub fill_color: String,
    #[property(bindable)]
    #[browsable(false)]
    pub fill_text: String,
    #[property(bindable)]
    #[browsable(false)]
    pub stroke_color: String,
    #[property(bindable)]
    #[browsable(false)]
    pub stroke_width: f32,
    #[property(bindable)]
    #[browsable(false)]
    pub stroke_style: String,
    #[property(bindable)]
    #[browsable(false)]
    pub rounded: f32,
    #[property(bindable)]
    #[browsable(false)]
    pub rounded_text: String,
    #[property(bindable)]
    #[browsable(false)]
    pub shadow: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub opacity: f32,
    #[property(bindable)]
    #[browsable(false)]
    pub opacity_text: String,
    #[property(bindable)]
    #[browsable(false)]
    pub label_text: String,
    #[property(bindable)]
    #[browsable(false)]
    pub font_family: String,
    #[property(bindable)]
    #[browsable(false)]
    pub font_size: f32,
    #[property(bindable)]
    #[browsable(false)]
    pub text_color: String,
    /// The on buttons of « B » / « I »: `bold`, `italic`, comma-separated.
    #[property(bindable)]
    #[browsable(false)]
    pub emphasis: String,
    #[property(bindable)]
    #[browsable(false)]
    pub emphasis_tips: String,
    #[property(bindable)]
    #[browsable(false)]
    pub align: String,
    #[property(bindable)]
    #[browsable(false)]
    pub valign: String,
    #[property(bindable)]
    #[browsable(false)]
    pub pos_x: f32,
    #[property(bindable)]
    #[browsable(false)]
    pub pos_y: f32,
    #[property(bindable)]
    #[browsable(false)]
    pub pos_w: f32,
    #[property(bindable)]
    #[browsable(false)]
    pub pos_h: f32,
    #[property(bindable)]
    #[browsable(false)]
    pub rotation: f32,
    #[property(bindable)]
    #[browsable(false)]
    pub multi: bool,
    #[property(bindable)]
    #[browsable(false)]
    pub can_ungroup: bool,
    /// The tooltips of the order buttons (`title` on the first and the last).
    #[property(bindable)]
    #[browsable(false)]
    pub order_tips: String,
    #[property(bindable)]
    #[browsable(false)]
    pub conn_color: String,
    #[property(bindable)]
    #[browsable(false)]
    pub conn_width: f32,
    #[property(bindable)]
    #[browsable(false)]
    pub conn_stroke_style: String,
    #[property(bindable)]
    #[browsable(false)]
    pub conn_routing: String,
    #[property(bindable)]
    #[browsable(false)]
    pub conn_arrow_end: String,
    #[property(bindable)]
    #[browsable(false)]
    pub conn_label: String,
    /// The options of the line style, routing and arrow lists (`Label`, `Value`).
    #[property(bindable)]
    #[browsable(false)]
    pub line_styles: Rows,
    #[property(bindable)]
    #[browsable(false)]
    pub routings: Rows,
    #[property(bindable)]
    #[browsable(false)]
    pub arrows: Rows,
    state: FormatState,
}

impl Default for FormatPanel {
    fn default() -> Self {
        Self {
            base: UserControlCore::default(),
            format_edited: Event::default(),
            show_hint: true,
            show_shape: false,
            show_conn: false,
            tab: 0.0,
            tab_labels: [Resources::fmt_tab_style(), Resources::fmt_tab_text(), Resources::fmt_tab_arrange()].join("|"),
            show_style: false,
            show_text: false,
            show_arrange: false,
            fill_color: "#dae8fc".into(),
            fill_text: "#dae8fc".into(),
            stroke_color: "#6c8ebf".into(),
            stroke_width: 1.5,
            stroke_style: "solid".into(),
            rounded: 0.0,
            rounded_text: Resources::fmt_rounded().replace("{value}", "0"),
            shadow: false,
            opacity: 100.0,
            opacity_text: Resources::fmt_opacity().replace("{value}", "100"),
            label_text: String::new(),
            font_family: "Inter".into(),
            font_size: 12.0,
            text_color: "#000000".into(),
            emphasis: String::new(),
            emphasis_tips: [Resources::fmt_bold(), Resources::fmt_italic()].join("|"),
            align: "center".into(),
            valign: "middle".into(),
            pos_x: 0.0,
            pos_y: 0.0,
            pos_w: 0.0,
            pos_h: 0.0,
            rotation: 0.0,
            multi: false,
            can_ungroup: false,
            order_tips: [Resources::fmt_to_front(), Resources::fmt_forward(), Resources::fmt_backward(), Resources::fmt_to_back()].join("|"),
            conn_color: "#6c8ebf".into(),
            conn_width: 1.5,
            conn_stroke_style: "solid".into(),
            conn_routing: "straight".into(),
            conn_arrow_end: "block".into(),
            conn_label: String::new(),
            line_styles: options(&[("solid", Resources::fmt_line_solid()), ("dashed", Resources::fmt_line_dashed()), ("dotted", Resources::fmt_line_dotted())]),
            routings: options(&[("straight", Resources::fmt_route_straight()), ("orthogonal", Resources::fmt_route_orthogonal()), ("curved", Resources::fmt_route_curved())]),
            arrows: options(&[("none", Resources::fmt_arrow_none()), ("block", Resources::fmt_arrow_block()), ("classic", Resources::fmt_arrow_classic()), ("open", Resources::fmt_arrow_open())]),
            state: FormatState::None,
        }
    }
}

impl FormatPanel {
    /// Shows `state` (called by the window after every change of the canvas).
    pub fn set_state(&mut self, state: FormatState) {
        if self.state != state {
            self.state = state;
            self.apply_state();
        }
    }

    pub fn state(&self) -> &FormatState {
        &self.state
    }

    /// The bound values of the current state.
    fn apply_state(&mut self) {
        self.show_hint = matches!(self.state, FormatState::None);
        self.show_shape = matches!(self.state, FormatState::Shape(_));
        self.show_conn = matches!(self.state, FormatState::Connector(_));
        self.show_tabs();
        match self.state.clone() {
            FormatState::Shape(s) => {
                let st = &s.style;
                self.fill_color = swatch(&st.fill_color, "#ffffff");
                self.fill_text = st.fill_color.clone();
                self.stroke_color = swatch(&st.stroke_color, "#000000");
                self.stroke_width = st.stroke_width as f32;
                self.stroke_style = st.stroke_style.clone();
                self.rounded = st.rounded as f32;
                self.rounded_text = Resources::fmt_rounded().replace("{value}", &num_text(st.rounded));
                self.shadow = st.shadow;
                self.opacity = st.opacity as f32;
                self.opacity_text = Resources::fmt_opacity().replace("{value}", &num_text(st.opacity));
                let ls = &s.label_style;
                self.label_text = s.label.clone();
                self.font_family = ls.font_family.clone();
                self.font_size = ls.font_size as f32;
                self.text_color = swatch(&ls.color, "#000000");
                self.emphasis = [(ls.bold, "bold"), (ls.italic, "italic")].iter().filter(|(on, _)| *on).map(|(_, v)| *v).collect::<Vec<_>>().join(",");
                self.align = ls.align.clone();
                self.valign = ls.vertical_align.clone();
                // `Math.round(selectedShape[k])`.
                self.pos_x = s.x.round() as f32;
                self.pos_y = s.y.round() as f32;
                self.pos_w = s.w.round() as f32;
                self.pos_h = s.h.round() as f32;
                self.rotation = s.rotation.round() as f32;
                self.multi = s.selected >= 2;
                self.can_ungroup = s.has_group;
            }
            FormatState::Connector(c) => {
                self.conn_color = swatch(&c.style.stroke_color, "#000000");
                self.conn_width = c.style.stroke_width as f32;
                self.conn_stroke_style = c.style.stroke_style.clone();
                self.conn_routing = c.style.routing.as_str().to_string();
                self.conn_arrow_end = c.style.arrow_end.clone();
                self.conn_label = c.label.clone();
            }
            FormatState::None => {}
        }
    }

    /// The shape's tab shown.
    fn show_tabs(&mut self) {
        let tab = self.tab as usize;
        self.show_style = self.show_shape && tab == 0;
        self.show_text = self.show_shape && tab == 1;
        self.show_arrange = self.show_shape && tab == 2;
    }

    fn emit(&mut self, what: &str, key: &str, value: Value) {
        self.raise_format_edited(FormatEditEventArgs::new(what, key, value));
    }

    fn by_user(source: ChangeSource) -> bool {
        source == ChangeSource::User
    }

    /// A text edit by the user, `None` otherwise (or when empty and `allow_empty` is false).
    fn user_text(e: &TextChangedEventArgs, allow_empty: bool) -> Option<Value> {
        (Self::by_user(e.source) && (allow_empty || !e.new.is_empty())).then(|| Value::String(e.new.clone()))
    }
}

#[kubuno_desktop::views::event_handlers]
impl FormatPanel {
    fn tab_selected(&mut self, e: &ItemEventArgs) {
        self.tab = e.index.min(2) as f32;
        self.show_tabs();
    }

    // ── Style ──
    fn preset_clicked(&mut self, e: &ItemEventArgs) {
        self.emit("preset", &e.index.to_string(), Value::Null);
    }

    fn fill_color_changed(&mut self, e: &TextChangedEventArgs) {
        if let Some(v) = Self::user_text(e, false) {
            self.emit("style", "fillColor", v);
        }
    }

    fn fill_text_changed(&mut self, e: &TextChangedEventArgs) {
        if let Some(v) = Self::user_text(e, true) {
            self.emit("style", "fillColor", v);
        }
    }

    fn no_fill_click(&mut self) {
        self.emit("style", "fillColor", Value::String("none".into()));
    }

    fn stroke_color_changed(&mut self, e: &TextChangedEventArgs) {
        if let Some(v) = Self::user_text(e, false) {
            self.emit("style", "strokeColor", v);
        }
    }

    fn stroke_width_changed(&mut self, e: &NumericValueChangedEventArgs) {
        if Self::by_user(e.source) {
            self.emit("style", "strokeWidth", field_number(e.new));
        }
    }

    fn stroke_style_changed(&mut self, e: &TextChangedEventArgs) {
        if let Some(v) = Self::user_text(e, false) {
            self.emit("style", "strokeStyle", v);
        }
    }

    fn rounded_changed(&mut self, e: &NumericValueChangedEventArgs) {
        if Self::by_user(e.source) {
            self.emit("style", "rounded", js_number(e.new.round() as f64));
        }
    }

    fn shadow_changed(&mut self, e: &CheckedChangedEventArgs) {
        if Self::by_user(e.source) {
            self.emit("style", "shadow", Value::Bool(e.new));
        }
    }

    fn opacity_changed(&mut self, e: &NumericValueChangedEventArgs) {
        if Self::by_user(e.source) {
            self.emit("style", "opacity", js_number(e.new.round() as f64));
        }
    }

    fn edit_style_click(&mut self) {
        self.emit("edit_style", "", Value::Null);
    }

    // ── Texte ──
    fn label_changed(&mut self, e: &TextChangedEventArgs) {
        if let Some(v) = Self::user_text(e, true) {
            self.emit("label", "", v);
        }
    }

    fn font_family_changed(&mut self, e: &TextChangedEventArgs) {
        if let Some(v) = Self::user_text(e, false) {
            self.emit("label_style", "fontFamily", v);
        }
    }

    fn font_size_changed(&mut self, e: &NumericValueChangedEventArgs) {
        if Self::by_user(e.source) {
            // `parseInt(v, 10) || 12`.
            let n = e.new.trunc() as f64;
            self.emit("label_style", "fontSize", js_number(if n == 0.0 { 12.0 } else { n }));
        }
    }

    fn text_color_changed(&mut self, e: &TextChangedEventArgs) {
        if let Some(v) = Self::user_text(e, false) {
            self.emit("label_style", "color", v);
        }
    }

    /// « B » / « I »: `setLS({ bold: !ls.bold })`.
    fn emphasis_clicked(&mut self, e: &TextChangedEventArgs) {
        let on = self.emphasis.split(',').any(|v| v == e.new);
        if matches!(e.new.as_str(), "bold" | "italic") {
            self.emit("label_style", &e.new, Value::Bool(!on));
        }
    }

    fn align_clicked(&mut self, e: &TextChangedEventArgs) {
        self.emit("label_style", "align", Value::String(e.new.clone()));
    }

    fn valign_clicked(&mut self, e: &TextChangedEventArgs) {
        self.emit("label_style", "verticalAlign", Value::String(e.new.clone()));
    }

    // ── Disposition ──
    fn pos_x_changed(&mut self, e: &NumericValueChangedEventArgs) {
        if Self::by_user(e.source) {
            self.emit("geom", "x", js_number(e.new.trunc() as f64));
        }
    }

    fn pos_y_changed(&mut self, e: &NumericValueChangedEventArgs) {
        if Self::by_user(e.source) {
            self.emit("geom", "y", js_number(e.new.trunc() as f64));
        }
    }

    fn pos_w_changed(&mut self, e: &NumericValueChangedEventArgs) {
        if Self::by_user(e.source) {
            self.emit("geom", "w", js_number(e.new.trunc() as f64));
        }
    }

    fn pos_h_changed(&mut self, e: &NumericValueChangedEventArgs) {
        if Self::by_user(e.source) {
            self.emit("geom", "h", js_number(e.new.trunc() as f64));
        }
    }

    fn rotation_changed(&mut self, e: &NumericValueChangedEventArgs) {
        if Self::by_user(e.source) {
            self.emit("rotation", "", js_number(e.new.trunc() as f64));
        }
    }

    fn flip_clicked(&mut self, e: &TextChangedEventArgs) {
        self.emit("flip", &e.new, Value::Null);
    }

    fn order_clicked(&mut self, e: &TextChangedEventArgs) {
        self.emit("order", &e.new, Value::Null);
    }

    fn arrange_align_clicked(&mut self, e: &TextChangedEventArgs) {
        self.emit("align", &e.new, Value::Null);
    }

    fn group_click(&mut self) {
        self.emit("group", "", Value::Null);
    }

    fn ungroup_click(&mut self) {
        self.emit("ungroup", "", Value::Null);
    }

    fn delete_click(&mut self) {
        self.emit("delete", "", Value::Null);
    }

    // ── Connecteur ──
    fn conn_color_changed(&mut self, e: &TextChangedEventArgs) {
        if let Some(v) = Self::user_text(e, false) {
            self.emit("conn_style", "strokeColor", v);
        }
    }

    fn conn_width_changed(&mut self, e: &NumericValueChangedEventArgs) {
        if Self::by_user(e.source) {
            self.emit("conn_style", "strokeWidth", field_number(e.new));
        }
    }

    fn conn_stroke_style_changed(&mut self, e: &TextChangedEventArgs) {
        if let Some(v) = Self::user_text(e, false) {
            self.emit("conn_style", "strokeStyle", v);
        }
    }

    fn conn_routing_changed(&mut self, e: &TextChangedEventArgs) {
        if let Some(v) = Self::user_text(e, false) {
            self.emit("conn_style", "routing", v);
        }
    }

    fn conn_arrow_changed(&mut self, e: &TextChangedEventArgs) {
        if let Some(v) = Self::user_text(e, false) {
            self.emit("conn_style", "arrowEnd", v);
        }
    }

    fn conn_label_changed(&mut self, e: &TextChangedEventArgs) {
        if let Some(v) = Self::user_text(e, true) {
            self.emit("conn_label", "", v);
        }
    }
}

/// The custom-drawn parts of the panel.
mod parts {
    use kubuno_desktop::prelude::*;
    use kubuno_desktop::ui::graphics::Color;
    use kubuno_desktop::ui::{Canvas, Rect, Size};
    use kubuno_desktop::views::component::{Control, ControlCore, EventCx, PaintEventCx};
    use kubuno_desktop::views::events::{ChangeSource, EmptyEventArgs, Event, ItemEventArgs};
    use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

    use super::STYLE_PRESETS;
    use crate::Resources;

    fn hex(s: &str) -> Option<D2D1_COLOR_F> {
        Color::from_hex(s).map(|c| c.to_d2d())
    }

    fn with_alpha(c: D2D1_COLOR_F, a: f32) -> D2D1_COLOR_F {
        D2D1_COLOR_F { a: c.a * a, ..c }
    }

    fn offset(r: &Rect, b: &Rect) -> Rect {
        Rect::new(b.left + r.left, b.top + r.top, b.left + r.right, b.top + r.bottom)
    }

    /// `grid-cols-6 gap-1.5` of `h-6` swatches.
    const PRESET_COLS: usize = 6;
    const PRESET_GAP: f32 = 6.0;
    const PRESET_H: f32 = 24.0;

    /// The cells of the presets grid in a `w`-wide box at the origin.
    pub(super) fn preset_cells(w: f32) -> Vec<Rect> {
        let cw = ((w - PRESET_GAP * (PRESET_COLS - 1) as f32) / PRESET_COLS as f32).max(1.0);
        (0..STYLE_PRESETS.len())
            .map(|i| {
                let (col, row) = ((i % PRESET_COLS) as f32, (i / PRESET_COLS) as f32);
                let (x, y) = (col * (cw + PRESET_GAP), row * (PRESET_H + PRESET_GAP));
                Rect::new(x, y, x + cw, y + PRESET_H)
            })
            .collect()
    }

    /// The style presets (`STYLE_PRESETS`): a swatch per preset, its fill inside its stroke; « none » is
    /// white with a red diagonal. A click raises `PresetClicked` with the preset's index.
    #[derive(kubuno_desktop::views::component::Component, Default)]
    #[kubuno(extends = Control, overrides(Control))]
    #[category("Diagrams")]
    #[toolbox(icon = "palette")]
    #[default_event("PresetClicked")]
    pub struct StylePresets {
        base: ControlCore,
        /// Occurs when a preset is clicked (`e.index`).
        #[event]
        #[category("Action")]
        pub preset_clicked: Event<ItemEventArgs>,
        hot: Option<usize>,
        pressed: Option<usize>,
    }

    impl StylePresets {
        fn at(&self, x: f32, y: f32) -> Option<usize> {
            preset_cells(self.size().width).iter().position(|r| r.contains(x, y))
        }
    }

    impl Control for StylePresets {
        fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
            Size { width: proposed.width.max(120.0), height: PRESET_H * 2.0 + PRESET_GAP }
        }

        fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
            let b = e.clip_rectangle;
            let c: &dyn Canvas = e.graphics;
            let t = c.theme();
            let red = hex("#d93025").unwrap_or(t.danger);
            let white = D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
            for (i, cell) in preset_cells(b.right - b.left).iter().enumerate() {
                let r = offset(cell, &b);
                let (fill, stroke) = STYLE_PRESETS[i];
                c.fill_rounded(&r, 4.0, &hex(fill).unwrap_or(white));
                if fill == "none" {
                    // `linear-gradient(45deg, transparent 45%, #d93025 45%, #d93025 55%, transparent 55%)`: a
                    // band from the bottom left corner to the top right one.
                    c.push_clip_rounded(&r, 4.0);
                    let half = 0.05 * ((r.right - r.left) + (r.bottom - r.top)) / 2.0;
                    let a = (r.left - half, r.bottom - half);
                    let b2 = (r.left + half, r.bottom + half);
                    let c2 = (r.right + half, r.top + half);
                    let d = (r.right - half, r.top - half);
                    c.fill_triangle(a, b2, c2, &red);
                    c.fill_triangle(a, c2, d, &red);
                    c.pop_clip_rounded();
                }
                c.stroke_rounded(&r, 4.0, &hex(stroke).unwrap_or(t.text_primary));
                if self.hot == Some(i) {
                    c.stroke_rounded_w(&Rect::new(r.left - 1.5, r.top - 1.5, r.right + 1.5, r.bottom + 1.5), 5.0, &with_alpha(t.accent, 0.6), 1.0);
                }
            }
            e.raise(self, "OnPaint");
        }

        fn tool_tip_at(&self, x: f32, y: f32) -> Option<String> {
            self.at(x, y).map(|_| Resources::fmt_apply_style().to_string())
        }

        fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
            if e.args().button == MouseButton::Left {
                self.pressed = self.at(e.args().x, e.args().y);
            }
            e.raise(&*self, "OnMouseDown");
        }

        fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
            let hot = self.at(e.args().x, e.args().y);
            if hot != self.hot {
                self.hot = hot;
                self.invalidate();
            }
            e.raise(&*self, "OnMouseMove");
        }

        fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
            let released = self.at(e.args().x, e.args().y);
            if let Some(i) = self.pressed.take().filter(|p| Some(*p) == released) {
                self.raise_preset_clicked(ItemEventArgs { index: i });
            }
            e.raise(&*self, "OnMouseUp");
        }

        fn on_mouse_leave(&mut self, e: &mut EventCx<'_, EmptyEventArgs>) {
            if self.hot.take().is_some() {
                self.invalidate();
            }
            e.raise(&*self, "OnMouseLeave");
        }
    }

    /// The shape's tabs (`flex border-b`, each `flex-1 py-1.5 text-[11px] border-b-2`, the active one
    /// `border-primary text-primary font-medium`, the others `text-text-secondary hover:bg-surface-1`):
    /// `Labels` is `|`-separated, `Selected` the active index; a click raises `TabSelected` (`e.index`).
    #[derive(kubuno_desktop::views::component::Component, Default)]
    #[kubuno(extends = Control, overrides(Control))]
    #[category("Diagrams")]
    #[toolbox(icon = "panel-top")]
    #[default_event("TabSelected")]
    pub struct PanelTabs {
        base: ControlCore,
        /// The tabs' texts, `|`-separated.
        #[property(bindable)]
        #[category("Appearance")]
        pub labels: String,
        /// The active tab.
        #[property(bindable)]
        #[category("Data")]
        pub selected: f32,
        /// Occurs when a tab is clicked (`e.index`).
        #[event]
        #[category("Action")]
        pub tab_selected: Event<ItemEventArgs>,
        hot: Option<usize>,
        pressed: Option<usize>,
    }

    impl PanelTabs {
        fn count(&self) -> usize {
            self.labels.split('|').count()
        }

        fn at(&self, x: f32, y: f32) -> Option<usize> {
            let size = self.size();
            segment_cells(self.count(), size.width, size.height, 0.0).iter().position(|r| r.contains(x, y))
        }
    }

    impl Control for PanelTabs {
        fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
            Size { width: proposed.width.max(120.0), height: 30.0 }
        }

        fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
            let b = e.clip_rectangle;
            let c: &dyn Canvas = e.graphics;
            let t = c.theme();
            c.fill_rounded(&Rect::new(b.left, b.bottom - 1.0, b.right, b.bottom), 0.0, &t.divider);
            let active = self.selected.max(0.0) as usize;
            for (i, (cell, label)) in segment_cells(self.count(), b.right - b.left, b.bottom - b.top, 0.0).iter().zip(self.labels.split('|')).enumerate() {
                let r = offset(cell, &b);
                let on = i == active;
                if !on && self.hot == Some(i) {
                    c.fill_rounded(&Rect::new(r.left, r.top, r.right, r.bottom - 2.0), 0.0, &t.surface_2);
                }
                if on {
                    c.fill_rounded(&Rect::new(r.left, r.bottom - 2.0, r.right, r.bottom), 0.0, &t.accent);
                }
                let format = if on { &c.formats().caption_strong } else { &c.formats().caption };
                c.text(label.trim(), &Rect::new(r.left, r.top, r.right, r.bottom - 2.0), format, if on { &t.accent } else { &t.text_secondary }, true);
            }
            e.raise(self, "OnPaint");
        }

        fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
            if e.args().button == MouseButton::Left {
                self.pressed = self.at(e.args().x, e.args().y);
            }
            e.raise(&*self, "OnMouseDown");
        }

        fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
            let hot = self.at(e.args().x, e.args().y);
            if hot != self.hot {
                self.hot = hot;
                self.invalidate();
            }
            e.raise(&*self, "OnMouseMove");
        }

        fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
            let released = self.at(e.args().x, e.args().y);
            if let Some(i) = self.pressed.take().filter(|p| Some(*p) == released) {
                self.selected = i as f32;
                self.raise_tab_selected(ItemEventArgs { index: i });
                self.invalidate();
            }
            e.raise(&*self, "OnMouseUp");
        }

        fn on_mouse_leave(&mut self, e: &mut EventCx<'_, EmptyEventArgs>) {
            if self.hot.take().is_some() {
                self.invalidate();
            }
            e.raise(&*self, "OnMouseLeave");
        }
    }

    /// The icons a [`SegmentBar`] may show (`vector_icon` takes a `'static` name).
    pub(super) const SEGMENT_ICONS: &[&str] = &[
        "Bold",
        "Italic",
        "AlignLeft",
        "AlignCenter",
        "AlignRight",
        "AlignStartVertical",
        "AlignCenterVertical",
        "AlignEndVertical",
        "FlipHorizontal2",
        "FlipVertical2",
        "ArrowUpToLine",
        "ChevronUp",
        "ChevronDown",
        "ArrowDownToLine",
    ];

    /// The `'static` name of `icon`, when it is one of [`SEGMENT_ICONS`].
    pub(super) fn segment_icon(icon: &str) -> Option<&'static str> {
        SEGMENT_ICONS.iter().copied().find(|i| *i == icon)
    }

    /// A row of `n` equal buttons `gap` apart in a `w`×`h` box at the origin.
    pub(super) fn segment_cells(n: usize, w: f32, h: f32, gap: f32) -> Vec<Rect> {
        if n == 0 {
            return Vec::new();
        }
        let cw = ((w - gap * (n - 1) as f32) / n as f32).max(1.0);
        (0..n)
            .map(|i| {
                let x = i as f32 * (cw + gap);
                Rect::new(x, 0.0, x + cw, h)
            })
            .collect()
    }

    /// A row of square icon buttons (`flex-1 h-7 rounded border`): one per value of `Values`, showing the
    /// icon of the same rank in `Icons`; the values listed in `Selected` are drawn on (`bg-primary/10
    /// border-primary text-primary`). A click raises `SegmentClicked` with the value (`e.new`).
    #[derive(kubuno_desktop::views::component::Component)]
    #[kubuno(extends = Control, overrides(Control))]
    #[category("Diagrams")]
    #[toolbox(icon = "columns-3")]
    #[default_event("SegmentClicked")]
    pub struct SegmentBar {
        base: ControlCore,
        /// The icons, comma-separated.
        #[property(bindable)]
        #[category("Appearance")]
        pub icons: String,
        /// The values, comma-separated.
        #[property(bindable)]
        #[category("Data")]
        pub values: String,
        /// The values drawn on, comma-separated.
        #[property(bindable)]
        #[category("Data")]
        pub selected: String,
        /// The tooltips, `|`-separated (an empty one for none).
        #[property(bindable)]
        #[category("Appearance")]
        pub tips: String,
        /// The space between two buttons.
        #[property(bindable)]
        #[category("Layout")]
        pub gap: f32,
        /// Occurs when a button is clicked: its value is `e.new`.
        #[event]
        #[category("Action")]
        pub segment_clicked: Event<TextChangedEventArgs>,
        hot: Option<usize>,
        pressed: Option<usize>,
    }

    impl Default for SegmentBar {
        fn default() -> Self {
            Self {
                base: ControlCore::default(),
                icons: String::new(),
                values: String::new(),
                selected: String::new(),
                tips: String::new(),
                gap: 4.0,
                segment_clicked: Event::default(),
                hot: None,
                pressed: None,
            }
        }
    }

    fn split(s: &str) -> Vec<&str> {
        s.split(',').map(str::trim).filter(|p| !p.is_empty()).collect()
    }

    impl SegmentBar {
        fn value_list(&self) -> Vec<&str> {
            split(&self.values)
        }

        fn at(&self, x: f32, y: f32) -> Option<usize> {
            let size = self.size();
            segment_cells(self.value_list().len(), size.width, size.height, self.gap).iter().position(|r| r.contains(x, y))
        }
    }

    impl Control for SegmentBar {
        fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
            Size { width: proposed.width.max(28.0 * self.value_list().len() as f32), height: 28.0 }
        }

        fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
            let b = e.clip_rectangle;
            let c: &dyn Canvas = e.graphics;
            let t = c.theme();
            let values = self.value_list();
            let icons = split(&self.icons);
            let selected = split(&self.selected);
            let enabled = self.enabled();
            for (i, cell) in segment_cells(values.len(), b.right - b.left, b.bottom - b.top, self.gap).iter().enumerate() {
                let r = offset(cell, &b);
                let on = selected.contains(&values[i]);
                if on {
                    c.fill_rounded(&r, 4.0, &with_alpha(t.accent, 0.10));
                } else if enabled && (self.hot == Some(i) || self.pressed == Some(i)) {
                    c.fill_rounded(&r, 4.0, &t.surface_2);
                }
                let border = if on { t.accent } else { t.card_stroke };
                c.stroke_rounded(&Rect::new(r.left + 0.5, r.top + 0.5, r.right - 0.5, r.bottom - 0.5), 4.0, &border);
                let ink = if on { t.accent } else { t.text_secondary };
                let ink = if enabled { ink } else { with_alpha(ink, 0.4) };
                if let Some(icon) = icons.get(i).and_then(|n| segment_icon(n)) {
                    let (cx, cy) = ((r.left + r.right) / 2.0, (r.top + r.bottom) / 2.0);
                    c.vector_icon(icon, &Rect::new(cx - 7.0, cy - 7.0, cx + 7.0, cy + 7.0), 14.0, &ink);
                }
            }
            e.raise(self, "OnPaint");
        }

        fn tool_tip_at(&self, x: f32, y: f32) -> Option<String> {
            let i = self.at(x, y)?;
            self.tips.split('|').nth(i).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
        }

        fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
            if e.args().button == MouseButton::Left {
                self.pressed = self.at(e.args().x, e.args().y);
                self.invalidate();
            }
            e.raise(&*self, "OnMouseDown");
        }

        fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
            let hot = self.at(e.args().x, e.args().y);
            if hot != self.hot {
                self.hot = hot;
                self.invalidate();
            }
            e.raise(&*self, "OnMouseMove");
        }

        fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
            let released = self.at(e.args().x, e.args().y);
            if let Some(i) = self.pressed.take().filter(|p| Some(*p) == released) {
                if let Some(v) = self.value_list().get(i).map(|v| v.to_string()) {
                    self.raise_segment_clicked(TextChangedEventArgs::new(String::new(), v, ChangeSource::User));
                }
            }
            self.invalidate();
            e.raise(&*self, "OnMouseUp");
        }

        fn on_mouse_leave(&mut self, e: &mut EventCx<'_, EmptyEventArgs>) {
            if self.hot.take().is_some() {
                self.invalidate();
            }
            e.raise(&*self, "OnMouseLeave");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shape() -> ShapeFormat {
        ShapeFormat {
            style: ShapeStyle { fill_color: "none".into(), rounded: 12.0, ..ShapeStyle::default() },
            label_style: LabelStyle { italic: true, ..LabelStyle::default() },
            label: "A".into(),
            x: 10.4,
            y: 20.6,
            w: 120.0,
            h: 60.0,
            rotation: 30.0,
            selected: 1,
            has_group: true,
        }
    }

    #[test]
    fn a_shape_shows_its_values_as_the_web_does() {
        let mut p = FormatPanel::default();
        assert!(p.show_hint && !p.show_shape);
        p.set_state(FormatState::Shape(Box::new(shape())));
        assert!(p.show_shape && !p.show_hint && !p.show_conn);
        assert_eq!(p.fill_color, "#ffffff", "a `none` fill shows white in the picker");
        assert_eq!(p.fill_text, "none", "and `none` in the text box");
        assert_eq!((p.pos_x, p.pos_y), (10.0, 21.0), "Math.round");
        assert_eq!(p.rounded, 12.0);
        assert!(p.can_ungroup && !p.multi);
        assert_eq!(p.font_size, 12.0);
        assert_eq!(p.emphasis, "italic");
        assert!(p.show_style && !p.show_text);
        p.tab_selected(&ItemEventArgs { index: 2 });
        assert!(p.show_arrange && !p.show_style, "the Disposition tab");
        p.set_state(FormatState::Connector(Box::new(ConnFormat { style: ConnStyle::read(&serde_json::Map::new()), label: "x".into() })));
        assert!(p.show_conn && !p.show_shape);
        assert_eq!((p.conn_routing.as_str(), p.conn_arrow_end.as_str()), ("straight", "block"));
        p.set_state(FormatState::None);
        assert!(p.show_hint && !p.show_conn);
    }

    #[test]
    fn edits_map_to_the_editor_commands() {
        let e = FormatEditEventArgs::new;
        assert_eq!(FormatCommand::of(&e("style", "strokeWidth", field_number(2.3))), Some(FormatCommand::ShapeStyle("strokeWidth".into(), json!(2.3))));
        assert_eq!(FormatCommand::of(&e("preset", "8", Value::Null)), Some(FormatCommand::Preset("none".into(), "#000000".into())));
        assert_eq!(FormatCommand::of(&e("geom", "w", json!(140))), Some(FormatCommand::Geometry("w".into(), 140.0)));
        assert_eq!(FormatCommand::of(&e("flip", "v", Value::Null)), Some(FormatCommand::Flip(false)));
        assert_eq!(FormatCommand::of(&e("flip", "h", Value::Null)), Some(FormatCommand::Flip(true)));
        assert_eq!(FormatCommand::of(&e("conn_label", "", json!("oui"))), Some(FormatCommand::ConnLabel("oui".into())));
        assert_eq!(FormatCommand::of(&e("label_style", "bold", json!(true))), Some(FormatCommand::LabelStyle("bold".into(), json!(true))));
        assert_eq!(FormatCommand::of(&e("preset", "12", Value::Null)), None);
        assert_eq!(FormatCommand::of(&e("geom", "x", Value::Null)), None);
        assert_eq!(FormatCommand::of(&e("nope", "", Value::Null)), None);
    }

    #[test]
    fn field_numbers_are_written_like_parse_float() {
        assert_eq!(field_number(2.3), json!(2.3));
        assert_eq!(field_number(2.0), json!(2));
        assert_eq!(field_number(0.5), json!(0.5));
    }

    #[test]
    fn the_presets_grid_is_six_columns_of_two_rows() {
        let cells = parts::preset_cells(226.0);
        assert_eq!(cells.len(), 12);
        assert_eq!(cells[6].top, 30.0, "h-6 + gap-1.5");
        assert!((cells[5].right - 226.0).abs() < 0.01);
        let segs = parts::segment_cells(4, 226.0, 28.0, 4.0);
        assert!((segs[3].right - 226.0).abs() < 0.01);
        assert!(parts::segment_cells(0, 100.0, 28.0, 4.0).is_empty());
    }

    #[test]
    fn every_segment_icon_is_embedded() {
        let names: Vec<String> = kubuno_drive_desktop_app_controls::themed_icon::catalog().into_iter().map(|e| e.name.to_string()).collect();
        for icon in parts::SEGMENT_ICONS {
            assert!(names.iter().any(|n| n == icon), "`{icon}` is not embedded");
        }
        let mut rest = include_str!("format_panel.kbcontrol");
        while let Some(i) = rest.find("Icons=\"") {
            let after = &rest[i + 7..];
            let list: String = after.chars().take_while(|c| *c != '"').collect();
            for icon in list.split(',') {
                assert!(parts::segment_icon(icon).is_some(), "the view names `{icon}`, which SegmentBar cannot draw");
            }
            rest = after;
        }
    }
}
