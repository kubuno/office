//! Corpus tests for the `.doc` reader.
//!
//! Like the DOCX suite, the point is to run against real Word binaries rather
//! than hand-built ones: the piece table and the property bins are exactly the
//! places where synthetic files hide bugs.

use super::import_doc;
use crate::converters::types::PmNode;

fn all_text(node: &PmNode) -> String {
    let mut out = String::new();
    if let Some(t) = &node.text {
        out.push_str(t);
    }
    for c in node.children() {
        out.push_str(&all_text(c));
        out.push(' ');
    }
    out
}

#[test]
fn import_never_panics_on_the_corpus() {
    let Ok(root) = std::env::var("KUBUNO_DOC_CORPUS") else {
        eprintln!("KUBUNO_DOC_CORPUS absent — test ignoré");
        return;
    };
    let (mut seen, mut ok, mut chars) = (0usize, 0usize, 0usize);
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "doc") {
                seen += 1;
                let Ok(bytes) = std::fs::read(&p) else { continue };
                match import_doc(&bytes) {
                    Ok((body, _, _, _)) => {
                        ok += 1;
                        chars += all_text(&body).len();
                    }
                    Err(e) => eprintln!("  échec {}: {e}", p.display()),
                }
            }
        }
    }
    eprintln!("corpus .doc: {ok}/{seen} importés, {chars} caractères extraits");
    assert!(seen > 0, "aucun .doc trouvé dans KUBUNO_DOC_CORPUS");
}

/// The whole pipeline, measured: how much structure and formatting actually
/// comes out of real documents. A plain-text importer scores zero here.
#[test]
fn assembled_documents_carry_structure_and_formatting() {
    let Ok(root) = std::env::var("KUBUNO_DOC_CORPUS") else {
        eprintln!("KUBUNO_DOC_CORPUS absent — test ignoré");
        return;
    };
    fn walk(n: &PmNode, c: &mut std::collections::BTreeMap<String, usize>) {
        *c.entry(format!("node:{}", n.node_type)).or_default() += 1;
        for m in n.marks.iter().flatten() {
            *c.entry(format!("mark:{}", m.mark_type)).or_default() += 1;
        }
        if let Some(serde_json::Value::Object(a)) = n.attrs.as_ref() {
            for k in a.keys() {
                *c.entry(format!("attr:{k}")).or_default() += 1;
            }
        }
        for ch in n.children() {
            walk(ch, c);
        }
    }
    let mut counts = std::collections::BTreeMap::new();
    let mut files = 0usize;
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "doc") {
                let Ok(bytes) = std::fs::read(&p) else { continue };
                if let Ok((body, _, _, _)) = import_doc(&bytes) {
                    files += 1;
                    walk(&body, &mut counts);
                }
            }
        }
    }
    eprintln!("pipeline .doc sur {files} fichiers :");
    for (k, v) in &counts {
        if *v >= 20 {
            eprintln!("  {v:>7}  {k}");
        }
    }
    // A body made only of bare paragraphs would mean the layers are not wired.
    assert!(files > 0, "aucun .doc importé");
    assert!(
        counts.get("mark:bold").copied().unwrap_or(0) > 0,
        "aucune mise en forme de caractère n'atteint le document"
    );
    assert!(
        counts.get("node:table").copied().unwrap_or(0) > 0,
        "aucun tableau reconstruit"
    );
}

/// Headers, footers and table-of-contents entries recovered from real `.doc`.
#[test]
fn headers_and_toc_are_recovered() {
    let Ok(root) = std::env::var("KUBUNO_DOC_CORPUS") else { return };
    fn count_attr(n: &PmNode, key: &str, acc: &mut usize) {
        if let Some(serde_json::Value::Object(a)) = n.attrs.as_ref() {
            if a.contains_key(key) {
                *acc += 1;
            }
        }
        for c in n.children() {
            count_attr(c, key, acc);
        }
    }
    let (mut files, mut hdr, mut ftr, mut lvl, mut page, mut tok) = (0, 0, 0, 0, 0, 0);
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "doc") {
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let Ok((body, h, f, _)) = import_doc(&bytes) else { continue };
                files += 1;
                if let Some(h) = &h {
                    hdr += 1;
                    if all_text(h).contains("{page}") {
                        tok += 1;
                    }
                }
                if f.is_some() {
                    ftr += 1;
                }
                count_attr(&body, "tocLevel", &mut lvl);
                count_attr(&body, "tocPage", &mut page);
            }
        }
    }
    eprintln!(
        "corpus .doc : {files} lus · {hdr} en-têtes ({tok} avec jeton page) · {ftr} pieds · \
         TDM {lvl} entrées dont {page} avec numéro"
    );
}
