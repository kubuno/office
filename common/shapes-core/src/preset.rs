//! Interpreter for LibreOffice's OOXML preset geometries — a port of the web's
//! `shapes/preset-engine.ts` over the same data (`data/presets.json`, extracted from the web's generated
//! `shapes/preset-data.ts`, itself extracted from LibreOffice's `oox-drawingml-cs-presets`).
//!
//! Everything a preset needs is data: adjustment defaults, guide EQUATIONS (`logwidth*$0 /100000`,
//! `if(?2 ,?2 ,?3 )`…), a path as typed coordinates plus segment commands, the text frame, and the
//! handles with their ranges. This module evaluates that data for a given box and adjustment values,
//! so a shape is drawn exactly as the web (and Office, and LibreOffice) draw it.
//!
//! Equation language (svx `EnhancedCustomShapeFunctionParser`): `+ - * /`, unary minus, `abs sqrt sin
//! cos tan atan2(y,x) min max`, `if(a,b,c)` = `a>0?b:c`, the constant `pi`, `logwidth`/`logheight` (the
//! box), `$N` (adjustment N, raw), `?N` (equation N). Division by zero gives 0, an unknown word 0 — the
//! web's rules, quirks included.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::path::{Cmd, Path, Rect, SubPath};

/// `[type, value]`: type 0 constant, 1 equation index, 2 adjustment index.
pub type Param = (u8, f64);

#[derive(Debug, Clone, Deserialize)]
pub struct Handle {
    pub p: (Param, Param),
    #[serde(default)]
    pub rx: Option<f64>,
    #[serde(default)]
    pub ry: Option<f64>,
    #[serde(default)]
    pub ra: Option<f64>,
    #[serde(default)]
    pub rr: Option<f64>,
    #[serde(default)]
    pub xm: Option<Param>,
    #[serde(default, rename = "xM")]
    pub x_max: Option<Param>,
    #[serde(default)]
    pub ym: Option<Param>,
    #[serde(default, rename = "yM")]
    pub y_max: Option<Param>,
    #[serde(default)]
    pub rm: Option<Param>,
    #[serde(default, rename = "rM")]
    pub r_max: Option<Param>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawPreset {
    a: Vec<(String, f64)>,
    e: Vec<String>,
    c: Vec<(Param, Param)>,
    s: Vec<(u32, u32)>,
    #[serde(default)]
    t: Option<((Param, Param), (Param, Param))>,
    #[serde(default)]
    h: Option<Vec<Handle>>,
    #[serde(default)]
    v: Option<Vec<(f64, f64)>>,
}

/// One preset geometry, its equations compiled.
#[derive(Debug, Clone)]
pub struct Preset {
    /// Adjustment names and defaults (raw OOXML units).
    pub adjustments: Vec<(String, f64)>,
    equations: Vec<Expr>,
    coords: Vec<(Param, Param)>,
    segments: Vec<(u32, u32)>,
    text_frame: Option<((Param, Param), (Param, Param))>,
    pub handles: Vec<Handle>,
    sub_views: Vec<(f64, f64)>,
}

static PRESETS: OnceLock<HashMap<String, Preset>> = OnceLock::new();
static KIND_TO_PRESET: OnceLock<HashMap<String, String>> = OnceLock::new();

fn presets() -> &'static HashMap<String, Preset> {
    PRESETS.get_or_init(|| {
        let raw: HashMap<String, RawPreset> = serde_json::from_str(include_str!("../data/presets.json")).unwrap_or_default();
        raw.into_iter()
            .map(|(name, r)| {
                let equations = r.e.iter().map(|src| Parser::new(src).parse()).collect();
                (
                    name,
                    Preset {
                        adjustments: r.a,
                        equations,
                        coords: r.c,
                        segments: r.s,
                        text_frame: r.t,
                        handles: r.h.unwrap_or_default(),
                        sub_views: r.v.unwrap_or_default(),
                    },
                )
            })
            .collect()
    })
}

fn kind_map() -> &'static HashMap<String, String> {
    KIND_TO_PRESET.get_or_init(|| serde_json::from_str(include_str!("../data/kind-to-preset.json")).unwrap_or_default())
}

