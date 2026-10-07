//! The task table's columns (`web/src/project/ganttTableConstants.ts`): identity, default and minimum widths,
//! what is hidden by default, the compact (name-only) grid, and the per-project layout the user changes by
//! dragging a header edge, the splitter, or the header's menu.

use std::collections::HashMap;

/// The columns, left to right (`GANTT_COL_IDS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Col {
    Idx,
    Mode,
    Name,
    Dur,
    Progress,
    Priority,
    Start,
    End,
    Variance,
    Pred,
    Res,
}

pub const COLS: [Col; 11] = [Col::Idx, Col::Mode, Col::Name, Col::Dur, Col::Progress, Col::Priority, Col::Start, Col::End, Col::Variance, Col::Pred, Col::Res];

impl Col {
    /// `GanttColId`.
    pub fn id(self) -> &'static str {
        match self {
            Col::Idx => "idx",
            Col::Mode => "mode",
            Col::Name => "name",
            Col::Dur => "dur",
            Col::Progress => "progress",
            Col::Priority => "priority",
            Col::Start => "start",
            Col::End => "end",
            Col::Variance => "variance",
            Col::Pred => "pred",
            Col::Res => "res",
        }
    }

    /// Default width (`COL_W`), also what a double click on its edge restores.
    pub fn default_width(self) -> f64 {
        match self {
            Col::Idx | Col::Mode => 34.0,
            Col::Name => 200.0,
            Col::Dur => 56.0,
            Col::Progress => 48.0,
            Col::Priority => 108.0,
            Col::Start | Col::End => 84.0,
            Col::Variance => 68.0,
            Col::Pred => 96.0,
            Col::Res => 120.0,
        }
    }

    /// Lower bound (`COL_MIN`).
    pub fn min_width(self) -> f64 {
        match self {
            Col::Idx | Col::Mode => 28.0,
            Col::Name => 96.0,
            Col::Dur => 44.0,
            Col::Progress => 40.0,
            Col::Priority => 64.0,
            Col::Start | Col::End => 60.0,
            Col::Variance => 48.0,
            Col::Pred => 48.0,
            Col::Res => 60.0,
        }
    }

    /// Pictogram columns cannot be resized (`isColResizable`).
    pub fn resizable(self) -> bool {
        !matches!(self, Col::Idx | Col::Mode)
    }

    /// The name is the row's identity: never hidden (`isColHideable`).
    pub fn hideable(self) -> bool {
        self != Col::Name
    }
}

/// Upper bound of a column (`COL_MAX`), bounds of the pane (`PANE_MIN`, `PANE_GANTT_ROOM`).
pub const COL_MAX: f64 = 480.0;
pub const PANE_MIN: f64 = 160.0;
pub const PANE_GANTT_ROOM: f64 = 240.0;

/// Hidden out of the box (`COL_DEFAULT_HIDDEN`).
pub const DEFAULT_HIDDEN: [Col; 5] = [Col::Mode, Col::Priority, Col::Variance, Col::Pred, Col::Res];

/// The table layout of one project (`GanttColLayout`).
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub widths: HashMap<Col, f64>,
    pub hidden: Vec<Col>,
    /// The pane's width once the splitter was dragged; `None`: the pane hugs its columns.
    pub pane_width: Option<f64>,
    /// Name-only grid (`compact`).
    pub compact: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self { widths: HashMap::new(), hidden: DEFAULT_HIDDEN.to_vec(), pane_width: None, compact: false }
    }
}

impl Layout {
    pub fn width(&self, c: Col) -> f64 {
        self.widths.get(&c).copied().unwrap_or(c.default_width())
    }

    /// Is the column on screen (`isShown`): compact keeps the number and the name; the variance column only
    /// means something against a baseline (`colsVisible`).
    pub fn shown(&self, c: Col, has_baseline: bool) -> bool {
        if c == Col::Variance && !has_baseline {
            return false;
        }
        if self.compact {
            matches!(c, Col::Idx | Col::Name)
        } else {
            !self.hidden.contains(&c)
        }
    }

    /// The visible columns, left to right.
    pub fn visible(&self, has_baseline: bool) -> Vec<Col> {
        COLS.iter().copied().filter(|c| self.shown(*c, has_baseline)).collect()
    }

    /// The sum of the visible columns (`--gcol-table`).
    pub fn table_width(&self, has_baseline: bool) -> f64 {
        self.visible(has_baseline).iter().map(|c| self.width(*c)).sum()
    }

    /// The pane's width for a chart area of `avail` DIP: its columns, or what the splitter set, within bounds.
    pub fn pane_width(&self, has_baseline: bool, avail: f64) -> f64 {
        let hug = self.table_width(has_baseline);
        let w = if self.compact { hug } else { self.pane_width.unwrap_or(hug) };
        w.max(PANE_MIN).min((avail - PANE_GANTT_ROOM).max(PANE_MIN))
    }

    pub fn set_width(&mut self, c: Col, w: f64) {
        self.widths.insert(c, w.round().clamp(c.min_width(), COL_MAX));
    }

    pub fn reset_width(&mut self, c: Col) {
        self.widths.remove(&c);
    }

    pub fn toggle(&mut self, c: Col) {
        if !c.hideable() {
            return;
        }
        if let Some(i) = self.hidden.iter().position(|x| *x == c) {
            self.hidden.remove(i);
        } else {
            self.hidden.push(c);
        }
    }

    /// Something differs from the defaults (« Réinitialiser les colonnes » is enabled).
    pub fn customised(&self) -> bool {
        *self != Layout::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_compact() {
        let mut l = Layout::default();
        assert_eq!(l.visible(false), vec![Col::Idx, Col::Name, Col::Dur, Col::Progress, Col::Start, Col::End]);
        assert_eq!(l.table_width(false), 34.0 + 200.0 + 56.0 + 48.0 + 84.0 + 84.0);
        l.toggle(Col::Name);
        assert!(l.shown(Col::Name, false));
        l.toggle(Col::Variance);
        assert!(!l.shown(Col::Variance, false));
        assert!(l.shown(Col::Variance, true));
        l.compact = true;
        assert_eq!(l.visible(true), vec![Col::Idx, Col::Name]);
        l.set_width(Col::Name, 10.0);
        assert_eq!(l.width(Col::Name), 96.0);
        assert!(l.customised());
        assert_eq!(Layout::default().pane_width(false, 2000.0), 506.0);
        assert_eq!(Layout::default().pane_width(false, 500.0), 260.0);
    }
}
