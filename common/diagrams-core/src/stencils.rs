//! The stencils: the catalogue and how each one is painted (`stencils.ts`), which stencils the shared
//! office shape engine paints (`diagram-shape-kinds.ts`), their yellow adjustment knobs
//! (`diagram-adjust.ts`) and the draw-to-create gesture with its live ghost (`diagram-draw.ts` over
//! `shapes/draw.ts`).
//!
//! Painting goes through [`Ctx2D`], so every primitive below reads like its web original. One browser
//! behaviour matters throughout: assigning a colour the canvas does not understand (`'none'`) to
//! `fillStyle`/`strokeStyle` is IGNORED, the previous style staying in force; [`fill_style`] and
//! [`stroke_style`] reproduce that, so an unguarded `ctx.fillStyle = fill; ctx.fill()` with a `none`
//! fill paints with whatever colour was set before, exactly as on the web (QUIRK).

use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::OnceLock;

use kubuno_office_shapes_core::path::{js_round, shade_colour, Rect};
use kubuno_office_shapes_core::{adjust, catalog, native, ViewOptions};
use serde_json::Value;

use crate::canvas::{Ctx2D, Point, TextAlign, TextBaseline};
use crate::hardware;
use crate::model::{js_number, LabelStyle, Obj, Shape, ShapeStyle};

// ── The catalogue ────────────────────────────────────────────────────────────

/// A stencil (`StencilDef`).
#[derive(Debug, Clone, PartialEq)]
pub struct StencilDef {
    pub id: &'static str,
    /// The French default name (the UI translates `stencil_<id>`).
    pub name: &'static str,
    /// The French category name (`Formes basiques`, `Formes · Étoiles et bannières`…).
    pub category: String,
    pub default_w: f64,
    pub default_h: f64,
    /// The partial style a new shape gets.
    pub style: Obj,
}

/// A partial style: `fillColor`, `strokeColor`, and `rounded` when given.
fn st(fill: &str, stroke: &str, rounded: Option<f64>) -> Obj {
    let mut o = Obj::new();
    o.insert("fillColor".into(), fill.into());
    o.insert("strokeColor".into(), stroke.into());
    if let Some(r) = rounded {
        o.insert("rounded".into(), js_number(r));
    }
    o
}

/// `(id, name, category, w, h, fill, stroke, rounded)` of `STENCILS`, in the web's order
/// (`stencils.ts:827-945`).
/// `(id, name, category, w, h, fill, stroke, rounded)`.
type OwnStencil = (&'static str, &'static str, &'static str, f64, f64, &'static str, &'static str, Option<f64>);

#[rustfmt::skip]
const OWN: &[OwnStencil] = &[
    // Formes basiques
    ("rect", "Rectangle", "Formes basiques", 120.0, 60.0, "#dae8fc", "#6c8ebf", None),
    ("rounded_rect", "Rect. arrondi", "Formes basiques", 120.0, 60.0, "#d5e8d4", "#82b366", Some(12.0)),
    ("ellipse", "Ellipse", "Formes basiques", 120.0, 70.0, "#fff2cc", "#d6b656", None),
    ("diamond", "Losange", "Formes basiques", 120.0, 80.0, "#ffe6cc", "#d79b00", None),
    ("cylinder", "Cylindre", "Formes basiques", 80.0, 100.0, "#dae8fc", "#6c8ebf", None),
    ("parallelogram", "Parallélogramme", "Formes basiques", 120.0, 60.0, "#e1d5e7", "#9673a6", None),
    ("triangle", "Triangle", "Formes basiques", 100.0, 80.0, "#fff2cc", "#d6b656", None),
    ("hexagon", "Hexagone", "Formes basiques", 120.0, 70.0, "#dae8fc", "#6c8ebf", None),
    ("cloud", "Nuage", "Formes basiques", 120.0, 80.0, "#f5f5f5", "#666666", None),
    ("cross", "Croix", "Formes basiques", 80.0, 80.0, "#f8cecc", "#b85450", None),
    ("star", "Étoile", "Formes basiques", 80.0, 80.0, "#fff2cc", "#d6b656", None),
    ("callout", "Bulle", "Formes basiques", 120.0, 80.0, "#ffffff", "#000000", None),
    ("trapezoid", "Trapèze", "Formes basiques", 120.0, 60.0, "#dae8fc", "#6c8ebf", None),
    ("text", "Texte", "Formes basiques", 120.0, 40.0, "none", "none", None),
    // Flux
    ("flow_start", "Début/Fin", "Flux", 120.0, 50.0, "#d5e8d4", "#82b366", Some(30.0)),
    ("flow_process", "Processus", "Flux", 120.0, 60.0, "#dae8fc", "#6c8ebf", None),
    ("flow_decision", "Décision", "Flux", 120.0, 80.0, "#fff2cc", "#d6b656", None),
    ("flow_data", "Données", "Flux", 120.0, 60.0, "#e1d5e7", "#9673a6", None),
    ("flow_document", "Document", "Flux", 120.0, 70.0, "#dae8fc", "#6c8ebf", None),
    ("flow_db", "Base de données", "Flux", 80.0, 100.0, "#dae8fc", "#6c8ebf", None),
    ("flow_prep", "Préparation", "Flux", 120.0, 60.0, "#fff2cc", "#d6b656", None),
    ("flow_connector", "Connecteur", "Flux", 40.0, 40.0, "#ffffff", "#000000", None),
    ("flow_manual", "Saisie manuelle", "Flux", 120.0, 60.0, "#ffe6cc", "#d79b00", None),
    // Réseau
    ("net_server", "Serveur", "Réseau", 100.0, 70.0, "#dae8fc", "#6c8ebf", None),
    ("net_router", "Routeur", "Réseau", 80.0, 70.0, "#fff2cc", "#d6b656", None),
    ("net_database", "Base de données", "Réseau", 80.0, 100.0, "#dae8fc", "#6c8ebf", None),
    ("net_cloud", "Cloud", "Réseau", 120.0, 80.0, "#e8f0fe", "#1a73e8", None),
    ("net_firewall", "Pare-feu", "Réseau", 80.0, 80.0, "#fce8e6", "#d93025", None),
    ("net_desktop", "Ordinateur", "Réseau", 80.0, 80.0, "#e8eaed", "#5f6368", None),
    // UML
    ("uml_class", "Classe", "UML", 140.0, 100.0, "#dae8fc", "#6c8ebf", None),
    ("uml_interface", "Interface", "UML", 140.0, 100.0, "#d5e8d4", "#82b366", None),
    ("uml_actor", "Acteur", "UML", 60.0, 100.0, "#ffffff", "#000000", None),
    ("uml_usecase", "Cas d'utilisation", "UML", 140.0, 60.0, "#fff2cc", "#d6b656", None),
    ("uml_state", "État", "UML", 120.0, 60.0, "#e1d5e7", "#9673a6", Some(20.0)),
    ("uml_initial", "État initial", "UML", 30.0, 30.0, "#000000", "#000000", None),
    ("uml_final", "État final", "UML", 30.0, 30.0, "#000000", "#000000", None),
    ("uml_system", "Système", "UML", 200.0, 150.0, "none", "#000000", None),
    // Cloud / infra
    ("aws_ec2", "EC2", "AWS", 80.0, 80.0, "#fce8d8", "#e07624", None),
    ("aws_s3", "S3", "AWS", 80.0, 80.0, "#d5e8d4", "#3b803c", None),
    ("aws_rds", "RDS", "AWS", 80.0, 80.0, "#dae8fc", "#1a73e8", None),
    ("aws_lambda", "Lambda", "AWS", 80.0, 80.0, "#fff2cc", "#d6b656", None),
    ("aws_vpc", "VPC", "AWS", 200.0, 150.0, "none", "#1a73e8", None),
    ("k8s_pod", "Pod", "Kubernetes", 80.0, 80.0, "#e8f0fe", "#1a73e8", None),
    ("k8s_service", "Service", "Kubernetes", 80.0, 80.0, "#e6f4ea", "#34a853", None),
    ("k8s_deployment", "Deployment", "Kubernetes", 100.0, 60.0, "#fce8e6", "#d93025", None),
    // Formes basiques (étendu)
    ("shp_pentagon", "Pentagone", "Formes basiques", 100.0, 90.0, "#dae8fc", "#6c8ebf", None),
    ("shp_octagon", "Octogone", "Formes basiques", 100.0, 90.0, "#dae8fc", "#6c8ebf", None),
    ("shp_arrow_right", "Flèche droite", "Formes basiques", 120.0, 60.0, "#d5e8d4", "#82b366", None),
    ("shp_arrow_left", "Flèche gauche", "Formes basiques", 120.0, 60.0, "#d5e8d4", "#82b366", None),
    ("shp_arrow_up", "Flèche haut", "Formes basiques", 60.0, 120.0, "#d5e8d4", "#82b366", None),
    ("shp_arrow_down", "Flèche bas", "Formes basiques", 60.0, 120.0, "#d5e8d4", "#82b366", None),
    ("shp_cube", "Cube", "Formes basiques", 100.0, 90.0, "#dae8fc", "#6c8ebf", None),
    ("shp_step", "Chevron", "Formes basiques", 120.0, 60.0, "#ffe6cc", "#d79b00", None),
    ("shp_card", "Carte", "Formes basiques", 120.0, 70.0, "#fff2cc", "#d6b656", None),
    ("shp_note", "Note", "Formes basiques", 100.0, 90.0, "#fff2cc", "#d6b656", None),
    // Flux (étendu)
    ("flow_terminator", "Terminaison", "Flux", 120.0, 50.0, "#d5e8d4", "#82b366", None),
    ("flow_predefined", "Sous-programme", "Flux", 120.0, 60.0, "#dae8fc", "#6c8ebf", None),
    ("flow_internal_storage", "Stockage interne", "Flux", 100.0, 80.0, "#dae8fc", "#6c8ebf", None),
    ("flow_display", "Affichage", "Flux", 120.0, 70.0, "#fff2cc", "#d6b656", None),
    ("flow_delay", "Délai", "Flux", 110.0, 60.0, "#ffe6cc", "#d79b00", None),
    ("flow_offpage", "Hors page", "Flux", 90.0, 90.0, "#fff2cc", "#d6b656", None),
    ("flow_or", "Ou (logique)", "Flux", 60.0, 60.0, "#ffffff", "#000000", None),
    ("flow_summing", "Jonction", "Flux", 60.0, 60.0, "#ffffff", "#000000", None),
    ("flow_manual_op", "Opération manuelle", "Flux", 120.0, 60.0, "#ffe6cc", "#d79b00", None),
    ("flow_card", "Carte (perfo)", "Flux", 120.0, 70.0, "#dae8fc", "#6c8ebf", None),
    // Entité-association
    ("er_entity", "Entité", "Entité-association", 140.0, 60.0, "#dae8fc", "#6c8ebf", None),
    ("er_weak_entity", "Entité faible", "Entité-association", 140.0, 60.0, "#dae8fc", "#6c8ebf", None),
    ("er_attribute", "Attribut", "Entité-association", 110.0, 50.0, "#d5e8d4", "#82b366", None),
    ("er_key_attribute", "Attribut clé", "Entité-association", 110.0, 50.0, "#fff2cc", "#d6b656", None),
    ("er_relationship", "Relation", "Entité-association", 120.0, 70.0, "#ffe6cc", "#d79b00", None),
    // BPMN
    ("bpmn_task", "Tâche", "BPMN", 120.0, 70.0, "#dae8fc", "#6c8ebf", Some(10.0)),
    ("bpmn_start", "Début", "BPMN", 50.0, 50.0, "#d5e8d4", "#82b366", None),
    ("bpmn_end", "Fin", "BPMN", 50.0, 50.0, "#f8cecc", "#b85450", None),
    ("bpmn_event", "Événement", "BPMN", 50.0, 50.0, "#fff2cc", "#d6b656", None),
    ("bpmn_gateway", "Passerelle", "BPMN", 70.0, 70.0, "#fff2cc", "#d6b656", None),
    // UML (étendu)
    ("uml_package", "Paquetage", "UML", 160.0, 110.0, "#dae8fc", "#6c8ebf", None),
    ("uml_component", "Composant", "UML", 140.0, 80.0, "#d5e8d4", "#82b366", None),
    ("uml_node", "Nœud", "UML", 120.0, 90.0, "#dae8fc", "#6c8ebf", None),
    ("uml_object", "Objet", "UML", 140.0, 60.0, "#e1d5e7", "#9673a6", None),
    ("uml_note", "Note", "UML", 120.0, 80.0, "#fff2cc", "#d6b656", None),
    // Réseau (étendu)
    ("net_user", "Utilisateur", "Réseau", 60.0, 80.0, "#dae8fc", "#6c8ebf", None),
    ("net_laptop", "Portable", "Réseau", 100.0, 70.0, "#e8eaed", "#5f6368", None),
    ("net_mobile", "Mobile", "Réseau", 60.0, 90.0, "#e8eaed", "#5f6368", None),
    ("net_switch", "Commutateur", "Réseau", 110.0, 50.0, "#dae8fc", "#6c8ebf", None),
    ("net_printer", "Imprimante", "Réseau", 80.0, 80.0, "#e8eaed", "#5f6368", None),
    ("net_wifi", "Wi-Fi", "Réseau", 70.0, 60.0, "none", "#1a73e8", None),
    ("net_internet", "Internet", "Réseau", 120.0, 80.0, "#e8f0fe", "#1a73e8", None),
    // AWS (étendu — rendu en carré arrondi coloré)
    ("aws_dynamodb", "DynamoDB", "AWS", 80.0, 80.0, "#dae8fc", "#1a73e8", Some(8.0)),
    ("aws_cloudfront", "CloudFront", "AWS", 80.0, 80.0, "#e1d5e7", "#9673a6", Some(8.0)),
    ("aws_sqs", "SQS", "AWS", 80.0, 80.0, "#e1d5e7", "#9673a6", Some(8.0)),
    ("aws_sns", "SNS", "AWS", 80.0, 80.0, "#e1d5e7", "#9673a6", Some(8.0)),
    ("aws_apigw", "API Gateway", "AWS", 80.0, 80.0, "#e1d5e7", "#9673a6", Some(8.0)),
    ("aws_elb", "Load Balancer", "AWS", 80.0, 80.0, "#fce8d8", "#e07624", Some(8.0)),
    ("aws_route53", "Route 53", "AWS", 80.0, 80.0, "#e1d5e7", "#9673a6", Some(8.0)),
    // Maquettes (UI)
    ("ui_button", "Bouton", "Maquettes", 100.0, 36.0, "#1a73e8", "#1557b0", None),
    ("ui_input", "Champ", "Maquettes", 160.0, 34.0, "#ffffff", "#bdc1c6", None),
    ("ui_checkbox", "Case", "Maquettes", 120.0, 24.0, "none", "#5f6368", None),
    ("ui_radio", "Radio", "Maquettes", 120.0, 24.0, "none", "#5f6368", None),
    ("ui_dropdown", "Liste", "Maquettes", 160.0, 34.0, "#ffffff", "#bdc1c6", None),
    ("ui_browser", "Navigateur", "Maquettes", 240.0, 160.0, "#ffffff", "#5f6368", None),
    ("ui_image", "Image", "Maquettes", 120.0, 90.0, "#f1f3f4", "#9aa0a6", None),
    // Conteneurs / couloirs (déplacer le conteneur déplace son contenu)
    ("container", "Conteneur", "Conteneurs", 240.0, 160.0, "#ffffff", "#666666", None),
    ("swimlane_v", "Couloir vertical", "Conteneurs", 160.0, 240.0, "#ffffff", "#666666", None),
    ("swimlane_h", "Couloir horizontal", "Conteneurs", 280.0, 120.0, "#ffffff", "#666666", None),
];

/// `'Formes · ' + title` lowercased, every run of characters outside `[a-z0-9]` replaced by one `-`
/// (`/[^a-z0-9]+/g`). JavaScript's `toLowerCase` lowers accented letters too, which then fall outside
/// `[a-z0-9]` (`Étoiles` → `-toiles`).
fn slug(title: &str) -> String {
    let lower = title.to_lowercase();
    let mut out = String::new();
    let mut in_run = false;
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            in_run = false;
        } else if !in_run {
            out.push('-');
            in_run = true;
        }
    }
    out
}