/// The preset backing a catalogue kind, or `None` (lines and connectors stay bespoke).
pub fn preset_of(kind: &str) -> Option<&'static Preset> {
    let name = kind_map().get(kind)?;
    presets().get(name)
}

/// The number of presets loaded (for the tests: the data must parse).
pub fn preset_count() -> usize {
    presets().len()
}

// ── Equations ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Expr {
    Num(f64),
    Adj(usize),
    Eq(usize),
    Width,
    Height,
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
}

#[derive(Debug, Clone, Copy)]
enum Func {
    Abs,
    Sqrt,
    Sin,
    Cos,
    Tan,
    Atan2,
    Min,
    Max,
    If,
    Unknown,
}

/// The web's recursive-descent parser (`evalExpr`), compiled once instead of re-read at each call.
struct Parser<'a> {
    src: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self { src: src.as_bytes(), i: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.i).copied()
    }

    fn skip(&mut self) {
        while self.peek() == Some(b' ') {
            self.i += 1;
        }
    }

    fn parse(mut self) -> Expr {
        self.sum()
    }

    fn digits(&mut self) -> usize {
        let s = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        std::str::from_utf8(&self.src[s..self.i]).ok().and_then(|t| t.parse().ok()).unwrap_or(0)
    }

    fn primary(&mut self) -> Expr {
        self.skip();
        let Some(c) = self.peek() else { return Expr::Num(0.0) };
        if c == b'(' {
            self.i += 1;
            let v = self.sum();
            self.skip();
            if self.peek() == Some(b')') {
                self.i += 1;
            }
            return v;
        }
        if c == b'-' {
            self.i += 1;
            return Expr::Neg(Box::new(self.primary()));
        }
        if c == b'$' {
            self.i += 1;
            return Expr::Adj(self.digits());
        }
        if c == b'?' {
            self.i += 1;
            return Expr::Eq(self.digits());
        }
        if c.is_ascii_digit() {
            let s = self.i;
            while self.peek().is_some_and(|c| c.is_ascii_digit() || c == b'.') {
                self.i += 1;
            }
            // `Number("1.2.3")` is NaN in the web; the data never holds one.
            let v = std::str::from_utf8(&self.src[s..self.i]).ok().and_then(|t| t.parse::<f64>().ok()).unwrap_or(f64::NAN);
            return Expr::Num(v);
        }
        let s = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_lowercase() || c == b'2') {
            self.i += 1;
        }
        let word = std::str::from_utf8(&self.src[s..self.i]).unwrap_or("");
        match word {
            "logwidth" => return Expr::Width,
            "logheight" => return Expr::Height,
            "pi" => return Expr::Num(std::f64::consts::PI),
            _ => {}
        }
        self.skip();
        if self.peek() == Some(b'(') {
            self.i += 1;
            let mut args = vec![self.sum()];
            self.skip();
            while self.peek() == Some(b',') {
                self.i += 1;
                args.push(self.sum());
                self.skip();
            }
            if self.peek() == Some(b')') {
                self.i += 1;
            }
            let f = match word {
                "abs" => Func::Abs,
                "sqrt" => Func::Sqrt,
                "sin" => Func::Sin,
                "cos" => Func::Cos,
                "tan" => Func::Tan,
                "atan2" => Func::Atan2,
                "min" => Func::Min,
                "max" => Func::Max,
                "if" => Func::If,
                _ => Func::Unknown,
            };
            return Expr::Call(f, args);
        }
        Expr::Num(0.0)
    }

    fn product(&mut self) -> Expr {
        let mut v = self.primary();
        loop {
            self.skip();
            match self.peek() {
                Some(b'*') => {
                    self.i += 1;
                    v = Expr::Mul(Box::new(v), Box::new(self.primary()));
                }
                Some(b'/') => {
                    self.i += 1;
                    v = Expr::Div(Box::new(v), Box::new(self.primary()));
                }
                _ => return v,
            }
        }
    }

    fn sum(&mut self) -> Expr {
        let mut v = self.product();
        loop {
            self.skip();
            match self.peek() {
                Some(b'+') => {
                    self.i += 1;
                    v = Expr::Add(Box::new(v), Box::new(self.product()));
                }
                Some(b'-') => {
                    self.i += 1;
                    v = Expr::Sub(Box::new(v), Box::new(self.product()));
                }
                _ => return v,
            }
        }
    }
}

