//! Port of the web's `formula-fns/lookup.ts`: CHOOSE, COLUMNS/ROWS, TRANSPOSE, LOOKUP, XLOOKUP/XMATCH,
//! the dynamic-array helpers (FILTER, SORT, SORTBY, UNIQUE, HSTACK, VSTACK, TAKE, DROP, CHOOSECOLS,
//! CHOOSEROWS, TOCOL, TOROW, EXPAND), the reference functions (ROW, COLUMN, ADDRESS, INDIRECT, OFFSET,
//! AREAS, FORMULATEXT) and GROUPBY / PIVOTBY.
//!
//! Notes on fidelity: a JavaScript array cell that does not exist (`row[c]` past the end of a ragged
//! row) is `undefined` on the web; a Rust matrix cannot hold it, so such a cell reads as the empty
//! string here (only reachable with ragged matrices, e.g. XLOOKUP over a return array narrower than
//! the lookup vector).

// The index loops mirror the web code (parallel arrays indexed by position).
#![allow(clippy::needless_range_loop)]

use std::collections::{HashMap, HashSet};

use super::base::index;
use super::Registry;
use crate::formula::ast::Node;
use crate::formula::criteria::wildcard_match;
use crate::formula::eval::{arg, Evaluator};
use crate::formula::value::{as_mat, compare_scalar, flatten, to_bool, to_num, to_str, ErrCode, Matrix, Scalar, Value};
use crate::js;

type R = Result<Value, ErrCode>;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// `firstErr(v)`: the first error found while scanning a matrix.
fn first_err(m: &Matrix) -> Option<ErrCode> {
    m.iter().flat_map(|r| r.iter()).find_map(|s| s.err().cloned())
}

/// `row[c]`, `undefined` read as the empty string.
fn cell_at(row: &[Scalar], c: usize) -> Scalar {
    row.get(c).cloned().unwrap_or_else(Scalar::empty)
}

/// `m[0]?.length ?? 0`.
fn first_width(m: &Matrix) -> usize {
    m.first().map(Vec::len).unwrap_or(0)
}

/// `index_to_col` with the web's float arithmetic (`indexToCol`); a non-finite index (an endless loop
/// on the web) gives the empty string.
fn index_to_col_f(i: f64) -> String {
    if !i.is_finite() {
        return String::new();
    }
    let mut s = String::new();
    let mut i = i + 1.0;
    while i > 0.0 {
        let r = (i - 1.0) % 26.0;
        s = js::from_char_code(65 + r as u32) + &s;
        i = ((i - 1.0) / 26.0).floor();
    }
    s
}

/// `colToIndex` over ASCII upper-case letters, in floats.
fn col_to_index_f(col: &str) -> f64 {
    let mut n = 0.0;
    for ch in col.chars() {
        n = n * 26.0 + (ch as u32 as f64 - 64.0);
    }
    n - 1.0
}

/// `parseRef`: `A1` into a 0-based (row, col).
fn parse_ref(r: &str) -> Option<(f64, f64)> {
    let split = r.find(|c: char| !c.is_ascii_alphabetic())?;
    let (letters, digits) = r.split_at(split);
    if letters.is_empty() || digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((js::parse_int(digits, 10) - 1.0, col_to_index_f(&letters.to_ascii_uppercase())))
}

/// 1-based bounds of a referenced range.
struct Bounds {
    r1: f64,
    c1: f64,
    r2: f64,
    c2: f64,
}

/// `rangeBounds(node)`: only `ref` and `range` nodes carry positional information.
fn range_bounds(node: Option<&Node>) -> Option<Bounds> {
    match node? {
        Node::Ref { r, .. } => {
            let (row, col) = parse_ref(r)?;
            Some(Bounds { r1: row + 1.0, c1: col + 1.0, r2: row + 1.0, c2: col + 1.0 })
        }
        Node::Range { a, b, .. } => {
            let (ar, ac) = parse_ref(a)?;
            let (br, bc) = parse_ref(b)?;
            Some(Bounds { r1: ar.min(br) + 1.0, c1: ac.min(bc) + 1.0, r2: ar.max(br) + 1.0, c2: ac.max(bc) + 1.0 })
        }
        _ => None,
    }
}

/// V8's `Array.prototype.sort` ordering of `0..n` by `cmp` (a comparator returning a number, NaN read
/// as 0): binary insertion after the initial run for short arrays (so even an inconsistent comparator
/// gives the web's order), a stable merge sort above 63 elements.
fn sort_indices(n: usize, mut cmp: impl FnMut(usize, usize) -> f64) -> Vec<usize> {
    let mut order = |x: usize, y: usize| -> f64 {
        let c = cmp(x, y);
        if c.is_nan() {
            0.0
        } else {
            c
        }
    };
    let mut a: Vec<usize> = (0..n).collect();
    if n < 2 {
        return a;
    }
    if n < 64 {
        // CountAndMakeRun(0, n)
        let mut run = 2;
        let descending = order(a[1], a[0]) < 0.0;
        let mut prev = a[1];
        for idx in 2..n {
            let cur = a[idx];
            let o = order(cur, prev);
            if descending {
                if o >= 0.0 {
                    break;
                }
            } else if o < 0.0 {
                break;
            }
            prev = cur;
            run += 1;
        }
        if descending {
            a[..run].reverse();
        }
        // BinaryInsertionSort(0, run, n)
        for start in run..n {
            let (mut left, mut right) = (0, start);
            let pivot = a[start];
            while left < right {
                let mid = left + ((right - left) >> 1);
                if order(pivot, a[mid]) < 0.0 {
                    right = mid;
                } else {
                    left = mid + 1;
                }
            }
            let mut p = start;
            while p > left {
                a[p] = a[p - 1];
                p -= 1;
            }
            a[left] = pivot;
        }
        return a;
    }
    // Stable bottom-up merge sort.
    let mut width = 1;
    let mut buf = a.clone();
    while width < n {
        let mut i = 0;
        while i < n {
            let mid = (i + width).min(n);
            let hi = (i + 2 * width).min(n);
            let (mut l, mut r, mut k) = (i, mid, i);
            while l < mid && r < hi {
                if order(a[r], a[l]) < 0.0 {
                    buf[k] = a[r];
                    r += 1;
                } else {
                    buf[k] = a[l];
                    l += 1;
                }
                k += 1;
            }
            while l < mid {
                buf[k] = a[l];
                l += 1;
                k += 1;
            }
            while r < hi {
                buf[k] = a[r];
                r += 1;
                k += 1;
            }
            i += 2 * width;
        }
        std::mem::swap(&mut a, &mut buf);
        width *= 2;
    }
    a
}

