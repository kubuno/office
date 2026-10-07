//! KaTeX's `svgGeometry.ts` (the surds and tall delimiters drawn as SVG paths) and an SVG path-data parser
//! that turns those paths into drawing operations.

const H_LINE_PAD: f64 = 80.0;

fn n(v: f64) -> String {
    // JavaScript number to string for the values used here (integers or short decimals).
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v}");
        s
    }
}

fn sqrt_main(ev: f64, p: f64) -> String {
    format!(
        "M95,{a}\nc-2.7,0,-7.17,-2.7,-13.5,-8c-5.8,-5.3,-9.5,-10,-9.5,-14\nc0,-2,0.3,-3.3,1,-4c1.3,-2.7,23.83,-20.7,67.5,-54\nc44.2,-33.3,65.8,-50.3,66.5,-51c1.3,-1.3,3,-2,5,-2c4.7,0,8.7,3.3,12,10\ns173,378,173,378c0.7,0,35.3,-71,104,-213c68.7,-142,137.5,-285,206.5,-429\nc69,-144,104.5,-217.7,106.5,-221\nl{b} -{c}\nc5.3,-9.3,12,-14,20,-14\nH400000v{d}H845.2724\ns-225.272,467,-225.272,467s-235,486,-235,486c-2.7,4.7,-9,7,-19,7\nc-6,0,-10,-1,-12,-3s-194,-422,-194,-422s-65,47,-65,47z\nM{e} {f}h400000v{d}h-400000z",
        a = n(622.0 + ev + p),
        b = n(ev / 2.075),
        c = n(ev),
        d = n(40.0 + ev),
        e = n(834.0 + ev),
        f = n(p)
    )
}

fn sqrt_size1(ev: f64, p: f64) -> String {
    format!(
        "M263,{a}c0.7,0,18,39.7,52,119\nc34,79.3,68.167,158.7,102.5,238c34.3,79.3,51.8,119.3,52.5,120\nc340,-704.7,510.7,-1060.3,512,-1067\nl{b} -{c}\nc4.7,-7.3,11,-11,19,-11\nH40000v{d}H1012.3\ns-271.3,567,-271.3,567c-38.7,80.7,-84,175,-136,283c-52,108,-89.167,185.3,-111.5,232\nc-22.3,46.7,-33.8,70.3,-34.5,71c-4.7,4.7,-12.3,7,-23,7s-12,-1,-12,-1\ns-109,-253,-109,-253c-72.7,-168,-109.3,-252,-110,-252c-10.7,8,-22,16.7,-34,26\nc-22,17.3,-33.3,26,-34,26s-26,-26,-26,-26s76,-59,76,-59s76,-60,76,-60z\nM{e} {f}h400000v{d}h-400000z",
        a = n(601.0 + ev + p),
        b = n(ev / 2.084),
        c = n(ev),
        d = n(40.0 + ev),
        e = n(1001.0 + ev),
        f = n(p)
    )
}

fn sqrt_size2(ev: f64, p: f64) -> String {
    format!(
        "M983 {a}\nl{b} -{c}\nc4,-6.7,10,-10,18,-10 H400000v{d}\nH1013.1s-83.4,268,-264.1,840c-180.7,572,-277,876.3,-289,913c-4.7,4.7,-12.7,7,-24,7\ns-12,0,-12,0c-1.3,-3.3,-3.7,-11.7,-7,-25c-35.3,-125.3,-106.7,-373.3,-214,-744\nc-10,12,-21,25,-33,39s-32,39,-32,39c-6,-5.3,-15,-14,-27,-26s25,-30,25,-30\nc26.7,-32.7,52,-63,76,-91s52,-60,52,-60s208,722,208,722\nc56,-175.3,126.3,-397.3,211,-666c84.7,-268.7,153.8,-488.2,207.5,-658.5\nc53.7,-170.3,84.5,-266.8,92.5,-289.5z\nM{e} {f}h400000v{d}h-400000z",
        a = n(10.0 + ev + p),
        b = n(ev / 3.13),
        c = n(ev),
        d = n(40.0 + ev),
        e = n(1001.0 + ev),
        f = n(p)
    )
}