/// `STENCILS`: the diagrams' own stencils, the hardware icons, then the office catalogue's shapes.
pub fn stencils() -> &'static [StencilDef] {
    static ALL: OnceLock<Vec<StencilDef>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut v: Vec<StencilDef> = OWN
            .iter()
            .map(|&(id, name, category, w, h, fill, stroke, rounded)| StencilDef {
                id,
                name,
                category: category.into(),
                default_w: w,
                default_h: h,
                style: st(fill, stroke, rounded),
            })
            .collect();
        // `...HW_STENCILS`: category « Matériel », 64×64, an empty style.
        v.extend(hardware::HW_STENCILS.iter().map(|&(id, name)| StencilDef {
            id,
            name,
            category: "Matériel".into(),
            default_w: hardware::HW_SIZE,
            default_h: hardware::HW_SIZE,
            style: Obj::new(),
        }));
        // `OFFICE_SHAPE_STENCILS`: the catalogue minus `textBox`, the ids above and kinds with no shared
        // geometry; half the insertion size of the other editors.
        let own: std::collections::HashSet<&str> = v.iter().map(|s| s.id).collect();
        let mut extra = Vec::new();
        for cat in catalog::SHAPE_CATALOG {
            for sp in cat.shapes {
                if sp.kind == "textBox" || own.contains(sp.kind) || !kubuno_office_shapes_core::has_shape_geometry(sp.kind) {
                    continue;
                }
                let (w, h) = catalog::shape_default_size(sp.kind);
                extra.push(StencilDef {
                    id: sp.kind,
                    name: sp.label,
                    category: format!("Formes · {}", cat.title),
                    default_w: js_round(w / 2.0),
                    default_h: js_round(h / 2.0),
                    style: st("#ffffff", "#000000", None),
                });
            }
        }
        v.extend(extra);
        v
    })
}

/// `STENCIL_MAP[id]`.
pub fn stencil(id: &str) -> Option<&'static StencilDef> {
    static MAP: OnceLock<HashMap<&'static str, usize>> = OnceLock::new();
    let map = MAP.get_or_init(|| {
        // Later entries win, as `Object.fromEntries` does with a repeated key.
        stencils().iter().enumerate().map(|(i, s)| (s.id, i)).collect()
    });
    map.get(id).map(|&i| &stencils()[i])
}

/// `CATEGORY_ID`: the stable id of a French category name.
pub fn category_id(category: &str) -> Option<&'static str> {
    Some(match category {
        "Formes basiques" => "basic",
        "Flux" => "flow",
        "Entité-association" => "er",
        "BPMN" => "bpmn",
        "Réseau" => "network",
        "UML" => "uml",
        "AWS" => "aws",
        "Kubernetes" => "k8s",
        "Maquettes" => "mockup",
        "Conteneurs" => "container",
        "Matériel" => "hardware",
        _ => return None,
    })
}

/// `CATEGORY_LABELS`: the human label of every category id, the office catalogue's groups included.
pub fn category_labels() -> &'static [(String, String)] {
    static LABELS: OnceLock<Vec<(String, String)>> = OnceLock::new();
    LABELS.get_or_init(|| {
        let mut v: Vec<(String, String)> = [
            ("basic", "Formes basiques"),
            ("flow", "Flux"),
            ("er", "Entité-association"),
            ("bpmn", "BPMN"),
            ("network", "Réseau"),
            ("uml", "UML"),
            ("aws", "AWS"),
            ("k8s", "Kubernetes"),
            ("mockup", "Maquettes"),
            ("container", "Conteneurs"),
            ("hardware", "Ordinateur et Matériel"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        for c in catalog::SHAPE_CATALOG {
            v.push((format!("shapes-{}", slug(c.title)), format!("Formes · {}", c.title)));
        }
        v
    })
}

/// The label of a category id (`CATEGORY_LABELS[cat] ?? cat`).
pub fn category_label(cat: &str) -> String {
    category_labels().iter().find(|(id, _)| id == cat).map(|(_, l)| l.clone()).unwrap_or_else(|| cat.to_string())
}

/// `categoryIdOf`.
pub fn category_id_of(s: &StencilDef) -> String {
    if let Some(rest) = s.category.strip_prefix("Formes · ") {
        return format!("shapes-{}", slug(rest));
    }
    category_id(&s.category).unwrap_or("basic").to_string()
}

/// `getCategories`: the category ids in the order their first stencil appears.
pub fn get_categories() -> Vec<String> {
    let mut cats: Vec<String> = Vec::new();
    for s in stencils() {
        let c = category_id_of(s);
        if !cats.contains(&c) {
            cats.push(c);
        }
    }
    cats
}

/// `getStencilsByCategory`.
pub fn get_stencils_by_category(cat: &str) -> Vec<&'static StencilDef> {
    stencils().iter().filter(|s| category_id_of(s) == cat).collect()
}

