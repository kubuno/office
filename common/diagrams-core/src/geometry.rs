//! Geometry of shapes and connectors — `DiagramEditorPage.tsx:163-426`: coordinate conversion, ports,
//! the anchor a glued connector end takes, routing (straight, orthogonal elbows, Catmull-Rom curves), the
//! connector label's centre, segment intersections (line hops), hit tests, grid snapping and the 90°
//! magnet.

use crate::canvas::Point;
use crate::model::{Connector, Routing, Shape};

/// Resize handle half-size (world px), `HANDLE_R`.
pub const HANDLE_R: f64 = 5.0;
/// Port radius, `PORT_R`.
pub const PORT_R: f64 = 5.0;
pub const MIN_ZOOM: f64 = 0.1;
pub const MAX_ZOOM: f64 = 4.0;
/// How far the resize box sits outside the shape (world px at zoom 1), `HANDLE_MARGIN`.
pub const HANDLE_MARGIN: f64 = 14.0;
/// The grid step (`GRID_SIZE`).
pub const GRID_SIZE: f64 = 10.0;

/// `canvasToWorld`.
pub fn canvas_to_world(cx: f64, cy: f64, pan_x: f64, pan_y: f64, zoom: f64) -> Point {
    Point::new((cx - pan_x) / zoom, (cy - pan_y) / zoom)
}

/// `worldToCanvas`.
pub fn world_to_canvas(wx: f64, wy: f64, pan_x: f64, pan_y: f64, zoom: f64) -> Point {
    Point::new(wx * zoom + pan_x, wy * zoom + pan_y)
}

/// `getPortPoints`: top, right, bottom, left.
pub fn port_points(s: &Shape) -> [Point; 4] {
    let (x, y, w, h) = (s.x(), s.y(), s.w(), s.h());
    [Point::new(x + w / 2.0, y), Point::new(x + w, y + h / 2.0), Point::new(x + w / 2.0, y + h), Point::new(x, y + h / 2.0)]
}

/// `shapeCenter`.
pub fn shape_center(s: &Shape) -> Point {
    s.center()
}

/// `anchorToward`: the side facing `toward` (horizontal when |Δx| ≥ |Δy|).
pub fn anchor_toward(s: &Shape, toward: Point) -> Point {
    let c = s.center();
    let (dx, dy) = (toward.x - c.x, toward.y - c.y);
    let p = port_points(s);
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            p[1]
        } else {
            p[3]
        }
    } else if dy >= 0.0 {
        p[2]
    } else {
        p[0]
    }
}

/// The type of a resize handle (`nw`, `n`, … `w`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    Nw,
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
}

impl Handle {
    pub fn has_n(self) -> bool {
        matches!(self, Handle::Nw | Handle::N | Handle::Ne)
    }
    pub fn has_s(self) -> bool {
        matches!(self, Handle::Sw | Handle::S | Handle::Se)
    }
    pub fn has_e(self) -> bool {
        matches!(self, Handle::Ne | Handle::E | Handle::Se)
    }
    pub fn has_w(self) -> bool {
        matches!(self, Handle::Nw | Handle::W | Handle::Sw)
    }

    /// The CSS cursor the web shows (`nw-resize`…).
    pub fn cursor(self) -> &'static str {
        match self {
            Handle::Nw => "nw-resize",
            Handle::N => "n-resize",
            Handle::Ne => "ne-resize",
            Handle::E => "e-resize",
            Handle::Se => "se-resize",
            Handle::S => "s-resize",
            Handle::Sw => "sw-resize",
            Handle::W => "w-resize",
        }
    }
}

/// `getResizeHandles(s, margin)`.
pub fn resize_handles(s: &Shape, margin: f64) -> [(Point, Handle); 8] {
    let m = margin;
    let (x0, y0, x1, y1) = (s.x() - m, s.y() - m, s.x() + s.w() + m, s.y() + s.h() + m);
    let (cx, cy) = (s.x() + s.w() / 2.0, s.y() + s.h() / 2.0);
    [
        (Point::new(x0, y0), Handle::Nw),
        (Point::new(cx, y0), Handle::N),
        (Point::new(x1, y0), Handle::Ne),
        (Point::new(x1, cy), Handle::E),
        (Point::new(x1, y1), Handle::Se),
        (Point::new(cx, y1), Handle::S),
        (Point::new(x0, y1), Handle::Sw),
        (Point::new(x0, cy), Handle::W),
    ]
}

/// Un-rotates a world point into a rotated shape's own frame.
pub fn to_local(s: &Shape, wx: f64, wy: f64) -> Point {
    let rot = s.rotation();
    if rot == 0.0 {
        return Point::new(wx, wy);
    }
    let (cx, cy) = (s.x() + s.w() / 2.0, s.y() + s.h() / 2.0);
    let a = (-rot).to_radians();
    let (dx, dy) = (wx - cx, wy - cy);
    Point::new(cx + dx * a.cos() - dy * a.sin(), cy + dx * a.sin() + dy * a.cos())
}