/// `arr.slice(start, end)` with JavaScript's index rules, on a vector.
fn js_slice<T: Clone>(v: &[T], start: f64, end: Option<f64>) -> Vec<T> {
    let len = v.len() as f64;
    let norm = |i: f64| -> usize {
        let i = if i.is_nan() { 0.0 } else { i.trunc() };
        let i = if i < 0.0 { (len + i).max(0.0) } else { i.min(len) };
        i as usize
    };
    let a = norm(start);
    let b = end.map(norm).unwrap_or(v.len());
    if a >= b {
        Vec::new()
    } else {
        v[a..b].to_vec()
    }
}

// ── CHOOSE / COLUMNS / ROWS / TRANSPOSE ───────────────────────────────────────

fn choose(ev: &Evaluator, a: &[Node]) -> R {
    let idx = ev.num(arg(a, 0)?)?;
    let i = js::trunc(idx);
    if i < 1.0 || i >= a.len() as f64 {
        return Err(ErrCode::VALUE);
    }
    Ok(ev.eval(&a[i as usize]))
}

fn columns(ev: &Evaluator, a: &[Node]) -> R {
    let m = as_mat(ev.eval(arg(a, 0)?));
    Ok(Value::num(first_width(&m) as f64))
}

fn rows(ev: &Evaluator, a: &[Node]) -> R {
    let m = as_mat(ev.eval(arg(a, 0)?));
    Ok(Value::num(m.len() as f64))
}

fn transpose(ev: &Evaluator, a: &[Node]) -> R {
    let m = as_mat(ev.eval(arg(a, 0)?));
    let cols = first_width(&m);
    let out: Matrix = (0..cols).map(|c| m.iter().map(|row| cell_at(row, c)).collect()).collect();
    Ok(Value::M(out))
}

// ── LOOKUP (vector form) ──────────────────────────────────────────────────────

fn lookup(ev: &Evaluator, a: &[Node]) -> R {
    let target = ev.scalar(arg(a, 0)?);
    if let Scalar::Err(e) = target {
        return Err(e);
    }
    let lookup_vec = flatten(&ev.eval(arg(a, 1)?));
    let result_vec = match a.get(2) {
        Some(n) => flatten(&ev.eval(n)),
        None => lookup_vec.clone(),
    };
    let mut best: Option<usize> = None;
    for (i, v) in lookup_vec.iter().enumerate() {
        if compare_scalar(v, &target) <= 0.0 {
            best = Some(i);
        }
    }
    let Some(best) = best else { return Err(ErrCode::NA) };
    Ok(Value::S(result_vec.get(best).cloned().unwrap_or(Scalar::Err(ErrCode::NA))))
}

// ── XLOOKUP / XMATCH ──────────────────────────────────────────────────────────

/// `xfind`: the 0-based index of `target` in `vec` per the match / search modes.
fn xfind(vec: &[Scalar], target: &Scalar, match_mode: f64, search_mode: f64) -> Option<usize> {
    if match_mode == 0.0 || match_mode == 2.0 {
        let wildcard = match_mode == 2.0;
        let pattern = to_str(target);
        let hit = |i: usize| -> bool {
            if wildcard {
                wildcard_match(&pattern, &to_str(&vec[i]))
            } else {
                compare_scalar(&vec[i], target) == 0.0
            }
        };
        if search_mode == -1.0 {
            return (0..vec.len()).rev().find(|&i| hit(i));
        }
        return (0..vec.len()).find(|&i| hit(i));
    }
    // Approximate: the closest value on the requested side (the search mode is ignored here).
    let mut best: Option<usize> = None;
    let mut best_diff = f64::INFINITY;
    for (i, v) in vec.iter().enumerate() {
        let cmp = compare_scalar(v, target);
        if cmp == 0.0 {
            return Some(i);
        }
        if match_mode == -1.0 && cmp < 0.0 {
            let diff = -cmp;
            if diff < best_diff {
                best_diff = diff;
                best = Some(i);
            }
        }
        if match_mode == 1.0 && cmp > 0.0 {
            let diff = cmp;
            if diff < best_diff {
                best_diff = diff;
                best = Some(i);
            }
        }
    }
    best
}

fn xlookup(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() < 3 {
        return Err(ErrCode::VALUE);
    }
    let target = ev.scalar(&a[0]);
    if let Scalar::Err(e) = target {
        return Err(e);
    }
    let lookup_arr = flatten(&ev.eval(&a[1]));
    let return_mat = as_mat(ev.eval(&a[2]));
    let vertical = return_mat.len() == lookup_arr.len();
    let match_mode = match a.get(4) {
        Some(n) => ev.num(n)?,
        None => 0.0,
    };
    let search_mode = match a.get(5) {
        Some(n) => ev.num(n)?,
        None => 1.0,
    };
    let Some(idx) = xfind(&lookup_arr, &target, match_mode, search_mode) else {
        return match a.get(3) {
            Some(n) => Ok(ev.eval(n)),
            None => Err(ErrCode::NA),
        };
    };
    if vertical {
        let Some(row) = return_mat.get(idx) else { return Err(ErrCode::NA) };
        return Ok(if row.len() == 1 { Value::S(row[0].clone()) } else { Value::M(vec![row.clone()]) });
    }
    // Horizontal return array: the matching column.
    let out: Matrix = return_mat.iter().map(|row| vec![cell_at(row, idx)]).collect();
    Ok(if out.len() == 1 { Value::S(out[0][0].clone()) } else { Value::M(out) })
}

