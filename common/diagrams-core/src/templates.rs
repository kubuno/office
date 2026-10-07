//! The template gallery (« Insertion › Modèles »), a port of `office/web/src/diagramTemplates.ts`: each
//! template builds the shapes and connectors it inserts into the current page.
//!
//! The web numbers the ids with a module-level counter (`'t' + _n++`) that keeps growing across builds;
//! a process-wide counter does the same here. Only uniqueness matters: the editor gives every inserted
//! object a fresh id anyway (`importIoData` remaps them).

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use crate::io::IoData;
use crate::model::{js_number, Connector, Obj, Shape};

/// One template: an id, its French name (as the web shows it), and what it inserts.
#[derive(Debug, Clone, Copy)]
pub struct DiagramTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub build: fn() -> IoData,
}

/// `[fill, stroke]`.
type Pal = (&'static str, &'static str);
const BLUE: Pal = ("#dae8fc", "#6c8ebf");
const GREEN: Pal = ("#d5e8d4", "#82b366");
const ORANGE: Pal = ("#ffe6cc", "#d79b00");
const YELLOW: Pal = ("#fff2cc", "#d6b656");
const PURPLE: Pal = ("#e1d5e7", "#9673a6");
const RED: Pal = ("#f8cecc", "#b85450");
const GREY: Pal = ("#f5f5f5", "#666666");

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// `sid()`.
fn sid() -> String {
    format!("t{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// `S(type, x, y, w, h, label, pal, extra)`; `rounded` is the only extra key the templates pass.
#[allow(clippy::too_many_arguments)]
fn s(kind: &str, x: f64, y: f64, w: f64, h: f64, label: &str, pal: Pal, rounded: Option<f64>) -> Shape {
    let mut style = Obj::new();
    style.insert("fillColor".into(), pal.0.into());
    style.insert("strokeColor".into(), pal.1.into());
    if let Some(r) = rounded {
        style.insert("rounded".into(), js_number(r));
    }
    let mut o = Obj::new();
    o.insert("id".into(), sid().into());
    o.insert("type".into(), kind.into());
    o.insert("x".into(), js_number(x));
    o.insert("y".into(), js_number(y));
    o.insert("w".into(), js_number(w));
    o.insert("h".into(), js_number(h));
    o.insert("label".into(), label.into());
    o.insert("style".into(), Value::Object(style));
    o.insert("labelStyle".into(), Value::Object(Obj::new()));
    o.insert("zIndex".into(), js_number(0.0));
    Shape(o)
}

/// `E(src, tgt, label = '', routing = 'orthogonal')`.
fn e(src: &Shape, tgt: &Shape, label: &str, routing: &str) -> Connector {
    let mut style = Obj::new();
    style.insert("strokeColor".into(), "#6c8ebf".into());
    style.insert("strokeWidth".into(), js_number(1.5));
    style.insert("strokeStyle".into(), "solid".into());
    style.insert("arrowStart".into(), "none".into());
    style.insert("arrowEnd".into(), "block".into());
    style.insert("routing".into(), routing.into());
    let mut o = Obj::new();
    o.insert("id".into(), sid().into());
    o.insert("sourceId".into(), src.id().into());
    o.insert("targetId".into(), tgt.id().into());
    o.insert("sourcePoint".into(), Value::Null);
    o.insert("targetPoint".into(), Value::Null);
    o.insert("waypoints".into(), Value::Array(Vec::new()));
    o.insert("label".into(), label.into());
    o.insert("style".into(), Value::Object(style));
    Connector(o)
}

fn set_arrow_end(mut c: Connector, v: &str) -> Connector {
    let mut p = Obj::new();
    p.insert("arrowEnd".into(), v.into());
    c.patch_style(&p);
    c
}

fn blank() -> IoData {
    IoData::default()
}

fn flowchart() -> IoData {
    let a = s("flow_start", 200.0, 40.0, 140.0, 50.0, "Début", GREEN, Some(30.0));
    let b = s("flow_process", 200.0, 140.0, 140.0, 60.0, "Traitement", BLUE, None);
    let c = s("flow_decision", 190.0, 250.0, 160.0, 90.0, "Condition ?", YELLOW, None);
    let d = s("flow_process", 60.0, 390.0, 140.0, 60.0, "Oui", BLUE, None);
    let f = s("flow_terminator", 360.0, 390.0, 140.0, 50.0, "Non", RED, Some(30.0));
    let connectors = vec![e(&a, &b, "", "orthogonal"), e(&b, &c, "", "orthogonal"), e(&c, &d, "oui", "orthogonal"), e(&c, &f, "non", "orthogonal")];
    IoData { shapes: vec![a, b, c, d, f], connectors }
}

fn org() -> IoData {
    let ceo = s("rounded_rect", 240.0, 40.0, 140.0, 56.0, "Direction", BLUE, Some(8.0));
    let m1 = s("rounded_rect", 90.0, 180.0, 140.0, 56.0, "Manager A", GREEN, Some(8.0));
    let m2 = s("rounded_rect", 390.0, 180.0, 140.0, 56.0, "Manager B", GREEN, Some(8.0));
    let s1 = s("rounded_rect", 20.0, 320.0, 120.0, 50.0, "Équipe 1", GREY, Some(8.0));
    let s2 = s("rounded_rect", 170.0, 320.0, 120.0, 50.0, "Équipe 2", GREY, Some(8.0));
    let s3 = s("rounded_rect", 330.0, 320.0, 120.0, 50.0, "Équipe 3", GREY, Some(8.0));
    let s4 = s("rounded_rect", 480.0, 320.0, 120.0, 50.0, "Équipe 4", GREY, Some(8.0));
    let o = "orthogonal";
    let connectors = vec![e(&ceo, &m1, "", o), e(&ceo, &m2, "", o), e(&m1, &s1, "", o), e(&m1, &s2, "", o), e(&m2, &s3, "", o), e(&m2, &s4, "", o)];
    IoData { shapes: vec![ceo, m1, m2, s1, s2, s3, s4], connectors }
}

fn mindmap() -> IoData {
    let c = s("ellipse", 240.0, 200.0, 160.0, 80.0, "Idée centrale", PURPLE, None);
    let b1 = s("ellipse", 30.0, 60.0, 130.0, 60.0, "Branche 1", BLUE, None);
    let b2 = s("ellipse", 480.0, 60.0, 130.0, 60.0, "Branche 2", GREEN, None);
    let b3 = s("ellipse", 30.0, 360.0, 130.0, 60.0, "Branche 3", ORANGE, None);
    let b4 = s("ellipse", 480.0, 360.0, 130.0, 60.0, "Branche 4", YELLOW, None);
    let edge = |t: &Shape| set_arrow_end(e(&c, t, "", "curved"), "none");
    let connectors = vec![edge(&b1), edge(&b2), edge(&b3), edge(&b4)];
    IoData { shapes: vec![c.clone(), b1, b2, b3, b4], connectors }
}

fn network() -> IoData {
    let i = s("net_internet", 220.0, 30.0, 120.0, 80.0, "Internet", BLUE, None);
    let f = s("net_firewall", 240.0, 160.0, 80.0, 80.0, "Pare-feu", RED, None);
    let sv = s("net_server", 130.0, 290.0, 100.0, 70.0, "Serveur", BLUE, None);
    let db = s("net_database", 350.0, 280.0, 80.0, 100.0, "BD", BLUE, None);
    let o = "orthogonal";
    let connectors = vec![e(&i, &f, "", o), e(&f, &sv, "", o), e(&sv, &db, "", o)];
    IoData { shapes: vec![i, f, sv, db], connectors }
}

fn aws() -> IoData {
    let u = s("net_user", 40.0, 200.0, 60.0, 80.0, "Utilisateur", BLUE, None);
    let cf = s("aws_cloudfront", 180.0, 200.0, 80.0, 80.0, "CloudFront", PURPLE, Some(8.0));
    let ec2 = s("aws_ec2", 330.0, 200.0, 80.0, 80.0, "EC2", ORANGE, None);
    let rds = s("aws_rds", 480.0, 200.0, 80.0, 80.0, "RDS", BLUE, None);
    let o = "orthogonal";
    let connectors = vec![e(&u, &cf, "", o), e(&cf, &ec2, "", o), e(&ec2, &rds, "", o)];
    IoData { shapes: vec![u, cf, ec2, rds], connectors }
}

fn uml() -> IoData {
    let a = s("uml_class", 60.0, 80.0, 160.0, 110.0, "Compte", BLUE, None);
    let b = s("uml_class", 360.0, 80.0, 160.0, 110.0, "Client", GREEN, None);
    let ed = set_arrow_end(e(&b, &a, "1..*", "orthogonal"), "open");
    IoData { shapes: vec![a, b], connectors: vec![ed] }
}

fn bpmn() -> IoData {
    let st = s("bpmn_start", 40.0, 200.0, 50.0, 50.0, "", GREEN, None);
    let t1 = s("bpmn_task", 140.0, 190.0, 120.0, 70.0, "Recevoir", BLUE, Some(10.0));
    let g = s("bpmn_gateway", 320.0, 200.0, 70.0, 70.0, "OK ?", YELLOW, None);
    let t2 = s("bpmn_task", 440.0, 190.0, 120.0, 70.0, "Traiter", BLUE, Some(10.0));
    let end = s("bpmn_end", 620.0, 200.0, 50.0, 50.0, "", RED, None);
    let o = "orthogonal";
    let connectors = vec![e(&st, &t1, "", o), e(&t1, &g, "", o), e(&g, &t2, "oui", o), e(&t2, &end, "", o)];
    IoData { shapes: vec![st, t1, g, t2, end], connectors }
}

/// `TEMPLATES`, in the web's order.
pub const TEMPLATES: &[DiagramTemplate] = &[
    DiagramTemplate { id: "blank", name: "Vierge", build: blank },
    DiagramTemplate { id: "flowchart", name: "Organigramme", build: flowchart },
    DiagramTemplate { id: "org", name: "Organigramme hiérarchique", build: org },
    DiagramTemplate { id: "mindmap", name: "Carte mentale", build: mindmap },
    DiagramTemplate { id: "network", name: "Réseau", build: network },
    DiagramTemplate { id: "aws", name: "AWS", build: aws },
    DiagramTemplate { id: "uml", name: "Classe UML", build: uml },
    DiagramTemplate { id: "bpmn", name: "BPMN", build: bpmn },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_template_builds_connected_diagrams() {
        let ids: Vec<&str> = TEMPLATES.iter().map(|t| t.id).collect();
        assert_eq!(ids, ["blank", "flowchart", "org", "mindmap", "network", "aws", "uml", "bpmn"]);
        let mut seen = HashSet::new();
        for t in TEMPLATES {
            let io = (t.build)();
            let shape_ids: HashSet<String> = io.shapes.iter().map(|s| s.id().to_string()).collect();
            assert_eq!(shape_ids.len(), io.shapes.len(), "{}", t.id);
            for c in &io.connectors {
                assert!(shape_ids.contains(c.source_id().unwrap_or("")), "{}", t.id);
                assert!(shape_ids.contains(c.target_id().unwrap_or("")), "{}", t.id);
            }
            for id in shape_ids.iter().cloned().chain(io.connectors.iter().map(|c| c.id().to_string())) {
                assert!(seen.insert(id), "ids stay unique across builds");
            }
        }
        assert!((TEMPLATES[0].build)().shapes.is_empty());
    }

    #[test]
    fn templates_carry_the_webs_details() {
        let f = flowchart();
        assert_eq!(f.shapes[0].kind(), "flow_start");
        assert_eq!(f.shapes[0].obj()["style"], serde_json::json!({ "fillColor": "#d5e8d4", "strokeColor": "#82b366", "rounded": 30 }));
        assert_eq!(f.connectors[2].label(), "oui");
        let m = mindmap();
        assert_eq!(m.connectors[0].style().arrow_end, "none");
        assert_eq!(m.connectors[0].style().routing, crate::model::Routing::Curved);
        let u = uml();
        assert_eq!(u.connectors[0].style().arrow_end, "open");
        assert_eq!(u.connectors[0].source_id(), Some(u.shapes[1].id()));
    }
}
