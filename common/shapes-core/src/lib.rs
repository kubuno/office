//! `kubuno-office-shapes-core` — the shape geometry every Kubuno Office editor shares, ported from the
//! web's `office/web/src/shapes` (presentations, documents, spreadsheets, whiteboard and diagrams all
//! draw the same geometries).
//!
//! | Module | Web origin | Content |
//! |---|---|---|
//! | [`path`] | `paths.ts` (`ShapeSubPath`, `shadeColour`) | renderer-neutral path commands, the web's SVG formatting |
//! | [`preset`] | `preset-engine.ts`, `preset-data.ts` | LibreOffice's OOXML presets: equations, paths, text frames, handles |
//! | [`native`] | `native-geometry.ts` | the suite's own fraction-adjusted geometries |
//! | [`adjust`] | `adjust.ts` | the yellow knobs: values, positions, drags |
//! | [`catalog`] | `catalog.ts` | the gallery's kinds, labels and default sizes |
//! | [`draw`] | `draw.ts` | the rubber-band box of a draw-to-create gesture |
//!
//! The web's module registry (`registry.ts`, geometries contributed at run time by other modules) has
//! no desktop counterpart yet: no desktop module registers shapes, so [`shape_view`] routes the native
//! and preset kinds only.

pub mod adjust;
pub mod catalog;
pub mod draw;
pub mod native;
pub mod path;
pub mod preset;

pub use path::{Cmd, Path, Rect, SubPath};

/// Legacy kind names of editors written before the shared catalogue (`KIND_ALIASES`).
pub fn alias(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "rightArrow" => "arrow",
        "leftArrow" => "arrowLeft",
        "upArrow" => "arrowUp",
        "downArrow" => "arrowDown",
        "speech" => "calloutRoundRect",
        "square" | "rectangle" => "rect",
        "circle" | "oval" => "ellipse",
        "rounded" => "roundRect",
        "process" => "flowProcess",
        "decision" => "flowDecision",
        "terminator" => "flowTerminator",
        "document" => "flowDocument",
        "data" => "flowData",
        _ => return None,
    })
}

/// The canonical (preset-backed) kind for an editor's stored value (`resolveShapeKind`).
pub fn resolve_shape_kind(kind: &str) -> Option<&str> {
    if preset::preset_of(kind).is_some() {
        return Some(kind);
    }
    alias(kind).filter(|a| preset::preset_of(a).is_some())
}

/// True when the shared machinery can draw the kind.
pub fn has_shape_geometry(kind: &str) -> bool {
    resolve_shape_kind(kind).is_some()
}

/// The preset sub-paths of a kind in a `w`×`h` box (`shapePaths`), `None` for a kind the engine does not
/// know (lines and connectors).
pub fn shape_paths(kind: &str, w: f64, h: f64, adj: Option<&[f64]>) -> Option<Vec<SubPath>> {
    let p = preset::preset_of(resolve_shape_kind(kind)?)?;
    Some(preset::preset_path(p, &preset::Env { w, h, adj: preset::adj_values(p, adj) }))
}

/// The preset's text frame in a `w`×`h` box (`shapeTextBox` of `paths.ts`; the whole box when unknown).
pub fn shape_text_box(kind: &str, w: f64, h: f64, adj: Option<&[f64]>) -> Rect {
    match resolve_shape_kind(kind).and_then(preset::preset_of) {
        Some(p) => preset::preset_text_frame(p, &preset::Env { w, h, adj: preset::adj_values(p, adj) }),
        None => Rect { x: 0.0, y: 0.0, w, h },
    }
}

/// How a shape is painted (`PaintShapeViewOpts`).
#[derive(Debug, Clone, Default)]
pub struct ViewOptions<'a> {
    pub adj: Option<&'a [f64]>,
    /// The outline is stroked: native geometry is inset by half its width.
    pub stroke: bool,
    pub stroke_width: f64,
}

/// The geometry a canvas paints for a kind in a `w`×`h` box at the origin (`paintShapeView`): the
/// native fraction geometry for the fraction-adjusted kinds, else the OOXML preset. `None` when no shared
/// geometry exists (lines, connectors), so the caller draws its own.
pub fn shape_view(kind: &str, w: f64, h: f64, opts: &ViewOptions) -> Option<Vec<SubPath>> {
    if adjust::is_fraction_kind(kind) && native::has_native_geometry(kind) {
        let inset = if opts.stroke { opts.stroke_width / 2.0 } else { 0.0 };
        let g = native::shape_geometry(kind, w, h, inset, opts.adj);
        return Some(vec![SubPath { path: g.path, fill: true, stroke: true, shade: 1.0 }]);
    }
    shape_paths(kind, w, h, opts.adj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_names_resolve_to_the_catalogue() {
        assert_eq!(resolve_shape_kind("rightArrow"), Some("arrow"));
        assert_eq!(resolve_shape_kind("speech"), Some("calloutRoundRect"));
        assert_eq!(resolve_shape_kind("nonsense"), None);
    }

    #[test]
    fn fraction_kinds_are_drawn_natively_and_others_from_presets() {
        let native = shape_view("star", 100.0, 100.0, &ViewOptions::default()).expect("star");
        assert_eq!(native[0].path.cmds.len(), 11);
        let cube = shape_view("cube", 100.0, 100.0, &ViewOptions::default()).expect("cube");
        assert!(cube.iter().any(|s| s.shade != 1.0), "the cube's faces are shaded");
        assert!(shape_view("polyline", 10.0, 10.0, &ViewOptions::default()).is_none());
    }
}