fn sqrt_size3(ev: f64, p: f64) -> String {
    format!(
        "M424,{a}\nc-1.3,-0.7,-38.5,-172,-111.5,-514c-73,-342,-109.8,-513.3,-110.5,-514\nc0,-2,-10.7,14.3,-32,49c-4.7,7.3,-9.8,15.7,-15.5,25c-5.7,9.3,-9.8,16,-12.5,20\ns-5,7,-5,7c-4,-3.3,-8.3,-7.7,-13,-13s-13,-13,-13,-13s76,-122,76,-122s77,-121,77,-121\ns209,968,209,968c0,-2,84.7,-361.7,254,-1079c169.3,-717.3,254.7,-1077.7,256,-1081\nl{b} -{c}c4,-6.7,10,-10,18,-10 H400000\nv{d}H1014.6\ns-87.3,378.7,-272.6,1166c-185.3,787.3,-279.3,1182.3,-282,1185\nc-2,6,-10,9,-24,9\nc-8,0,-12,-0.7,-12,-2z M{e} {f}\nh400000v{d}h-400000z",
        a = n(2398.0 + ev + p),
        b = n(ev / 4.223),
        c = n(ev),
        d = n(40.0 + ev),
        e = n(1001.0 + ev),
        f = n(p)
    )
}

fn sqrt_size4(ev: f64, p: f64) -> String {
    format!(
        "M473,{a}\nc339.3,-1799.3,509.3,-2700,510,-2702 l{b} -{c}\nc3.3,-7.3,9.3,-11,18,-11 H400000v{d}H1017.7\ns-90.5,478,-276.2,1466c-185.7,988,-279.5,1483,-281.5,1485c-2,6,-10,9,-24,9\nc-8,0,-12,-0.7,-12,-2c0,-1.3,-5.3,-32,-16,-92c-50.7,-293.3,-119.7,-693.3,-207,-1200\nc0,-1.3,-5.3,8.7,-16,30c-10.7,21.3,-21.3,42.7,-32,64s-16,33,-16,33s-26,-26,-26,-26\ns76,-153,76,-153s77,-151,77,-151c0.7,0.7,35.7,202,105,604c67.3,400.7,102,602.7,104,\n606zM{e} {f}h400000v{d}H1017.7z",
        a = n(2713.0 + ev + p),
        b = n(ev / 5.298),
        c = n(ev),
        d = n(40.0 + ev),
        e = n(1001.0 + ev),
        f = n(p)
    )
}

fn sqrt_tall(ev: f64, p: f64, view_box_height: f64) -> String {
    let vert = view_box_height - 54.0 - p - ev;
    format!(
        "M702 {a}H400000{d}\nH742v{v}l-4 4-4 4c-.667.7 -2 1.5-4 2.5s-4.167 1.833-6.5 2.5-5.5 1-9.5 1\nh-12l-28-84c-16.667-52-96.667 -294.333-240-727l-212 -643 -85 170\nc-4-3.333-8.333-7.667-13 -13l-13-13l77-155 77-156c66 199.333 139 419.667\n219 661 l218 661zM702 {f}H400000v{d}H742z",
        a = n(ev + p),
        d = n(40.0 + ev),
        v = n(vert),
        f = n(p)
    )
}

/// `sqrtPath`.
pub fn sqrt_path(size: &str, extra_vinculum: f64, view_box_height: f64) -> String {
    let ev = 1000.0 * extra_vinculum;
    match size {
        "sqrtMain" => sqrt_main(ev, H_LINE_PAD),
        "sqrtSize1" => sqrt_size1(ev, H_LINE_PAD),
        "sqrtSize2" => sqrt_size2(ev, H_LINE_PAD),
        "sqrtSize3" => sqrt_size3(ev, H_LINE_PAD),
        "sqrtSize4" => sqrt_size4(ev, H_LINE_PAD),
        _ => sqrt_tall(ev, H_LINE_PAD, view_box_height),
    }
}

fn double(p: String) -> String {
    format!("{p} {p}")
}

