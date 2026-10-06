//! Word fields: `w:fldChar` / `w:instrText` / `w:fldSimple`, and the bookmark
//! ranges they point at.
//!
//! A field is not an element — it is a *run sequence*:
//!
//! ```text
//! fldChar "begin" → instrText… → fldChar "separate" → cached result → fldChar "end"
//! ```
//!
//! Four things make a naive reader produce garbage:
//!
//! - the instruction is split across an arbitrary number of runs, mid-word
//!   (Word breaks on rsid and spell-check boundaries), so it must be
//!   concatenated verbatim and only parsed once complete;
//! - the `separate` delimiter is OPTIONAL — a field with no result goes straight
//!   from `begin` to `end`, and a machine that waits for `separate` swallows the
//!   rest of the document;
//! - fields NEST (`{ TOC }` contains `{ PAGEREF }`), so the state is a stack;
//! - the instruction must be tokenised, not split on spaces: quotes group,
//!   `\\` is a literal backslash and a lone `\x` is a switch.
//!
//! The golden rule for everything we cannot recompute (`AUTHOR`, `SEQ`, `REF`,
//! `DOCPROPERTY`, `EQ`, …) is LibreOffice's rule for `ww::eUNKNOWN`: keep the
//! **cached result**, never the raw instruction and never nothing.
//!
//! Reference: LibreOffice `sw/source/writerfilter/dmapper/DomainMapper.cxx:4356`
//! (`separate` is optional), `:4327` (field lock), `DomainMapper_Impl.cxx:5742`
//! (`lcl_ExtractToken`), `:5841` (`splitFieldCommand`), `:5952`
//! (`lcl_FindInCommand`), `:6720` (the field type table), `:7420` (`handleToc`),
//! `:9158` (`PopFieldContext` resolves a field that never got a separator),
//! `:9406` (`StartOrEndBookmark`).

use roxmltree::Node;
use serde_json::json;

use crate::converters::docx::xml::{attr_val, local};
use crate::converters::types::PmMark;

/// Word's outline depth limit (`WW_OUTLINE_MAX`).
const WW_OUTLINE_MAX: u8 = 9;
/// Deepest field nesting tracked. Word itself stops at 20 nested fields.
const MAX_FIELD_DEPTH: usize = 64;
/// Longest instruction accumulated — Word's own limit is a few KB.
const MAX_INSTR_LEN: usize = 8192;
/// A real instruction is split over a handful of runs. Past this, the file has
/// an unmatched `begin`: close the command instead of dropping the whole rest of
/// the document.
const MAX_COMMAND_RUNS: u32 = 512;

/// What a field's cached result should become in the document.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ResultAction {
    /// Emit the runs normally — the cached result is kept as plain text.
    Keep,
    /// Drop the runs: an instruction, or a result we render ourselves.
    Drop,
    /// Replace the first result run's text with this, drop the rest.
    Replace(String),
}

/// Coarse family of a field type, from the table of `lcl_GetFieldConversion`
/// (DomainMapper_Impl.cxx:6720). It answers one question: does this field have a
/// visible result worth freezing, or is it a marker with nothing to show?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldClass {
    /// Current page number.
    PageNumber,
    /// A page count (`NUMPAGES`, `SECTIONPAGES`).
    PageCount,
    /// Date or time.
    DateTime,
    /// Document metadata or statistics (`AUTHOR`, `TITLE`, `FILENAME`, …).
    DocInfo,
    /// Cross-reference (`REF`, `PAGEREF`, `STYLEREF`, `HYPERLINK`, …).
    Reference,
    /// Index / table structure (`TOC`, `INDEX`, `BIBLIOGRAPHY`).
    Index,
    /// An *entry marker* with no visible result (`TC`, `XE`, `TA`, `RD`): any
    /// cached text it carries is Word bookkeeping and must not be rendered.
    Marker,
    /// Interactive form field.
    Form,
    /// Any other field Word knows about.
    Other,
}

/// Family of a field type name (upper-cased), or `None` when the name is not a
/// Word field at all — which happens on malformed instructions and is exactly
/// why an unknown field still falls back to its cached result.
pub(crate) fn field_class(name: &str) -> Option<FieldClass> {
    use FieldClass::{DateTime, DocInfo, Form, Index, Marker, Other, PageCount, PageNumber, Reference};
    Some(match name {
        "PAGE" => PageNumber,
        "NUMPAGES" | "SECTIONPAGES" => PageCount,
        "DATE" | "TIME" | "CREATEDATE" | "SAVEDATE" | "PRINTDATE" | "EDITTIME" => DateTime,
        "AUTHOR" | "LASTSAVEDBY" | "USERNAME" | "USERINITIALS" | "USERADDRESS" | "TITLE"
        | "SUBJECT" | "KEYWORDS" | "COMMENTS" | "CATEGORY" | "COMPANY" | "MANAGER" | "REVNUM"
        | "FILENAME" | "FILESIZE" | "TEMPLATE" | "NUMCHARS" | "NUMWORDS" | "DOCPROPERTY"
        | "DOCVARIABLE" | "INFO" => DocInfo,
        "REF" | "PAGEREF" | "STYLEREF" | "NOTEREF" | "HYPERLINK" | "GOTOBUTTON" => Reference,
        "TOC" | "INDEX" | "BIBLIOGRAPHY" | "TOA" | "CITATION" => Index,
        // Markers: they close on `end` with no result at all.
        "TC" | "XE" | "TA" | "RD" | "PRIVATE" => Marker,
        "FORMTEXT" | "FORMCHECKBOX" | "FORMDROPDOWN" | "FILLIN" | "ASK" | "MACROBUTTON" => Form,
        "SEQ" | "SET" | "IF" | "AUTONUM" | "AUTONUMLGL" | "AUTONUMOUT" | "AUTOTEXT"
        | "AUTOTEXTLIST" | "LISTNUM" | "QUOTE" | "SYMBOL" | "EQ" | "FORMULA" | "ADDIN"
        | "ADVANCE" | "BARCODE" | "COMPARE" | "DATABASE" | "DDE" | "DDEAUTO" | "EMBED"
        | "GLOSSARY" | "INCLUDEPICTURE" | "INCLUDETEXT" | "LINK" | "MERGEFIELD" | "MERGEREC"
        | "MERGESEQ" | "NEXT" | "NEXTIF" | "SKIPIF" | "PRINT" | "SHAPE" | "SUBSCRIBER" => Other,
        _ => return None,
    })
}

/// Header/footer token for a field type, or `None` when it has no token form.
/// Tokens are the ones `expandHFDoc` understands in the editor.
pub(crate) fn hf_token(name: &str) -> Option<&'static str> {
    match name {
        "PAGE" => Some("{page}"),
        "NUMPAGES" | "SECTIONPAGES" => Some("{pages}"),
        "DATE" | "TIME" | "CREATEDATE" | "SAVEDATE" | "PRINTDATE" => Some("{date}"),
        "TITLE" => Some("{titre}"),
        _ => None,
    }
}