/// The box and the adjustment values an evaluation reads.
#[derive(Debug, Clone)]
pub struct Env {
    pub w: f64,
    pub h: f64,
    pub adj: Vec<f64>,
}

struct Eval<'p> {
    preset: &'p Preset,
    env: &'p Env,
    memo: Vec<Option<f64>>,
    depth: u32,
}

impl<'p> Eval<'p> {
    fn new(preset: &'p Preset, env: &'p Env) -> Self {
        Self { preset, env, memo: vec![None; preset.equations.len()], depth: 0 }
    }

    fn expr(&mut self, e: &Expr) -> f64 {
        match e {
            Expr::Num(v) => *v,
            Expr::Adj(i) => self.env.adj.get(*i).copied().unwrap_or(0.0),
            Expr::Eq(i) => self.equation(*i),
            Expr::Width => self.env.w,
            Expr::Height => self.env.h,
            Expr::Neg(a) => -self.expr(a),
            Expr::Add(a, b) => self.expr(a) + self.expr(b),
            Expr::Sub(a, b) => self.expr(a) - self.expr(b),
            Expr::Mul(a, b) => self.expr(a) * self.expr(b),
            Expr::Div(a, b) => {
                let n = self.expr(a);
                let d = self.expr(b);
                if d == 0.0 {
                    0.0
                } else {
                    n / d
                }
            }
            Expr::Call(f, args) => {
                let v: Vec<f64> = args.iter().map(|a| self.expr(a)).collect();
                let a0 = v.first().copied().unwrap_or(f64::NAN);
                let a1 = v.get(1).copied().unwrap_or(f64::NAN);
                match f {
                    Func::Abs => a0.abs(),
                    Func::Sqrt => a0.max(0.0).sqrt(),
                    Func::Sin => a0.sin(),
                    Func::Cos => a0.cos(),
                    Func::Tan => a0.tan(),
                    Func::Atan2 => a0.atan2(a1),
                    // Math.min / Math.max: NaN propagates.
                    Func::Min => v.iter().copied().fold(f64::INFINITY, |m, x| if x.is_nan() || m.is_nan() { f64::NAN } else { m.min(x) }),
                    Func::Max => v.iter().copied().fold(f64::NEG_INFINITY, |m, x| if x.is_nan() || m.is_nan() { f64::NAN } else { m.max(x) }),
                    Func::If => {
                        if a0 > 0.0 {
                            a1
                        } else {
                            v.get(2).copied().unwrap_or(f64::NAN)
                        }
                    }
                    Func::Unknown => 0.0,
                }
            }
        }
    }

    fn equation(&mut self, idx: usize) -> f64 {
        if let Some(Some(hit)) = self.memo.get(idx) {
            return *hit;
        }
        if self.depth > 128 {
            return 0.0;
        }
        self.depth += 1;
        let preset: &'p Preset = self.preset;
        let v = match preset.equations.get(idx) {
            Some(e) => self.expr(e),
            None => 0.0,
        };
        self.depth -= 1;
        if let Some(slot) = self.memo.get_mut(idx) {
            *slot = Some(v);
        }
        v
    }

    fn param(&mut self, p: Param) -> f64 {
        match p.0 {
            0 => p.1,
            1 => self.equation(p.1 as usize),
            2 => self.env.adj.get(p.1 as usize).copied().unwrap_or(0.0),
            _ => p.1,
        }
    }
}

/// Adjustment defaults of a preset (raw OOXML units).
pub fn adj_defaults(preset: &Preset) -> Vec<f64> {
    preset.adjustments.iter().map(|(_, v)| *v).collect()
}