/// `innerPath`: the repeated middle of a stacked brace.
pub fn inner_path(name: char, height: f64) -> String {
    let h = n(height);
    match name {
        '\u{239c}' => double(format!("M291 0 H417 V{h} H291z")),
        '\u{2223}' => double(format!("M145 0 H188 V{h} H145z")),
        '\u{2225}' => double(format!("M145 0 H188 V{h} H145z")) + &double(format!("M367 0 H410 V{h} H367z")),
        '\u{239f}' => double(format!("M457 0 H583 V{h} H457z")),
        '\u{23a2}' => double(format!("M319 0 H403 V{h} H319z")),
        '\u{23a5}' => double(format!("M263 0 H347 V{h} H263z")),
        '\u{23aa}' => double(format!("M384 0 H504 V{h} H384z")),
        '\u{23d0}' => double(format!("M312 0 H355 V{h} H312z")),
        '\u{2016}' => double(format!("M257 0 H300 V{h} H257z")) + &double(format!("M478 0 H521 V{h} H478z")),
        _ => String::new(),
    }
}

/// `tallDelim`.
pub fn tall_delim(label: &str, mid: f64) -> String {
    let m = n(mid);
    let mneg = n(-mid);
    match label {
        "lbrack" => format!("M403 1759 V84 H666 V0 H319 V1759 v{m} v1759 v84 h347 v-84\nH403z M403 1759 V0 H319 V1759 v{m} v1759 v84 h84z"),
        "rbrack" => format!("M347 1759 V0 H0 V84 H263 V1759 v{m} v1759 H0 v84 H347z\nM347 1759 V0 H263 V1759 v{m} v1759 h84z"),
        "vert" => format!("M145 15 v585 v{m} v585 c2.667,10,9.667,15,21,15\nc10,0,16.667,-5,20,-15 v-585 v{mneg} v-585 c-2.667,-10,-9.667,-15,-21,-15\nc-10,0,-16.667,5,-20,15z M188 15 H145 v585 v{m} v585 h43z"),
        "doublevert" => format!("M145 15 v585 v{m} v585 c2.667,10,9.667,15,21,15\nc10,0,16.667,-5,20,-15 v-585 v{mneg} v-585 c-2.667,-10,-9.667,-15,-21,-15\nc-10,0,-16.667,5,-20,15z M188 15 H145 v585 v{m} v585 h43z\nM367 15 v585 v{m} v585 c2.667,10,9.667,15,21,15\nc10,0,16.667,-5,20,-15 v-585 v{mneg} v-585 c-2.667,-10,-9.667,-15,-21,-15\nc-10,0,-16.667,5,-20,15z M410 15 H367 v585 v{m} v585 h43z"),
        "lfloor" => format!("M319 602 V0 H403 V602 v{m} v1715 h263 v84 H319z\nMM319 602 V0 H403 V602 v{m} v1715 H319z"),
        "rfloor" => format!("M319 602 V0 H403 V602 v{m} v1799 H0 v-84 H319z\nMM319 602 V0 H403 V602 v{m} v1715 H319z"),
        "lceil" => format!("M403 1759 V84 H666 V0 H319 V1759 v{m} v602 h84z\nM403 1759 V0 H319 V1759 v{m} v602 h84z"),
        "rceil" => format!("M347 1759 V0 H0 V84 H263 V1759 v{m} v602 h84z\nM347 1759 V0 h-84 V1759 v{m} v602 h84z"),
        "lparen" => format!(
            "M863,9c0,-2,-2,-5,-6,-9c0,0,-17,0,-17,0c-12.7,0,-19.3,0.3,-20,1\nc-5.3,5.3,-10.3,11,-15,17c-242.7,294.7,-395.3,682,-458,1162c-21.3,163.3,-33.3,349,\n-36,557 l0,{a}c0.2,6,0,26,0,60c2,159.3,10,310.7,24,454c53.3,528,210,\n949.7,470,1265c4.7,6,9.7,11.7,15,17c0.7,0.7,7,1,19,1c0,0,18,0,18,0c4,-4,6,-7,6,-9\nc0,-2.7,-3.3,-8.7,-10,-18c-135.3,-192.7,-235.5,-414.3,-300.5,-665c-65,-250.7,-102.5,\n-544.7,-112.5,-882c-2,-104,-3,-167,-3,-189\nl0,-{b}c0,-162.7,5.7,-314,17,-454c20.7,-272,63.7,-513,129,-723c65.3,\n-210,155.3,-396.3,270,-559c6.7,-9.3,10,-15.3,10,-18z",
            a = n(mid + 84.0),
            b = n(mid + 92.0)
        ),
        "rparen" => format!(
            "M76,0c-16.7,0,-25,3,-25,9c0,2,2,6.3,6,13c21.3,28.7,42.3,60.3,\n63,95c96.7,156.7,172.8,332.5,228.5,527.5c55.7,195,92.8,416.5,111.5,664.5\nc11.3,139.3,17,290.7,17,454c0,28,1.7,43,3.3,45l0,{a}\nc-3,4,-3.3,16.7,-3.3,38c0,162,-5.7,313.7,-17,455c-18.7,248,-55.8,469.3,-111.5,664\nc-55.7,194.7,-131.8,370.3,-228.5,527c-20.7,34.7,-41.7,66.3,-63,95c-2,3.3,-4,7,-6,11\nc0,7.3,5.7,11,17,11c0,0,11,0,11,0c9.3,0,14.3,-0.3,15,-1c5.3,-5.3,10.3,-11,15,-17\nc242.7,-294.7,395.3,-681.7,458,-1161c21.3,-164.7,33.3,-350.7,36,-558\nl0,-{b}c-2,-159.3,-10,-310.7,-24,-454c-53.3,-528,-210,-949.7,\n-470,-1265c-4.7,-6,-9.7,-11.7,-15,-17c-0.7,-0.7,-6.7,-1,-18,-1z",
            a = n(mid + 9.0),
            b = n(mid + 144.0)
        ),
        _ => String::new(),
    }
}

