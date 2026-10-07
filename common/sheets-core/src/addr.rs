//! Cell addresses: column letters, `A1` keys and rectangular ranges.
//!
//! Columns are 0-based indexes (`A` = 0) and rows 1-based numbers, as the web's `colToIndex` /
//! `indexToCol` and its `"A1"` cell keys have them. The grid is Excel's: 16 384 columns (XFD) and
//! 1 048 576 rows.

/// The number of columns of a sheet (A … XFD).
pub const MAX_COLS: u32 = 16_384;
/// The number of rows of a sheet.
pub const MAX_ROWS: u32 = 1_048_576;

/// `colToIndex("AB")` = 27. Any character counts as `ch − 'A' + 1` like the web's.
pub fn col_to_index(col: &str) -> i64 {
    let mut n: i64 = 0;
    for ch in col.chars() {
        n = n * 26 + (ch as i64 - 64);
    }
    n - 1
}

/// `indexToCol(27)` = `"AB"`.
pub fn index_to_col(i: i64) -> String {
    let mut s = Vec::new();
    let mut i = i + 1;
    while i > 0 {
        let r = (i - 1) % 26;
        s.push((b'A' + r as u8) as char);
        i = (i - 1) / 26;
    }
    s.iter().rev().collect()
}

/// The cell key `"B3"` of column index 1, row 3.
pub fn key(col: u32, row: u32) -> String {
    let mut s = index_to_col(col as i64);
    s.push_str(&row.to_string());
    s
}

/// A cell position: 0-based column, 1-based row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cell {
    pub col: u32,
    pub row: u32,
}

impl Cell {
    pub fn new(col: u32, row: u32) -> Self {
        Self { col, row }
    }
    /// The `A1` key.
    pub fn key(self) -> String {
        key(self.col, self.row)
    }
    /// Parses an upper-case `A1` key (letters then digits, no `$`).
    pub fn parse(key: &str) -> Option<Cell> {
        let b = key.as_bytes();
        let split = b.iter().position(|c| c.is_ascii_digit())?;
        if split == 0 || !b[..split].iter().all(|c| c.is_ascii_uppercase()) || !b[split..].iter().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let col = col_to_index(&key[..split]);
        let row: u64 = key[split..].parse().ok()?;
        if col < 0 || col >= u32::MAX as i64 || row > u32::MAX as u64 {
            return None;
        }
        Some(Cell { col: col as u32, row: row as u32 })
    }
    /// Parses `A1`, `$A$1`, `a1` (letters are upper-cased), within the grid's limits of the web's
    /// `parseA1Rect` (1–3 letters, 1–7 digits).
    pub fn parse_loose(text: &str) -> Option<Cell> {
        let t: String = text.trim().chars().filter(|&c| c != '$').collect();
        let up = t.to_ascii_uppercase();
        let split = up.find(|c: char| c.is_ascii_digit())?;
        let (letters, digits) = up.split_at(split);
        if letters.is_empty() || letters.len() > 3 || digits.is_empty() || digits.len() > 7 {
            return None;
        }
        Cell::parse(&up)
    }
}

/// A rectangle of cells, inclusive on both ends, normalised (c1 ≤ c2, r1 ≤ r2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rect {
    pub c1: u32,
    pub r1: u32,
    pub c2: u32,
    pub r2: u32,
}

impl Rect {
    pub fn new(c1: u32, r1: u32, c2: u32, r2: u32) -> Self {
        Self { c1: c1.min(c2), r1: r1.min(r2), c2: c1.max(c2), r2: r1.max(r2) }
    }
    pub fn cell(c: Cell) -> Self {
        Self { c1: c.col, r1: c.row, c2: c.col, r2: c.row }
    }
    pub fn contains(&self, c: Cell) -> bool {
        c.col >= self.c1 && c.col <= self.c2 && c.row >= self.r1 && c.row <= self.r2
    }
    pub fn width(&self) -> u32 {
        self.c2 - self.c1 + 1
    }
    pub fn height(&self) -> u32 {
        self.r2 - self.r1 + 1
    }
    /// The number of cells (as u64: a whole sheet overflows u32).
    pub fn area(&self) -> u64 {
        self.width() as u64 * self.height() as u64
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        self.c1 <= o.c2 && o.c1 <= self.c2 && self.r1 <= o.r2 && o.r1 <= self.r2
    }
    pub fn union(&self, o: &Rect) -> Rect {
        Rect { c1: self.c1.min(o.c1), r1: self.r1.min(o.r1), c2: self.c2.max(o.c2), r2: self.r2.max(o.r2) }
    }
    pub fn top_left(&self) -> Cell {
        Cell::new(self.c1, self.r1)
    }
    /// `"A1:B2"`, or `"A1"` for one cell.
    pub fn a1(&self) -> String {
        if self.c1 == self.c2 && self.r1 == self.r2 {
            key(self.c1, self.r1)
        } else {
            format!("{}:{}", key(self.c1, self.r1), key(self.c2, self.r2))
        }
    }
    /// `"A1:B2"` always (two corners, even for one cell).
    pub fn a1_pair(&self) -> String {
        format!("{}:{}", key(self.c1, self.r1), key(self.c2, self.r2))
    }
    /// The web's `parseA1Rect` / `parseRangeAddr`: `A1`, `$A$1`, `A1:B2` (case-insensitive), normalised.
    pub fn parse(text: &str) -> Option<Rect> {
        let t = text.trim();
        match t.split_once(':') {
            Some((a, b)) => {
                let a = Cell::parse_loose(a)?;
                let b = Cell::parse_loose(b)?;
                Some(Rect::new(a.col, a.row, b.col, b.row))
            }
            None => Cell::parse_loose(t).map(Rect::cell),
        }
    }
    /// Every cell, row by row.
    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        (self.r1..=self.r2).flat_map(move |r| (self.c1..=self.c2).map(move |c| Cell::new(c, r)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_round_trip() {
        assert_eq!(col_to_index("A"), 0);
        assert_eq!(col_to_index("Z"), 25);
        assert_eq!(col_to_index("AA"), 26);
        assert_eq!(col_to_index("XFD"), 16_383);
        for i in [0, 25, 26, 701, 702, 16_383] {
            assert_eq!(col_to_index(&index_to_col(i)), i);
        }
    }

    #[test]
    fn keys_and_ranges() {
        assert_eq!(Cell::parse("B3"), Some(Cell::new(1, 3)));
        assert_eq!(Cell::parse("b3"), None);
        assert_eq!(Cell::parse_loose("$b$3"), Some(Cell::new(1, 3)));
        let r = Rect::parse("C5:A1").unwrap();
        assert_eq!((r.c1, r.r1, r.c2, r.r2), (0, 1, 2, 5));
        assert_eq!(r.a1(), "A1:C5");
        assert_eq!(Rect::parse("A1").unwrap().a1(), "A1");
        assert_eq!(Rect::parse("ABCD1"), None);
    }
}
