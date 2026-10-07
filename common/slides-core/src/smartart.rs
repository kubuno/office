//! SmartArt geometry — a port of the web's `presentationSmartArt.ts`: boxes and connectors in slide fractions,
//! inside the central region of the slide.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartArtKind {
    Process,
    List,
    Cycle,
    Hierarchy,
    Pyramid,
    Matrix,
}

impl SmartArtKind {
    pub fn parse(s: &str) -> Option<SmartArtKind> {
        Some(match s {
            "process" => SmartArtKind::Process,
            "list" => SmartArtKind::List,
            "cycle" => SmartArtKind::Cycle,
            "hierarchy" => SmartArtKind::Hierarchy,
            "pyramid" => SmartArtKind::Pyramid,
            "matrix" => SmartArtKind::Matrix,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SmartBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub shape: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SmartConn {
    pub x: f64,
    pub y: f64,
    pub x2: f64,
    pub y2: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SmartLayout {
    pub boxes: Vec<SmartBox>,
    pub connectors: Vec<SmartConn>,
    pub shape: &'static str,
}

const R: (f64, f64, f64, f64) = (0.08, 0.24, 0.84, 0.52);

fn fb(x: f64, y: f64, w: f64, h: f64) -> SmartBox {
    SmartBox { x, y, w, h, shape: None }
}

/// `smartArtLayout(kind, n)`, `n` clamped to 1..8.
pub fn smartart_layout(kind: SmartArtKind, n: usize) -> SmartLayout {
    let (rx, ry, rw, rh) = R;
    let n = n.clamp(1, 8);
    let nf = n as f64;
    let mut boxes = Vec::new();
    let mut connectors = Vec::new();
    match kind {
        SmartArtKind::Process => {
            let gap = 0.03;
            let bw = (rw - gap * (nf - 1.0)) / nf;
            let bh = 0.22f64.min(rh * 0.6);
            let cy = ry + (rh - bh) / 2.0;
            for i in 0..n {
                let x = rx + i as f64 * (bw + gap);
                boxes.push(fb(x, cy, bw, bh));
                if i > 0 {
                    let px = rx + (i as f64 - 1.0) * (bw + gap) + bw;
                    connectors.push(SmartConn { x: px, y: cy + bh / 2.0, x2: x, y2: cy + bh / 2.0 });
                }
            }
            SmartLayout { boxes, connectors, shape: "roundRect" }
        }
        SmartArtKind::List => {
            let gap = 0.03;
            let bh = (rh - gap * (nf - 1.0)) / nf;
            for i in 0..n {
                boxes.push(fb(rx, ry + i as f64 * (bh + gap), rw, bh));
            }
            SmartLayout { boxes, connectors, shape: "roundRect" }
        }
        SmartArtKind::Cycle => {
            let (cxc, cyc) = (rx + rw / 2.0, ry + rh / 2.0);
            let (rrx, rry) = (rw / 2.0 - 0.09, rh / 2.0 - 0.06);
            let (bw, bh) = (0.16, 0.12);
            let mut pts = Vec::new();
            for i in 0..n {
                let a = -std::f64::consts::FRAC_PI_2 + (i as f64 / nf) * std::f64::consts::TAU;
                let (px, py) = (cxc + rrx * a.cos(), cyc + rry * a.sin());
                boxes.push(fb(px - bw / 2.0, py - bh / 2.0, bw, bh));
                pts.push((px, py));
            }
            for i in 0..n {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                connectors.push(SmartConn { x: a.0, y: a.1, x2: b.0, y2: b.1 });
            }
            SmartLayout { boxes, connectors, shape: "ellipse" }
        }
        SmartArtKind::Hierarchy => {
            let (top_w, top_h) = (0.26, 0.14);
            let (tx, ty) = (rx + (rw - top_w) / 2.0, ry);
            boxes.push(fb(tx, ty, top_w, top_h));
            let kids = n.saturating_sub(1).max(1);
            let kf = kids as f64;
            let gap = 0.03;
            let bw = (rw - gap * (kf - 1.0)) / kf;
            let bh = 0.14;
            let ky = ry + rh - bh;
            for i in 0..kids {
                let x = rx + i as f64 * (bw + gap);
                boxes.push(fb(x, ky, bw, bh));
                connectors.push(SmartConn { x: tx + top_w / 2.0, y: ty + top_h, x2: x + bw / 2.0, y2: ky });
            }
            SmartLayout { boxes, connectors, shape: "roundRect" }
        }
        SmartArtKind::Matrix => {
            let gap = 0.02;
            let (bw, bh) = ((rw - gap) / 2.0, (rh - gap) / 2.0);
            let pos = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
            for &(cxi, ryi) in pos.iter().take(n.min(4)) {
                boxes.push(fb(rx + cxi * (bw + gap), ry + ryi * (bh + gap), bw, bh));
            }
            SmartLayout { boxes, connectors, shape: "rect" }
        }
        SmartArtKind::Pyramid => {
            let ph = rh / nf;
            for i in 0..n {
                let frac = (i as f64 + 1.0) / nf;
                let bw = rw * frac;
                boxes.push(SmartBox { x: rx + (rw - bw) / 2.0, y: ry + i as f64 * ph, w: bw, h: ph - 0.012, shape: Some("trapezoid") });
            }
            SmartLayout { boxes, connectors, shape: "trapezoid" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_chains_its_boxes() {
        let l = smartart_layout(SmartArtKind::Process, 3);
        assert_eq!((l.boxes.len(), l.connectors.len()), (3, 2));
        assert!((l.connectors[0].x - (l.boxes[0].x + l.boxes[0].w)).abs() < 1e-12);
        assert!((l.connectors[0].x2 - l.boxes[1].x).abs() < 1e-12);
    }

    #[test]
    fn counts_are_clamped_and_a_cycle_closes() {
        assert_eq!(smartart_layout(SmartArtKind::List, 20).boxes.len(), 8);
        let c = smartart_layout(SmartArtKind::Cycle, 4);
        assert_eq!(c.connectors.len(), 4);
        assert_eq!(smartart_layout(SmartArtKind::Matrix, 6).boxes.len(), 4);
        assert_eq!(smartart_layout(SmartArtKind::Hierarchy, 1).boxes.len(), 2, "one child at least");
    }
}
