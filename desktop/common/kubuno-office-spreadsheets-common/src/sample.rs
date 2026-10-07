//! The workbook the app opens when it is started without a file or a server workbook: a small budget
//! that shows what the grid draws (styles, merges, number formats, formulas, a cross-sheet reference,
//! conditional formatting, a frozen row).

use kubuno_office_sheets_core::model::workbook::Workbook;

/// The sample as the bundle the app reads (`{ spreadsheet, sheets, content }`).
pub const SAMPLE: &str = r##"{
  "spreadsheet": { "title": "Budget 2026" },
  "sheets": [
    { "id": "00000000-0000-4000-8000-000000000001", "name": "Budget", "position": 0 },
    { "id": "00000000-0000-4000-8000-000000000002", "name": "Paramètres", "position": 1 }
  ],
  "content": {
    "version": 1,
    "names": { "TAUX_TVA": "='Paramètres'!B1" },
    "sheets": {
      "00000000-0000-4000-8000-000000000001": {
        "frozen_rows": 2, "frozen_cols": 0,
        "col_widths": { "A": 180, "B": 110, "C": 110, "D": 110, "E": 90 },
        "row_heights": { "1": 34 },
        "merges": ["A1:E1"],
        "cf": [
          { "ranges": ["D3:D7"], "rules": [ { "type": "dataBar", "op": "", "formulas": [], "dxf": {}, "stop": false, "bar": { "color": "#638ec6" } } ] },
          { "ranges": ["E3:E7"], "rules": [ { "type": "iconSet", "op": "", "formulas": [], "dxf": {}, "stop": false, "icons": { "set": "3TrafficLights1" } } ] }
        ],
        "cells": {
          "A1": { "v": "Budget prévisionnel 2026", "s": { "bold": true, "fontSize": 18, "color": "#0f7b3f", "align": "center" } },
          "A2": { "v": "Poste", "s": { "bold": true, "bg": "#e6f4ea", "bb": "#5f6368" } },
          "B2": { "v": "Prévu", "s": { "bold": true, "bg": "#e6f4ea", "bb": "#5f6368", "align": "right" } },
          "C2": { "v": "Réalisé", "s": { "bold": true, "bg": "#e6f4ea", "bb": "#5f6368", "align": "right" } },
          "D2": { "v": "Écart", "s": { "bold": true, "bg": "#e6f4ea", "bb": "#5f6368", "align": "right" } },
          "E2": { "v": "Taux", "s": { "bold": true, "bg": "#e6f4ea", "bb": "#5f6368", "align": "right" } },
          "A3": { "v": "Loyer" }, "B3": { "v": 12000, "s": { "numFmt": "currency" } }, "C3": { "v": 12000, "s": { "numFmt": "currency" } },
          "A4": { "v": "Salaires" }, "B4": { "v": 85000, "s": { "numFmt": "currency" } }, "C4": { "v": 91250.5, "s": { "numFmt": "currency" } },
          "A5": { "v": "Matériel" }, "B5": { "v": 9500, "s": { "numFmt": "currency" } }, "C5": { "v": 7310, "s": { "numFmt": "currency" } },
          "A6": { "v": "Formation" }, "B6": { "v": 4000, "s": { "numFmt": "currency" } }, "C6": { "v": 4410, "s": { "numFmt": "currency" } },
          "A7": { "v": "Déplacements" }, "B7": { "v": 3000, "s": { "numFmt": "currency" } }, "C7": { "v": 1280, "s": { "numFmt": "currency" } },
          "D3": { "f": "=C3-B3", "s": { "numFmt": "currency" } }, "E3": { "f": "=C3/B3", "s": { "numFmt": "percent" } },
          "D4": { "f": "=C4-B4", "s": { "numFmt": "currency" } }, "E4": { "f": "=C4/B4", "s": { "numFmt": "percent" } },
          "D5": { "f": "=C5-B5", "s": { "numFmt": "currency" } }, "E5": { "f": "=C5/B5", "s": { "numFmt": "percent" } },
          "D6": { "f": "=C6-B6", "s": { "numFmt": "currency" } }, "E6": { "f": "=C6/B6", "s": { "numFmt": "percent" } },
          "D7": { "f": "=C7-B7", "s": { "numFmt": "currency" } }, "E7": { "f": "=C7/B7", "s": { "numFmt": "percent" } },
          "A8": { "v": "Total", "s": { "bold": true, "bt": "#5f6368" } },
          "B8": { "f": "=SUM(B3:B7)", "s": { "bold": true, "numFmt": "currency", "bt": "#5f6368" } },
          "C8": { "f": "=SUM(C3:C7)", "s": { "bold": true, "numFmt": "currency", "bt": "#5f6368" } },
          "D8": { "f": "=C8-B8", "s": { "bold": true, "numFmt": "currency", "bt": "#5f6368" } },
          "E8": { "f": "=C8/B8", "s": { "bold": true, "numFmt": "percent", "bt": "#5f6368" } },
          "A10": { "v": "Total TTC" }, "B10": { "f": "=B8*(1+TAUX_TVA)", "s": { "numFmt": "currency" } },
          "A11": { "v": "Mis à jour le" }, "B11": { "f": "=TODAY()", "s": { "numFmtCode": "dd/mm/yyyy" } },
          "A12": { "v": "Postes au-dessus du prévu" }, "B12": { "f": "=COUNTIF(D3:D7,\">0\")" },
          "A13": { "v": "Plus gros écart" }, "B13": { "f": "=INDEX(A3:A7,MATCH(MAX(D3:D7),D3:D7,0))", "c": "Le poste dont l'écart au prévu est le plus grand." }
        }
      },
      "00000000-0000-4000-8000-000000000002": {
        "cells": {
          "A1": { "v": "TVA" }, "B1": { "v": 0.2, "s": { "numFmt": "percent" } }
        }
      }
    }
  }
}"##;

/// The sample workbook.
pub fn workbook() -> Workbook {
    Workbook::from_bytes(SAMPLE.as_bytes(), "Budget 2026").unwrap_or_else(|_| Workbook::new_local("Classeur 1", "00000000-0000-4000-8000-000000000001"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_reads() {
        let wb = Workbook::from_bytes(SAMPLE.as_bytes(), "x").expect("the sample is a bundle");
        assert_eq!(wb.title, "Budget 2026");
        assert_eq!(wb.sheets.len(), 2);
        assert_eq!(wb.sheets[1].meta.name, "Paramètres");
    }
}