/// `searchStencils(q, nameOf?)`: with a translator, by translated name or id; without, by name or category.
pub fn search_stencils(q: &str, name_of: Option<&dyn Fn(&StencilDef) -> String>) -> Vec<&'static StencilDef> {
    let lq = q.to_lowercase();
    match name_of {
        Some(name) => stencils().iter().filter(|s| name(s).to_lowercase().contains(&lq) || s.id.to_lowercase().contains(&lq)).collect(),
        None => stencils().iter().filter(|s| s.name.to_lowercase().contains(&lq) || s.category.to_lowercase().contains(&lq)).collect(),
    }
}

// ── Which stencils the shared engine paints (`diagram-shape-kinds.ts`) ───────

/// `HOUSE_STENCIL_IDS`: ids spelt like a catalogue kind but drawn by the diagrams' own code.
pub const HOUSE_STENCIL_IDS: [&str; 12] =
    ["rect", "ellipse", "triangle", "diamond", "hexagon", "cloud", "cross", "star", "cylinder", "trapezoid", "parallelogram", "callout"];

/// `sharedKindOf`: the shared kind painting a stencil id, `None` for the house geometries, the business
/// templates and the hardware icons.
pub fn shared_kind_of(kind: &str) -> Option<&str> {
    if HOUSE_STENCIL_IDS.contains(&kind) || kind.starts_with("hw_") {
        return None;
    }
    (kubuno_office_shapes_core::has_shape_geometry(kind) || native::has_native_geometry(kind)).then_some(kind)
}

/// `isSharedStencil`.
pub fn is_shared_stencil(kind: &str) -> bool {
    shared_kind_of(kind).is_some()
}

fn rect_of(s: &Shape) -> Rect {
    Rect { x: s.x(), y: s.y(), w: s.w(), h: s.h() }
}

/// `stencilAdjustHandles`: the knobs of a shape (world coordinates, unrotated frame); empty for a stencil
/// the shared engine does not paint.
pub fn stencil_adjust_handles(s: &Shape) -> Vec<adjust::Handle> {
    match shared_kind_of(s.kind()) {
        Some(kind) => adjust::adjust_handles(kind, rect_of(s), s.adj().as_deref()),
        None => Vec::new(),
    }
}

// ── Adjustment knobs (`diagram-adjust.ts`) ───────────────────────────────────

/// A world point brought into the shape's own (unrotated) frame.
fn to_local(s: &Shape, px: f64, py: f64) -> Point {
    let rot = s.rotation();
    if rot == 0.0 {
        return Point::new(px, py);
    }
    let (cx, cy) = (s.x() + s.w() / 2.0, s.y() + s.h() / 2.0);
    let a = -rot * PI / 180.0;
    let (dx, dy) = (px - cx, py - cy);
    Point::new(cx + dx * a.cos() - dy * a.sin(), cy + dx * a.sin() + dy * a.cos())
}

/// `hitShapeAdjust`: the knob under a world point (`None` for the web's -1).
pub fn hit_shape_adjust(s: &Shape, px: f64, py: f64, slop: f64) -> Option<usize> {
    let kind = shared_kind_of(s.kind())?;
    let p = to_local(s, px, py);
    adjust::hit_adjust(kind, rect_of(s), p.x, p.y, s.adj().as_deref(), slop)
}

/// `shapeAdjustFromDrag`: the adjustment values after dragging knob `index` to a world point.
pub fn shape_adjust_from_drag(s: &Shape, index: usize, px: f64, py: f64) -> Option<Vec<f64>> {
    let kind = shared_kind_of(s.kind())?;
    let p = to_local(s, px, py);
    Some(adjust::adjust_from_drag(kind, rect_of(s), index, p.x, p.y, s.adj().as_deref()))
}

/// `paintAdjustHandles`: the yellow knobs (canvas already in world space), turning with the shape.
pub fn paint_adjust_handles(ctx: &mut Ctx2D<'_>, s: &Shape, zoom: f64) {
    let knobs = stencil_adjust_handles(s);
    if knobs.is_empty() {
        return;
    }
    ctx.save();
    let rot = s.rotation();
    if rot != 0.0 {
        let (cx, cy) = (s.x() + s.w() / 2.0, s.y() + s.h() / 2.0);
        ctx.translate(cx, cy);
        ctx.rotate(rot * PI / 180.0);
        ctx.translate(-cx, -cy);
    }
    ctx.set_line_dash(&[]);
    for k in knobs {
        ctx.begin_path();
        ctx.arc(k.x, k.y, 4.5 / zoom, 0.0, PI * 2.0, false);
        ctx.set_fill_style("#ffd400");
        ctx.fill();
        ctx.set_stroke_style("#8a6d00");
        ctx.set_line_width(1.0 / zoom);
        ctx.stroke();
    }
    ctx.restore();
}

// ── Draw-to-create (`diagram-draw.ts`, `shapes/draw.ts`) ─────────────────────

/// A draw gesture in progress: the armed stencil and the rubber-band box (world coordinates, snapped).
#[derive(Debug, Clone, PartialEq)]
pub struct DrawingShape {
    pub kind: String,
    pub start_x: f64,
    pub start_y: f64,
    pub rect: Rect,
}

/// `beginDraw`.
pub fn begin_draw(kind: &str, wx: f64, wy: f64, snap: &dyn Fn(f64) -> f64) -> DrawingShape {
    let (x, y) = (snap(wx), snap(wy));
    DrawingShape { kind: kind.to_string(), start_x: x, start_y: y, rect: Rect { x, y, w: 0.0, h: 0.0 } }
}

/// `updateDraw`: both ends snapped before the box is built.
pub fn update_draw(st: &DrawingShape, wx: f64, wy: f64, square: bool, from_centre: bool, snap: &dyn Fn(f64) -> f64) -> DrawingShape {
    let rect = kubuno_office_shapes_core::draw::draw_box_from(st.start_x, st.start_y, snap(wx), snap(wy), square, from_centre);
    DrawingShape { rect, ..st.clone() }
}

/// `finishDraw`: the drawn box, or a `default_size` box centred on a click; snapped origin, rounded size.
pub fn finish_draw(st: &DrawingShape, min_size: f64, default_size: (f64, f64), snap: &dyn Fn(f64) -> f64) -> Rect {
    let b = kubuno_office_shapes_core::draw::finalize_draw_box(&st.kind, st.rect, st.start_x, st.start_y, min_size, Some(default_size));
    Rect { x: snap(b.x), y: snap(b.y), w: js_round(b.w), h: js_round(b.h) }
}

/// `paintShapeGhost` (`shapes/draw.ts`): the real geometry, translucent; a dashed band for a kind with
/// no area.
fn paint_shape_ghost(ctx: &mut Ctx2D<'_>, kind: &str, b: Rect, line_width: f64) {
    if b.w <= 0.0 || b.h <= 0.0 {
        return;
    }
    ctx.save();
    ctx.set_fill_style("rgba(26, 115, 232, 0.12)");
    ctx.set_stroke_style("#1a73e8");
    ctx.set_line_width(line_width);
    let drawn = paint_shape_view(ctx, kind, b.x, b.y, b.w, b.h, None, true, true, line_width, None);
    if !drawn {
        ctx.set_line_dash(&[6.0 * line_width, 3.0 * line_width]);
        ctx.stroke_rect(b.x, b.y, b.w, b.h);
    }
    ctx.restore();
}

/// `paintDrawGhost`: the shape being drawn, live (canvas in world space).
pub fn paint_draw_ghost(ctx: &mut Ctx2D<'_>, st: &DrawingShape, zoom: f64) {
    let b = st.rect;
    if b.w <= 0.0 || b.h <= 0.0 {
        return;
    }
    if let Some(kind) = shared_kind_of(&st.kind) {
        paint_shape_ghost(ctx, kind, b, 1.5 / zoom);
    } else {
        ctx.save();
        ctx.set_global_alpha(0.6);
        let mut partial = Obj::new();
        partial.insert("fillColor".into(), "rgba(26,115,232,0.12)".into());
        partial.insert("strokeColor".into(), "#1a73e8".into());
        partial.insert("strokeWidth".into(), Value::from(1.5 / zoom));
        let ghost_label = LabelStyle { color: "#1a73e8".into(), ..LabelStyle::default() };
        render_shape(ctx, &st.kind, b.x, b.y, b.w, b.h, &ShapeStyle::merge(&partial), "", &ghost_label, None);
        ctx.restore();
    }
    ctx.save();
    ctx.set_stroke_style("rgba(26, 115, 232, 0.55)");
    ctx.set_line_width(1.0 / zoom);
    ctx.set_line_dash(&[4.0 / zoom, 3.0 / zoom]);
    ctx.stroke_rect(b.x, b.y, b.w, b.h);
    ctx.restore();
}

// ── The shared engine on a canvas (`shapes/canvas.ts` `paintShapeView`, `shapes/paths.ts` `paintShape`) ──

/// Paints a catalogue kind in the box with the caller's fill and stroke styles; `false` when no shared
/// geometry exists. The paths are the web's 2-decimal ones (`Path2D` of the formatted SVG data).
#[allow(clippy::too_many_arguments)]
pub fn paint_shape_view(
    ctx: &mut Ctx2D<'_>,
    kind: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    adj: Option<&[f64]>,
    fill: bool,
    stroke: bool,
    stroke_width: f64,
    solid_fill: Option<&str>,
) -> bool {
    let opts = ViewOptions { adj, stroke, stroke_width };
    let Some(subs) = kubuno_office_shapes_core::shape_view(kind, w, h, &opts) else {
        return false;
    };
    ctx.save();
    ctx.translate(x, y);
    for sp in &subs {
        let path = sp.path.rounded();
        if fill && sp.fill {
            match solid_fill {
                Some(solid) if sp.shade != 1.0 => {
                    let prev = ctx.fill_color();
                    fill_style(ctx, &shade_colour(solid, sp.shade));
                    ctx.fill_shape_path(&path);
                    ctx.set_fill_color(prev);
                }
                _ => ctx.fill_shape_path(&path),
            }
        }
        if stroke && sp.stroke {
            ctx.stroke_shape_path(&path);
        }
    }
    ctx.restore();
    true
}