/// The kinds of field we do more than freeze.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FieldKind {
    Page,
    NumPages,
    Date,
    Title,
    Toc(TocSpec),
    PageRef,
    Hyperlink(String),
    /// An index/entry marker: it has no visible result (`TC`, `XE`, `TA`, `RD`).
    Marker,
    /// Everything else: keep the cached result, never the instruction.
    Frozen,
}

/// The switches of a `TOC` field that affect what we build.
///
/// `Copy`, deliberately: callers hold it by value out of `FieldState::toc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TocSpec {
    /// Highest outline level included (`\o "1-3"` → 3).
    pub(crate) levels: u8,
    /// `\h` — entries are hyperlinks.
    pub(crate) hyperlinks: bool,
    /// False when no level at all shows a page number (a bare `\n`).
    pub(crate) page_numbers: bool,
    /// `\n "2-3"` — the level range that hides its page number. `Some((0, 0))`
    /// is the inert range Word falls back to on a malformed value.
    pub(crate) no_page_numbers: Option<(u8, u8)>,
    /// Built from outline levels (`\o`) or from the applied outline level (`\u`).
    /// Also the default when neither `\o`, `\f` nor `\t` is given.
    pub(crate) from_outline: bool,
    /// `\f` — built from `TC` entries instead of outline levels.
    pub(crate) from_entries: bool,
    /// `\t "Style,level,…"` — built from arbitrary style names.
    pub(crate) from_styles: bool,
    /// `\a` / `\c` — this is a table of figures, not a table of contents.
    pub(crate) table_of_figures: bool,
    /// `\z` — hide page numbers in web layout only (we ignore it when rendering).
    pub(crate) hide_web_page_numbers: bool,
    /// `\w` — preserve tab characters inside entries.
    pub(crate) preserve_tabs: bool,
}

impl Default for TocSpec {
    /// The table of contents the editor itself inserts: three outline levels
    /// with page numbers. A TOC *read from a file* goes through
    /// [`parse_toc_switches`], where an absent `\o` means "every level", as in
    /// Word.
    fn default() -> Self {
        TocSpec {
            levels: 3,
            hyperlinks: false,
            page_numbers: true,
            no_page_numbers: None,
            from_outline: true,
            from_entries: false,
            from_styles: false,
            table_of_figures: false,
            hide_web_page_numbers: false,
            preserve_tabs: false,
        }
    }
}

impl TocSpec {
    /// Does an entry at this level carry a page number? `\n` can hide it for a
    /// range of levels only (handleToc, DomainMapper_Impl.cxx:7488).
    pub(crate) fn page_numbers_at(&self, level: u8) -> bool {
        match self.no_page_numbers {
            Some((from, to)) => level < from || level > to,
            None => true,
        }
    }
}

#[derive(Default)]
struct Frame {
    instr: String,
    in_result: bool,
    emitted: bool,
    kind: Option<FieldKind>,
    /// Field type name, kept after the instruction is consumed.
    name: String,
    /// `w:fldLock` — the field must never be refreshed, so a `DATE` is a frozen
    /// date and not a live one (DomainMapper.cxx:4327, `IsFieldLocked`).
    locked: bool,
    /// Runs seen while still reading this frame's instruction.
    cmd_runs: u32,
}

/// Field and bookmark state while walking a body. Lives above the paragraph
/// loop: a field, like a bookmark, routinely spans paragraphs.
#[derive(Default)]
pub(crate) struct FieldState {
    stack: Vec<Frame>,
    /// Nested `begin`s dropped because the stack is at `MAX_FIELD_DEPTH`; the
    /// matching `end`s must be dropped too or the stack unwinds too far.
    overflow: u32,
    /// Bookmarks open at this point, as `(w:id, w:name)` — ids are not names,
    /// and `bookmarkEnd` only carries the id.
    open_bookmarks: Vec<(String, String)>,
    /// True while parsing a header or footer part, where PAGE & co. become the
    /// `{page}`-style tokens the editor understands.
    pub(crate) in_hf: bool,
    /// Set while inside a TOC field's result.
    pub(crate) toc: Option<TocSpec>,
    /// Page number captured from the PAGEREF that closes a TOC entry.
    toc_page: Option<u32>,
    /// `\* ROMAN` and friends seen on a PAGE field.
    pub(crate) page_num_format: Option<&'static str>,
}

impl FieldState {
    pub(crate) fn for_header() -> Self {
        FieldState { in_hf: true, ..Default::default() }
    }

    fn begin(&mut self) {
        // A pathological file could nest for ever; the stack is bounded, and the
        // overflow is counted so `end` stays balanced.
        if self.stack.len() >= MAX_FIELD_DEPTH {
            self.overflow = self.overflow.saturating_add(1);
            return;
        }
        self.stack.push(Frame::default());
    }

    /// True while the current field is still reading its instruction, i.e. every
    /// text seen belongs to the command (LibreOffice: `IsOpenFieldCommand`).
    pub(crate) fn in_command(&self) -> bool {
        self.stack.last().is_some_and(|f| !f.in_result)
    }

    /// `w:fldLock` on the current field.
    fn set_locked(&mut self) {
        if let Some(f) = self.stack.last_mut() {
            f.locked = true;
        }
    }

    /// Append one `instrText` chunk VERBATIM. Never trim, never parse here:
    /// Word splits `PAGE` into `PA` + `GE` on an rsid boundary.
    fn append_instr(&mut self, s: &str) {
        if let Some(f) = self.stack.last_mut() {
            if !f.in_result && f.instr.len() + s.len() <= MAX_INSTR_LEN {
                f.instr.push_str(s);
            }
        }
    }

    /// The instruction is complete: resolve what this field is.
    fn separate(&mut self) {
        let Some(f) = self.stack.last_mut() else { return };
        if f.in_result {
            return;
        }
        f.in_result = true;
        let instr = std::mem::take(&mut f.instr);
        let (ty, args, switches) = split_field_command(&instr);
        f.name = ty.clone();
        let kind = match ty.as_str() {
            "PAGE" => FieldKind::Page,
            "NUMPAGES" | "SECTIONPAGES" => FieldKind::NumPages,
            "DATE" | "TIME" | "CREATEDATE" | "SAVEDATE" | "PRINTDATE" => FieldKind::Date,
            "TITLE" => FieldKind::Title,
            "PAGEREF" => FieldKind::PageRef,
            "HYPERLINK" => FieldKind::Hyperlink(hyperlink_url(&args, &switches)),
            "TOC" => FieldKind::Toc(parse_toc_switches(&instr)),
            _ => match field_class(&ty) {
                Some(FieldClass::Marker) => FieldKind::Marker,
                _ => FieldKind::Frozen,
            },
        };
        f.kind = Some(kind.clone());
        if kind == FieldKind::Page {
            // `\*` is case sensitive and must not be normalised: see
            // `parse_numbering_type`.
            if let Some(fmt) = parse_numbering_type(&instr) {
                self.page_num_format = Some(fmt);
            }
        }
        if let FieldKind::Toc(spec) = kind {
            self.toc = Some(spec);
        }
    }