/// `getShapeAt`: the topmost shape under the point (rotation honoured). Returns its index in `shapes`.
pub fn shape_at(shapes: &[&Shape], wx: f64, wy: f64) -> Option<usize> {
    (0..shapes.len()).rev().find(|&i| {
        let s = shapes[i];
        let p = to_local(s, wx, wy);
        p.x >= s.x() && p.x <= s.x() + s.w() && p.y >= s.y() && p.y <= s.y() + s.h()
    })
}

/// `getHandleAt`: a resize handle of a selected shape under the point.
pub fn handle_at<'a>(shapes: &'a [Shape], selected: &[String], wx: f64, wy: f64, margin: f64) -> Option<(&'a Shape, Handle)> {
    let r = HANDLE_R;
    for sid in selected {
        let Some(s) = shapes.iter().find(|sh| sh.id() == sid) else { continue };
        for (p, h) in resize_handles(s, margin) {
            if (wx - p.x).abs() <= r && (wy - p.y).abs() <= r {
                return Some((s, h));
            }
        }
    }
    None
}

/// `getPortAt`: a port of any (pickable) shape under the point: the shape's index and the port's.
pub fn port_at(shapes: &[&Shape], wx: f64, wy: f64) -> Option<(usize, usize, Point)> {
    let r = PORT_R + 4.0;
    for (si, s) in shapes.iter().enumerate() {
        for (i, p) in port_points(s).into_iter().enumerate() {
            if (wx - p.x).abs() <= r && (wy - p.y).abs() <= r {
                return Some((si, i, p));
            }
        }
    }
    None
}

/// `snapToGrid`: `step > 1` snaps to that grid, else rounds to the pixel.
pub fn snap_to_grid(v: f64, step: f64) -> f64 {
    if step > 1.0 {
        js_round(v / step) * step
    } else {
        js_round(v)
    }
}

/// JavaScript's `Math.round` (ties towards +∞).
pub fn js_round(v: f64) -> f64 {
    let f = v.floor();
    if v - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

/// `resolveConnectorEndpoints`: a glued end takes the anchor of the side facing the next point of the
/// path (its first/last waypoint, else the other shape's centre), recomputed every time.
pub fn connector_endpoints(conn: &Connector, shapes: &[Shape]) -> (Point, Point) {
    let find = |id: Option<&str>| id.and_then(|id| shapes.iter().find(|s| s.id() == id));
    let src = find(conn.source_id());
    let tgt = find(conn.target_id());
    let wps = conn.waypoints();
    let zero = Point::default();
    let src_toward = match wps.first() {
        Some(p) => *p,
        None => tgt.map(|t| t.center()).or(conn.target_point()).unwrap_or(zero),
    };
    let tgt_toward = match wps.last() {
        Some(p) => *p,
        None => src.map(|s| s.center()).or(conn.source_point()).unwrap_or(zero),
    };
    let from = match src {
        Some(s) => anchor_toward(s, src_toward),
        None => conn.source_point().unwrap_or(zero),
    };
    let to = match tgt {
        Some(t) => anchor_toward(t, tgt_toward),
        None => conn.target_point().unwrap_or(zero),
    };
    (from, to)
}

/// `connectorPoints`: `[from, ...waypoints, to]`.
pub fn connector_points(conn: &Connector, shapes: &[Shape]) -> Vec<Point> {
    let (from, to) = connector_endpoints(conn, shapes);
    let mut pts = vec![from];
    pts.extend(conn.waypoints());
    pts.push(to);
    pts
}

/// `orthogonalize`: axis-aligned elbows (the dominant axis first), near-duplicates dropped.
pub fn orthogonalize(base: &[Point]) -> Vec<Point> {
    if base.len() < 2 {
        return base.to_vec();
    }
    let mut out = vec![base[0]];
    for b in &base[1..] {
        let a = out[out.len() - 1];
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        if dx.abs() > 1.0 && dy.abs() > 1.0 {
            if dx.abs() >= dy.abs() {
                out.push(Point::new(b.x, a.y));
            } else {
                out.push(Point::new(a.x, b.y));
            }
        }
        out.push(*b);
    }
    // The web filters against the PREVIOUS ELEMENT of the unfiltered array (`arr[i - 1]`).
    let mut kept = Vec::with_capacity(out.len());
    for (i, p) in out.iter().enumerate() {
        if i == 0 || (p.x - out[i - 1].x).abs() > 0.01 || (p.y - out[i - 1].y).abs() > 0.01 {
            kept.push(*p);
        }
    }
    kept
}

fn catmull_rom(p0: Point, p1: Point, p2: Point, p3: Point, t: f64) -> Point {
    let (t2, t3) = (t * t, t * t * t);
    Point::new(
        0.5 * ((2.0 * p1.x) + (-p0.x + p2.x) * t + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2 + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3),
        0.5 * ((2.0 * p1.y) + (-p0.y + p2.y) * t + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2 + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3),
    )
}

/// `sampleCurve`: Catmull-Rom through the points, `per` samples a segment (straight with two points).
pub fn sample_curve(pts: &[Point], per: usize) -> Vec<Point> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for i in 0..pts.len() - 1 {
        let p0 = if i == 0 { pts[i] } else { pts[i - 1] };
        let (p1, p2) = (pts[i], pts[i + 1]);
        let p3 = *pts.get(i + 2).unwrap_or(&pts[i + 1]);
        for t in 1..=per {
            out.push(catmull_rom(p0, p1, p2, p3, t as f64 / per as f64));
        }
    }
    out
}