// ── Shape rendering (`renderShape`) ──────────────────────────────────────────

/// `ctx.fillStyle = css`, ignored for `none` as a browser ignores a colour it does not parse.
pub fn fill_style(ctx: &mut Ctx2D<'_>, css: &str) {
    if css.trim() != "none" {
        ctx.set_fill_style(css);
    }
}

/// `ctx.strokeStyle = css`, ignored for `none`.
pub fn stroke_style(ctx: &mut Ctx2D<'_>, css: &str) {
    if css.trim() != "none" {
        ctx.set_stroke_style(css);
    }
}

/// JavaScript's `a || b` for a number: `b` when `a` is 0 (or NaN).
fn or(a: f64, b: f64) -> f64 {
    if a == 0.0 || a.is_nan() {
        b
    } else {
        a
    }
}

/// Paints one shape: its geometry and its label (`renderShape`, `stencils.ts:44-268`).
#[allow(clippy::too_many_arguments)]
pub fn render_shape(
    ctx: &mut Ctx2D<'_>,
    kind: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    style: &ShapeStyle,
    label: &str,
    label_style: &LabelStyle,
    adj: Option<&[f64]>,
) {
    ctx.save();
    if style.opacity < 100.0 {
        ctx.set_global_alpha(style.opacity / 100.0);
    }
    if style.shadow {
        ctx.set_shadow_color("rgba(0,0,0,0.25)");
        ctx.set_shadow_blur(5.0);
        ctx.set_shadow_offset(2.0, 2.0);
    }
    let fill = style.fill_color.as_str();
    let stroke = style.stroke_color.as_str();
    let lw = style.stroke_width;
    ctx.set_line_dash(ShapeStyle::dash(&style.stroke_style));
    ctx.set_line_width(lw);

    match kind {
        "rect" | "flow_process" => draw_rect(ctx, x, y, w, h, 0.0, fill, stroke),
        "rounded_rect" => draw_rect(ctx, x, y, w, h, or(style.rounded, 10.0), fill, stroke),
        "ellipse" | "flow_start" | "uml_initial" | "uml_usecase" => draw_ellipse(ctx, x, y, w, h, fill, stroke),
        "diamond" | "flow_decision" => draw_diamond(ctx, x, y, w, h, fill, stroke),
        "cylinder" | "flow_db" | "net_database" => draw_cylinder(ctx, x, y, w, h, fill, stroke),
        "parallelogram" | "flow_data" => draw_parallelogram(ctx, x, y, w, h, fill, stroke),
        "triangle" => draw_triangle(ctx, x, y, w, h, fill, stroke),
        "hexagon" | "flow_prep" => draw_hexagon(ctx, x, y, w, h, fill, stroke),
        "cloud" | "net_cloud" => draw_cloud(ctx, x, y, w, h, fill, stroke),
        "cross" => draw_cross(ctx, x, y, w, h, fill, stroke),
        "star" => draw_star(ctx, x, y, w, h, fill, stroke),
        "callout" => draw_callout(ctx, x, y, w, h, fill, stroke),
        "text" => {}
        "flow_document" => draw_document(ctx, x, y, w, h, fill, stroke),
        "flow_manual" => draw_trapezoid(ctx, x, y, w, h, fill, stroke, true),
        "trapezoid" => draw_trapezoid(ctx, x, y, w, h, fill, stroke, false),
        "net_server" | "aws_ec2" => draw_server(ctx, x, y, w, h, fill, stroke),
        "net_router" => draw_router(ctx, x, y, w, h, fill, stroke),
        "net_desktop" => draw_desktop(ctx, x, y, w, h, fill, stroke),
        "net_firewall" => draw_firewall(ctx, x, y, w, h, fill, stroke),
        "uml_class" | "uml_interface" | "uml_abstract" => {
            draw_uml_class(ctx, x, y, w, h, fill, stroke, label);
            ctx.restore();
            return;
        }
        "uml_actor" => draw_actor(ctx, x, y, w, h, fill, stroke),
        "uml_state" => draw_rect(ctx, x, y, w, h, 20.0, fill, stroke),
        "uml_final" => draw_final_state(ctx, x, y, w, h, fill, stroke),
        "flow_connector" => draw_ellipse(ctx, x + w / 4.0, y + h / 4.0, w / 2.0, h / 2.0, fill, stroke),
        "k8s_pod" | "k8s_service" => draw_hexagon(ctx, x, y, w, h, fill, stroke),
        "aws_vpc" | "uml_system" => {
            stroke_style(ctx, stroke);
            ctx.set_line_width(lw);
            ctx.set_line_dash(&[6.0, 4.0]);
            ctx.stroke_rect(x, y, w, h);
            ctx.set_line_dash(&[]);
        }
        // Basic shapes (extended)
        "shp_pentagon" => draw_regular_polygon(ctx, x, y, w, h, 5, -90.0, fill, stroke),
        "shp_octagon" => draw_regular_polygon(ctx, x, y, w, h, 8, 22.5, fill, stroke),
        "shp_arrow_right" => draw_block_arrow(ctx, x, y, w, h, Dir::Right, fill, stroke),
        "shp_arrow_left" => draw_block_arrow(ctx, x, y, w, h, Dir::Left, fill, stroke),
        "shp_arrow_up" => draw_block_arrow(ctx, x, y, w, h, Dir::Up, fill, stroke),
        "shp_arrow_down" => draw_block_arrow(ctx, x, y, w, h, Dir::Down, fill, stroke),
        "shp_cube" | "uml_node" => draw_cube(ctx, x, y, w, h, fill, stroke),
        "shp_step" => draw_step(ctx, x, y, w, h, fill, stroke),
        "shp_card" | "flow_card" => draw_card(ctx, x, y, w, h, fill, stroke),
        "shp_note" | "uml_note" => draw_note(ctx, x, y, w, h, fill, stroke),
        // Flowchart (extended)
        "flow_terminator" | "bpmn_task" => draw_rect(ctx, x, y, w, h, (h / 2.0).min(or(style.rounded, 999.0)), fill, stroke),
        "flow_predefined" => draw_predefined_process(ctx, x, y, w, h, fill, stroke),
        "flow_internal_storage" => draw_internal_storage(ctx, x, y, w, h, fill, stroke),
        "flow_display" => draw_display(ctx, x, y, w, h, fill, stroke),
        "flow_delay" => draw_delay(ctx, x, y, w, h, fill, stroke),
        "flow_offpage" => draw_off_page(ctx, x, y, w, h, fill, stroke),
        "flow_or" => draw_op_sign(ctx, x, y, w, h, fill, stroke, true),
        "flow_summing" => draw_op_sign(ctx, x, y, w, h, fill, stroke, false),
        "flow_manual_op" => draw_trapezoid(ctx, x, y, w, h, fill, stroke, true),
        // Entity-relation
        "er_entity" => draw_rect(ctx, x, y, w, h, 0.0, fill, stroke),
        "er_weak_entity" => draw_double_rect(ctx, x, y, w, h, fill, stroke),
        "er_attribute" | "er_key_attribute" => draw_ellipse(ctx, x, y, w, h, fill, stroke),
        "er_relationship" | "bpmn_gateway" => draw_diamond(ctx, x, y, w, h, fill, stroke),
        // BPMN
        "bpmn_start" => draw_ellipse(ctx, x, y, w, h, fill, stroke),
        "bpmn_end" | "bpmn_event" => draw_double_ellipse(ctx, x, y, w, h, fill, stroke),
        // UML (extended)
        "uml_package" => draw_uml_package(ctx, x, y, w, h, fill, stroke),
        "uml_component" => draw_uml_component(ctx, x, y, w, h, fill, stroke),
        "uml_object" => draw_rect(ctx, x, y, w, h, 0.0, fill, stroke),
        // Network (extended)
        "net_user" => draw_person(ctx, x, y, w, h, fill, stroke),
        "net_laptop" => draw_laptop(ctx, x, y, w, h, fill, stroke),
        "net_mobile" => draw_mobile(ctx, x, y, w, h, fill, stroke),
        "net_switch" => draw_switch(ctx, x, y, w, h, fill, stroke),
        "net_printer" => draw_printer(ctx, x, y, w, h, fill, stroke),
        "net_wifi" => draw_wifi(ctx, x, y, w, h, fill, stroke),
        "net_internet" => draw_cloud(ctx, x, y, w, h, fill, stroke),
        // UI mock-ups
        "ui_button" => draw_rect(ctx, x, y, w, h, 6.0, fill, stroke),
        "ui_input" => draw_input_field(ctx, x, y, w, h, fill, stroke),
        "ui_checkbox" => draw_checkbox(ctx, x, y, w, h, fill, stroke),
        "ui_radio" => draw_radio(ctx, x, y, w, h, fill, stroke),
        "ui_dropdown" => draw_dropdown_field(ctx, x, y, w, h, fill, stroke),
        "ui_browser" => draw_browser(ctx, x, y, w, h, fill, stroke),
        "ui_image" => draw_image_placeholder(ctx, x, y, w, h, fill, stroke),
        // Containers / swimlanes (own title, no generic label)
        "container" | "swimlane_v" => {
            draw_swimlane(ctx, x, y, w, h, fill, stroke, true, label, label_style);
            ctx.restore();
            return;
        }
        "swimlane_h" => {
            draw_swimlane(ctx, x, y, w, h, fill, stroke, false, label, label_style);
            ctx.restore();
            return;
        }
        _ => {
            if kind.starts_with("hw_") {
                // QUIRK: an unknown `hw_` id draws nothing (the web's image never loads).
                hardware::draw_hw_image(ctx, kind, x, y, w, h);
            } else if let Some(shared) = shared_kind_of(kind) {
                // The office catalogue's shapes, painted by the shared engine (adjustments included).
                fill_style(ctx, fill);
                stroke_style(ctx, stroke);
                paint_shape_view(ctx, shared, x, y, w, h, adj, fill != "none", stroke != "none" && lw > 0.0, lw, (fill != "none").then_some(fill));
            } else {
                draw_rect(ctx, x, y, w, h, style.rounded, fill, stroke);
            }
        }
    }

    ctx.set_shadow_color("transparent");
    ctx.set_shadow_blur(0.0);
    ctx.set_line_dash(&[]);

    // The label (the UML class draws its own text).
    if !label.is_empty() && !matches!(kind, "uml_class" | "uml_interface" | "uml_abstract") {
        draw_label(ctx, label, x, y, w, h, label_style);
    }
    ctx.restore();
}