    /// `fldChar "end"`. Word omits `separate` for a result-less field, so the
    /// instruction is resolved here too (LibreOffice: `PopFieldContext` calls
    /// `CloseFieldCommand` when the command was never closed) — otherwise the
    /// machine would still be "reading the instruction" for the rest of the
    /// document and drop every run it meets.
    fn end(&mut self) {
        if self.overflow > 0 {
            self.overflow -= 1;
            return;
        }
        if self.in_command() {
            self.separate();
        }
        let was_toc = matches!(self.stack.pop().and_then(|f| f.kind), Some(FieldKind::Toc(_)));
        if was_toc {
            // Re-read the enclosing TOC, if this one was nested inside another.
            self.toc = self.stack.iter().rev().find_map(|f| match &f.kind {
                Some(FieldKind::Toc(spec)) => Some(*spec),
                _ => None,
            });
            // `toc_page` is deliberately NOT cleared here: Word often puts the
            // closing `fldChar` in the last entry's own paragraph, and the page
            // number is only read when that paragraph is finished.
        }
    }

    /// Guard against an unmatched `begin`: without it, one broken field drops the
    /// rest of the document. Counted on the runs that carry no field markup.
    fn count_command_run(&mut self) {
        let runaway = match self.stack.last_mut() {
            Some(f) if !f.in_result => {
                f.cmd_runs = f.cmd_runs.saturating_add(1);
                f.cmd_runs > MAX_COMMAND_RUNS
            }
            _ => false,
        };
        if runaway {
            self.separate();
        }
    }

    /// Open a bookmark range. Ids are what `bookmarkEnd` carries; the name is
    /// only meaningful to us (`StartOrEndBookmark`, DomainMapper_Impl.cxx:9406).
    fn bookmark_start(&mut self, id: String, name: String) {
        if is_internal_bookmark(&name) || self.open_bookmarks.len() >= MAX_FIELD_DEPTH {
            return;
        }
        self.open_bookmarks.push((id, name));
    }

    fn bookmark_end(&mut self, id: &str) {
        self.open_bookmarks.retain(|(i, _)| i != id);
    }

    /// Handle a `<w:bookmarkStart>` / `<w:bookmarkEnd>` element. Both appear at
    /// body level AND inside a paragraph, so both walks call this. Returns true
    /// when the node was one of them.
    pub(crate) fn handle_bookmark(&mut self, node: &Node<'_, '_>) -> bool {
        match local(node) {
            "bookmarkStart" => {
                if let (Some(id), Some(name)) = (attr_val(node, "id"), attr_val(node, "name")) {
                    self.bookmark_start(id, name);
                }
                true
            }
            "bookmarkEnd" => {
                if let Some(id) = attr_val(node, "id") {
                    self.bookmark_end(&id);
                }
                true
            }
            _ => false,
        }
    }

    /// What to do with the runs at the current position.
    pub(crate) fn result_action(&self) -> ResultAction {
        let Some(f) = self.stack.last() else { return ResultAction::Keep };
        if !f.in_result {
            return ResultAction::Drop; // still reading the instruction
        }
        match &f.kind {
            // A TOC's result is a run of PARAGRAPHS, handled a level up.
            Some(FieldKind::Toc(_)) => ResultAction::Keep,
            // Inside a TOC, the trailing PAGEREF is the entry's page number: it
            // is captured, never rendered inline.
            Some(FieldKind::PageRef) if self.toc.is_some() => ResultAction::Drop,
            // An index entry marker has no visible result.
            Some(FieldKind::Marker) => ResultAction::Drop,
            // A locked field is frozen by definition: keep its text as it is,
            // even in a header (a locked DATE is a fixed date, not `{date}`).
            _ if self.in_hf && !f.locked => match hf_token(&f.name) {
                Some(tok) if !f.emitted => ResultAction::Replace(tok.to_string()),
                Some(_) => ResultAction::Drop,
                None => ResultAction::Keep,
            },
            // A field we cannot recompute keeps its last known value — never the
            // raw instruction, never nothing.
            _ => ResultAction::Keep,
        }
    }

    pub(crate) fn mark_emitted(&mut self) {
        if let Some(f) = self.stack.last_mut() {
            f.emitted = true;
        }
    }

    /// URL of the innermost enclosing HYPERLINK field, if any. A `HYPERLINK`
    /// field is a *command*: the visible text is its result, so the mark belongs
    /// to the result runs (StartField_Impl, docxattributeoutput.cxx:3033).
    pub(crate) fn href(&self) -> Option<&str> {
        self.stack.iter().rev().find_map(|f| match &f.kind {
            Some(FieldKind::Hyperlink(u)) if !u.is_empty() => Some(u.as_str()),
            _ => None,
        })
    }

    /// Names of the bookmarks open at this point.
    pub(crate) fn bookmarks(&self) -> impl Iterator<Item = &str> + '_ {
        self.open_bookmarks.iter().map(|(_, name)| name.as_str())
    }

    /// Marks contributed by the open bookmarks and the enclosing HYPERLINK
    /// field — what a run at this position inherits from its surroundings.
    pub(crate) fn extra_marks(&self) -> Vec<PmMark> {
        let mut marks = Vec::new();
        for name in self.bookmarks() {
            marks.push(PmMark {
                mark_type: "bookmark".into(),
                attrs: Some(json!({ "name": name })),
            });
        }
        if let Some(url) = self.href() {
            marks.push(PmMark { mark_type: "link".into(), attrs: Some(json!({ "href": url })) });
        }
        marks
    }

    /// Text inside a PAGEREF result while a TOC is open is the page number.
    pub(crate) fn note_toc_page(&mut self, txt: &str) {
        if self.toc.is_some()
            && self.stack.last().is_some_and(|f| f.kind == Some(FieldKind::PageRef))
        {
            if let Ok(n) = txt.trim().parse::<u32>() {
                self.toc_page = Some(n);
            }
        }
    }

    pub(crate) fn take_toc_page(&mut self) -> Option<u32> {
        self.toc_page.take()
    }
}

/// All the text of an element, entity references included. Word can split the
/// character data of a single `w:instrText` into several text nodes.
fn elem_text(n: &Node<'_, '_>) -> String {
    let mut s = String::new();
    for d in n.descendants().filter(|d| d.is_text()) {
        s.push_str(d.text().unwrap_or(""));
    }
    s
}

/// An OOXML on/off attribute value (`w:fldLock="true"`, `="1"`, `="on"`).
fn on_off(v: Option<&str>) -> bool {
    matches!(v, Some("1") | Some("true") | Some("on"))
}