/// A drawing operation of a parsed path (absolute coordinates, SVG y-down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathOp {
    Move(f64, f64),
    Line(f64, f64),
    Quad(f64, f64, f64, f64),
    Cubic(f64, f64, f64, f64, f64, f64),
    Close,
}

struct Scanner<'a> {
    b: &'a [u8],
    i: usize,
}

impl Scanner<'_> {
    fn skip_sep(&mut self) {
        while self.i < self.b.len() && (self.b[self.i].is_ascii_whitespace() || self.b[self.i] == b',') {
            self.i += 1;
        }
    }

    fn number(&mut self) -> Option<f64> {
        self.skip_sep();
        let start = self.i;
        let b = self.b;
        if self.i < b.len() && (b[self.i] == b'-' || b[self.i] == b'+') {
            self.i += 1;
        }
        let mut seen_digit = false;
        while self.i < b.len() && b[self.i].is_ascii_digit() {
            self.i += 1;
            seen_digit = true;
        }
        if self.i < b.len() && b[self.i] == b'.' {
            self.i += 1;
            while self.i < b.len() && b[self.i].is_ascii_digit() {
                self.i += 1;
                seen_digit = true;
            }
        }
        if seen_digit && self.i < b.len() && (b[self.i] == b'e' || b[self.i] == b'E') {
            let save = self.i;
            self.i += 1;
            if self.i < b.len() && (b[self.i] == b'-' || b[self.i] == b'+') {
                self.i += 1;
            }
            let ds = self.i;
            while self.i < b.len() && b[self.i].is_ascii_digit() {
                self.i += 1;
            }
            if ds == self.i {
                self.i = save;
            }
        }
        if !seen_digit {
            self.i = start;
            return None;
        }
        std::str::from_utf8(&b[start..self.i]).ok()?.parse().ok()
    }
}