// ── Primitives ───────────────────────────────────────────────────────────────

/// `fs`: fill and stroke the current path, skipping a `none` side.
fn fs(ctx: &mut Ctx2D<'_>, fill: &str, stroke: &str) {
    if fill != "none" {
        fill_style(ctx, fill);
        ctx.fill();
    }
    if stroke != "none" {
        stroke_style(ctx, stroke);
        ctx.stroke();
    }
}

/// `polyPath`: a closed polygon.
fn poly_path(ctx: &mut Ctx2D<'_>, pts: &[(f64, f64)]) {
    ctx.begin_path();
    for (i, &(px, py)) in pts.iter().enumerate() {
        if i > 0 {
            ctx.line_to(px, py);
        } else {
            ctx.move_to(px, py);
        }
    }
    ctx.close_path();
}

#[allow(clippy::too_many_arguments)]
fn draw_rect(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, r: f64, fill: &str, stroke: &str) {
    ctx.begin_path();
    if r > 0.0 {
        ctx.round_rect(x, y, w, h, r);
    } else {
        ctx.rect(x, y, w, h);
    }
    fs(ctx, fill, stroke);
}

fn draw_ellipse(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    ctx.begin_path();
    ctx.ellipse(x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0, 0.0, 0.0, PI * 2.0, false);
    fs(ctx, fill, stroke);
}

fn draw_diamond(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    ctx.begin_path();
    ctx.move_to(x + w / 2.0, y);
    ctx.line_to(x + w, y + h / 2.0);
    ctx.line_to(x + w / 2.0, y + h);
    ctx.line_to(x, y + h / 2.0);
    ctx.close_path();
    fs(ctx, fill, stroke);
}

fn draw_cylinder(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let ry = (h * 0.15).min(15.0);
    ctx.begin_path();
    ctx.ellipse(x + w / 2.0, y + ry, w / 2.0, ry, 0.0, 0.0, PI * 2.0, false);
    fs(ctx, fill, stroke);
    ctx.begin_path();
    ctx.move_to(x, y + ry);
    ctx.line_to(x, y + h - ry);
    // QUIRK: the half ellipse starts at angle 0 (the right end), so a line crosses the bottom first.
    ctx.ellipse(x + w / 2.0, y + h - ry, w / 2.0, ry, 0.0, 0.0, PI, false);
    ctx.line_to(x + w, y + ry);
    fs(ctx, fill, stroke);
}

fn draw_parallelogram(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let s = w * 0.15;
    ctx.begin_path();
    ctx.move_to(x + s, y);
    ctx.line_to(x + w, y);
    ctx.line_to(x + w - s, y + h);
    ctx.line_to(x, y + h);
    ctx.close_path();
    fs(ctx, fill, stroke);
}

fn draw_triangle(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    ctx.begin_path();
    ctx.move_to(x + w / 2.0, y);
    ctx.line_to(x + w, y + h);
    ctx.line_to(x, y + h);
    ctx.close_path();
    fs(ctx, fill, stroke);
}

fn draw_hexagon(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let s = w * 0.25;
    ctx.begin_path();
    ctx.move_to(x + s, y);
    ctx.line_to(x + w - s, y);
    ctx.line_to(x + w, y + h / 2.0);
    ctx.line_to(x + w - s, y + h);
    ctx.line_to(x + s, y + h);
    ctx.line_to(x, y + h / 2.0);
    ctx.close_path();
    fs(ctx, fill, stroke);
}

fn draw_cloud(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    ctx.begin_path();
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    // QUIRK: five full circles in one path, joined by lines (no move between the arcs).
    ctx.arc(cx - w * 0.2, cy + h * 0.1, h * 0.28, 0.0, PI * 2.0, false);
    ctx.arc(cx, cy - h * 0.05, h * 0.32, 0.0, PI * 2.0, false);
    ctx.arc(cx + w * 0.2, cy + h * 0.05, h * 0.25, 0.0, PI * 2.0, false);
    ctx.arc(cx - w * 0.3, cy + h * 0.15, h * 0.2, 0.0, PI * 2.0, false);
    ctx.arc(cx + w * 0.32, cy + h * 0.18, h * 0.22, 0.0, PI * 2.0, false);
    fs(ctx, fill, stroke);
}

fn draw_cross(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let (a, b) = (w * 0.3, h * 0.3);
    ctx.begin_path();
    ctx.move_to(x + a, y);
    ctx.line_to(x + w - a, y);
    ctx.line_to(x + w - a, y + b);
    ctx.line_to(x + w, y + b);
    ctx.line_to(x + w, y + h - b);
    ctx.line_to(x + w - a, y + h - b);
    ctx.line_to(x + w - a, y + h);
    ctx.line_to(x + a, y + h);
    ctx.line_to(x + a, y + h - b);
    ctx.line_to(x, y + h - b);
    ctx.line_to(x, y + b);
    ctx.line_to(x + a, y + b);
    ctx.close_path();
    fs(ctx, fill, stroke);
}

fn draw_star(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let ro = w.min(h) / 2.0;
    let ri = ro * 0.4;
    let n = 5;
    ctx.begin_path();
    for i in 0..n * 2 {
        let r = if i % 2 == 0 { ro } else { ri };
        let a = (i as f64 * PI) / n as f64 - PI / 2.0;
        if i == 0 {
            ctx.move_to(cx + r * a.cos(), cy + r * a.sin());
        } else {
            ctx.line_to(cx + r * a.cos(), cy + r * a.sin());
        }
    }
    ctx.close_path();
    fs(ctx, fill, stroke);
}

fn draw_callout(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let bh = h * 0.8;
    ctx.begin_path();
    ctx.round_rect(x, y, w, bh, 6.0);
    fs(ctx, fill, stroke);
    ctx.begin_path();
    ctx.move_to(x + w * 0.2, y + bh);
    ctx.line_to(x + w * 0.12, y + h);
    ctx.line_to(x + w * 0.32, y + bh);
    ctx.close_path();
    fs(ctx, fill, stroke);
}

fn draw_document(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let wave = h * 0.12;
    ctx.begin_path();
    ctx.move_to(x, y);
    ctx.line_to(x + w, y);
    ctx.line_to(x + w, y + h - wave);
    ctx.quadratic_curve_to(x + w * 0.75, y + h - wave * 0.5, x + w * 0.5, y + h - wave);
    ctx.quadratic_curve_to(x + w * 0.25, y + h - wave * 1.5, x, y + h - wave);
    ctx.close_path();
    fs(ctx, fill, stroke);
}

#[allow(clippy::too_many_arguments)]
fn draw_trapezoid(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str, inverted: bool) {
    let s = w * 0.15;
    ctx.begin_path();
    if inverted {
        ctx.move_to(x + s, y);
        ctx.line_to(x + w - s, y);
        ctx.line_to(x + w, y + h);
        ctx.line_to(x, y + h);
    } else {
        ctx.move_to(x, y);
        ctx.line_to(x + w, y);
        ctx.line_to(x + w - s, y + h);
        ctx.line_to(x + s, y + h);
    }
    ctx.close_path();
    fs(ctx, fill, stroke);
}

fn draw_server(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let rows = 3;
    let rh = h / (rows as f64 + 0.5);
    for i in 0..rows {
        let ry = y + i as f64 * rh + rh * 0.1;
        ctx.begin_path();
        ctx.rect(x, ry, w, rh * 0.8);
        // QUIRK: unguarded — a `none` fill or stroke keeps the previous style.
        fill_style(ctx, fill);
        ctx.fill();
        stroke_style(ctx, stroke);
        ctx.stroke();
        ctx.begin_path();
        ctx.arc(x + w - 12.0, ry + rh * 0.4, 3.0, 0.0, PI * 2.0, false);
        ctx.set_fill_style(if i == 0 { "#34a853" } else { "#9aa0a6" });
        ctx.fill();
    }
}

fn draw_router(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    draw_ellipse(ctx, x + w * 0.1, y + h * 0.2, w * 0.8, h * 0.5, fill, stroke);
    stroke_style(ctx, stroke);
    ctx.set_line_width(1.5);
    for (x1, y1, x2, y2) in [(0.2, 0.75, 0.1, 1.0), (0.5, 0.75, 0.5, 1.0), (0.8, 0.75, 0.9, 1.0)] {
        ctx.begin_path();
        ctx.move_to(x + w * x1, y + h * y1);
        ctx.line_to(x + w * x2, y + h * y2);
        ctx.stroke();
    }
}

fn draw_desktop(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let mh = h * 0.65;
    draw_rect(ctx, x, y, w, mh, 4.0, fill, stroke);
    ctx.set_fill_style("#aecbfa");
    ctx.fill_rect(x + 4.0, y + 4.0, w - 8.0, mh - 8.0);
    ctx.begin_path();
    ctx.move_to(x + w * 0.4, y + mh);
    ctx.line_to(x + w * 0.35, y + h);
    ctx.move_to(x + w * 0.6, y + mh);
    ctx.line_to(x + w * 0.65, y + h);
    ctx.move_to(x + w * 0.25, y + h);
    ctx.line_to(x + w * 0.75, y + h);
    stroke_style(ctx, stroke);
    ctx.stroke();
}