/// Does this run have anything to render? Used to decide whether a run that also
/// carries field markup must still be handed to the renderer: some producers put
/// `<w:t>` and `<w:fldChar w:fldCharType="end"/>` in the SAME run, and skipping
/// it wholesale would silently drop the field's result.
fn has_renderable(r: &Node<'_, '_>) -> bool {
    r.children().filter(|n| n.is_element()).any(|c| {
        matches!(
            local(&c),
            "t" | "delText"
                | "drawing"
                | "pict"
                | "object"
                | "br"
                | "tab"
                | "cr"
                | "noBreakHyphen"
                | "softHyphen"
                | "sym"
                | "ruby"
                | "footnoteReference"
                | "endnoteReference"
                | "fldSimple"
        )
    })
}

/// Drive the machine for one `<w:r>`. Returns true when the run is purely
/// structural (`fldChar` / `instrText` only) and must not be rendered; a run
/// that mixes field markup with visible content returns false, and
/// [`FieldState::result_action`] then decides what happens to that content.
pub(crate) fn handle_field_run(r: &Node<'_, '_>, fs: &mut FieldState) -> bool {
    let mut field_markup = false;
    for c in r.children().filter(|n| n.is_element()) {
        match local(&c) {
            "fldChar" => {
                field_markup = true;
                match attr_val(&c, "fldCharType").as_deref() {
                    Some("begin") => fs.begin(),
                    Some("separate") => fs.separate(),
                    Some("end") => fs.end(),
                    _ => {}
                }
                // After `begin` pushed the frame, so the lock lands on it.
                if on_off(attr_val(&c, "fldLock").as_deref()) {
                    fs.set_locked();
                }
            }
            "instrText" | "delInstrText" => {
                field_markup = true;
                fs.append_instr(&elem_text(&c));
            }
            // Some producers write part of the instruction as ordinary text.
            // LibreOffice routes ANY text seen while the command is open to the
            // command (DomainMapper.cxx:4409, `IsOpenFieldCommand`).
            "t" | "delText" if fs.in_command() => fs.append_instr(&elem_text(&c)),
            _ => {}
        }
    }
    if !field_markup {
        fs.count_command_run();
        return false;
    }
    !has_renderable(r)
}

/// The compact form `<w:fldSimple w:instr="…">`: instruction in an attribute,
/// children are the cached result. It nests (a `fldSimple` result may contain
/// another `fldSimple`), which the frame stack handles for free — as long as
/// every [`begin_simple`] is matched by exactly one [`end_simple`].
pub(crate) fn begin_simple(node: &Node<'_, '_>, fs: &mut FieldState) {
    fs.begin();
    fs.append_instr(&attr_val(node, "instr").unwrap_or_default());
    if on_off(attr_val(node, "fldLock").as_deref()) {
        fs.set_locked();
    }
    fs.separate();
}

pub(crate) fn end_simple(fs: &mut FieldState) {
    fs.end();
}

/// One parameter of a field instruction, honouring quoting and `\\` escapes.
/// Port of `lcl_ExtractToken` (DomainMapper_Impl.cxx:5742). Returns
/// `(token, have_token, is_switch)` and always advances `i` unless it returns a
/// non-empty token — so callers cannot loop for ever.
fn extract_token(c: &[char], i: &mut usize) -> (String, bool, bool) {
    let mut tok = String::new();
    let mut quoted = false;
    while *i < c.len() {
        match c[*i] {
            '\\' => {
                if *i + 1 >= c.len() {
                    *i += 1;
                    return (String::new(), false, false); // trailing escape
                }
                let next = c[*i + 1];
                // Inside quotes — and for `\\` anywhere — a backslash escapes the
                // next character: `"C:\\dir\\a.docx"` is one token.
                if quoted || next == '\\' {
                    *i += 2;
                    tok.push(next);
                    continue;
                }
                if tok.is_empty() {
                    *i += 2;
                    return (format!("\\{next}").to_ascii_uppercase(), true, true);
                }
                // Leave `i` on the backslash: the switch is read next round.
                return (tok, true, false);
            }
            '"' => {
                if quoted || !tok.is_empty() {
                    if quoted {
                        *i += 1;
                    }
                    return (tok, true, false);
                }
                quoted = true;
            }
            ' ' => {
                if quoted {
                    tok.push(' ');
                } else if !tok.is_empty() {
                    *i += 1;
                    return (tok, true, false);
                }
            }
            // A leading `=` is the FORMULA field: `{ = SUM(A1:A2) }`.
            '=' => {
                if tok.is_empty() {
                    *i += 1;
                    return ("FORMULA".into(), true, false);
                }
                tok.push('=');
            }
            ch => tok.push(ch),
        }
        *i += 1;
    }
    // Word tolerates an unterminated quote, so no error here.
    let have = !tok.is_empty();
    (tok, have, false)
}

/// Switches of `STYLEREF` that take no argument. Word evaluates
/// `STYLEREF \t "Heading 1" \* MERGEFORMAT` even though the grammar puts the
/// style name first, so the token after such a switch is an ARGUMENT, not the
/// switch's value (`noArgumentSwitches`, DomainMapper_Impl.cxx:5866).
const STYLEREF_NO_ARG: [&str; 6] = ["\\L", "\\N", "\\P", "\\R", "\\T", "\\W"];

/// `splitFieldCommand` (DomainMapper_Impl.cxx:5841) → (type, args, switches).
/// The type is upper-cased (Word field names are case insensitive); switch
/// *values* are returned verbatim, because `\*` is case sensitive.
pub(crate) fn split_field_command(cmd: &str) -> (String, Vec<String>, Vec<String>) {
    let c: Vec<char> = cmd.chars().collect();
    let mut i = 0usize;
    // tdf#54584: the field name may carry a stray leading backslash, which Word
    // reads as a literal and skips.
    if c.len() >= 2 && c[0] == '\\' && c[1] != '\\' && c[1] != ' ' {
        i = 1;
    }
    let (mut ty, mut args, mut switches) = (String::new(), Vec::new(), Vec::new());
    while i < c.len() {
        let before = i;
        let (tok, have, is_switch) = extract_token(&c, &mut i);
        if i == before {
            i += 1; // never loop for ever on malformed input
        }
        if !have {
            continue;
        }
        if ty.is_empty() {
            ty = tok.to_ascii_uppercase();
        } else if is_switch {
            switches.push(tok);
        } else if switches.is_empty() {
            args.push(tok);
        } else if ty == "STYLEREF"
            && switches.last().is_some_and(|s| STYLEREF_NO_ARG.contains(&s.as_str()))
        {
            // The previous switch takes nothing: this token is the style name.
            args.push(tok);
        } else {
            switches.push(tok);
        }
    }
    (ty, args, switches)
}