/// Adjustment values in use: the shape's own (raw units) over the defaults.
pub fn adj_values(preset: &Preset, adj: Option<&[f64]>) -> Vec<f64> {
    adj_defaults(preset)
        .into_iter()
        .enumerate()
        .map(|(i, d)| match adj.and_then(|a| a.get(i)) {
            Some(v) if v.is_finite() => *v,
            _ => d,
        })
        .collect()
}

/// Builds the sub-paths (LibreOffice's `EnhancedCustomShapeSegmentCommand`s: MOVETO 1, LINETO 2,
/// CURVETO 3, CLOSE 4, END 5, NOFILL 6, NOSTROKE 7, QUADRATICCURVETO 16, ARCANGLETO 17, and the shading
/// markers 18–21), exactly as `presetPath`.
pub fn preset_path(preset: &Preset, env: &Env) -> Vec<SubPath> {
    let mut ev = Eval::new(preset, env);
    let mut out = Vec::new();
    let mut ci = 0usize;
    let mut sub = 0usize;
    let mut d = Path::new();
    let (mut fill, mut stroke, mut shade) = (true, true, 1.0);
    let (mut cx, mut cy) = (0.0f64, 0.0f64);

    let scale = |sub: usize| -> (f64, f64) {
        if preset.sub_views.is_empty() {
            return (1.0, 1.0);
        }
        let v = preset.sub_views[sub.min(preset.sub_views.len() - 1)];
        (if v.0 > 0.0 { env.w / v.0 } else { 1.0 }, if v.1 > 0.0 { env.h / v.1 } else { 1.0 })
    };
    let point = |ev: &mut Eval, ci: &mut usize, sub: usize, cx: f64, cy: f64| -> (f64, f64) {
        let Some(pair) = preset.coords.get(*ci).copied() else { return (cx, cy) };
        *ci += 1;
        let (sx, sy) = scale(sub);
        let x = if pair.0 .0 == 0 { pair.0 .1 * sx } else { ev.param(pair.0) };
        let y = if pair.1 .0 == 0 { pair.1 .1 * sy } else { ev.param(pair.1) };
        (x, y)
    };

    for &(cmd, count) in &preset.segments {
        match cmd {
            1 => {
                for _ in 0..count {
                    let p = point(&mut ev, &mut ci, sub, cx, cy);
                    (cx, cy) = p;
                    d.cmds.push(Cmd::MoveTo(p.0, p.1));
                }
            }
            2 => {
                for _ in 0..count {
                    let p = point(&mut ev, &mut ci, sub, cx, cy);
                    (cx, cy) = p;
                    d.cmds.push(Cmd::LineTo(p.0, p.1));
                }
            }
            3 => {
                for _ in 0..count {
                    let a = point(&mut ev, &mut ci, sub, cx, cy);
                    let b = point(&mut ev, &mut ci, sub, cx, cy);
                    let e = point(&mut ev, &mut ci, sub, cx, cy);
                    (cx, cy) = e;
                    d.cmds.push(Cmd::CubicTo(a.0, a.1, b.0, b.1, e.0, e.1));
                }
            }
            16 => {
                for _ in 0..count {
                    let a = point(&mut ev, &mut ci, sub, cx, cy);
                    let e = point(&mut ev, &mut ci, sub, cx, cy);
                    (cx, cy) = e;
                    d.cmds.push(Cmd::QuadTo(a.0, a.1, e.0, e.1));
                }
            }
            17 => {
                for _ in 0..count {
                    // (wR, hR) then (stAng, swAng) — OOXML arcTo, the angles in DEGREES. The current point
                    // lies at stAng on the ellipse; sweep by swAng (parametric angles).
                    let r = point(&mut ev, &mut ci, sub, cx, cy);
                    let pair = preset.coords.get(ci).copied();
                    ci += 1;
                    let st = pair.map(|p| ev.param(p.0)).unwrap_or(0.0).to_radians();
                    let sw = pair.map(|p| ev.param(p.1)).unwrap_or(0.0).to_radians();
                    let ecx = cx - r.0 * st.cos();
                    let ecy = cy - r.1 * st.sin();
                    let sweep = sw >= 0.0;
                    // A full-turn sweep degenerates as one SVG arc: two halves (any sweep > 180°).
                    let steps = if sw.abs() > std::f64::consts::PI { 2 } else { 1 };
                    for part in 1..=steps {
                        let a2 = st + (sw * part as f64) / steps as f64;
                        let ex = ecx + r.0 * a2.cos();
                        let ey = ecy + r.1 * a2.sin();
                        d.cmds.push(Cmd::ArcTo { rx: r.0, ry: r.1, large: false, sweep, x: ex, y: ey });
                        cx = ex;
                        cy = ey;
                    }
                }
            }
            4 => d.cmds.push(Cmd::Close),
            5 => {
                if !d.is_empty() {
                    out.push(SubPath { path: std::mem::take(&mut d), fill, stroke, shade });
                }
                fill = true;
                stroke = true;
                shade = 1.0;
                sub += 1;
            }
            6 => fill = false,
            7 => stroke = false,
            18 => shade = 0.7,
            19 => shade = 0.85,
            20 => shade = 1.3,
            21 => shade = 1.15,
            _ => {}
        }
    }
    if !d.is_empty() {
        out.push(SubPath { path: d, fill, stroke, shade });
    }
    // The web builds its path strings with two decimals: the coordinates painted are those.
    for sp in &mut out {
        sp.path = sp.path.rounded();
    }
    out
}

