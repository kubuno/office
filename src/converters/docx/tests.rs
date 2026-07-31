//! Round-trip and corpus tests for the DOCX converter.
//!
//! The corpus test walks real Word files instead of hand-written XML: it is the
//! only way to catch the malformed-but-tolerated markup Word actually emits.
//! Point `KUBUNO_DOCX_CORPUS` at a directory of `.docx` files to enable it
//! (e.g. a LibreOffice checkout's `qa/data` trees); it is skipped when unset.

use serde_json::json;

use super::{export_docx, import_docx};
use crate::converters::types::PmNode;

fn para(text: &str) -> PmNode {
    PmNode {
        node_type: "paragraph".into(),
        attrs: None,
        content: Some(vec![PmNode {
            node_type: "text".into(),
            attrs: None,
            content: None,
            marks: None,
            text: Some(text.into()),
        }]),
        marks: None,
        text: None,
    }
}

/// Collect every text node of a document, depth first.
fn all_text(node: &PmNode) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(t) = &node.text {
        out.push(t.clone());
    }
    for c in node.children() {
        out.extend(all_text(c));
    }
    out
}

#[test]
fn export_then_import_keeps_paragraph_text() {
    let doc = PmNode {
        node_type: "doc".into(),
        attrs: None,
        content: Some(vec![para("Premier paragraphe"), para("Second paragraphe")]),
        marks: None,
        text: None,
    };
    let bytes = export_docx(&doc, "Titre").expect("export");
    let (body, _, _, _) = import_docx(&bytes).expect("import");
    let texts = all_text(&body);
    assert!(
        texts.iter().any(|t| t == "Premier paragraphe"),
        "texte perdu au round-trip: {texts:?}"
    );
    assert!(texts.iter().any(|t| t == "Second paragraphe"));
}

#[test]
fn export_produces_a_readable_zip() {
    let doc = PmNode {
        node_type: "doc".into(),
        attrs: Some(json!({})),
        content: Some(vec![para("x")]),
        marks: None,
        text: None,
    };
    let bytes = export_docx(&doc, "T").expect("export");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes[..])).expect("zip valide");
    let names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_string())
        .collect();
    for required in [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/document.xml",
        "word/_rels/document.xml.rels",
    ] {
        assert!(names.iter().any(|n| n == required), "partie manquante: {required}");
    }
}

/// Word 2013+ wraps a table of contents in `<w:sdt>`. Before the container walk
/// descended into `<w:sdtContent>`, every such block — the whole TOC — was
/// dropped silently.
#[test]
fn import_descends_into_block_level_sdt() {
    let Ok(root) = std::env::var("KUBUNO_DOCX_CORPUS") else {
        eprintln!("KUBUNO_DOCX_CORPUS absent — test ignoré");
        return;
    };
    let path = std::path::Path::new(&root).join("sw/qa/core/unocore/data/tdf149555.docx");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("{} absent — test ignoré", path.display());
        return;
    };
    let (body, _, _, _) = import_docx(&bytes).expect("import");
    let joined = all_text(&body).join(" ");
    assert!(
        joined.contains("Contents"),
        "contenu du <w:sdt> perdu à l'import: {joined:?}"
    );
}

#[test]
fn import_never_panics_on_the_corpus() {
    let Ok(root) = std::env::var("KUBUNO_DOCX_CORPUS") else {
        eprintln!("KUBUNO_DOCX_CORPUS absent — test ignoré");
        return;
    };
    let mut seen = 0usize;
    let mut ok = 0usize;
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "docx") {
                seen += 1;
                let Ok(bytes) = std::fs::read(&p) else { continue };
                // A malformed file may legitimately fail; it must never panic.
                match import_docx(&bytes) {
                    Ok(_) => ok += 1,
                    Err(e) => eprintln!("  échec {}: {e}", p.display()),
                }
            }
        }
    }
    eprintln!("corpus: {ok}/{seen} fichiers importés sans erreur");
    assert!(seen > 0, "aucun .docx trouvé dans KUBUNO_DOCX_CORPUS");
}