/// Value of `\<sw>` in a RAW instruction: the text up to the next unquoted
/// backslash. Port of `lcl_FindInCommand` (DomainMapper_Impl.cxx:5952), which
/// `handleToc` uses instead of the tokeniser — hence both functions here, as in
/// LibreOffice. Unlike `findCode` this matching is case insensitive, because
/// Word accepts `\O` for `\o`.
pub(crate) fn find_switch(instr: &str, sw: char) -> Option<String> {
    let c: Vec<char> = instr.chars().collect();
    let mut quoted = false;
    let mut i = 0usize;
    while i + 1 < c.len() {
        match c[i] {
            '"' => quoted = !quoted,
            // A backslash always escapes the next character; outside quotes that
            // character is the switch letter.
            '\\' => {
                if !quoted && c[i + 1].eq_ignore_ascii_case(&sw) {
                    let start = i + 2;
                    let mut j = start;
                    let mut q = false;
                    while j < c.len() {
                        match c[j] {
                            '"' => q = !q,
                            '\\' if !q => break,
                            '\\' => j += 1, // escaped character inside quotes
                            _ => {}
                        }
                        j += 1;
                    }
                    let value: String = c[start..j.min(c.len())].iter().collect();
                    return Some(value.trim().to_string());
                }
                i += 1; // skip the escaped character
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Index type identifiers accept any kind of quotation mark around them
/// (`lcl_trim`, DomainMapper_Impl.cxx:5969).
fn unquote(v: &str) -> String {
    v.replace(['"', '\u{201C}', '\u{201D}'], "").trim().to_string()
}

/// Leading digits of a value as an outline level, `"3\""` included — Word's
/// `toInt32` stops at the first non-digit.
fn parse_level(v: &str) -> Option<u8> {
    let digits: String = v.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u8>().ok().filter(|n| (1..=WW_OUTLINE_MAX).contains(n))
}

/// TOC switches, following `handleToc` (DomainMapper_Impl.cxx:7420-7560).
///
/// `\a`/`\c` table of figures · `\f` from TC entries · `\h` hyperlinked entries ·
/// `\n` hide page numbers (optionally for a level range only) · `\o "1-3"`
/// outline levels · `\t "Style,level"` from styles · `\u` applied outline level ·
/// `\w` keep tabs · `\z` hide page numbers in web layout.
pub(crate) fn parse_toc_switches(instr: &str) -> TocSpec {
    let o = find_switch(instr, 'o').map(|v| unquote(&v));
    // `\o "1-3"`: only the number AFTER the dash counts (`getToken(sValue, 0,
    // '-', nIndex)` then `toInt32(subView(nIndex))`, :7515). An empty `\o` means
    // every outline level, and so does no `\o` at all — LibreOffice's `nMaxLevel`
    // starts at 10, i.e. "all" (:7439).
    let levels = match o.as_deref() {
        Some(v) if !v.is_empty() => match v.split_once('-') {
            Some((_, hi)) => parse_level(hi).unwrap_or(WW_OUTLINE_MAX),
            // No dash: LibreOffice yields 0 here, which would hide everything;
            // read the bare number instead.
            None => parse_level(v).unwrap_or(WW_OUTLINE_MAX),
        },
        _ => WW_OUTLINE_MAX,
    };

    // `\n` alone hides every page number; `\n "2-3"` only those levels. The
    // format is strict — anything else is ignored (:7495).
    let no_page_numbers = find_switch(instr, 'n').map(|v| {
        let v = unquote(&v);
        if v.is_empty() {
            return (1, WW_OUTLINE_MAX);
        }
        match v.split_once('-') {
            Some((lo, hi)) => match (parse_level(lo), parse_level(hi)) {
                (Some(a), Some(b)) if a <= b => (a, b),
                _ => (0, 0),
            },
            None => (0, 0),
        }
    });

    let from_entries = find_switch(instr, 'f').is_some();
    let from_styles = find_switch(instr, 't').is_some_and(|v| !unquote(&v).is_empty());
    let mut from_outline = o.is_some() || find_switch(instr, 'u').is_some();
    // With no `\o`, no `\f` and no `\t`, the index is built from the outline
    // (:7559).
    if !from_outline && !from_entries && !from_styles {
        from_outline = true;
    }

    TocSpec {
        levels: levels.clamp(1, WW_OUTLINE_MAX),
        hyperlinks: find_switch(instr, 'h').is_some(),
        page_numbers: no_page_numbers != Some((1, WW_OUTLINE_MAX)),
        no_page_numbers,
        from_outline,
        from_entries,
        from_styles,
        table_of_figures: find_switch(instr, 'a').is_some() || find_switch(instr, 'c').is_some(),
        hide_web_page_numbers: find_switch(instr, 'z').is_some(),
        preserve_tabs: find_switch(instr, 'w').is_some(),
    }
}

/// `HYPERLINK "url"`, `HYPERLINK \l "anchor"` or both. `#anchor` is the
/// convention the rest of the reader already uses for an internal target.
fn hyperlink_url(args: &[String], switches: &[String]) -> String {
    let mut url = args.first().cloned().unwrap_or_default();
    if let Some(p) = switches.iter().position(|s| s == "\\L") {
        if let Some(anchor) = switches.get(p + 1).filter(|a| !a.starts_with('\\')) {
            url.push('#');
            url.push_str(anchor);
        }
    }
    url
}

/// `\* ROMAN` → a `PageNumFormat` value of the editor. CASE MATTERS: `ROMAN` is
/// upper-case numbering, `roman` lower — cf. `lcl_ParseNumberingType`
/// (DomainMapper_Impl.cxx:5603). Never upper-case this argument.
pub(crate) fn parse_numbering_type(instr: &str) -> Option<&'static str> {
    let v = find_switch(instr, '*')?;
    match v.split_whitespace().next()? {
        "ROMAN" => Some("roman-upper"),
        "roman" => Some("roman-lower"),
        "ALPHABETIC" => Some("alpha-upper"),
        "alphabetic" => Some("alpha-lower"),
        "Arabic" | "ARABIC" | "arabic" => Some("arabic"),
        _ => None,
    }
}

/// Bookmarks Word generates for itself. `_GoBack` is the cursor marker, `_Hlk` /
/// `_Hlt` are hyperlink anchors and `_Ref` cross-reference targets: importing
/// them would pollute the "go to" dialog. `_Toc…` bookmarks are KEPT — they are
/// the targets a table of contents links to.
pub(crate) fn is_internal_bookmark(name: &str) -> bool {
    name == "_GoBack"
        || name.starts_with("_Hlk")
        || name.starts_with("_Hlt")
        || name.starts_with("_Ref")
        || name.starts_with("OLE_LINK")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every instruction used below was extracted from the `.docx` corpus
    /// (`grep` on `w:instrText`), so the tokeniser is checked against what Word
    /// and its friends actually write, not against the grammar.
    fn split(cmd: &str) -> (String, Vec<String>, Vec<String>) {
        split_field_command(cmd)
    }

    #[test]
    fn tokenises_real_instructions() {
        // The two most frequent forms in the corpus.
        assert_eq!(split(" PAGE ").0, "PAGE");
        let (ty, args, switches) = split(" PAGE   \\* MERGEFORMAT ");
        assert_eq!(ty, "PAGE");
        assert!(args.is_empty());
        assert_eq!(switches, vec!["\\*".to_string(), "MERGEFORMAT".to_string()]);

        // A quoted argument keeps its spaces; `\\` is one literal backslash.
        let (ty, args, _) = split(" HYPERLINK \"file:///C:\\\\TEMP\\\\test.docx\" ");
        assert_eq!(ty, "HYPERLINK");
        assert_eq!(args, vec!["file:///C:\\TEMP\\test.docx".to_string()]);

        // `SEQ Figure \* ARABIC`: one argument, then a switch and its value.
        let (ty, args, switches) = split(" SEQ Figure \\* ARABIC ");
        assert_eq!((ty.as_str(), args.len()), ("SEQ", 1));
        assert_eq!(args[0], "Figure");
        assert_eq!(switches, vec!["\\*".to_string(), "ARABIC".to_string()]);

        // A leading `=` is the FORMULA field.
        assert_eq!(split(" = \"1\" \"\" \"x\" ").0, "FORMULA");

        // Quoted argument with an escaped space inside (a DATE format).
        let (ty, _, switches) = split(" TIME \\@\"dd\\/MM\\/yyyy\\ H:mm:ss\\ AM/PM\" ");
        assert_eq!(ty, "TIME");
        assert_eq!(switches.first().map(String::as_str), Some("\\@"));
    }

    #[test]
    fn styleref_switches_that_take_no_argument() {
        // Corpus: the style name comes first, `\l` is a bare switch.
        let (ty, args, switches) = split(" STYLEREF \"Main Index Entry\" \\l ");
        assert_eq!(ty, "STYLEREF");
        assert_eq!(args, vec!["Main Index Entry".to_string()]);
        assert_eq!(switches, vec!["\\L".to_string()]);

        // The LibreOffice case (:5866): the switch comes FIRST and takes no
        // argument, so "Heading 1" is the style name, not the switch's value.
        let (ty, args, switches) = split(" STYLEREF \\t \"Heading 1\" \\* MERGEFORMAT ");
        assert_eq!(ty, "STYLEREF");
        assert_eq!(args, vec!["Heading 1".to_string()]);
        assert_eq!(switches, vec!["\\T".to_string(), "\\*".to_string(), "MERGEFORMAT".to_string()]);

        // Any other field keeps the naive reading: a token after a switch is
        // that switch's value.
        let (_, args, switches) = split(" REF SSNumber \\h  \\* MERGEFORMAT ");
        assert_eq!(args, vec!["SSNumber".to_string()]);
        assert_eq!(switches, vec!["\\H".to_string(), "\\*".to_string(), "MERGEFORMAT".to_string()]);
    }

    #[test]
    fn hyperlink_targets() {
        let (_, args, switches) = split(" HYPERLINK \"http://x.org/a.html\" \\l \"Lecturas\" ");
        assert_eq!(hyperlink_url(&args, &switches), "http://x.org/a.html#Lecturas");

        // Corpus: an anchor-only hyperlink inside a table of contents entry.
        let (_, args, switches) = split("HYPERLINK \\l \"_Toc54867841\"");
        assert_eq!(hyperlink_url(&args, &switches), "#_Toc54867841");

        // A tooltip (`\o`) must not end up in the URL.
        let (_, args, switches) = split(" HYPERLINK \"http://x.org/\" \\o \"Jor-El\" ");
        assert_eq!(hyperlink_url(&args, &switches), "http://x.org/");
    }

    #[test]
    fn malformed_instructions_terminate() {
        // A trailing escape, an unterminated quote, nothing but separators.
        for cmd in ["\\", " PAGE \\", "\"", " \" ", "", " ", "\\\\", "= ", "\\ PAGE"] {
            let _ = split(cmd);
            let _ = find_switch(cmd, 'o');
            let _ = parse_toc_switches(cmd);
        }
        // tdf#54584: a stray leading backslash before the field name.
        assert_eq!(split("\\PAGE ").0, "PAGE");
    }

    #[test]
    fn find_switch_reads_up_to_the_next_backslash() {
        let toc = " TOC \\o \"1-3\" \\h \\z \\u ";
        assert_eq!(find_switch(toc, 'o').as_deref(), Some("\"1-3\""));
        assert_eq!(find_switch(toc, 'h').as_deref(), Some(""));
        assert_eq!(find_switch(toc, 'f'), None);
        // A backslash inside quotes is not a switch.
        assert_eq!(find_switch(" HYPERLINK \"C:\\\\a\\\\b.docx\" ", 'a'), None);
        // Case insensitive, like Word.
        assert_eq!(find_switch(" TOC \\O \"1-2\" ", 'o').as_deref(), Some("\"1-2\""));
    }

    #[test]
    fn toc_switches_from_the_corpus() {
        // The overwhelmingly most common form.
        let s = parse_toc_switches(" TOC \\o \"1-3\" \\h \\z \\u ");
        assert_eq!(s.levels, 3);
        assert!(s.hyperlinks && s.page_numbers && s.from_outline && s.hide_web_page_numbers);
        assert!(!s.from_entries && !s.from_styles && !s.table_of_figures);

        // Switch order is free.
        assert_eq!(parse_toc_switches(" TOC \\z \\o \"1-3\" \\u \\h").levels, 3);
        assert_eq!(parse_toc_switches(" TOC \\o \"1-5\" \\h \\z \\u ").levels, 5);

        // Only the number AFTER the dash counts.
        assert_eq!(parse_toc_switches(" TOC \\o \"2-4\" ").levels, 4);
        // An empty `\o` means every level.
        assert_eq!(parse_toc_switches(" TOC \\o \\h \\z \\u ").levels, 9);
        // No `\o` at all: from the outline, all levels.
        let bare = parse_toc_switches(" TOC ");
        assert_eq!(bare.levels, 9);
        assert!(bare.from_outline);

        // `\n` with no range: no page number anywhere.
        let n = parse_toc_switches(" TOC \\o \"2-3\" \\n ");
        assert!(!n.page_numbers);
        assert!(!n.page_numbers_at(1) && !n.page_numbers_at(9));

        // `\n "2-3"`: only those levels lose it.
        let n = parse_toc_switches(" TOC \\o \"1-3\" \\n \"2-3\" ");
        assert!(n.page_numbers);
        assert!(n.page_numbers_at(1) && !n.page_numbers_at(2) && !n.page_numbers_at(3));

        // From TC entries, and from styles.
        assert!(parse_toc_switches(" TOC \\f \\o \"1-9\" \\h").from_entries);
        let t = parse_toc_switches(" TOC \\o \"1-3\" \\h \\z \\t \"Intensives Zitat,3,Custom1,2\" ");
        assert!(t.from_styles);
        // A table of figures.
        let c = parse_toc_switches(" TOC \\h \\z \\c \"Figure\" ");
        assert!(c.table_of_figures && c.from_outline);
        // `\w` keeps tabs inside entries.
        assert!(parse_toc_switches(" TOC \\o \"1-3\" \\h \\z \\w").preserve_tabs);
    }

    #[test]
    fn numbering_type_is_case_sensitive() {
        assert_eq!(parse_numbering_type("PAGE  \\* Arabic  \\* MERGEFORMAT"), Some("arabic"));
        assert_eq!(parse_numbering_type(" PAGE \\* ROMAN "), Some("roman-upper"));
        assert_eq!(parse_numbering_type(" PAGE \\* roman "), Some("roman-lower"));
        assert_eq!(parse_numbering_type(" PAGE \\* ALPHABETIC "), Some("alpha-upper"));
        assert_eq!(parse_numbering_type(" PAGE \\* alphabetic "), Some("alpha-lower"));
        // MERGEFORMAT is not a numbering type.
        assert_eq!(parse_numbering_type(" PAGE   \\* MERGEFORMAT "), None);
        assert_eq!(parse_numbering_type(" PAGE "), None);
    }

    #[test]
    fn field_types_are_classified() {
        assert_eq!(field_class("PAGE"), Some(FieldClass::PageNumber));
        assert_eq!(field_class("NUMPAGES"), Some(FieldClass::PageCount));
        assert_eq!(field_class("XE"), Some(FieldClass::Marker));
        assert_eq!(field_class("STYLEREF"), Some(FieldClass::Reference));
        assert_eq!(field_class("FORMCHECKBOX"), Some(FieldClass::Form));
        assert_eq!(field_class("ADDIN"), Some(FieldClass::Other));
        assert_eq!(field_class("NOT_A_FIELD"), None);
    }

    // ─── the state machine, driven by real markup ───────────────────────────

    const NS: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";

    /// Walk `<w:p>`'s children exactly as the reader does, and return what each
    /// run produced: `None` for a dropped or structural run, `Some(text)`
    /// otherwise (the replacement text for a token field).
    fn run_texts(body: &str) -> Vec<Option<String>> {
        let xml = format!("<w:p {NS}>{body}</w:p>");
        let doc = roxmltree::Document::parse(&xml).expect("test xml");
        let mut fs = FieldState::default();
        walk(&doc.root_element(), &mut fs)
    }

    fn run_texts_hf(body: &str) -> Vec<Option<String>> {
        let xml = format!("<w:p {NS}>{body}</w:p>");
        let doc = roxmltree::Document::parse(&xml).expect("test xml");
        let mut fs = FieldState::for_header();
        walk(&doc.root_element(), &mut fs)
    }

    fn walk(parent: &Node<'_, '_>, fs: &mut FieldState) -> Vec<Option<String>> {
        let mut out = Vec::new();
        for node in parent.children().filter(|n| n.is_element()) {
            if fs.handle_bookmark(&node) {
                continue;
            }
            match local(&node) {
                "fldSimple" => {
                    begin_simple(&node, fs);
                    out.extend(walk(&node, fs));
                    end_simple(fs);
                }
                "r" => {
                    if handle_field_run(&node, fs) {
                        continue;
                    }
                    let text: String = node
                        .children()
                        .filter(|n| local(n) == "t")
                        .map(|t| elem_text(&t))
                        .collect();
                    match fs.result_action() {
                        ResultAction::Drop => {
                            fs.note_toc_page(&text);
                            out.push(None);
                        }
                        ResultAction::Replace(tok) => {
                            fs.mark_emitted();
                            out.push(Some(tok));
                        }
                        ResultAction::Keep => out.push(Some(text)),
                    }
                }
                _ => {}
            }
        }
        out
    }

    fn kept(body: &str) -> Vec<String> {
        run_texts(body).into_iter().flatten().collect()
    }

    #[test]
    fn instruction_never_reaches_the_document() {
        // The regression this module exists for: ` TOC \o "1-3" \h ` as text.
        let out = kept(concat!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>",
            "<w:r><w:instrText xml:space=\"preserve\"> TOC \\o \"1-3\" \\h \\z \\u </w:instrText></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
            "<w:r><w:t>Chapitre 1</w:t></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        ));
        assert_eq!(out, vec!["Chapitre 1".to_string()]);
    }

    #[test]
    fn instruction_split_mid_word_across_runs() {
        // Word breaks `PAGE` into `PA` + `GE` on an rsid boundary; in a header
        // the field must still be recognised and become `{page}`.
        let out: Vec<String> = run_texts_hf(concat!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>",
            "<w:r><w:instrText xml:space=\"preserve\"> PA</w:instrText></w:r>",
            "<w:r><w:instrText xml:space=\"preserve\">GE  \\* MERGEFORMAT </w:instrText></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
            "<w:r><w:t>7</w:t></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        ))
        .into_iter()
        .flatten()
        .collect();
        assert_eq!(out, vec!["{page}".to_string()]);
    }

    #[test]
    fn only_the_first_result_run_is_replaced() {
        let out: Vec<String> = run_texts_hf(concat!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>",
            "<w:r><w:instrText> NUMPAGES </w:instrText></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
            "<w:r><w:t>1</w:t></w:r>",
            "<w:r><w:t>2</w:t></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        ))
        .into_iter()
        .flatten()
        .collect();
        assert_eq!(out, vec!["{pages}".to_string()]);
    }

    #[test]
    fn a_field_without_separator_does_not_swallow_the_document() {
        // Word omits `separate` when the field has no result (here an index
        // entry). Everything after `end` must come back.
        let out = kept(concat!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>",
            "<w:r><w:instrText> XE \"terme\" </w:instrText></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
            "<w:r><w:t>texte visible</w:t></w:r>",
        ));
        assert_eq!(out, vec!["texte visible".to_string()]);
    }

    #[test]
    fn marker_fields_drop_their_cached_result() {
        // A `TC` entry sometimes carries text; it is bookkeeping, not content.
        let out = kept(concat!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>",
            "<w:r><w:instrText> TC \"Titre\" \\f C \\l \"1\" </w:instrText></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
            "<w:r><w:t>Titre</w:t></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
            "<w:r><w:t>suite</w:t></w:r>",
        ));
        assert_eq!(out, vec!["suite".to_string()]);
    }

    #[test]
    fn unknown_field_keeps_its_cached_result() {
        // The golden rule: never the instruction, never nothing.
        let out = kept(concat!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>",
            "<w:r><w:instrText> ADDIN EN.CITE &lt;EndNote&gt; </w:instrText></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
            "<w:r><w:t>(Andrews, 2003)</w:t></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        ));
        assert_eq!(out, vec!["(Andrews, 2003)".to_string()]);
    }

    #[test]
    fn nested_pageref_inside_a_toc_yields_the_page() {
        let xml = format!(
            "<w:p {NS}>{}</w:p>",
            concat!(
                "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>",
                "<w:r><w:instrText> TOC \\o \"1-3\" \\h \\z \\u </w:instrText></w:r>",
                "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
                "<w:r><w:t>Introduction</w:t></w:r>",
                "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>",
                "<w:r><w:instrText> PAGEREF _Toc355095261 \\h </w:instrText></w:r>",
                "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
                "<w:r><w:t>3</w:t></w:r>",
                "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
                "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
                "<w:r><w:t>après</w:t></w:r>",
            )
        );
        let doc = roxmltree::Document::parse(&xml).expect("test xml");
        let mut fs = FieldState::default();
        let out: Vec<String> = walk(&doc.root_element(), &mut fs).into_iter().flatten().collect();
        // The page number is captured, not rendered.
        assert_eq!(out, vec!["Introduction".to_string(), "après".to_string()]);
        assert_eq!(fs.take_toc_page(), Some(3));
        // The TOC frame is gone once its `end` is seen.
        assert!(fs.toc.is_none());
    }

    #[test]
    fn a_locked_date_stays_frozen_even_in_a_header() {
        let out: Vec<String> = run_texts_hf(concat!(
            "<w:r><w:fldChar w:fldCharType=\"begin\" w:fldLock=\"true\"/></w:r>",
            "<w:r><w:instrText> DATE \\@ \"yy.MM.dd\" </w:instrText></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
            "<w:r><w:t>26.07.28</w:t></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        ))
        .into_iter()
        .flatten()
        .collect();
        assert_eq!(out, vec!["26.07.28".to_string()]);

        // Unlocked, the same field becomes the editor's token.
        let out: Vec<String> = run_texts_hf(concat!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>",
            "<w:r><w:instrText> DATE \\@ \"yy.MM.dd\" </w:instrText></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
            "<w:r><w:t>26.07.28</w:t></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        ))
        .into_iter()
        .flatten()
        .collect();
        assert_eq!(out, vec!["{date}".to_string()]);
    }

    #[test]
    fn fld_simple_nests() {
        let xml = format!(
            "<w:p {NS}>{}</w:p>",
            concat!(
                "<w:fldSimple w:instr=\" HYPERLINK \\l &quot;_Toc1&quot; \">",
                "<w:r><w:t>Chapitre</w:t></w:r>",
                "<w:fldSimple w:instr=\" PAGEREF _Toc1 \\h \"><w:r><w:t>4</w:t></w:r></w:fldSimple>",
                "</w:fldSimple>",
                "<w:r><w:t>fin</w:t></w:r>",
            )
        );
        let doc = roxmltree::Document::parse(&xml).expect("test xml");
        let mut fs = FieldState::default();
        let out: Vec<String> = walk(&doc.root_element(), &mut fs).into_iter().flatten().collect();
        // Outside a TOC the PAGEREF result is frozen, not swallowed.
        assert_eq!(out, vec!["Chapitre".to_string(), "4".to_string(), "fin".to_string()]);
        assert!(fs.href().is_none(), "le champ est refermé");
    }

    #[test]
    fn hyperlink_field_marks_its_result() {
        let xml = format!(
            "<w:p {NS}>{}</w:p>",
            concat!(
                "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>",
                "<w:r><w:instrText>HYPERLINK \\l \"_Toc54867841\"</w:instrText></w:r>",
                "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
                "<w:r><w:t>Voir</w:t></w:r>",
            )
        );
        let doc = roxmltree::Document::parse(&xml).expect("test xml");
        let mut fs = FieldState::default();
        let mut href = None;
        for node in doc.root_element().children().filter(|n| n.is_element()) {
            if handle_field_run(&node, &mut fs) {
                continue;
            }
            href = fs.href().map(str::to_string);
        }
        assert_eq!(href.as_deref(), Some("#_Toc54867841"));
    }

    #[test]
    fn bookmarks_are_tracked_by_id_and_filtered_by_name() {
        let xml = format!(
            "<w:p {NS}>{}</w:p>",
            concat!(
                "<w:bookmarkStart w:id=\"1\" w:name=\"_GoBack\"/>",
                "<w:bookmarkStart w:id=\"2\" w:name=\"_Toc355095261\"/>",
                "<w:bookmarkStart w:id=\"3\" w:name=\"mon-repere\"/>",
                "<w:r><w:t>x</w:t></w:r>",
                "<w:bookmarkEnd w:id=\"3\"/>",
            )
        );
        let doc = roxmltree::Document::parse(&xml).expect("test xml");
        let mut fs = FieldState::default();
        let mut seen: Vec<String> = Vec::new();
        for node in doc.root_element().children().filter(|n| n.is_element()) {
            if fs.handle_bookmark(&node) {
                continue;
            }
            seen = fs.bookmarks().map(str::to_string).collect();
        }
        // `_GoBack` filtered, `_Toc…` kept (it is a TOC target), plus the user's.
        assert_eq!(seen, vec!["_Toc355095261".to_string(), "mon-repere".to_string()]);
        // Closing by id leaves the still-open ones alone.
        assert_eq!(fs.bookmarks().collect::<Vec<_>>(), vec!["_Toc355095261"]);
        assert_eq!(fs.extra_marks().len(), 1);
        assert!(is_internal_bookmark("_Hlk123") && is_internal_bookmark("_Ref9"));
        assert!(!is_internal_bookmark("_Toc9"));
    }

    #[test]
    fn a_run_that_mixes_field_end_and_text_keeps_its_text() {
        // Some producers put the result and the closing fldChar in one run.
        let out = kept(concat!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>",
            "<w:r><w:instrText> AUTHOR </w:instrText></w:r>",
            "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>",
            "<w:r><w:t>Martin</w:t><w:fldChar w:fldCharType=\"end\"/></w:r>",
        ));
        assert_eq!(out, vec!["Martin".to_string()]);
    }

    #[test]
    fn an_unmatched_begin_stops_eating_the_document() {
        let mut body = String::from(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> IF </w:instrText></w:r>",
        );
        for _ in 0..(MAX_COMMAND_RUNS + 8) {
            body.push_str("<w:r><w:t>a</w:t></w:r>");
        }
        let out = kept(&body);
        assert!(!out.is_empty(), "le champ non refermé a avalé tout le document");
    }

    #[test]
    fn deep_nesting_stays_balanced() {
        let mut body = String::new();
        for _ in 0..(MAX_FIELD_DEPTH + 16) {
            body.push_str("<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>");
            body.push_str("<w:r><w:instrText> IF </w:instrText></w:r>");
            body.push_str("<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>");
        }
        for _ in 0..(MAX_FIELD_DEPTH + 16) {
            body.push_str("<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>");
        }
        body.push_str("<w:r><w:t>après</w:t></w:r>");
        assert_eq!(kept(&body), vec!["après".to_string()]);
    }
}
