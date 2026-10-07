//! Code-behind of « Imprimer » (`print_dialog.kbview`, web `PrintDialog.tsx`): the options, the pages laid
//! out by `kubuno_office_sheets_core::print`, the preview, and the print through Windows' print dialog.

use kubuno_desktop::prelude::*;
use kubuno_desktop::printing::{Margins, PaperSize};
use kubuno_office_sheets_core::print::{self, Page, PrintGrid, PrintOptions, ScaleMode, PAGE_FORMATS};

use crate::controls::print_paint;
use crate::Resources;

/// The two print areas, built when the dialog opens (`buildGrid(area)`).
pub struct Areas {
    /// The selection, when it is a range.
    pub selection: Option<PrintGrid>,
    /// The used range of the sheet.
    pub used: Option<PrintGrid>,
}

#[kubuno_desktop::view("print_dialog.kbview")]
pub struct SheetPrintDialog {
    #[bind]
    area_items: Rows,
    #[bind]
    area: String,
    #[bind]
    orientation: String,
    #[bind]
    paper_items: Rows,
    #[bind]
    paper: String,
    #[bind]
    scale_items: Rows,
    #[bind]
    scale_mode: String,
    #[bind]
    custom_scale: bool,
    #[bind]
    scale_pct: f64,
    #[bind]
    margin_mm: f64,
    #[bind]
    repeat_rows: f64,
    #[bind]
    gridlines: bool,
    #[bind]
    headings: bool,
    #[bind]
    center_h: bool,
    #[bind]
    has_pages: bool,
    #[bind]
    no_pages: bool,
    areas: Option<Areas>,
    /// The options of the last print, kept by the window for the next one.
    pub options: PrintOptions,
    pages: Vec<Page>,
    scale: f64,
    loading: bool,
}

fn rows(items: &[(&str, &str)]) -> Rows {
    Rows::from(items.iter().map(|(v, t)| Row::new().with("Value", Value::Str(v.to_string())).with("Text", Value::Str(t.to_string()))).collect::<Vec<_>>())
}

fn scale_id(m: ScaleMode) -> &'static str {
    match m {
        ScaleMode::Normal => "normal",
        ScaleMode::FitWidth => "fitWidth",
        ScaleMode::FitPage => "fitPage",
        ScaleMode::Custom => "custom",
    }
}

impl SheetPrintDialog {
    pub fn new(areas: Areas, options: PrintOptions) -> Self {
        let has_sel = areas.selection.is_some();
        let mut area_list: Vec<(&str, &str)> = Vec::new();
        if has_sel {
            area_list.push(("selection", Resources::pr_area_sel()));
        }
        area_list.push(("used", Resources::pr_area_used()));
        let mut d = Self {
            area_items: rows(&area_list),
            area: if has_sel { "selection".into() } else { "used".into() },
            paper_items: rows(&PAGE_FORMATS.iter().map(|p| (p.id, p.name)).collect::<Vec<_>>()),
            scale_items: rows(&[("normal", Resources::pr_scale_normal()), ("fitWidth", Resources::pr_scale_fitw()), ("fitPage", Resources::pr_scale_fitp()), ("custom", Resources::pr_scale_custom())]),
            areas: Some(areas),
            loading: true,
            ..Self::default()
        };
        d.set_options(&options);
        d.options = options;
        d.initialize_component();
        d
    }

    /// Shows the dialog over `owner`; returns the options to keep for the next print.
    pub fn ask(owner: &dyn AsForm, areas: Areas, options: PrintOptions) -> PrintOptions {
        let mut d = Self::new(areas, options);
        d.show_dialog(owner);
        d.read_options();
        d.options.clone()
    }

    fn set_options(&mut self, o: &PrintOptions) {
        self.orientation = if o.landscape { "landscape".into() } else { "portrait".into() };
        self.paper = o.paper.clone();
        self.scale_mode = scale_id(o.scale_mode).into();
        self.custom_scale = o.scale_mode == ScaleMode::Custom;
        self.scale_pct = o.scale_pct;
        self.margin_mm = o.margin_mm;
        self.repeat_rows = o.repeat_rows as f64;
        self.gridlines = o.gridlines;
        self.headings = o.headings;
        self.center_h = o.center_h;
    }