fn draw_firewall(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    draw_rect(ctx, x, y, w, h, 0.0, fill, stroke);
    ctx.set_stroke_style("#d93025");
    ctx.set_line_width(2.0);
    for xr in [0.25, 0.5, 0.75] {
        ctx.begin_path();
        ctx.move_to(x + w * xr, y + 4.0);
        ctx.line_to(x + w * xr, y + h - 4.0);
        ctx.stroke();
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_uml_class(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str, label: &str) {
    let hh = (h * 0.3).min(28.0);
    draw_rect(ctx, x, y, w, hh, 0.0, fill, stroke);
    ctx.set_fill_style("#202124");
    ctx.set_font("bold 11px Outfit, Arial");
    ctx.set_text_align(TextAlign::Center);
    ctx.set_text_baseline(TextBaseline::Middle);
    // QUIRK: every class stencil (class, interface, abstract) is titled « «interface» ».
    ctx.fill_text("«interface»", x + w / 2.0, y + hh / 2.0 - 6.0);
    ctx.fill_text(if label.is_empty() { "ClassName" } else { label }, x + w / 2.0, y + hh / 2.0 + 6.0);
    let body = if fill == "none" { "#f8f9fa" } else { fill };
    draw_rect(ctx, x, y + hh, w, (h - hh) / 2.0, 0.0, body, stroke);
    draw_rect(ctx, x, y + hh + (h - hh) / 2.0, w, (h - hh) / 2.0, 0.0, body, stroke);
}

fn draw_actor(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let (hw, hh) = (w * 0.35, h * 0.22);
    stroke_style(ctx, stroke);
    ctx.set_line_width(1.5);
    fill_style(ctx, fill);
    ctx.begin_path();
    ctx.ellipse(x + w / 2.0, y + hh * 0.6, hw * 0.45, hh * 0.55, 0.0, 0.0, PI * 2.0, false);
    ctx.fill();
    ctx.stroke();
    ctx.begin_path();
    ctx.move_to(x + w / 2.0, y + hh * 1.2);
    ctx.line_to(x + w / 2.0, y + h * 0.68);
    ctx.move_to(x + w / 2.0 - hw, y + h * 0.45);
    ctx.line_to(x + w / 2.0 + hw, y + h * 0.45);
    ctx.move_to(x + w / 2.0, y + h * 0.68);
    ctx.line_to(x + w / 2.0 - hw * 0.8, y + h);
    ctx.move_to(x + w / 2.0, y + h * 0.68);
    ctx.line_to(x + w / 2.0 + hw * 0.8, y + h);
    ctx.stroke();
}

fn draw_final_state(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, _fill: &str, stroke: &str) {
    let r = w.min(h) / 2.0;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    ctx.begin_path();
    ctx.arc(cx, cy, r, 0.0, PI * 2.0, false);
    fill_style(ctx, stroke);
    ctx.fill();
    ctx.begin_path();
    ctx.arc(cx, cy, r * 0.6, 0.0, PI * 2.0, false);
    ctx.set_fill_style("#ffffff");
    ctx.fill();
}

#[allow(clippy::too_many_arguments)]
fn draw_regular_polygon(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, n: usize, rot_deg: f64, fill: &str, stroke: &str) {
    let (cx, cy, rx, ry) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
    let rot = rot_deg * PI / 180.0;
    ctx.begin_path();
    for i in 0..n {
        let a = rot + (i as f64 * 2.0 * PI) / n as f64;
        let (px, py) = (cx + rx * a.cos(), cy + ry * a.sin());
        if i > 0 {
            ctx.line_to(px, py);
        } else {
            ctx.move_to(px, py);
        }
    }
    ctx.close_path();
    fs(ctx, fill, stroke);
}

#[derive(Clone, Copy, PartialEq)]
enum Dir {
    Right,
    Left,
    Up,
    Down,
}

#[allow(clippy::too_many_arguments)]
fn draw_block_arrow(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, dir: Dir, fill: &str, stroke: &str) {
    let (t, head) = (0.35, 0.45);
    let pts: Vec<(f64, f64)> = if dir == Dir::Right || dir == Dir::Left {
        let hx = x + w * if dir == Dir::Right { 1.0 - head } else { head };
        let (ty0, ty1) = (y + h * (0.5 - t / 2.0), y + h * (0.5 + t / 2.0));
        if dir == Dir::Right {
            vec![(x, ty0), (hx, ty0), (hx, y), (x + w, y + h / 2.0), (hx, y + h), (hx, ty1), (x, ty1)]
        } else {
            vec![(x + w, ty0), (hx, ty0), (hx, y), (x, y + h / 2.0), (hx, y + h), (hx, ty1), (x + w, ty1)]
        }
    } else {
        let hy = y + h * if dir == Dir::Down { 1.0 - head } else { head };
        let (tx0, tx1) = (x + w * (0.5 - t / 2.0), x + w * (0.5 + t / 2.0));
        if dir == Dir::Down {
            vec![(tx0, y), (tx0, hy), (x, hy), (x + w / 2.0, y + h), (x + w, hy), (tx1, hy), (tx1, y)]
        } else {
            vec![(tx0, y + h), (tx0, hy), (x, hy), (x + w / 2.0, y), (x + w, hy), (tx1, hy), (tx1, y + h)]
        }
    };
    poly_path(ctx, &pts);
    fs(ctx, fill, stroke);
}

fn draw_cube(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let d = w.min(h) * 0.22;
    poly_path(ctx, &[(x, y + d), (x + w - d, y + d), (x + w - d, y + h), (x, y + h)]);
    fs(ctx, fill, stroke);
    poly_path(ctx, &[(x, y + d), (x + d, y), (x + w, y), (x + w - d, y + d)]);
    fs(ctx, fill, stroke);
    poly_path(ctx, &[(x + w - d, y + d), (x + w, y), (x + w, y + h - d), (x + w - d, y + h)]);
    fs(ctx, fill, stroke);
}

fn draw_step(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let s = h * 0.5;
    poly_path(ctx, &[(x, y), (x + w - s, y), (x + w, y + h / 2.0), (x + w - s, y + h), (x, y + h), (x + s, y + h / 2.0)]);
    fs(ctx, fill, stroke);
}

fn draw_card(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let c = w.min(h) * 0.25;
    poly_path(ctx, &[(x + c, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y + c)]);
    fs(ctx, fill, stroke);
}

fn draw_note(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let c = w.min(h) * 0.25;
    poly_path(ctx, &[(x, y), (x + w - c, y), (x + w, y + c), (x + w, y + h), (x, y + h)]);
    fs(ctx, fill, stroke);
    // The folded corner.
    poly_path(ctx, &[(x + w - c, y), (x + w - c, y + c), (x + w, y + c)]);
    fs(ctx, "none", stroke);
}

fn draw_predefined_process(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    draw_rect(ctx, x, y, w, h, 0.0, fill, stroke);
    let i = w * 0.1;
    ctx.begin_path();
    ctx.move_to(x + i, y);
    ctx.line_to(x + i, y + h);
    ctx.move_to(x + w - i, y);
    ctx.line_to(x + w - i, y + h);
    if stroke != "none" {
        stroke_style(ctx, stroke);
        ctx.stroke();
    }
}

fn draw_double_rect(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    draw_rect(ctx, x, y, w, h, 0.0, fill, stroke);
    draw_rect(ctx, x + 4.0, y + 4.0, w - 8.0, h - 8.0, 0.0, "none", stroke);
}

fn draw_internal_storage(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    draw_rect(ctx, x, y, w, h, 0.0, fill, stroke);
    ctx.begin_path();
    ctx.move_to(x + w * 0.18, y);
    ctx.line_to(x + w * 0.18, y + h);
    ctx.move_to(x, y + h * 0.25);
    ctx.line_to(x + w, y + h * 0.25);
    if stroke != "none" {
        stroke_style(ctx, stroke);
        ctx.stroke();
    }
}

fn draw_display(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    ctx.begin_path();
    ctx.move_to(x, y + h / 2.0);
    ctx.line_to(x + w * 0.18, y);
    ctx.line_to(x + w * 0.8, y);
    ctx.quadratic_curve_to(x + w, y, x + w, y + h / 2.0);
    ctx.quadratic_curve_to(x + w, y + h, x + w * 0.8, y + h);
    ctx.line_to(x + w * 0.18, y + h);
    ctx.close_path();
    fs(ctx, fill, stroke);
}

fn draw_delay(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    ctx.begin_path();
    ctx.move_to(x, y);
    ctx.line_to(x + w * 0.6, y);
    ctx.quadratic_curve_to(x + w, y, x + w, y + h / 2.0);
    ctx.quadratic_curve_to(x + w, y + h, x + w * 0.6, y + h);
    ctx.line_to(x, y + h);
    ctx.close_path();
    fs(ctx, fill, stroke);
}

fn draw_off_page(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    poly_path(ctx, &[(x, y), (x + w, y), (x + w, y + h * 0.6), (x + w / 2.0, y + h), (x, y + h * 0.6)]);
    fs(ctx, fill, stroke);
}

#[allow(clippy::too_many_arguments)]
fn draw_op_sign(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str, or_sign: bool) {
    draw_ellipse(ctx, x, y, w, h, fill, stroke);
    let (cx, cy, r) = (x + w / 2.0, y + h / 2.0, w.min(h) / 2.0);
    stroke_style(ctx, stroke);
    ctx.begin_path();
    if or_sign {
        ctx.move_to(cx - r, cy);
        ctx.line_to(cx + r, cy);
        ctx.move_to(cx, cy - r);
        ctx.line_to(cx, cy + r);
    } else {
        let d = r * 0.7;
        ctx.move_to(cx - d, cy - d);
        ctx.line_to(cx + d, cy + d);
        ctx.move_to(cx + d, cy - d);
        ctx.line_to(cx - d, cy + d);
    }
    ctx.stroke();
}

fn draw_double_ellipse(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    draw_ellipse(ctx, x, y, w, h, fill, stroke);
    draw_ellipse(ctx, x + 4.0, y + 4.0, w - 8.0, h - 8.0, "none", stroke);
}

fn draw_uml_package(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let (th, tw) = ((h * 0.22).min(20.0), w * 0.4);
    draw_rect(ctx, x, y, tw, th, 0.0, fill, stroke);
    draw_rect(ctx, x, y + th, w, h - th, 0.0, fill, stroke);
}

fn draw_uml_component(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    draw_rect(ctx, x + 8.0, y, w - 8.0, h, 0.0, fill, stroke);
    draw_rect(ctx, x, y + h * 0.2, 16.0, h * 0.2, 0.0, fill, stroke);
    draw_rect(ctx, x, y + h * 0.6, 16.0, h * 0.2, 0.0, fill, stroke);
}

fn draw_person(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let cx = x + w / 2.0;
    ctx.begin_path();
    ctx.arc(cx, y + h * 0.28, w.min(h) * 0.22, 0.0, PI * 2.0, false);
    fs(ctx, fill, stroke);
    ctx.begin_path();
    ctx.move_to(x + w * 0.15, y + h);
    ctx.quadratic_curve_to(x + w * 0.15, y + h * 0.55, cx, y + h * 0.55);
    ctx.quadratic_curve_to(x + w * 0.85, y + h * 0.55, x + w * 0.85, y + h);
    ctx.close_path();
    fs(ctx, fill, stroke);
}

fn draw_laptop(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    let sh = h * 0.7;
    draw_rect(ctx, x + w * 0.1, y, w * 0.8, sh, 3.0, fill, stroke);
    ctx.set_fill_style("#aecbfa");
    ctx.fill_rect(x + w * 0.1 + 3.0, y + 3.0, w * 0.8 - 6.0, sh - 6.0);
    poly_path(ctx, &[(x, y + h), (x + w * 0.1, y + sh), (x + w * 0.9, y + sh), (x + w, y + h)]);
    fs(ctx, fill, stroke);
}

fn draw_mobile(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    draw_rect(ctx, x + w * 0.25, y, w * 0.5, h, 6.0, fill, stroke);
    ctx.set_fill_style("#aecbfa");
    ctx.fill_rect(x + w * 0.28, y + h * 0.1, w * 0.44, h * 0.72);
    ctx.begin_path();
    ctx.arc(x + w / 2.0, y + h * 0.9, 2.5, 0.0, PI * 2.0, false);
    fill_style(ctx, stroke);
    ctx.fill();
}

fn draw_switch(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    draw_rect(ctx, x, y, w, h, 4.0, fill, stroke);
    stroke_style(ctx, stroke);
    for i in 0..4 {
        let ax = x + w * (0.2 + i as f64 * 0.2);
        ctx.begin_path();
        ctx.move_to(ax, y + h * 0.35);
        ctx.line_to(ax + w * 0.08, y + h * 0.35);
        ctx.move_to(ax, y + h * 0.65);
        ctx.line_to(ax + w * 0.08, y + h * 0.65);
        ctx.stroke();
    }
}

fn draw_printer(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    draw_rect(ctx, x + w * 0.15, y, w * 0.7, h * 0.3, 2.0, "#ffffff", stroke);
    draw_rect(ctx, x, y + h * 0.3, w, h * 0.45, 3.0, fill, stroke);
    draw_rect(ctx, x + w * 0.2, y + h * 0.6, w * 0.6, h * 0.4, 0.0, "#ffffff", stroke);
    ctx.set_fill_style("#34a853");
    ctx.begin_path();
    ctx.arc(x + w * 0.85, y + h * 0.45, 2.5, 0.0, PI * 2.0, false);
    ctx.fill();
}

fn draw_wifi(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, _fill: &str, stroke: &str) {
    let (cx, cy) = (x + w / 2.0, y + h * 0.85);
    stroke_style(ctx, stroke);
    ctx.set_line_width(2.0);
    for i in 1..=3 {
        ctx.begin_path();
        ctx.arc(cx, cy, (w.min(h) * 0.28) * i as f64, PI * 1.25, PI * 1.75, false);
        ctx.stroke();
    }
    ctx.begin_path();
    ctx.arc(cx, cy, 3.0, 0.0, PI * 2.0, false);
    fill_style(ctx, stroke);
    ctx.fill();
}

fn draw_browser(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    draw_rect(ctx, x, y, w, h, 4.0, fill, stroke);
    let bar = (h * 0.22).min(22.0);
    ctx.set_fill_style("#e8eaed");
    ctx.fill_rect(x + 1.0, y + 1.0, w - 2.0, bar);
    stroke_style(ctx, stroke);
    ctx.begin_path();
    ctx.move_to(x, y + bar);
    ctx.line_to(x + w, y + bar);
    ctx.stroke();
    for (i, c) in ["#ea4335", "#fbbc04", "#34a853"].iter().enumerate() {
        ctx.set_fill_style(c);
        ctx.begin_path();
        ctx.arc(x + 10.0 + i as f64 * 12.0, y + bar / 2.0, 3.0, 0.0, PI * 2.0, false);
        ctx.fill();
    }
}

fn draw_input_field(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, _fill: &str, stroke: &str) {
    draw_rect(ctx, x, y, w, h, 4.0, "#ffffff", stroke);
}

fn draw_checkbox(ctx: &mut Ctx2D<'_>, x: f64, y: f64, _w: f64, h: f64, _fill: &str, stroke: &str) {
    let s = h.min(18.0);
    draw_rect(ctx, x, y + (h - s) / 2.0, s, s, 3.0, "#ffffff", stroke);
    ctx.set_stroke_style("#34a853");
    ctx.set_line_width(2.0);
    ctx.begin_path();
    ctx.move_to(x + s * 0.2, y + (h - s) / 2.0 + s * 0.5);
    ctx.line_to(x + s * 0.45, y + (h - s) / 2.0 + s * 0.75);
    ctx.line_to(x + s * 0.8, y + (h - s) / 2.0 + s * 0.25);
    ctx.stroke();
}

fn draw_radio(ctx: &mut Ctx2D<'_>, x: f64, y: f64, _w: f64, h: f64, _fill: &str, stroke: &str) {
    let (s, cy) = (h.min(18.0), y + h / 2.0);
    ctx.begin_path();
    ctx.arc(x + s / 2.0, cy, s / 2.0, 0.0, PI * 2.0, false);
    ctx.set_fill_style("#ffffff");
    ctx.fill();
    stroke_style(ctx, stroke);
    ctx.stroke();
    ctx.begin_path();
    ctx.arc(x + s / 2.0, cy, s * 0.25, 0.0, PI * 2.0, false);
    fill_style(ctx, stroke);
    ctx.fill();
}

fn draw_dropdown_field(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, _fill: &str, stroke: &str) {
    draw_rect(ctx, x, y, w, h, 4.0, "#ffffff", stroke);
    fill_style(ctx, stroke);
    let (cx, cy) = (x + w - 12.0, y + h / 2.0);
    ctx.begin_path();
    ctx.move_to(cx - 4.0, cy - 2.0);
    ctx.line_to(cx + 4.0, cy - 2.0);
    ctx.line_to(cx, cy + 3.0);
    ctx.close_path();
    ctx.fill();
}

fn draw_image_placeholder(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str) {
    draw_rect(ctx, x, y, w, h, 0.0, fill, stroke);
    stroke_style(ctx, stroke);
    ctx.begin_path();
    ctx.move_to(x, y);
    ctx.line_to(x + w, y + h);
    ctx.move_to(x + w, y);
    ctx.line_to(x, y + h);
    ctx.stroke();
}

/// A container or a swimlane: a title band (`vertical`: across the top) and the label in it.
#[allow(clippy::too_many_arguments)]
fn draw_swimlane(ctx: &mut Ctx2D<'_>, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: &str, vertical: bool, label: &str, ls: &LabelStyle) {
    draw_rect(ctx, x, y, w, h, 0.0, if fill == "none" { "#ffffff" } else { fill }, stroke);
    let band: f64 = 28.0;
    ctx.save();
    ctx.set_fill_style("#eef1f5");
    if vertical {
        ctx.fill_rect(x + 0.5, y + 0.5, w - 1.0, band.min(h) - 1.0);
    } else {
        ctx.fill_rect(x + 0.5, y + 0.5, band.min(w) - 1.0, h - 1.0);
    }
    stroke_style(ctx, stroke);
    ctx.begin_path();
    if vertical {
        ctx.move_to(x, y + band);
        ctx.line_to(x + w, y + band);
    } else {
        ctx.move_to(x + band, y);
        ctx.line_to(x + band, y + h);
    }
    ctx.stroke();
    ctx.restore();
    if !label.is_empty() {
        ctx.save();
        fill_style(ctx, if ls.color.is_empty() { "#202124" } else { &ls.color });
        ctx.set_font(&format!("bold {}px Outfit, Inter, sans-serif", or(ls.font_size, 12.0)));
        ctx.set_text_align(TextAlign::Center);
        ctx.set_text_baseline(TextBaseline::Middle);
        if vertical {
            ctx.fill_text(label, x + w / 2.0, y + band / 2.0);
        } else {
            ctx.translate(x + band / 2.0, y + h / 2.0);
            ctx.rotate(-PI / 2.0);
            ctx.fill_text(label, 0.0, 0.0);
        }
        ctx.restore();
    }
}

// ── Labels and arrows ────────────────────────────────────────────────────────

/// `drawLabel`: the label word-wrapped (on spaces) at `w - 16`, aligned in the box with 8 px insets, line
/// height 1.3 × the size.
pub fn draw_label(ctx: &mut Ctx2D<'_>, label: &str, x: f64, y: f64, w: f64, h: f64, ls: &LabelStyle) {
    if label.is_empty() {
        return;
    }
    fill_style(ctx, &ls.color);
    let weight = if ls.bold { "bold " } else { "" };
    let style = if ls.italic { "italic " } else { "" };
    ctx.set_font(&format!("{style}{weight}{}px \"{}\", Arial, sans-serif", ls.font_size, ls.font_family));
    if let Some(a) = TextAlign::parse(&ls.align) {
        ctx.set_text_align(a);
    }
    ctx.set_text_baseline(match ls.vertical_align.as_str() {
        "top" => TextBaseline::Top,
        "bottom" => TextBaseline::Bottom,
        _ => TextBaseline::Middle,
    });
    let tx = match ls.align.as_str() {
        "left" => x + 8.0,
        "right" => x + w - 8.0,
        _ => x + w / 2.0,
    };
    let ty = match ls.vertical_align.as_str() {
        "top" => y + 8.0,
        "bottom" => y + h - 8.0,
        _ => y + h / 2.0,
    };
    let max_w = w - 16.0;
    let lh = ls.font_size * 1.3;
    let mut line = String::new();
    let mut cy = ty;
    // QUIRK: the lines grow downward from the anchor whatever the vertical alignment (a bottom-aligned
    // label of several lines overflows the box).
    for word in label.split(' ') {
        let test = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if ctx.measure_text(&test).width > max_w && !line.is_empty() {
            ctx.fill_text(&line, tx, cy);
            line = word.to_string();
            cy += lh;
        } else {
            line = test;
        }
    }
    ctx.fill_text(&line, tx, cy);
}

/// `drawArrow`: an arrow head at `to`, pointing from `from` (`block`, `classic`, `open`, `oval`,
/// `diamond`; anything else draws nothing).
pub fn draw_arrow(ctx: &mut Ctx2D<'_>, from: Point, to: Point, kind: &str, color: &str, lw: f64) {
    let angle = (to.y - from.y).atan2(to.x - from.x);
    let size = (lw * 4.0).max(8.0);
    fill_style(ctx, color);
    stroke_style(ctx, color);
    ctx.set_line_width(lw);
    ctx.set_line_dash(&[]);
    let p = |r: f64, a: f64| (to.x - r * a.cos(), to.y - r * a.sin());
    match kind {
        "classic" | "block" => {
            ctx.begin_path();
            ctx.move_to(to.x, to.y);
            let (ax, ay) = p(size, angle - PI / 6.0);
            ctx.line_to(ax, ay);
            let (bx, by) = p(size, angle + PI / 6.0);
            ctx.line_to(bx, by);
            ctx.close_path();
            if kind == "block" {
                ctx.fill();
            } else {
                ctx.stroke();
            }
        }
        "open" => {
            ctx.begin_path();
            let (ax, ay) = p(size, angle - PI / 6.0);
            ctx.move_to(ax, ay);
            ctx.line_to(to.x, to.y);
            let (bx, by) = p(size, angle + PI / 6.0);
            ctx.line_to(bx, by);
            ctx.stroke();
        }
        "oval" => {
            let (cx, cy) = p(size / 2.0, angle);
            ctx.begin_path();
            ctx.arc(cx, cy, size / 2.0, 0.0, PI * 2.0, false);
            ctx.fill();
        }
        "diamond" => {
            ctx.begin_path();
            ctx.move_to(to.x, to.y);
            let (ax, ay) = p(size * 0.6, angle - PI / 6.0);
            ctx.line_to(ax, ay);
            let (bx, by) = p(size, angle);
            ctx.line_to(bx, by);
            let (cx, cy) = p(size * 0.6, angle + PI / 6.0);
            ctx.line_to(cx, cy);
            ctx.close_path();
            ctx.fill();
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{Op, Recorder};
    use crate::color::parse_color;

    fn record(f: impl FnOnce(&mut Ctx2D<'_>)) -> Vec<Op> {
        let mut r = Recorder::default();
        {
            let mut c = Ctx2D::new(&mut r);
            f(&mut c);
        }
        r.ops
    }

    #[test]
    fn the_catalogue_keeps_the_web_order() {
        let all = stencils();
        assert_eq!(all[0].id, "rect");
        assert_eq!(all[1].id, "rounded_rect");
        assert_eq!(all[1].style.get("rounded"), Some(&Value::from(12)));
        assert_eq!(all[OWN.len()].id, "hw_disk_arrow2");
        assert_eq!(all[OWN.len() + 30].category, "Formes · Rectangles", "the lines have no area geometry");
        assert_eq!(all[OWN.len() + 30].id, "roundRect", "rect is a house stencil");
        assert_eq!(OWN.len(), 104);
        // No duplicate ids, and the shapes appended carry half the catalogue size.
        let mut ids: Vec<&str> = all.iter().map(|s| s.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
        assert!(stencil("star").is_some_and(|s| s.category == "Formes basiques"), "the house star wins");
        assert!(stencil("textBox").is_none());
        let star4 = stencil("star4").expect("catalogue star");
        assert_eq!((star4.default_w, star4.default_h), (100.0, 100.0));
        assert_eq!(stencil("heart").map(|s| (s.default_w, s.default_h)), Some((120.0, 90.0)));
    }

    #[test]
    fn categories_and_search_follow_the_web() {
        let cats = get_categories();
        assert_eq!(&cats[..6], ["basic", "flow", "network", "uml", "aws", "k8s"]);
        assert!(cats.contains(&"hardware".to_string()));
        assert!(cats.contains(&"shapes-rectangles".to_string()) && !cats.contains(&"shapes-traits".to_string()));
        assert_eq!(slug("Étoiles et bannières"), "-toiles-et-banni-res");
        assert_eq!(category_label("hardware"), "Ordinateur et Matériel");
        assert_eq!(get_stencils_by_category("container").len(), 3);
        let r = search_stencils("cyl", None);
        assert!(r.iter().any(|s| s.id == "cylinder"));
        let by_id = search_stencils("FLOW_DB", Some(&|s: &StencilDef| s.name.to_string()));
        assert_eq!(by_id.len(), 1);
        assert!(search_stencils("réseau", None).len() >= 13, "the category matches without a translator");
    }

    #[test]
    fn shared_kinds_are_the_catalogue_shapes_only() {
        for id in HOUSE_STENCIL_IDS {
            assert_eq!(shared_kind_of(id), None);
        }
        assert_eq!(shared_kind_of("hw_cd"), None);
        assert_eq!(shared_kind_of("flow_process"), None);
        assert_eq!(shared_kind_of("star4"), Some("star4"));
        assert_eq!(shared_kind_of("roundRect"), Some("roundRect"));
    }

    #[test]
    fn every_stencil_renders_and_draws_something() {
        let style_of = |s: &StencilDef| ShapeStyle::merge(&s.style);
        for s in stencils() {
            let ops = record(|c| render_shape(c, s.id, 10.0, 10.0, s.default_w, s.default_h, &style_of(s), "", &LabelStyle::default(), None));
            if s.id == "text" {
                assert!(ops.is_empty());
            } else {
                assert!(!ops.is_empty(), "{} draws nothing", s.id);
            }
        }
    }

    #[test]
    fn a_none_fill_is_skipped_and_the_shadow_reaches_the_fill() {
        let st = ShapeStyle { fill_color: "none".into(), shadow: true, ..ShapeStyle::default() };
        let ops = record(|c| render_shape(c, "rect", 0.0, 0.0, 10.0, 10.0, &st, "", &LabelStyle::default(), None));
        assert!(matches!(&ops[..], [Op::Stroke { shadow: Some(_), .. }]), "{ops:?}");
    }

    #[test]
    fn catalogue_shapes_are_shaded_by_sub_path() {
        let st = ShapeStyle { fill_color: "#808080".into(), ..ShapeStyle::default() };
        let ops = record(|c| render_shape(c, "cube", 0.0, 0.0, 100.0, 100.0, &st, "", &LabelStyle::default(), None));
        let fills: Vec<_> = ops.iter().filter_map(|o| if let Op::Fill { color, .. } = o { Some(*color) } else { None }).collect();
        assert!(fills.len() > 1);
        assert!(fills.iter().any(|c| *c != parse_color("#808080").expect("grey")), "a face is darker or lighter");
    }

    #[test]
    fn labels_wrap_on_spaces_and_lines_step_down() {
        let ls = LabelStyle::default();
        // Fixed metrics: 6 px a character at 12 px; max width 64 - 16 = 48 px = 8 characters.
        let ops = record(|c| draw_label(c, "aaaa bbbb cccc", 0.0, 0.0, 64.0, 40.0, &ls));
        let texts: Vec<(String, f64)> = ops.iter().filter_map(|o| if let Op::Text { text, origin, .. } = o { Some((text.clone(), origin.y)) } else { None }).collect();
        assert_eq!(texts.len(), 3);
        assert_eq!(texts[0].0, "aaaa");
        assert!((texts[1].1 - texts[0].1 - 15.6).abs() < 1e-9);
        let Op::Text { font, .. } = &ops[0] else { panic!("text") };
        assert_eq!(font.families, vec!["Inter", "Arial", "sans-serif"]);
    }

    #[test]
    fn arrows_fill_or_stroke_by_kind() {
        let at = |k: &str| record(|c| draw_arrow(c, Point::new(0.0, 0.0), Point::new(10.0, 0.0), k, "#000000", 1.5));
        assert!(matches!(at("block")[..], [Op::Fill { .. }]));
        assert!(matches!(at("classic")[..], [Op::Stroke { .. }]));
        assert!(matches!(at("open")[..], [Op::Stroke { .. }]));
        assert!(matches!(at("oval")[..], [Op::Fill { .. }]));
        assert!(matches!(at("diamond")[..], [Op::Fill { .. }]));
        assert!(at("none").is_empty());
    }

    #[test]
    fn knobs_exist_for_shared_kinds_and_follow_rotation() {
        let mut s = Shape::new("a", "roundRect", 0.0, 0.0, 200.0, 100.0, "", Obj::new(), 0.0, "default");
        let k = stencil_adjust_handles(&s);
        assert_eq!(k.len(), 1);
        assert_eq!(hit_shape_adjust(&s, k[0].x, k[0].y, 7.0), Some(0));
        let house = Shape::new("b", "rect", 0.0, 0.0, 200.0, 100.0, "", Obj::new(), 0.0, "default");
        assert!(stencil_adjust_handles(&house).is_empty());
        assert_eq!(hit_shape_adjust(&house, 0.0, 0.0, 7.0), None);
        // Turned half a turn, the knob is found mirrored through the centre.
        s.set_rotation(180.0);
        let (mx, my) = (200.0 - k[0].x, 100.0 - k[0].y);
        assert_eq!(hit_shape_adjust(&s, mx, my, 7.0), Some(0));
        assert!(shape_adjust_from_drag(&s, 0, mx, my).is_some());
        let ops = record(|c| paint_adjust_handles(c, &s, 1.0));
        assert_eq!(ops.len(), 2);
    }

    #[test]
    fn the_draw_gesture_snaps_and_clicks_drop_the_default_size() {
        let snap = |v: f64| (v / 10.0).round() * 10.0;
        let d = begin_draw("rect", 12.0, 18.0, &snap);
        assert_eq!((d.start_x, d.start_y), (10.0, 20.0));
        let d2 = update_draw(&d, 54.0, 61.0, false, false, &snap);
        assert_eq!(d2.rect, Rect { x: 10.0, y: 20.0, w: 40.0, h: 40.0 });
        assert_eq!(finish_draw(&d2, 6.0, (120.0, 60.0), &snap), Rect { x: 10.0, y: 20.0, w: 40.0, h: 40.0 });
        assert_eq!(finish_draw(&d, 6.0, (120.0, 60.0), &snap), Rect { x: -50.0, y: -10.0, w: 120.0, h: 60.0 });
        // Ghosts: a house template through render_shape, a catalogue shape through the shared engine;
        // both end with the dashed frame.
        for kind in ["flow_process", "star4"] {
            let st = DrawingShape { rect: Rect { x: 0.0, y: 0.0, w: 50.0, h: 40.0 }, ..begin_draw(kind, 0.0, 0.0, &snap) };
            let ops = record(|c| paint_draw_ghost(c, &st, 2.0));
            assert!(matches!(ops.last(), Some(Op::Stroke { stroke, .. }) if !stroke.dash.is_empty()), "{kind}");
            assert!(ops.len() >= 3, "{kind}");
        }
    }
}