fn xmatch(ev: &Evaluator, a: &[Node]) -> R {
    let target = ev.scalar(arg(a, 0)?);
    if let Scalar::Err(e) = target {
        return Err(e);
    }
    let vec = flatten(&ev.eval(arg(a, 1)?));
    let match_mode = match a.get(2) {
        Some(n) => ev.num(n)?,
        None => 0.0,
    };
    let search_mode = match a.get(3) {
        Some(n) => ev.num(n)?,
        None => 1.0,
    };
    Ok(match xfind(&vec, &target, match_mode, search_mode) {
        None => return Err(ErrCode::NA),
        Some(i) => Value::num((i + 1) as f64),
    })
}

// ── FILTER / SORT / SORTBY / UNIQUE ───────────────────────────────────────────

fn filter(ev: &Evaluator, a: &[Node]) -> R {
    let m = as_mat(ev.eval(arg(a, 0)?));
    if let Some(e) = first_err(&m) {
        return Err(e);
    }
    let include = as_mat(ev.eval(arg(a, 1)?));
    // `include` is a column or row vector; match by row count.
    let flags: Vec<Option<Scalar>> = if include.len() == m.len() {
        include.iter().map(|r| r.first().cloned()).collect()
    } else {
        include.first().map(|r| r.iter().cloned().map(Some).collect()).unwrap_or_default()
    };
    let mut out: Matrix = Vec::new();
    for (i, row) in m.iter().enumerate() {
        if let Some(Some(f)) = flags.get(i) {
            if !f.is_err() && to_bool(f) {
                out.push(row.clone());
            }
        }
    }
    if out.is_empty() {
        return match a.get(2) {
            Some(n) => Ok(ev.eval(n)),
            None => Err(ErrCode::NA),
        };
    }
    Ok(Value::M(out))
}

fn sort_fn(ev: &Evaluator, a: &[Node]) -> R {
    let m = as_mat(ev.eval(arg(a, 0)?));
    let sort_index = match a.get(1) {
        Some(n) => ev.num(n)?,
        None => 1.0,
    };
    let order = match a.get(2) {
        Some(n) => ev.num(n)?,
        None => 1.0,
    };
    let col = js::trunc(sort_index) - 1.0;
    let dir = if order < 0.0 { -1.0 } else { 1.0 };
    let empty = Scalar::empty();
    let key = |i: usize| -> &Scalar { index(col).and_then(|c| m[i].get(c)).unwrap_or(&empty) };
    let idx = sort_indices(m.len(), |x, y| compare_scalar(key(x), key(y)) * dir);
    Ok(Value::M(idx.into_iter().map(|i| m[i].clone()).collect()))
}

fn sortby(ev: &Evaluator, a: &[Node]) -> R {
    let m = as_mat(ev.eval(arg(a, 0)?));
    // The (by-vector, order) key pairs.
    let mut keys: Vec<(Vec<Scalar>, f64)> = Vec::new();
    let mut i = 1;
    while i < a.len() {
        let vec = flatten(&ev.eval(&a[i]));
        let dir = match a.get(i + 1) {
            Some(n) => ev.num(n)?,
            None => 1.0,
        };
        keys.push((vec, if dir < 0.0 { -1.0 } else { 1.0 }));
        i += 2;
    }
    let empty = Scalar::empty();
    let idx = sort_indices(m.len(), |x, y| {
        for (vec, dir) in &keys {
            let c = compare_scalar(vec.get(x).unwrap_or(&empty), vec.get(y).unwrap_or(&empty)) * dir;
            if c != 0.0 {
                return c;
            }
        }
        0.0
    });
    Ok(Value::M(idx.into_iter().map(|i| m[i].clone()).collect()))
}

/// `keyOf`: the lower-cased texts joined by NUL.
fn key_of(arr: &[Scalar]) -> String {
    arr.iter().map(|s| js::lower(&to_str(s))).collect::<Vec<_>>().join("\0")
}

fn dedupe(rows: Vec<Vec<Scalar>>, exactly_once: bool) -> Vec<Vec<Scalar>> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for r in &rows {
        *counts.entry(key_of(r)).or_insert(0) += 1;
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for r in rows {
        let k = key_of(&r);
        if !seen.insert(k.clone()) {
            continue;
        }
        if exactly_once && counts.get(&k) != Some(&1) {
            continue;
        }
        out.push(r);
    }
    out
}

fn unique(ev: &Evaluator, a: &[Node]) -> R {
    let m = as_mat(ev.eval(arg(a, 0)?));
    let by_col = match a.get(1) {
        Some(n) => to_bool(&ev.scalar(n)),
        None => false,
    };
    let exactly_once = match a.get(2) {
        Some(n) => to_bool(&ev.scalar(n)),
        None => false,
    };
    if by_col {
        // Transpose, unique rows, transpose back.
        let cols = first_width(&m);
        let col_arr: Vec<Vec<Scalar>> = (0..cols).map(|c| m.iter().map(|r| cell_at(r, c)).collect()).collect();
        let kept = dedupe(col_arr, exactly_once);
        let out: Matrix = (0..m.len()).map(|r| kept.iter().map(|col| cell_at(col, r)).collect()).collect();
        return if out.is_empty() { Err(ErrCode::NA) } else { Ok(Value::M(out)) };
    }
    let out = dedupe(m, exactly_once);
    if out.is_empty() {
        Err(ErrCode::NA)
    } else {
        Ok(Value::M(out))
    }
}

// ── HSTACK / VSTACK ───────────────────────────────────────────────────────────

fn vstack(ev: &Evaluator, a: &[Node]) -> R {
    let mats: Vec<Matrix> = a.iter().map(|n| as_mat(ev.eval(n))).collect();
    let width = mats.iter().map(first_width).max().unwrap_or(0);
    let mut out: Matrix = Vec::new();
    for m in &mats {
        for row in m {
            let mut r = row.clone();
            while r.len() < width {
                r.push(Scalar::Err(ErrCode::NA));
            }
            out.push(r);
        }
    }
    Ok(Value::M(out))
}