    fn read_options(&mut self) {
        let o = &mut self.options;
        o.landscape = self.orientation == "landscape";
        o.paper = self.paper.clone();
        o.scale_mode = match self.scale_mode.as_str() {
            "fitWidth" => ScaleMode::FitWidth,
            "fitPage" => ScaleMode::FitPage,
            "custom" => ScaleMode::Custom,
            _ => ScaleMode::Normal,
        };
        o.scale_pct = self.scale_pct.clamp(10.0, 400.0);
        o.margin_mm = self.margin_mm.clamp(0.0, 40.0);
        o.repeat_rows = self.repeat_rows.clamp(0.0, 10.0) as usize;
        o.gridlines = self.gridlines;
        o.headings = self.headings;
        o.center_h = self.center_h;
    }

    fn grid(&self) -> Option<&PrintGrid> {
        let a = self.areas.as_ref()?;
        if self.area == "selection" {
            a.selection.as_ref().or(a.used.as_ref())
        } else {
            a.used.as_ref()
        }
    }

    /// Lays the pages out again and refreshes the preview (every option change).
    fn relayout(&mut self) {
        self.read_options();
        self.custom_scale = self.options.scale_mode == ScaleMode::Custom;
        let opt = self.options.clone();
        let (pages, scale) = match self.grid() {
            Some(g) => {
                let scale = print::print_scale(g, &opt);
                let (w, h) = print::printable_area(&opt, scale);
                (print::paginate(g, &opt, w, h), scale)
            }
            None => (Vec::new(), 1.0),
        };
        self.pages = pages;
        self.scale = scale;
        self.has_pages = !self.pages.is_empty();
        self.no_pages = !self.has_pages;
        // The job's page settings: the paper, the orientation, the uniform margin (hundredths of an inch).
        let mut ps = self.print_doc.default_page_settings();
        let f = print::paper(&opt.paper);
        let hund = |mm: f64| (mm / 25.4 * 100.0).round() as i32;
        ps.paper_size = Some(match f.id {
            "a3" => PaperSize::a3(),
            "a4" => PaperSize::a4(),
            "a5" => PaperSize::a5(),
            "letter" => PaperSize::letter(),
            "legal" => PaperSize::legal(),
            _ => PaperSize::custom(f.name, hund(f.w), hund(f.h)),
        });
        ps.landscape = opt.landscape;
        let m = hund(opt.margin_mm);
        ps.margins = Margins { left: m, right: m, top: m, bottom: m };
        self.print_doc.set_default_page_settings(ps);
        self.print_doc.set_document_name(if opt.title.is_empty() { "Kubuno Spreadsheets".to_string() } else { opt.title.clone() });
        if !self.loading {
            self.preview.invalidate_preview();
        }
    }

    fn print_dialog_load(&mut self) {
        self.relayout();
        self.loading = false;
        self.preview.invalidate_preview();
    }

    fn option_text_changed(&mut self, _e: &TextChangedEventArgs) {
        self.relayout();
    }

    fn option_checked_changed(&mut self, _e: &CheckedChangedEventArgs) {
        self.relayout();
    }

    fn option_value_changed(&mut self, _e: &NumericValueChangedEventArgs) {
        self.relayout();
    }

    /// `BeginPrint`: the pages are already laid out (by the last option change); the document itself must not
    /// be touched from its own event.
    fn print_doc_begin_print(&mut self, _e: &mut PrintEventArgs) {}

    fn print_doc_print_page(&mut self, e: &mut PrintPageEventArgs) {
        let i = e.page_number.saturating_sub(1) as usize;
        let opt = self.options.clone();
        let scale = self.scale;
        if let (Some(grid), Some(page)) = (self.grid(), self.pages.get(i)) {
            let origin = print_paint::table_origin(e.margin_bounds, grid, page, &opt, scale);
            print_paint::paint_page(e.graphics(), origin, grid, page, &opt, scale);
        }
        e.has_more_pages = i + 1 < self.pages.len();
    }

    fn print_click(&mut self) {
        self.read_options();
        if self.system_print.show_dialog(self) == DialogResult::Ok {
            if let Err(e) = self.print_doc.print() {
                kubuno_desktop::tracing::warn!("[spreadsheets] print: {e}");
                let _ = super::confirm_dialog::ConfirmDialog::choose(self, Resources::pr_title(), &Resources::pr_failed().replace("{why}", &e.to_string()), "OK", Resources::cancel());
                return;
            }
            self.set_dialog_result(DialogResult::Ok);
            self.close();
        }
    }

    fn cancel_click(&mut self) {
        self.set_dialog_result(DialogResult::Cancel);
        self.close();
    }
}
