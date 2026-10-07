//! `translateRefs(formula, dCol, dRow)` (`formula-engine.ts:934-964`): shifts the relative references (those
//! without `$`) of a formula, skipping strings, sheet-qualified references and function names, clamping at
//! column A and row 1. Used to anchor a conditional-format formula to each cell of its range.

use crate::addr::{col_to_index, index_to_col};

fn is_boundary_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '!' | '\'' | '$' | '.')
}

/// `translateRefs`.
pub fn translate_refs(formula: &str, d_col: i64, d_row: i64) -> String {
    if d_col == 0 && d_row == 0 {
        return formula.to_string();
    }
    let f: Vec<char> = formula.chars().collect();
    let mut out = String::with_capacity(formula.len() + 8);
    let mut i = 0;
    let mut in_str = false;
    while i < f.len() {
        let c = f[i];
        if in_str {
            out.push(c);
            if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
            i += 1;
            continue;
        }
        let prev = out.chars().last().unwrap_or(' ');
        let boundary = !is_boundary_char(prev);
        if boundary && (c == '$' || c.is_ascii_alphabetic()) {
            let mut j = i;
            let col_abs = f[j] == '$';
            if col_abs {
                j += 1;
            }
            let ls = j;
            while j < f.len() && f[j].is_ascii_alphabetic() {
                j += 1;
            }
            let letters: String = f[ls..j].iter().collect();
            let row_abs = f.get(j) == Some(&'$');
            let ds = if row_abs { j + 1 } else { j };
            let mut k = ds;
            while k < f.len() && f[k].is_ascii_digit() {
                k += 1;
            }
            let digits: String = f[ds.min(f.len())..k.min(f.len())].iter().collect();
            let next = f.get(k).copied().unwrap_or(' ');
            if !letters.is_empty() && letters.len() <= 3 && !digits.is_empty() && digits.len() <= 7 && next != '(' {
                let mut col = col_to_index(&letters.to_ascii_uppercase());
                let mut row: i64 = digits.parse().unwrap_or(1);
                if !col_abs {
                    col += d_col;
                }
                if !row_abs {
                    row += d_row;
                }
                col = col.max(0);
                row = row.max(1);
                if col_abs {
                    out.push('$');
                }
                out.push_str(&index_to_col(col));
                if row_abs {
                    out.push('$');
                }
                out.push_str(&row.to_string());
                i = k;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_parts_move() {
        assert_eq!(translate_refs("=A1>10", 0, 2), "=A3>10");
        assert_eq!(translate_refs("=$A1+B$2+$C$3", 1, 1), "=$A2+C$2+$C$3");
        assert_eq!(translate_refs("=SUM(A1:B2)", 1, 0), "=SUM(B1:C2)");
        assert_eq!(translate_refs("=\"A1\"&A1", 0, 1), "=\"A1\"&A2");
        assert_eq!(translate_refs("=Feuil1!A1", 0, 1), "=Feuil1!A1");
        assert_eq!(translate_refs("=LOG10(A1)", 0, 1), "=LOG10(A2)");
        assert_eq!(translate_refs("=A1", -5, -5), "=A1");
    }
}