fn hstack(ev: &Evaluator, a: &[Node]) -> R {
    let mats: Vec<Matrix> = a.iter().map(|n| as_mat(ev.eval(n))).collect();
    let height = mats.iter().map(Vec::len).max().unwrap_or(0);
    let mut out: Matrix = Vec::new();
    for r in 0..height {
        let mut row = Vec::new();
        for m in &mats {
            let w = first_width(m);
            let src = m.get(r);
            for c in 0..w {
                row.push(match src {
                    Some(s) => cell_at(s, c),
                    None => Scalar::Err(ErrCode::NA),
                });
            }
        }
        out.push(row);
    }
    Ok(Value::M(out))
}

// ── TAKE / DROP / CHOOSECOLS / CHOOSEROWS ─────────────────────────────────────

/// Keep (take) or remove (drop) `n` rows from the start (n > 0) or the end (n < 0).
fn slice_rows(m: Matrix, n: Option<f64>, take: bool) -> Matrix {
    let Some(n) = n else { return m };
    let k = js::trunc(n);
    let len = m.len() as f64;
    if take {
        if k >= 0.0 {
            js_slice(&m, 0.0, Some(k))
        } else {
            js_slice(&m, len + k, None)
        }
    } else if k >= 0.0 {
        js_slice(&m, k, None)
    } else {
        js_slice(&m, 0.0, Some(len + k))
    }
}

fn slice_cols(m: Matrix, n: Option<f64>, take: bool) -> Matrix {
    let Some(n) = n else { return m };
    let k = js::trunc(n);
    m.into_iter()
        .map(|row| {
            let len = row.len() as f64;
            if take {
                if k >= 0.0 {
                    js_slice(&row, 0.0, Some(k))
                } else {
                    js_slice(&row, len + k, None)
                }
            } else if k >= 0.0 {
                js_slice(&row, k, None)
            } else {
                js_slice(&row, 0.0, Some(len + k))
            }
        })
        .collect()
}

/// The optional numeric arguments of TAKE / DROP / EXPAND (rows first, then columns; the first error
/// wins).
fn opt_pair(ev: &Evaluator, a: &[Node]) -> Result<(Option<f64>, Option<f64>), ErrCode> {
    let rows = a.get(1).map(|n| ev.num(n));
    let cols = a.get(2).map(|n| ev.num(n));
    let rows = rows.transpose()?;
    let cols = cols.transpose()?;
    Ok((rows, cols))
}

fn take(ev: &Evaluator, a: &[Node]) -> R {
    let m = as_mat(ev.eval(arg(a, 0)?));
    let (rows, cols) = opt_pair(ev, a)?;
    let out = slice_cols(slice_rows(m, rows, true), cols, true);
    if out.is_empty() {
        Err(ErrCode::VALUE)
    } else {
        Ok(Value::M(out))
    }
}

fn drop_fn(ev: &Evaluator, a: &[Node]) -> R {
    let m = as_mat(ev.eval(arg(a, 0)?));
    let (rows, cols) = opt_pair(ev, a)?;
    let out = slice_cols(slice_rows(m, rows, false), cols, false);
    if out.is_empty() {
        Err(ErrCode::VALUE)
    } else {
        Ok(Value::M(out))
    }
}

/// The 0-based pick of `choosecols` / `chooserows` for the number `n` over `len` items.
fn pick_index(n: f64, len: usize) -> Result<usize, ErrCode> {
    let idx = if n < 0.0 { len as f64 + js::trunc(n) } else { js::trunc(n) - 1.0 };
    if idx < 0.0 || idx >= len as f64 {
        return Err(ErrCode::VALUE);
    }
    Ok(idx as usize)
}

fn choosecols(ev: &Evaluator, a: &[Node]) -> R {
    let m = as_mat(ev.eval(arg(a, 0)?));
    let width = first_width(&m);
    let mut picks: Vec<usize> = Vec::new();
    for node in a.iter().skip(1) {
        for s in flatten(&ev.eval(node)) {
            let n = to_num(&s)?;
            picks.push(pick_index(n, width)?);
        }
    }
    Ok(Value::M(m.iter().map(|row| picks.iter().map(|&c| cell_at(row, c)).collect()).collect()))
}

fn chooserows(ev: &Evaluator, a: &[Node]) -> R {
    let m = as_mat(ev.eval(arg(a, 0)?));
    let height = m.len();
    let mut out: Matrix = Vec::new();
    for node in a.iter().skip(1) {
        for s in flatten(&ev.eval(node)) {
            let n = to_num(&s)?;
            out.push(m[pick_index(n, height)?].clone());
        }
    }
    Ok(Value::M(out))
}

// ── TOCOL / TOROW ─────────────────────────────────────────────────────────────

fn flatten_ordered(ev: &Evaluator, a: &[Node]) -> Result<Vec<Scalar>, ErrCode> {
    let m = as_mat(ev.eval(arg(a, 0)?));
    // `Number(toNum(...))` of an error object is NaN, which matches no skip mode.
    let ignore = match a.get(1) {
        Some(n) => match to_num(&ev.scalar(n)) {
            Ok(v) => js::trunc(v),
            Err(_) => f64::NAN,
        },
        None => 0.0,
    };
    let by_col = match a.get(2) {
        Some(n) => to_bool(&ev.scalar(n)),
        None => false,
    };
    let rows = m.len();
    let cols = first_width(&m);
    let mut seq: Vec<Scalar> = Vec::new();
    if by_col {
        for c in 0..cols {
            for row in &m {
                seq.push(cell_at(row, c));
            }
        }
    } else {
        for row in m.iter().take(rows) {
            for c in 0..cols {
                seq.push(cell_at(row, c));
            }
        }
    }
    let skip_blank = ignore == 1.0 || ignore == 3.0;
    let skip_err = ignore == 2.0 || ignore == 3.0;
    Ok(seq.into_iter().filter(|s| !(skip_blank && s.is_blank()) && !(skip_err && s.is_err())).collect())
}

fn tocol(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::M(flatten_ordered(ev, a)?.into_iter().map(|s| vec![s]).collect()))
}

fn torow(ev: &Evaluator, a: &[Node]) -> R {
    Ok(Value::M(vec![flatten_ordered(ev, a)?]))
}

// ── EXPAND ────────────────────────────────────────────────────────────────────

