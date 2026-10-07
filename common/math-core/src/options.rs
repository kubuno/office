//! KaTeX's `Style.ts` and `Options.ts`: the math style (display, text, script, scriptscript, cramped) and the
//! options a group is built with (size, colour, font).

use crate::data::{self, FontParams};

/// A math style (`Style`): id 0..7 = D, Dc, T, Tc, S, Sc, SS, SSc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    pub id: u8,
}

const SUP: [u8; 8] = [4, 5, 4, 5, 6, 7, 6, 7];
const SUB: [u8; 8] = [5, 5, 5, 5, 7, 7, 7, 7];
const FRAC_NUM: [u8; 8] = [2, 3, 4, 5, 6, 7, 6, 7];
const FRAC_DEN: [u8; 8] = [3, 3, 5, 5, 7, 7, 7, 7];
const CRAMP: [u8; 8] = [1, 1, 3, 3, 5, 5, 7, 7];
const TEXT: [u8; 8] = [0, 1, 2, 3, 2, 3, 2, 3];

impl Style {
    pub const DISPLAY: Style = Style { id: 0 };
    pub const TEXT: Style = Style { id: 2 };
    pub const SCRIPT: Style = Style { id: 4 };
    pub const SCRIPTSCRIPT: Style = Style { id: 6 };

    pub fn size(self) -> u8 {
        self.id / 2
    }
    pub fn cramped(self) -> bool {
        self.id % 2 == 1
    }
    pub fn sup(self) -> Style {
        Style { id: SUP[self.id as usize] }
    }
    pub fn sub(self) -> Style {
        Style { id: SUB[self.id as usize] }
    }
    pub fn frac_num(self) -> Style {
        Style { id: FRAC_NUM[self.id as usize] }
    }
    pub fn frac_den(self) -> Style {
        Style { id: FRAC_DEN[self.id as usize] }
    }
    pub fn cramp(self) -> Style {
        Style { id: CRAMP[self.id as usize] }
    }
    pub fn text(self) -> Style {
        Style { id: TEXT[self.id as usize] }
    }
    pub fn is_tight(self) -> bool {
        self.size() >= 2
    }
}

const SIZE_STYLE_MAP: [[u8; 3]; 11] = [[1, 1, 1], [2, 1, 1], [3, 1, 1], [4, 2, 1], [5, 2, 1], [6, 3, 1], [7, 4, 2], [8, 6, 3], [9, 7, 6], [10, 8, 7], [11, 10, 9]];
pub const SIZE_MULTIPLIERS: [f64; 11] = [0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.2, 1.44, 1.728, 2.074, 2.488];

fn size_at_style(size: u8, style: Style) -> u8 {
    if style.size() < 2 {
        size
    } else {
        SIZE_STYLE_MAP[(size - 1) as usize][(style.size() - 1) as usize]
    }
}

/// `Options`.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub style: Style,
    pub color: Option<String>,
    pub size: u8,
    pub text_size: u8,
    pub phantom: bool,
    pub font: String,
    pub font_family: String,
    pub font_weight: String,
    pub font_shape: String,
    pub size_multiplier: f64,
    pub max_size: f64,
    pub min_rule_thickness: f64,
}

pub const BASESIZE: u8 = 6;

impl Options {
    /// The root options of a display-mode formula.
    pub fn display() -> Options {
        Options {
            style: Style::DISPLAY,
            color: None,
            size: BASESIZE,
            text_size: BASESIZE,
            phantom: false,
            font: String::new(),
            font_family: String::new(),
            font_weight: String::new(),
            font_shape: String::new(),
            size_multiplier: SIZE_MULTIPLIERS[(BASESIZE - 1) as usize],
            max_size: f64::INFINITY,
            min_rule_thickness: 0.0,
        }
    }

    fn with(&self, f: impl FnOnce(&mut Options)) -> Options {
        let mut o = self.clone();
        f(&mut o);
        o.size_multiplier = SIZE_MULTIPLIERS[(o.size.clamp(1, 11) - 1) as usize];
        o
    }

    pub fn having_style(&self, style: Style) -> Options {
        if self.style == style {
            self.clone()
        } else {
            let size = size_at_style(self.text_size, style);
            self.with(|o| {
                o.style = style;
                o.size = size;
            })
        }
    }

    pub fn having_cramped_style(&self) -> Options {
        self.having_style(self.style.cramp())
    }

    pub fn having_size(&self, size: u8) -> Options {
        if self.size == size && self.text_size == size {
            self.clone()
        } else {
            let st = self.style.text();
            self.with(|o| {
                o.style = st;
                o.size = size;
                o.text_size = size;
            })
        }
    }

    pub fn having_base_style(&self, style: Style) -> Options {
        let want = size_at_style(BASESIZE, style);
        if self.size == want && self.text_size == BASESIZE && self.style == style {
            self.clone()
        } else {
            self.with(|o| {
                o.style = style;
                o.size = want;
            })
        }
    }

    pub fn having_base_sizing(&self) -> Options {
        let size = match self.style.id {
            4 | 5 => 3,
            6 | 7 => 1,
            _ => 6,
        };
        let st = self.style.text();
        self.with(|o| {
            o.style = st;
            o.size = size;
        })
    }

    pub fn with_color(&self, color: &str) -> Options {
        self.with(|o| o.color = Some(color.to_string()))
    }

    pub fn with_phantom(&self) -> Options {
        self.with(|o| o.phantom = true)
    }

    pub fn with_font(&self, font: &str) -> Options {
        self.with(|o| o.font = font.to_string())
    }

    pub fn with_text_font_family(&self, family: &str) -> Options {
        self.with(|o| {
            o.font_family = family.to_string();
            o.font = String::new();
        })
    }

    pub fn with_text_font_weight(&self, weight: &str) -> Options {
        self.with(|o| {
            o.font_weight = weight.to_string();
            o.font = String::new();
        })
    }

    pub fn with_text_font_shape(&self, shape: &str) -> Options {
        self.with(|o| {
            o.font_shape = shape.to_string();
            o.font = String::new();
        })
    }

    /// The font-size ratio of the `sizing reset-sizeA sizeB` classes, `None` when the size is unchanged.
    pub fn sizing_scale(&self, old: &Options) -> Option<f64> {
        if old.size != self.size {
            Some(SIZE_MULTIPLIERS[(self.size - 1) as usize] / SIZE_MULTIPLIERS[(old.size - 1) as usize])
        } else {
            None
        }
    }

    pub fn font_metrics(&self) -> &'static FontParams {
        data::font_params(self.size)
    }

    /// `getColor` (phantom → transparent).
    pub fn get_color(&self) -> Option<String> {
        if self.phantom {
            Some("transparent".into())
        } else {
            self.color.clone()
        }
    }
}