/// Field instructions must never reach the document. Before the field state
/// machine was wired, ` TOC \o "1-3" \h ` and friends were imported as text.
#[test]
fn field_instructions_never_leak_into_the_text() {
    let Ok(root) = std::env::var("KUBUNO_DOCX_CORPUS") else {
        eprintln!("KUBUNO_DOCX_CORPUS absent — test ignoré");
        return;
    };
    // Fragments that can ONLY come from a field instruction. `STYLEREF` alone is
    // deliberately absent: tdf32363.docx contains the sentence "A Styles field
    // (STYLEREF in DOCX)" as real prose, and a marker that matches prose turns a
    // regression guard into noise.
    const MARKERS: [&str; 5] = [
        "MERGEFORMAT", "PAGEREF _Toc", "TOC \\o", "HYPERLINK \\l", "\\* Arabic",
    ];
    let (mut seen, mut dirty) = (0usize, 0usize);
    let mut examples: Vec<String> = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "docx") {
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let Ok((body, _, _, _)) = import_docx(&bytes) else { continue };
                seen += 1;
                let text = all_text(&body).join(" ");
                if let Some(m) = MARKERS.iter().find(|m| text.contains(**m)) {
                    dirty += 1;
                    if examples.len() < 5 {
                        examples.push(format!("{} → {m}", p.display()));
                    }
                }
            }
        }
    }
    eprintln!("fuite d'instructions : {dirty}/{seen} documents");
    for e in &examples {
        eprintln!("  {e}");
    }
    assert!(seen > 0, "aucun .docx importé");
    assert_eq!(dirty, 0, "des instructions de champ atteignent le document: {examples:?}");
}

/// How much table-of-contents structure the import actually recovers.
#[test]
fn toc_attributes_are_recovered_from_the_corpus() {
    let Ok(root) = std::env::var("KUBUNO_DOCX_CORPUS") else { return };
    fn walk(n: &PmNode, c: &mut std::collections::BTreeMap<&'static str, usize>) {
        if let Some(serde_json::Value::Object(a)) = n.attrs.as_ref() {
            for k in ["tocTitle", "tocLevel", "tocPage", "tocLeader"] {
                if a.contains_key(k) {
                    *c.entry(k).or_default() += 1;
                }
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
            } else if p.extension().is_some_and(|x| x == "docx") {
                let Ok(bytes) = std::fs::read(&p) else { continue };
                if let Ok((body, _, _, _)) = import_docx(&bytes) {
                    files += 1;
                    walk(&body, &mut counts);
                }
            }
        }
    }
    eprintln!("TDM sur {files} documents : {counts:?}");
}

#[test]
#[ignore = "writes a .docx for manual/LibreOffice inspection"]
fn e2e_toc_and_header() {
    let mk = |t: &str, attrs: serde_json::Value| {
        serde_json::json!({"type":"paragraph","attrs":attrs,
            "content":[{"type":"text","text":t}]})
    };
    let doc: PmNode = serde_json::from_value(json!({"type":"doc","content":[
        {"type":"heading","attrs":{"level":2,"tocTitle":true},
         "content":[{"type":"text","text":"Table des matières"}]},
        mk("Introduction", json!({"tocLevel":1,"tocPage":1,"tocLeader":true})),
        mk("Contexte",     json!({"tocLevel":2,"tocPage":3,"tocLeader":true})),
        mk("Conclusion",   json!({"tocLevel":1,"tocPage":128,"tocLeader":true})),
        {"type":"heading","attrs":{"level":1},"content":[{"type":"text","text":"Introduction"}]},
        mk("Texte du chapitre.", json!({})),
    ]})).expect("doc");
    let header: PmNode = serde_json::from_value(json!({"type":"doc","content":[
        {"type":"paragraph","content":[{"type":"text","text":"Rapport — page {page} sur {pages}"}]}
    ]})).expect("hdr");
    let layout = crate::converters::docx::SectionInfo::default();
    let bytes = crate::converters::docx::export_docx_full(
        &doc, "Rapport", &layout, Some(&header), None).expect("export");
    std::fs::write(
        "/tmp/claude-1000/-home-martinien-projects-kubuno/7fe831eb-4444-47a5-8bef-72baded22b82/scratchpad/e2e2/toc.docx",
        &bytes).expect("write");
    eprintln!("ecrit {} octets", bytes.len());
}