fn expand(ev: &Evaluator, a: &[Node]) -> R {
    let m = as_mat(ev.eval(arg(a, 0)?));
    let (cur_rows, cur_cols) = (m.len(), first_width(&m));
    let (rows, cols) = opt_pair(ev, a)?;
    let pad = match a.get(3) {
        Some(n) => ev.scalar(n),
        None => Scalar::Err(ErrCode::NA),
    };
    let target_rows = rows.map(js::trunc).unwrap_or(cur_rows as f64);
    let target_cols = cols.map(js::trunc).unwrap_or(cur_cols as f64);
    if target_rows < cur_rows as f64 || target_cols < cur_cols as f64 {
        return Err(ErrCode::VALUE);
    }
    // The web would run out of memory (or loop forever) on absurd sizes; stop at 50 million cells.
    if target_rows * target_cols > 5e7 {
        return Err(ErrCode::VALUE);
    }
    let (tr, tc) = (target_rows as usize, target_cols as usize);
    let out: Matrix = (0..tr)
        .map(|r| (0..tc).map(|c| if r < cur_rows && c < cur_cols { cell_at(&m[r], c) } else { pad.clone() }).collect())
        .collect();
    Ok(Value::M(out))
}

// ── ROW / COLUMN ──────────────────────────────────────────────────────────────

fn row_fn(_ev: &Evaluator, a: &[Node]) -> R {
    if a.is_empty() {
        return Ok(Value::num(1.0)); // no caller position available
    }
    Ok(Value::num(range_bounds(a.first()).map(|b| b.r1).unwrap_or(1.0)))
}

fn column_fn(_ev: &Evaluator, a: &[Node]) -> R {
    if a.is_empty() {
        return Ok(Value::num(1.0));
    }
    Ok(Value::num(range_bounds(a.first()).map(|b| b.c1).unwrap_or(1.0)))
}

// ── ADDRESS ───────────────────────────────────────────────────────────────────

fn address(ev: &Evaluator, a: &[Node]) -> R {
    let row = ev.num(arg(a, 0)?)?;
    let col = ev.num(arg(a, 1)?)?;
    let abs_num = match a.get(2) {
        Some(n) => ev.num(n)?,
        None => 1.0,
    };
    let a1 = match a.get(3) {
        Some(n) => to_bool(&ev.scalar(n)),
        None => true,
    };
    let sheet = match a.get(4) {
        Some(n) => ev.str(n),
        None => String::new(),
    };
    let (r, c) = (js::trunc(row), js::trunc(col));
    if r < 1.0 || c < 1.0 || !c.is_finite() {
        return Err(ErrCode::VALUE);
    }
    let reference = if a1 {
        let col_abs = if abs_num == 1.0 || abs_num == 3.0 { "$" } else { "" };
        let row_abs = if abs_num == 1.0 || abs_num == 2.0 { "$" } else { "" };
        format!("{col_abs}{}{row_abs}{}", index_to_col_f(c - 1.0), js::to_string(r))
    } else {
        format!("R{}C{}", js::to_string(r), js::to_string(c))
    };
    Ok(Value::str(if sheet.is_empty() { reference } else { format!("{sheet}!{reference}") }))
}

// ── INDIRECT ──────────────────────────────────────────────────────────────────

/// `^[A-Z]+\d+` at the start of `s`: the matched prefix and the rest.
fn cell_prefix(s: &str) -> Option<(&str, &str)> {
    let letters = s.bytes().take_while(u8::is_ascii_uppercase).count();
    if letters == 0 {
        return None;
    }
    let digits = s.as_bytes()[letters..].iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    Some(s.split_at(letters + digits))
}

fn indirect(ev: &Evaluator, a: &[Node]) -> R {
    let text = js::upper(js::trim(&ev.str(arg(a, 0)?)));
    if text.is_empty() {
        return Err(ErrCode::REF);
    }
    let Some((first, rest)) = cell_prefix(&text) else { return Err(ErrCode::REF) };
    let node = if rest.is_empty() {
        Node::Ref { r: first.to_string(), sheet: None }
    } else {
        let Some(tail) = rest.strip_prefix(':') else { return Err(ErrCode::REF) };
        match cell_prefix(tail) {
            Some((second, "")) => Node::Range { a: first.to_string(), b: second.to_string(), sheet: None },
            _ => return Err(ErrCode::REF),
        }
    };
    Ok(ev.eval(&node))
}

// ── OFFSET ────────────────────────────────────────────────────────────────────

fn offset(ev: &Evaluator, a: &[Node]) -> R {
    let Some(base) = range_bounds(a.first()) else { return Err(ErrCode::REF) };
    let d_row = ev.num(arg(a, 1)?)?;
    let d_col = ev.num(arg(a, 2)?)?;
    let base_h = base.r2 - base.r1 + 1.0;
    let base_w = base.c2 - base.c1 + 1.0;
    let h = match a.get(3) {
        Some(n) => ev.num(n)?,
        None => base_h,
    };
    let w = match a.get(4) {
        Some(n) => ev.num(n)?,
        None => base_w,
    };
    let r1 = base.r1 + js::trunc(d_row);
    let c1 = base.c1 + js::trunc(d_col);
    let r2 = r1 + js::trunc(h) - 1.0;
    let c2 = c1 + js::trunc(w) - 1.0;
    if r1 < 1.0 || c1 < 1.0 || r2 < r1 || c2 < c1 || !r2.is_finite() || !c2.is_finite() {
        return Err(ErrCode::REF);
    }
    let node = Node::Range {
        a: format!("{}{}", index_to_col_f(c1 - 1.0), js::to_string(r1)),
        b: format!("{}{}", index_to_col_f(c2 - 1.0), js::to_string(r2)),
        sheet: None,
    };
    Ok(ev.eval(&node))
}

// ── AREAS / FORMULATEXT ───────────────────────────────────────────────────────

fn areas(_ev: &Evaluator, _a: &[Node]) -> R {
    Ok(Value::num(1.0))
}

fn formulatext(ev: &Evaluator, a: &[Node]) -> R {
    let Some(b) = range_bounds(a.first()) else { return Err(ErrCode::NA) };
    let key = format!("{}{}", index_to_col_f(b.c1 - 1.0), js::to_string(b.r1));
    match ev.cell_raw(&key).and_then(|c| c.formula()) {
        Some(f) => Ok(Value::str(f)),
        None => Err(ErrCode::NA),
    }
}