/// The preset's text frame for the box (the whole box when it declares none).
pub fn preset_text_frame(preset: &Preset, env: &Env) -> Rect {
    let Some(t) = preset.text_frame else { return Rect { x: 0.0, y: 0.0, w: env.w, h: env.h } };
    let mut ev = Eval::new(preset, env);
    let x0 = ev.param(t.0 .0);
    let y0 = ev.param(t.0 .1);
    let x1 = ev.param(t.1 .0);
    let y1 = ev.param(t.1 .1);
    Rect { x: x0, y: y0, w: (x1 - x0).max(0.0), h: (y1 - y0).max(0.0) }
}

/// Handle positions in the box.
pub fn preset_handle_positions(preset: &Preset, env: &Env) -> Vec<(usize, f64, f64)> {
    let mut ev = Eval::new(preset, env);
    preset.handles.iter().enumerate().map(|(i, h)| (i, ev.param(h.p.0), ev.param(h.p.1))).collect()
}

fn handle_refs(h: &Handle) -> Vec<usize> {
    [h.rx, h.ry, h.ra, h.rr].iter().flatten().map(|v| *v as usize).collect()
}

fn ref_bounds(h: &Handle, r: usize, env: &Env, preset: &Preset) -> (f64, f64) {
    let mut ev = Eval::new(preset, env);
    if h.ra.map(|v| v as usize) == Some(r) {
        return (0.0, 21_600_000.0);
    }
    let mut rd = |p: Option<Param>| p.map(|p| ev.param(p));
    let (mut lo, mut hi) = (None, None);
    if h.rx.map(|v| v as usize) == Some(r) {
        lo = rd(h.xm);
        hi = rd(h.x_max);
    }
    if h.ry.map(|v| v as usize) == Some(r) {
        lo = rd(h.ym);
        hi = rd(h.y_max);
    }
    if h.rr.map(|v| v as usize) == Some(r) {
        lo = rd(h.rm);
        hi = rd(h.r_max);
    }
    let fin = |v: Option<f64>| v.filter(|v| v.abs() < 2_000_000_000.0);
    (fin(lo).unwrap_or(-100_000.0), fin(hi).unwrap_or(200_000.0))
}