/// Parses SVG path data (`M L H V C S Q T Z`, absolute and relative). Stops at the first error, as browsers do.
pub fn parse_path(d: &str) -> Vec<PathOp> {
    let mut s = Scanner { b: d.as_bytes(), i: 0 };
    let mut out = Vec::new();
    let (mut x, mut y) = (0.0f64, 0.0f64);
    let (mut sx, mut sy) = (0.0f64, 0.0f64);
    let mut last_ctrl: Option<(f64, f64)> = None;
    let mut last_quad: Option<(f64, f64)> = None;
    let mut cmd = 0u8;
    loop {
        s.skip_sep();
        if s.i >= s.b.len() {
            break;
        }
        let c = s.b[s.i];
        if c.is_ascii_alphabetic() {
            cmd = c;
            s.i += 1;
            if cmd == b'z' || cmd == b'Z' {
                out.push(PathOp::Close);
                x = sx;
                y = sy;
                last_ctrl = None;
                last_quad = None;
                continue;
            }
        } else if cmd == 0 {
            break;
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (x, y) } else { (0.0, 0.0) };
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let (Some(a), Some(b)) = (s.number(), s.number()) else { break };
                x = ox + a;
                y = oy + b;
                sx = x;
                sy = y;
                out.push(PathOp::Move(x, y));
                cmd = if rel { b'l' } else { b'L' };
                last_ctrl = None;
                last_quad = None;
            }
            b'L' => {
                let (Some(a), Some(b)) = (s.number(), s.number()) else { break };
                x = ox + a;
                y = oy + b;
                out.push(PathOp::Line(x, y));
                last_ctrl = None;
                last_quad = None;
            }
            b'H' => {
                let Some(a) = s.number() else { break };
                x = ox + a;
                out.push(PathOp::Line(x, y));
                last_ctrl = None;
                last_quad = None;
            }
            b'V' => {
                let Some(a) = s.number() else { break };
                y = oy + a;
                out.push(PathOp::Line(x, y));
                last_ctrl = None;
                last_quad = None;
            }
            b'C' => {
                let v: Vec<f64> = (0..6).filter_map(|_| s.number()).collect();
                if v.len() < 6 {
                    break;
                }
                let (x1, y1, x2, y2) = (ox + v[0], oy + v[1], ox + v[2], oy + v[3]);
                x = ox + v[4];
                y = oy + v[5];
                out.push(PathOp::Cubic(x1, y1, x2, y2, x, y));
                last_ctrl = Some((x2, y2));
                last_quad = None;
            }
            b'S' => {
                let v: Vec<f64> = (0..4).filter_map(|_| s.number()).collect();
                if v.len() < 4 {
                    break;
                }
                let (x1, y1) = match last_ctrl {
                    Some((cx, cy)) => (2.0 * x - cx, 2.0 * y - cy),
                    None => (x, y),
                };
                let (x2, y2) = (ox + v[0], oy + v[1]);
                x = ox + v[2];
                y = oy + v[3];
                out.push(PathOp::Cubic(x1, y1, x2, y2, x, y));
                last_ctrl = Some((x2, y2));
                last_quad = None;
            }
            b'Q' => {
                let v: Vec<f64> = (0..4).filter_map(|_| s.number()).collect();
                if v.len() < 4 {
                    break;
                }
                let (x1, y1) = (ox + v[0], oy + v[1]);
                x = ox + v[2];
                y = oy + v[3];
                out.push(PathOp::Quad(x1, y1, x, y));
                last_quad = Some((x1, y1));
                last_ctrl = None;
            }
            b'T' => {
                let (Some(a), Some(b)) = (s.number(), s.number()) else { break };
                let (x1, y1) = match last_quad {
                    Some((cx, cy)) => (2.0 * x - cx, 2.0 * y - cy),
                    None => (x, y),
                };
                x = ox + a;
                y = oy + b;
                out.push(PathOp::Quad(x1, y1, x, y));
                last_quad = Some((x1, y1));
                last_ctrl = None;
            }
            _ => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_relative_and_implicit_commands() {
        let ops = parse_path("M10,20 l5 5 5 5 h-10 v-10z");
        assert_eq!(ops[0], PathOp::Move(10.0, 20.0));
        assert_eq!(ops[1], PathOp::Line(15.0, 25.0));
        assert_eq!(ops[2], PathOp::Line(20.0, 30.0));
        assert_eq!(ops[3], PathOp::Line(10.0, 30.0));
        assert_eq!(ops[4], PathOp::Line(10.0, 20.0));
        assert_eq!(ops[5], PathOp::Close);
        let s = parse_path("M0 0c-.667.7 -2 1.5-4 2.5");
        assert_eq!(s[1], PathOp::Cubic(-0.667, 0.7, -2.0, 1.5, -4.0, 2.5));
    }

    #[test]
    fn the_surd_parses() {
        assert!(parse_path(&sqrt_path("sqrtMain", 0.0, 1080.0)).len() > 20);
        assert!(parse_path(&tall_delim("lparen", 500.0)).len() > 20);
    }
}