// ── GROUPBY / PIVOTBY ─────────────────────────────────────────────────────────

type Agg<'x> = Box<dyn Fn(&[Scalar]) -> Scalar + 'x>;

/// `literalArray`: a literal array node from scalars, so a bare aggregator NAME can be called.
fn literal_array(vals: &[Scalar]) -> Node {
    let mut rows: Vec<Vec<Node>> = vals
        .iter()
        .map(|v| {
            vec![match v {
                Scalar::Num(n) => Node::Num(*n),
                Scalar::Bool(b) => Node::Bool(*b),
                other => Node::Str(to_str(other)),
            }]
        })
        .collect();
    if rows.is_empty() {
        rows.push(vec![Node::Num(0.0)]);
    }
    Node::Array(rows)
}

/// `isMatrix(r) ? r[0]?.[0] ?? ERR.VALUE : r` (a lambda cannot be a cell: `#CALC!`).
fn first_or_value(r: Value) -> Scalar {
    match r {
        Value::M(m) => m.into_iter().next().and_then(|row| row.into_iter().next()).unwrap_or(Scalar::Err(ErrCode::VALUE)),
        Value::S(s) => s,
        Value::L(_) => Scalar::Err(ErrCode::CALC),
    }
}

fn sum_nums(rows: &[Vec<Scalar>]) -> f64 {
    let mut s = 0.0;
    for row in rows {
        for v in row {
            if let Ok(n) = to_num(v) {
                s += n;
            }
        }
    }
    s
}

/// `makeAggregator`: a bare name (SUM), PERCENTOF, or a LAMBDA; `None` for anything else.
fn make_aggregator<'x>(ev: &'x Evaluator<'_>, node: &'x Node, all_vals: &'x [Vec<Scalar>]) -> Option<Agg<'x>> {
    if let Node::Var(name) = node {
        let name = js::upper(name);
        if name == "PERCENTOF" {
            return Some(Box::new(move |vals: &[Scalar]| {
                let mut sum = 0.0;
                for v in vals {
                    if let Ok(n) = to_num(v) {
                        sum += n;
                    }
                }
                let g = sum_nums(all_vals);
                if g == 0.0 {
                    Scalar::Err(ErrCode::DIV0)
                } else {
                    Scalar::Num(sum / g)
                }
            }));
        }
        return Some(Box::new(move |vals: &[Scalar]| first_or_value(ev.call(&name, &[literal_array(vals)]))));
    }
    if let Value::L(lam) = ev.eval(node) {
        return Some(Box::new(move |vals: &[Scalar]| {
            let col: Matrix = vals.iter().map(|x| vec![x.clone()]).collect();
            first_or_value(ev.apply_lambda(&lam, vec![Value::M(col)]))
        }));
    }
    None
}

/// `tupleKey`: the case-insensitive grouping key of a tuple of field values.
fn tuple_key(t: &[Scalar]) -> String {
    let mut s = String::from("[");
    for (i, v) in t.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        match v {
            Scalar::Num(n) if n.is_finite() => s.push_str(&js::to_string(*n)),
            Scalar::Num(_) => s.push_str("null"),
            Scalar::Str(x) => s.push_str(&format!("{:?}", js::lower(x))),
            Scalar::Bool(b) => s.push_str(if *b { "true" } else { "false" }),
            Scalar::Err(e) => s.push_str(&format!("{{\"err\":{:?}}}", e.as_str())),
        }
    }
    s.push(']');
    s
}

/// `compareTuples`: order 0 → ascending by all fields; ±k → by field k (1-based), sign = direction.
fn compare_tuples(a: &[Scalar], b: &[Scalar], order: f64) -> f64 {
    let empty = Scalar::empty();
    if order != 0.0 {
        let i = order.abs().min(a.len() as f64) - 1.0;
        let at = |t: &'_ [Scalar]| -> Scalar { index(i).and_then(|k| t.get(k)).cloned().unwrap_or_else(|| empty.clone()) };
        let c = compare_scalar(&at(a), &at(b));
        return if order < 0.0 { -c } else { c };
    }
    for i in 0..a.len() {
        let c = compare_scalar(&a[i], b.get(i).unwrap_or(&empty));
        if c != 0.0 {
            return c;
        }
    }
    0.0
}

struct GroupedInput {
    keys: Vec<Vec<Scalar>>,
    vals: Vec<Vec<Scalar>>,
    header_fields: Vec<String>,
    show_headers: bool,
}

/// `decodeGrouped`: the common decoding of row_fields / values / field_headers / filter_array.
fn decode_grouped(ev: &Evaluator, fields: &Node, values: &Node, headers_arg: Option<&Node>, filter_arg: Option<&Node>) -> Result<GroupedInput, ErrCode> {
    let f = as_mat(ev.eval(fields));
    let v = as_mat(ev.eval(values));
    if let Some(e) = first_err(&f).or_else(|| first_err(&v)) {
        return Err(e);
    }
    if f.len() != v.len() || f.is_empty() {
        return Err(ErrCode::VALUE);
    }
    let mut headers = 0.0;
    if let Some(h) = headers_arg {
        headers = ev.num(h)?;
    }
    let has_header = headers == 1.0 || headers == 3.0;
    let start = if has_header { 1 } else { 0 };
    let keep: Option<Vec<bool>> = filter_arg.map(|n| {
        let flt = flatten(&ev.eval(n));
        (start..f.len()).map(|i| flt.get(i - start).map(to_bool).unwrap_or(false)).collect()
    });
    let mut keys = Vec::new();
    let mut vals = Vec::new();
    for i in start..f.len() {
        if let Some(k) = &keep {
            if !k[i - start] {
                continue;
            }
        }
        keys.push(f[i].clone());
        vals.push(v[i].clone());
    }
    if keys.is_empty() {
        return Err(ErrCode::CALC);
    }
    Ok(GroupedInput {
        keys,
        vals,
        header_fields: if has_header { f[0].iter().map(to_str).collect() } else { Vec::new() },
        show_headers: headers == 3.0,
    })
}