#[test]
#[ignore = "writes a .docx for LibreOffice inspection"]
fn e2e_notes_comments_bookmarks() {
    use crate::converters::docx::write::ctx::{CommentReplyIn, CommentThreadIn};
    let doc: PmNode = serde_json::from_value(json!({"type":"doc","content":[
      {"type":"paragraph","content":[
        {"type":"text","text":"Un texte "},
        {"type":"text","marks":[{"type":"comment","attrs":{"commentId":"c1"}}],"text":"commenté"},
        {"type":"text","text":" avec une note"},
        {"type":"footnote","attrs":{"text":"Ceci est la note de bas de page."}},
        {"type":"text","text":" et un "},
        {"type":"text","marks":[{"type":"bookmark","attrs":{"name":"Cible finale"}}],"text":"signet"},
        {"type":"text","text":"."}]},
      {"type":"paragraph","content":[
        {"type":"text","marks":[{"type":"link","attrs":{"href":"#Cible finale"}}],"text":"aller au signet"}]}
    ]})).expect("doc");
    let threads = [CommentThreadIn {
        id: "c1".into(),
        author: "Jean Dupont".into(),
        text: "Es-tu sûr de ce passage ?".into(),
        created_at: 1_700_000_000_000,
        resolved: true,
        replies: vec![CommentReplyIn {
            author: "Marie Curie".into(),
            text: "Oui, vérifié.".into(),
            created_at: 1_700_000_100_000,
        }],
    }];
    let bytes = crate::converters::docx::write::export_docx_all(
        &doc, "Notes", &crate::converters::docx::SectionInfo::default(), None, None, &threads,
    )
    .expect("export");
    std::fs::write(
        "/tmp/claude-1000/-home-martinien-projects-kubuno/7fe831eb-4444-47a5-8bef-72baded22b82/scratchpad/e2e2/notes.docx",
        &bytes,
    )
    .expect("write");
    eprintln!("ecrit {} octets", bytes.len());
}

#[test]
#[ignore = "writes a .docx for LibreOffice inspection"]
fn e2e_even_odd_and_app_props() {
    let hf = |t: &str| -> PmNode {
        serde_json::from_value(json!({"type":"doc","content":[
            {"type":"paragraph","content":[{"type":"text","text":t}]}]}))
        .expect("hf")
    };
    let doc: PmNode = serde_json::from_value(json!({"type":"doc","content":[
        {"type":"paragraph","content":[{"type":"text","text":"Corps du document, quelques mots."}]}
    ]})).expect("doc");
    let layout = crate::converters::docx::SectionInfo {
        even_odd_headers: true,
        ..crate::converters::docx::SectionInfo::default()
    };
    let bytes = crate::converters::docx::export_docx_hf(
        &doc,
        "Pair impair",
        &layout,
        Some(&hf("Impaire — page {page}")),
        None,
        Some(&hf("Paire — page {page}")),
        None,
        &[],
    )
    .expect("export");
    std::fs::write(
        "/tmp/claude-1000/-home-martinien-projects-kubuno/7fe831eb-4444-47a5-8bef-72baded22b82/scratchpad/e2e2/evenodd.docx",
        &bytes,
    )
    .expect("write");
    eprintln!("ecrit {} octets", bytes.len());
}

/// Footnotes were exported but never IMPORTED: a `.docx` carrying them lost them
/// on open, and re-saving dropped them. This measures the recovery.
#[test]
fn notes_are_recovered_from_the_corpus() {
    let Ok(root) = std::env::var("KUBUNO_DOCX_CORPUS") else { return };
    fn count(n: &PmNode, kind: &str, acc: &mut usize) {
        if n.node_type == kind {
            *acc += 1;
        }
        for c in n.children() {
            count(c, kind, acc);
        }
    }
    let (mut files, mut fnotes, mut enotes, mut with) = (0usize, 0usize, 0usize, 0usize);
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "docx") {
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let Ok((body, _, _, _)) = import_docx(&bytes) else { continue };
                files += 1;
                let (mut f, mut n) = (0, 0);
                count(&body, "footnote", &mut f);
                count(&body, "endnote", &mut n);
                if f + n > 0 {
                    with += 1;
                }
                fnotes += f;
                enotes += n;
            }
        }
    }
    eprintln!("notes: {fnotes} de bas de page + {enotes} de fin, dans {with}/{files} documents");
    assert!(files > 0);
}