/// The routing applied to a control polyline.
pub fn route(base: &[Point], routing: Routing) -> Vec<Point> {
    match routing {
        Routing::Orthogonal => orthogonalize(base),
        Routing::Curved => sample_curve(base, 16),
        Routing::Straight => base.to_vec(),
    }
}

/// `displayPoints`: the polyline drawn and hit-tested.
pub fn display_points(conn: &Connector, shapes: &[Shape]) -> Vec<Point> {
    route(&connector_points(conn, shapes), conn.style().routing)
}

/// `connectorLabelCenter`: the middle of the central segment of the displayed polyline, plus the offset.
pub fn connector_label_center(conn: &Connector, shapes: &[Shape]) -> Point {
    let pts = display_points(conn, shapes);
    let off = conn.label_offset();
    if pts.len() < 2 {
        let p = pts.first().copied().unwrap_or_default();
        return Point::new(p.x + off.x, p.y + off.y);
    }
    let si = (pts.len() - 1) / 2;
    let mid = Point::new((pts[si].x + pts[si + 1].x) / 2.0, (pts[si].y + pts[si + 1].y) / 2.0);
    Point::new(mid.x + off.x, mid.y + off.y)
}

/// `segIntersect`: the strictly interior intersection of two segments.
pub fn seg_intersect(a: Point, b: Point, c: Point, d: Point) -> Option<Point> {
    let (r1x, r1y) = (b.x - a.x, b.y - a.y);
    let (r2x, r2y) = (d.x - c.x, d.y - c.y);
    let den = r1x * r2y - r1y * r2x;
    if den.abs() < 1e-9 {
        return None;
    }
    let t = ((c.x - a.x) * r2y - (c.y - a.y) * r2x) / den;
    let u = ((c.x - a.x) * r1y - (c.y - a.y) * r1x) / den;
    let eps = 1e-3;
    if t <= eps || t >= 1.0 - eps || u <= eps || u >= 1.0 - eps {
        return None;
    }
    Some(Point::new(a.x + t * r1x, a.y + t * r1y))
}

/// `distToSeg`.
pub fn dist_to_seg(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 != 0.0 { ((px - ax) * dx + (py - ay) * dy) / len2 } else { 0.0 };
    let t = t.clamp(0.0, 1.0);
    (px - (ax + t * dx)).hypot(py - (ay + t * dy))
}

/// `getConnectorAt`: the topmost connector within `tol` of the point and the segment index (of the
/// DISPLAYED polyline).
pub fn connector_at(conns: &[&Connector], shapes: &[Shape], wx: f64, wy: f64, tol: f64) -> Option<(usize, usize)> {
    for i in (0..conns.len()).rev() {
        let pts = display_points(conns[i], shapes);
        for s in 0..pts.len().saturating_sub(1) {
            if dist_to_seg(wx, wy, pts[s].x, pts[s].y, pts[s + 1].x, pts[s + 1].y) <= tol {
                return Some((i, s));
            }
        }
    }
    None
}

/// `magnetizeRightAngle`: snaps a moved node to an axis-aligned elbow when both of its portions are
/// already within `threshold_deg` of their axis.
pub fn magnetize_right_angle(w: Point, p: Point, n: Point, threshold_deg: f64) -> (Point, bool) {
    let deg = |r: f64| r.to_degrees();
    let ah1 = deg((w.y - p.y).abs().atan2((w.x - p.x).abs()));
    let av1 = deg((w.x - n.x).abs().atan2((w.y - n.y).abs()));
    let av2 = deg((w.x - p.x).abs().atan2((w.y - p.y).abs()));
    let ah2 = deg((w.y - n.y).abs().atan2((w.x - n.x).abs()));
    let ok1 = ah1 <= threshold_deg && av1 <= threshold_deg;
    let ok2 = av2 <= threshold_deg && ah2 <= threshold_deg;
    if !ok1 && !ok2 {
        return (w, false);
    }
    let e1 = Point::new(n.x, p.y);
    let e2 = Point::new(p.x, n.y);
    let target = if ok1 && ok2 {
        let d1 = (w.x - e1.x).hypot(w.y - e1.y);
        let d2 = (w.x - e2.x).hypot(w.y - e2.y);
        if d1 <= d2 {
            e1
        } else {
            e2
        }
    } else if ok1 {
        e1
    } else {
        e2
    };
    (target, true)
}