struct Group {
    tuple: Vec<Scalar>,
    cols: Vec<Vec<Scalar>>,
}

/// The value column `c` of every data row (`g.vals.map(r => r[c])`).
fn column_of(vals: &[Vec<Scalar>], c: usize) -> Vec<Scalar> {
    vals.iter().map(|r| cell_at(r, c)).collect()
}

fn num_arg(ev: &Evaluator, a: &[Node], i: usize, def: f64) -> Result<f64, ErrCode> {
    match a.get(i) {
        Some(n) => ev.num(n),
        None => Ok(def),
    }
}

fn groupby(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() < 3 {
        return Err(ErrCode::VALUE);
    }
    let g = decode_grouped(ev, &a[0], &a[1], a.get(3), a.get(6))?;
    let Some(agg) = make_aggregator(ev, &a[2], &g.vals) else { return Err(ErrCode::VALUE) };
    let total_depth = num_arg(ev, a, 4, 1.0)?;
    let order = num_arg(ev, a, 5, 0.0)?;

    let nf = g.keys[0].len();
    let nv = g.vals[0].len();
    let mut groups: Vec<Group> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    for (i, tuple) in g.keys.iter().enumerate() {
        let k = tuple_key(tuple);
        let gi = *by_key.entry(k).or_insert_with(|| {
            groups.push(Group { tuple: tuple.clone(), cols: vec![Vec::new(); nv] });
            groups.len() - 1
        });
        for c in 0..nv {
            groups[gi].cols[c].push(cell_at(&g.vals[i], c));
        }
    }
    let sorted = sort_indices(groups.len(), |x, y| compare_tuples(&groups[x].tuple, &groups[y].tuple, order));

    let mut out: Matrix = Vec::new();
    if g.show_headers && !g.header_fields.is_empty() {
        out.push(g.header_fields.iter().map(|h| Scalar::Str(h.clone())).collect());
    }
    // Subtotals (total_depth >= 2) are emitted per block of the FIRST field.
    let want_sub = total_depth >= 2.0 && nf > 1;
    let mut cur_first: Option<String> = None;
    let mut sub_cols: Vec<Vec<Scalar>> = vec![Vec::new(); nv];
    let mut sub_label = Scalar::empty();
    let blanks = |n: usize| std::iter::repeat_n(Scalar::empty(), n);

    // `emitSub`: reading `subCols[0]` of a column-less value row is a TypeError (`#VALUE!`) on the web.
    let emit_sub = |out: &mut Matrix, sub_cols: &mut Vec<Vec<Scalar>>, cur_first: &Option<String>, sub_label: &Scalar| -> Result<(), ErrCode> {
        if !want_sub || cur_first.is_none() {
            return Ok(());
        }
        let Some(first) = sub_cols.first() else { return Err(ErrCode::VALUE) };
        if first.is_empty() {
            return Ok(());
        }
        let mut row = vec![Scalar::Str(to_str(sub_label) + " Total")];
        row.extend(blanks(nf.saturating_sub(1)));
        for col in sub_cols.iter() {
            row.push(agg(col));
        }
        out.push(row);
        *sub_cols = vec![Vec::new(); nv];
        Ok(())
    };

    for &gi in &sorted {
        let grp = &groups[gi];
        let first_key = tuple_key(&grp.tuple[..1.min(grp.tuple.len())]);
        if want_sub && cur_first.as_ref().is_some_and(|c| *c != first_key) {
            emit_sub(&mut out, &mut sub_cols, &cur_first, &sub_label)?;
        }
        if want_sub {
            cur_first = Some(first_key);
            sub_label = grp.tuple.first().cloned().unwrap_or_else(Scalar::empty);
        }
        let mut row: Vec<Scalar> = grp.tuple.clone();
        for c in 0..nv {
            row.push(agg(&grp.cols[c]));
            if want_sub {
                sub_cols[c].extend(grp.cols[c].iter().cloned());
            }
        }
        out.push(row);
    }
    emit_sub(&mut out, &mut sub_cols, &cur_first, &sub_label)?;
    if total_depth >= 1.0 {
        let mut row = vec![Scalar::str("Total")];
        row.extend(blanks(nf.saturating_sub(1)));
        for c in 0..nv {
            row.push(agg(&column_of(&g.vals, c)));
        }
        out.push(row);
    }
    Ok(Value::M(out))
}

struct RowGroup {
    tuple: Vec<Scalar>,
    by_col: HashMap<String, Vec<Scalar>>,
}