/// Dragging handle `index` to `(px, py)` in the box: the driven adjustment value(s) whose handle lands
/// closest (coarse grid then two refinements, like `presetAdjustFromDrag`).
pub fn preset_adjust_from_drag(preset: &Preset, env: &Env, index: usize, px: f64, py: f64) -> Vec<f64> {
    let mut adj = adj_values(preset, Some(&env.adj));
    let Some(h) = preset.handles.get(index) else { return adj };
    for r in handle_refs(h) {
        if r >= adj.len() {
            continue;
        }
        let (lo, hi) = ref_bounds(h, r, &Env { w: env.w, h: env.h, adj: adj.clone() }, preset);
        let dist = |v: f64, adj: &[f64]| -> f64 {
            let mut trial = adj.to_vec();
            trial[r] = v;
            let e = Env { w: env.w, h: env.h, adj: trial };
            let mut ev = Eval::new(preset, &e);
            let hx = ev.param(h.p.0);
            let hy = ev.param(h.p.1);
            (hx - px) * (hx - px) + (hy - py) * (hy - py)
        };
        let mut best = adj[r];
        let mut best_d = dist(best, &adj);
        let (mut a, mut b) = (lo, hi);
        for _ in 0..3 {
            let steps = 48;
            for k in 0..=steps {
                let v = a + ((b - a) * k as f64) / steps as f64;
                let dd = dist(v, &adj);
                if dd < best_d {
                    best_d = dd;
                    best = v;
                }
            }
            let span = (b - a) / steps as f64 * 2.0;
            a = lo.max(best - span);
            b = hi.min(best + span);
        }
        adj[r] = crate::path::js_round(best);
    }
    adj
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(w: f64, h: f64, preset: &Preset) -> Env {
        Env { w, h, adj: adj_defaults(preset) }
    }

    #[test]
    fn the_data_loads() {
        // Every preset of the web's data parses (134), and every kind the catalogue maps has its preset.
        assert_eq!(preset_count(), 134);
        for (kind, name) in kind_map() {
            assert!(presets().contains_key(name), "{kind} → {name} missing");
        }
        assert!(preset_of("roundRect").is_some());
    }

    #[test]
    fn a_rectangle_is_its_box() {
        let p = preset_of("rect").expect("rect");
        let sp = preset_path(p, &env(100.0, 50.0, p));
        assert_eq!(sp.len(), 1);
        assert_eq!(sp[0].path.to_svg(), "M 0,0 L 100,0 L 100,50 L 0,50 Z");
        let t = preset_text_frame(p, &env(100.0, 50.0, p));
        assert_eq!((t.x, t.y, t.w, t.h), (0.0, 0.0, 100.0, 50.0));
    }

    #[test]
    fn equations_follow_the_web_rules() {
        let preset = preset_of("rect").expect("rect");
        let e = Env { w: 10.0, h: 20.0, adj: vec![5.0] };
        let mut ev = Eval::new(preset, &e);
        let run = |ev: &mut Eval, s: &str| {
            let x = Parser::new(s).parse();
            ev.expr(&x)
        };
        assert_eq!(run(&mut ev, "logwidth*$0 /100000"), 10.0 * 5.0 / 100000.0);
        assert_eq!(run(&mut ev, "if(0-$0 ,0,if(50000-$0 ,$0 ,50000))"), 5.0);
        assert_eq!(run(&mut ev, "3/0"), 0.0);
        assert_eq!(run(&mut ev, "min(logwidth,logheight)"), 10.0);
        assert_eq!(run(&mut ev, "-(2+3)*2"), -10.0);
        assert_eq!(run(&mut ev, "foo(1)"), 0.0);
        assert!((run(&mut ev, "atan2(1,1)") - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
    }

    #[test]
    fn a_rounded_rectangle_has_four_arcs() {
        let p = preset_of("roundRect").expect("roundRect");
        let sp = preset_path(p, &env(200.0, 100.0, p));
        let arcs = sp[0].path.cmds.iter().filter(|c| matches!(c, Cmd::ArcTo { .. })).count();
        assert_eq!(arcs, 4, "{}", sp[0].path.to_svg());
    }

    #[test]
    fn dragging_a_handle_moves_its_value_towards_the_pointer() {
        let p = preset_of("roundRect").expect("roundRect");
        let e = env(200.0, 100.0, p);
        let before = preset_handle_positions(p, &e)[0];
        let adj = preset_adjust_from_drag(p, &e, 0, before.1 + 20.0, before.2);
        let after = preset_handle_positions(p, &Env { adj, ..e.clone() })[0];
        assert!((after.1 - (before.1 + 20.0)).abs() < 1.0, "{before:?} → {after:?}");
    }
}