/// A footnote and an endnote must survive export → import as themselves, in
/// their own parts and with their own numbering.
#[test]
fn notes_round_trip_as_distinct_kinds() {
    let doc: PmNode = serde_json::from_value(json!({"type":"doc","content":[
        {"type":"paragraph","content":[
            {"type":"text","text":"Bas"},
            {"type":"footnote","attrs":{"text":"Texte de la note de bas de page."}},
            {"type":"text","text":" et fin"},
            {"type":"endnote","attrs":{"text":"Texte de la note de fin."}},
            {"type":"text","text":"."}]}
    ]})).expect("doc");
    let bytes = export_docx(&doc, "Notes").expect("export");

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes[..])).expect("zip");
    let names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_string())
        .collect();
    assert!(names.iter().any(|n| n == "word/footnotes.xml"), "{names:?}");
    assert!(names.iter().any(|n| n == "word/endnotes.xml"), "{names:?}");

    let (body, _, _, _) = import_docx(&bytes).expect("import");
    fn find(n: &PmNode, kind: &str, out: &mut Vec<String>) {
        if n.node_type == kind {
            out.push(
                n.attrs
                    .as_ref()
                    .and_then(|a| a.get("text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            );
        }
        for c in n.children() {
            find(c, kind, out);
        }
    }
    let (mut f, mut e) = (Vec::new(), Vec::new());
    find(&body, "footnote", &mut f);
    find(&body, "endnote", &mut e);
    assert_eq!(f, vec!["Texte de la note de bas de page.".to_string()], "note de bas de page");
    // The kinds must NOT be conflated: an endnote coming back as a footnote is
    // exactly the silent degradation this work removes.
    assert_eq!(e, vec!["Texte de la note de fin.".to_string()], "note de fin");
}

#[test]
#[ignore = "writes a .docx for LibreOffice inspection"]
fn e2e_dump_notes_kinds() {
    let doc: PmNode = serde_json::from_value(json!({"type":"doc","content":[
        {"type":"paragraph","content":[
            {"type":"text","text":"Appel bas de page"},
            {"type":"footnote","attrs":{"text":"Note de bas de page numéro un."}},
            {"type":"text","text":" puis appel de fin"},
            {"type":"endnote","attrs":{"text":"Note de fin numéro un."}},
            {"type":"text","text":"."}]}
    ]})).expect("doc");
    let bytes = export_docx(&doc, "Deux sortes").expect("export");
    std::fs::write(
        "/tmp/claude-1000/-home-martinien-projects-kubuno/7fe831eb-4444-47a5-8bef-72baded22b82/scratchpad/e2e2/kinds.docx",
        &bytes,
    ).expect("write");
    eprintln!("ecrit {} octets", bytes.len());
}

/// `<w15:collapsed/>` in a heading's `w:pPr` — Word's « Réduire par défaut ».
/// The document must open with that heading COLLAPSED, and the flag must survive
/// a full round-trip through the exporter.
#[test]
fn collapsed_by_default_round_trips() {
    let docx = minimal_docx(COLLAPSED_DOC);
    let (body, _, _, _) = import_docx(&docx).expect("import");
    let heading = body
        .content
        .as_ref()
        .and_then(|c| c.iter().find(|n| n.node_type == "heading"))
        .expect("titre importé");
    let attrs = heading.attrs.as_ref().expect("attributs");
    assert_eq!(attrs.get("collapsedDefault"), Some(&serde_json::json!(true)));
    assert_eq!(attrs.get("collapsed"), Some(&serde_json::json!(true)), "doit s'ouvrir replié");

    let out = export_docx(&body, "Test").expect("export");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&out[..])).expect("zip valide");
    let mut xml = String::new();
    {
        use std::io::Read;
        zip.by_name("word/document.xml").expect("document.xml").read_to_string(&mut xml).expect("lecture");
    }
    assert!(xml.contains("<w15:collapsed/>"), "drapeau perdu à l'export");
    assert!(xml.contains(r#"xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml""#));
    assert!(xml.contains(r#"mc:Ignorable="w14 w15""#), "w15 doit être ignorable");
}

/// One `Heading1` paragraph marked collapsed-by-default, plus the body it hides.
const COLLAPSED_DOC: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
    xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml">
<w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/><w:outlineLvl w:val="0"/><w15:collapsed/></w:pPr>
<w:r><w:t>Titre replié</w:t></w:r></w:p>
<w:p><w:r><w:t>Contenu masqué</w:t></w:r></w:p>
</w:body></w:document>"#;

/// Smallest package `import_docx` accepts: `[Content_Types]` + the main part.
fn minimal_docx(document_xml: &str) -> Vec<u8> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("[Content_Types].xml", opts).expect("types");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
    ).expect("écriture");
    zip.start_file("word/document.xml", opts).expect("document");
    zip.write_all(document_xml.as_bytes()).expect("écriture");
    zip.finish().expect("finish").into_inner()
}