/// Container shapes carry their geometric children when moved (`isContainer`).
pub fn is_container(kind: &str) -> bool {
    matches!(kind, "container" | "swimlane_v" | "swimlane_h")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Obj;

    fn shape(id: &str, x: f64, y: f64, w: f64, h: f64) -> Shape {
        Shape::new(id, "rect", x, y, w, h, "", Obj::new(), 0.0, "default")
    }

    #[test]
    fn a_glued_end_faces_the_other_shape() {
        let shapes = vec![shape("a", 0.0, 0.0, 100.0, 50.0), shape("b", 300.0, 20.0, 100.0, 50.0)];
        let c = Connector::between("c", "a", "b", "default");
        let (from, to) = connector_endpoints(&c, &shapes);
        assert_eq!(from, Point::new(100.0, 25.0), "a's right side");
        assert_eq!(to, Point::new(300.0, 45.0), "b's left side");
        let mut c2 = c.clone();
        c2.set_waypoints(&[Point::new(50.0, 200.0)]);
        let (from, _) = connector_endpoints(&c2, &shapes);
        assert_eq!(from, Point::new(50.0, 50.0), "towards the waypoint below: the bottom side");
    }

    #[test]
    fn orthogonal_routing_inserts_elbows_on_the_dominant_axis() {
        let r = orthogonalize(&[Point::new(0.0, 0.0), Point::new(100.0, 40.0)]);
        assert_eq!(r, vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 40.0)]);
        let r = orthogonalize(&[Point::new(0.0, 0.0), Point::new(30.0, 100.0)]);
        assert_eq!(r[1], Point::new(0.0, 100.0));
        // An already straight segment gets no elbow.
        assert_eq!(orthogonalize(&[Point::new(0.0, 0.0), Point::new(100.0, 0.5)]).len(), 2);
    }

    #[test]
    fn curves_sample_sixteen_points_a_segment_and_pass_through_the_nodes() {
        let pts = [Point::new(0.0, 0.0), Point::new(50.0, 50.0), Point::new(100.0, 0.0)];
        let s = sample_curve(&pts, 16);
        assert_eq!(s.len(), 1 + 2 * 16);
        assert!((s[16].x - 50.0).abs() < 1e-9 && (s[16].y - 50.0).abs() < 1e-9);
        assert_eq!(sample_curve(&pts[..2], 16).len(), 2);
    }

    #[test]
    fn crossings_are_strictly_interior() {
        let x = seg_intersect(Point::new(0.0, 0.0), Point::new(10.0, 10.0), Point::new(0.0, 10.0), Point::new(10.0, 0.0));
        assert_eq!(x, Some(Point::new(5.0, 5.0)));
        assert_eq!(seg_intersect(Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0)), None);
    }

    #[test]
    fn hit_tests_honour_rotation_and_z_order() {
        let mut a = shape("a", 0.0, 0.0, 100.0, 20.0);
        let b = shape("b", 50.0, 0.0, 100.0, 20.0);
        let list = [&a, &b];
        assert_eq!(shape_at(&list, 60.0, 10.0), Some(1), "the later one is on top");
        a.set_rotation(90.0);
        // Rotated a quarter turn about (50, 10): now 20 wide and 100 tall.
        let list = [&a];
        assert_eq!(shape_at(&list, 50.0, 50.0), Some(0));
        assert_eq!(shape_at(&list, 5.0, 10.0), None);
    }

    #[test]
    fn the_magnet_snaps_near_right_angles_only() {
        let (p, snapped) = magnetize_right_angle(Point::new(98.0, 3.0), Point::new(0.0, 0.0), Point::new(100.0, 100.0), 15.0);
        assert!(snapped);
        assert_eq!(p, Point::new(100.0, 0.0));
        let (_, snapped) = magnetize_right_angle(Point::new(50.0, 50.0), Point::new(0.0, 0.0), Point::new(100.0, 100.0), 15.0);
        assert!(!snapped);
    }

    #[test]
    fn snapping_rounds_like_javascript() {
        assert_eq!(snap_to_grid(14.0, 10.0), 10.0);
        assert_eq!(snap_to_grid(15.0, 10.0), 20.0);
        assert_eq!(snap_to_grid(-2.5, 1.0), -2.0);
    }
}