fn pivotby(ev: &Evaluator, a: &[Node]) -> R {
    if a.len() < 4 {
        return Err(ErrCode::VALUE);
    }
    let g = decode_grouped(ev, &a[0], &a[2], a.get(4), a.get(9))?;
    let c_mat = as_mat(ev.eval(&a[1]));
    if let Some(e) = first_err(&c_mat) {
        return Err(e);
    }
    let headers = num_arg(ev, a, 4, 0.0)?;
    let has_header = headers == 1.0 || headers == 3.0;
    let mut col_keys_all: Vec<Scalar> = Vec::new();
    for row in c_mat.iter().skip(if has_header { 1 } else { 0 }) {
        col_keys_all.push(if row.len() > 1 {
            Scalar::Str(row.iter().map(to_str).collect::<Vec<_>>().join(" | "))
        } else {
            cell_at(row, 0)
        });
    }
    if col_keys_all.len() != g.keys.len() {
        return Err(ErrCode::VALUE);
    }
    let Some(agg) = make_aggregator(ev, &a[3], &g.vals) else { return Err(ErrCode::VALUE) };
    let row_total = num_arg(ev, a, 5, 1.0)?;
    let row_order = num_arg(ev, a, 6, 0.0)?;
    let col_total = num_arg(ev, a, 7, 1.0)?;
    let col_order = num_arg(ev, a, 8, 0.0)?;

    let nf = g.keys[0].len();
    let mut row_groups: Vec<RowGroup> = Vec::new();
    let mut row_by_key: HashMap<String, usize> = HashMap::new();
    let mut col_set: Vec<(String, Scalar)> = Vec::new();
    let mut col_by_key: HashMap<String, usize> = HashMap::new();
    for (i, tuple) in g.keys.iter().enumerate() {
        let rk = tuple_key(tuple);
        let ck = js::lower(&to_str(&col_keys_all[i]));
        if !col_by_key.contains_key(&ck) {
            col_by_key.insert(ck.clone(), col_set.len());
            col_set.push((ck.clone(), col_keys_all[i].clone()));
        }
        let gi = *row_by_key.entry(rk).or_insert_with(|| {
            row_groups.push(RowGroup { tuple: tuple.clone(), by_col: HashMap::new() });
            row_groups.len() - 1
        });
        row_groups[gi].by_col.entry(ck).or_default().push(cell_at(&g.vals[i], 0));
    }
    let row_order_idx = sort_indices(row_groups.len(), |x, y| compare_tuples(&row_groups[x].tuple, &row_groups[y].tuple, row_order));
    let col_order_idx = sort_indices(col_set.len(), |x, y| {
        let c = compare_scalar(&col_set[x].1, &col_set[y].1);
        if col_order < 0.0 {
            -c
        } else {
            c
        }
    });

    let mut out: Matrix = Vec::new();
    let mut header: Vec<Scalar> = (0..nf)
        .map(|i| match g.header_fields.get(i) {
            Some(h) if g.show_headers => Scalar::Str(h.clone()),
            _ => Scalar::empty(),
        })
        .collect();
    for &ci in &col_order_idx {
        header.push(col_set[ci].1.clone());
    }
    if row_total >= 1.0 {
        header.push(Scalar::str("Total"));
    }
    out.push(header);
    let mut col_all: HashMap<String, Vec<Scalar>> = HashMap::new();
    for &ri in &row_order_idx {
        let grp = &row_groups[ri];
        let mut row: Vec<Scalar> = grp.tuple.clone();
        let mut row_vals: Vec<Scalar> = Vec::new();
        for &ci in &col_order_idx {
            let ck = &col_set[ci].0;
            match grp.by_col.get(ck) {
                Some(cell) => {
                    row.push(agg(cell));
                    row_vals.extend(cell.iter().cloned());
                    col_all.entry(ck.clone()).or_default().extend(cell.iter().cloned());
                }
                None => row.push(Scalar::empty()),
            }
        }
        if row_total >= 1.0 {
            row.push(agg(&row_vals));
        }
        out.push(row);
    }
    if col_total >= 1.0 {
        let mut row = vec![Scalar::str("Total")];
        row.extend(std::iter::repeat_n(Scalar::empty(), nf.saturating_sub(1)));
        for &ci in &col_order_idx {
            row.push(match col_all.get(&col_set[ci].0) {
                Some(acc) => agg(acc),
                None => Scalar::empty(),
            });
        }
        if row_total >= 1.0 {
            row.push(agg(&column_of(&g.vals, 0)));
        }
        out.push(row);
    }
    Ok(Value::M(out))
}

// ── Registry ──────────────────────────────────────────────────────────────────

pub fn register(r: &mut Registry) {
    let list: &[(&'static str, crate::formula::eval::Fn)] = &[
        ("CHOOSE", choose),
        ("COLUMNS", columns),
        ("ROWS", rows),
        ("TRANSPOSE", transpose),
        ("LOOKUP", lookup),
        ("XLOOKUP", xlookup),
        ("XMATCH", xmatch),
        ("FILTER", filter),
        ("SORT", sort_fn),
        ("SORTBY", sortby),
        ("UNIQUE", unique),
        ("HSTACK", hstack),
        ("VSTACK", vstack),
        ("TAKE", take),
        ("DROP", drop_fn),
        ("CHOOSECOLS", choosecols),
        ("CHOOSEROWS", chooserows),
        ("TOCOL", tocol),
        ("TOROW", torow),
        ("EXPAND", expand),
        ("ROW", row_fn),
        ("COLUMN", column_fn),
        ("ADDRESS", address),
        ("INDIRECT", indirect),
        ("OFFSET", offset),
        ("AREAS", areas),
        ("FORMULATEXT", formulatext),
        ("GROUPBY", groupby),
        ("PIVOTBY", pivotby),
    ];
    for (name, f) in list {
        r.insert(name, *f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_is_stable_and_total_on_inconsistent_comparators() {
        let keys = [3.0, 1.0, 2.0, 1.0, 3.0];
        assert_eq!(sort_indices(5, |x, y| keys[x] - keys[y]), vec![1, 3, 2, 0, 4]);
        // A comparator that is not a total order must not panic.
        let order = sort_indices(100, |x, y| if (x + y) % 3 == 0 { f64::NAN } else { (x as f64) - (y as f64) * 0.5 });
        assert_eq!(order.len(), 100);
        // Above the insertion-sort threshold the merge sort is stable too.
        let big: Vec<f64> = (0..200).map(|i| (i % 7) as f64).collect();
        let o = sort_indices(200, |x, y| big[x] - big[y]);
        assert!(o.windows(2).all(|w| big[w[0]] < big[w[1]] || (big[w[0]] == big[w[1]] && w[0] < w[1])));
    }

    #[test]
    fn column_letters_and_refs() {
        assert_eq!(index_to_col_f(0.0), "A");
        assert_eq!(index_to_col_f(27.0), "AB");
        assert_eq!(index_to_col_f(f64::INFINITY), "");
        assert_eq!(parse_ref("AB12"), Some((11.0, 27.0)));
        assert_eq!(parse_ref("A"), None);
        assert_eq!(parse_ref("12"), None);
        assert!(cell_prefix("A1:B2").is_some());
    }

    #[test]
    fn js_slice_follows_array_slice() {
        let v = [1, 2, 3, 4];
        assert_eq!(js_slice(&v, -2.0, None), vec![3, 4]);
        assert_eq!(js_slice(&v, -10.0, None), vec![1, 2, 3, 4]);
        assert_eq!(js_slice(&v, 0.0, Some(f64::INFINITY)), vec![1, 2, 3, 4]);
        assert_eq!(js_slice(&v, 3.0, Some(1.0)), Vec::<i32>::new());
    }
}
