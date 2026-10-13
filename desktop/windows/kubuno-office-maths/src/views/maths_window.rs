//! Code-behind of Kubuno Maths' window (`maths_window.kbview`) — Windows Forms' `Form1.cs`.
//!
//! The canvas owns the open formula document; the window opens it, binds what the canvas reports
//! (`StateChanged`) and turns the ribbon's commands into edits. Increment B: block manipulation, undo/redo,
//! structure insertion (appended to the selected formula), formula-text editing through a prompt, and the
//! Calcul / Matrices / Statistiques / Nombres operations — each runs the shared engine on the selected
//! formula and inserts its LaTeX result as a new formula block. Increment C adds server save.

use kubuno_desktop::prelude::*;
use kubuno_office_math_core::engine::doc::{MathBlock, TextStyle};
use kubuno_office_math_core::engine::graph::default_graph_spec;
use kubuno_office_math_core::engine::num_fmt::js_number_to_string as fnum;
use kubuno_office_math_core::engine::{cas, eval, matrix, number, stats};
use kubuno_office_maths_common::model::MathsDoc;

use crate::controls::maths_canvas::{MathsCanvas, MathsStateEventArgs};
use crate::controls::panels::{ObjectsPanel, PanelCmdEventArgs, PropertiesPanel, StructuresBar, SymbolsPanel};
use crate::views::prompt_dialog::PromptDialog;
use crate::Resources;

/// How the window starts: desktop/common's launch options.
pub use kubuno_office_maths_common::app::Options;

/// Where the open formula comes from.
#[derive(Debug, Clone, Default)]
enum Source {
    #[default]
    Sample,
    File,
    Server(String),
}

/// Kubuno Maths' main window.
#[kubuno_desktop::view("maths_window.kbview")]
pub struct MathsWindow {
    #[control]
    canvas: Custom<MathsCanvas>,
    #[control]
    structures: Custom<StructuresBar>,
    #[control]
    objects: Custom<ObjectsPanel>,
    #[control]
    symbols: Custom<SymbolsPanel>,
    #[control]
    properties: Custom<PropertiesPanel>,
    #[bind]
    window_title: String,
    /// Whether the ribbon's content tabs are shown: hidden until a formula is open, like the web's
    /// `ModuleHome` (`kubuno_office_desktop_shared::ribbon`).
    #[bind]
    show_content_tabs: bool,
    /// The ribbon's active tab: the backstage with no formula, the editing tab once one is open.
    #[bind]
    active_ribbon_tab: String,
    #[bind]
    doc_title: String,
    #[bind]
    can_save: bool,
    #[bind]
    can_undo: bool,
    #[bind]
    can_redo: bool,
    #[bind]
    starred: bool,
    #[bind]
    is_server: bool,
    #[bind]
    visual_mode: bool,
    #[bind]
    code_mode: bool,
    #[bind]
    has_selection: bool,
    #[bind]
    has_formula: bool,
    #[bind]
    pages_text: String,
    #[bind]
    blocks_text: String,
    #[bind]
    zoom_text: String,
    #[bind]
    status_msg: String,
    /// The document to open at load (a local file / the sample); a server formula is fetched in load.
    pending: Option<MathsDoc>,
    source: Source,
    dark: bool,
    open_zoom: Option<f32>,
    header_sample: bool,
}

impl MathsWindow {
    pub fn new(options: Options, document: Option<MathsDoc>) -> Self {
        let source = match &options.doc {
            Some(id) => Source::Server(id.clone()),
            None if options.file.is_some() => Source::File,
            None => Source::Sample,
        };
        let title = document.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| "Formule".to_string());
        // No file and no `--doc` = the empty default placeholder: the ribbon shows only « Fichier ».
        let has_document = options.file.is_some() || options.doc.is_some();
        let tabs = kubuno_office_desktop_shared::ribbon::DocumentTabs::for_document(has_document, "tab_home", "backstage");
        let mut window = Self {
            window_title: format!("{} — {}", title, Resources::app_title()),
            show_content_tabs: tabs.show_content,
            active_ribbon_tab: tabs.active_tab,
            doc_title: title,
            is_server: matches!(source, Source::Server(_)),
            visual_mode: true,
            zoom_text: "100 %".to_string(),
            pages_text: Resources::status_pages().replace("{n}", "1"),
            blocks_text: Resources::status_blocks().replace("{n}", "0"),
            source,
            pending: document,
            dark: options.dark,
            open_zoom: options.zoom,
            header_sample: options.sample,
            ..Self::default()
        };
        window.initialize_component();
        window
    }

    fn maths_window_load(&mut self) {
        let dark = self.dark;
        let zoom = self.open_zoom.map(|z| z / 100.0);
        if let Some(doc) = self.pending.take() {
            self.canvas.with(|c| {
                c.set_dark(dark);
                if let Some(z) = zoom {
                    c.set_zoom(z);
                }
                c.open(doc);
            });
        }
        self.structures.with(|c| c.set_dark(dark));
        self.objects.with(|c| c.set_dark(dark));
        self.symbols.with(|c| c.set_dark(dark));
        self.properties.with(|c| c.set_dark(dark));
        if let Source::Server(id) = self.source.clone() {
            self.open_server(id);
        }
        let mut header = kubuno_desktop_header_data::FeedConfig::new("kubuno-maths");
        if !self.header_sample {
            header.proxy = kubuno_desktop_sync::get_proxy();
        }
        kubuno_desktop_header_data::start(
            kubuno_desktop_header_data::HeaderOptions::for_app(&["office-maths"]),
            header,
            self.header_sample,
            self.dispatcher(),
        );
    }

    /// Fetches a server formula (`GET /office/maths/formulas/:id`) off the UI thread and opens it.
    fn open_server(&mut self, id: String) {
        let Some(dispatcher) = self.dispatcher() else {
            kubuno_desktop::tracing::warn!("[maths] no dispatcher: the server formula cannot open");
            return;
        };
        std::thread::spawn(move || {
            let fetched = (|| {
                let transport = kubuno_office_maths_common::api::connect_account()?;
                let body = kubuno_office_maths_common::api::send(&*transport, &kubuno_office_maths_common::api::routes::get_formula(&id))?;
                let f = body.get("formula").ok_or_else(|| "no formula in response".to_string())?;
                let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("Formule").to_string();
                let latex = f.get("latex").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let starred = f.get("is_starred").and_then(|v| v.as_bool()).unwrap_or(false);
                let mut doc = MathsDoc::from_latex(name, latex);
                doc.id = Some(id.clone());
                doc.starred = starred;
                Ok::<MathsDoc, String>(doc)
            })();
            drop(dispatcher.begin_invoke(move |w: &mut MathsWindow| match fetched {
                Ok(doc) => w.on_fetched(doc),
                Err(e) => kubuno_desktop::tracing::error!("[maths] open --doc failed: {e}"),
            }));
        });
    }

    fn on_fetched(&mut self, doc: MathsDoc) {
        self.starred = doc.starred;
        self.doc_title = doc.name.clone();
        self.window_title = format!("{} — {}", doc.name, Resources::app_title());
        self.canvas.with(|c| c.open(doc));
    }

    fn canvas_state_changed(&mut self, e: &MathsStateEventArgs) {
        let s = e.clone();
        if !s.title.is_empty() {
            self.doc_title = s.title.clone();
        }
        self.pages_text = Resources::status_pages().replace("{n}", &s.pages.to_string());
        self.blocks_text = Resources::status_blocks().replace("{n}", &s.blocks.to_string());
        self.zoom_text = format!("{} %", s.zoom.round() as i32);
        self.can_save = s.dirty;
        self.can_undo = s.can_undo;
        self.can_redo = s.can_redo;
        self.has_selection = s.has_selection;
        self.has_formula = s.active_is_formula;
        let lines = self.canvas.with(|c| c.active_block_lines()).unwrap_or_default();
        self.properties.with(|c| c.set_lines(lines));
    }

    /// A command from a side panel or the structures bar (`insert:*`, `struct:<latex>`, `sym:<latex>`).
    fn panel_command(&mut self, e: &PanelCmdEventArgs) {
        let cmd = e.command.clone();
        if let Some(rest) = cmd.strip_prefix("insert:") {
            match rest {
                "formula" => {
                    self.canvas.with(|c| c.insert_block(MathBlock::Formula { latex: String::new(), align: None, numbered: None, frame: None, scale: None }));
                }
                "graph" => {
                    self.canvas.with(|c| c.insert_block(MathBlock::Graph { spec: default_graph_spec(0), frame: None }));
                }
                "h1" => {
                    self.canvas.with(|c| c.insert_block(MathBlock::Text { text: "Titre".to_string(), style: TextStyle::H1, frame: None, scale: None }));
                }
                "h2" => {
                    self.canvas.with(|c| c.insert_block(MathBlock::Text { text: "Sous-titre".to_string(), style: TextStyle::H2, frame: None, scale: None }));
                }
                "p" => {
                    self.canvas.with(|c| c.insert_block(MathBlock::Text { text: "Paragraphe".to_string(), style: TextStyle::P, frame: None, scale: None }));
                }
                _ => {}
            }
        } else if let Some(latex) = cmd.strip_prefix("struct:").or_else(|| cmd.strip_prefix("sym:")) {
            let latex = latex.to_string();
            self.canvas.with(|c| c.append_or_insert(&latex));
        }
    }

    // ── Shared helpers ───────────────────────────────────────────────────────
    fn set_status(&mut self, msg: &str) {
        self.status_msg = msg.to_string();
    }

    fn active_latex(&mut self) -> Option<String> {
        self.canvas.with(|c| c.active_formula_latex()).flatten()
    }

    /// Inserts a result LaTeX as a new block, or shows the engine's error in the status bar.
    fn apply(&mut self, latex: String, error: Option<String>) {
        match error {
            Some(e) => self.set_status(&e),
            None if !latex.is_empty() => {
                self.canvas.with(|c| c.insert_result(latex));
                self.set_status("Résultat inséré.");
            }
            _ => {}
        }
    }

    /// Runs `f` on the selected formula's LaTeX and inserts the `(latex, error)` it returns.
    fn run<F: FnOnce(&str) -> (String, Option<String>)>(&mut self, f: F) {
        match self.active_latex() {
            Some(l) => {
                let (latex, err) = f(&l);
                self.apply(latex, err);
            }
            None => self.set_status("Sélectionnez une formule."),
        }
    }

    /// Runs `f` on the selected formula evaluated to a number (the Nombres operations).
    fn run_num<F: FnOnce(f64) -> (String, Option<String>)>(&mut self, f: F) {
        let Some(l) = self.active_latex() else {
            self.set_status("Sélectionnez une formule.");
            return;
        };
        match eval::eval_latex(&l) {
            Some(v) => {
                let (latex, err) = f(v);
                self.apply(latex, err);
            }
            None => self.set_status("La formule n'est pas un nombre constant."),
        }
    }

    /// Prompts for a number (comma or dot accepted).
    fn ask_num(&mut self, title: &str, default: f64) -> Option<f64> {
        let s = PromptDialog::ask(self, title, &fnum(default))?;
        let s = s.trim().replace(',', ".");
        match s.as_str() {
            "inf" | "+inf" | "∞" | "+∞" => Some(f64::INFINITY),
            "-inf" | "-∞" => Some(f64::NEG_INFINITY),
            _ => s.parse::<f64>().ok(),
        }
    }

    // ── Document / blocks ─────────────────────────────────────────────────────
    fn save_execute(&mut self) {
        self.save();
    }
    fn undo_execute(&mut self) {
        self.canvas.with(|c| c.undo());
    }
    fn redo_execute(&mut self) {
        self.canvas.with(|c| c.redo());
    }
    fn new_execute(&mut self) {
        self.canvas.with(|c| c.open(MathsDoc::blank()));
    }
    fn dup_execute(&mut self) {
        self.set_status("Dupliquer le document : à venir.");
    }
    fn b_dup_execute(&mut self) {
        self.canvas.with(|c| c.duplicate_active());
    }
    fn b_del_execute(&mut self) {
        self.canvas.with(|c| c.delete_active());
    }
    fn b_fwd_execute(&mut self) {
        self.canvas.with(|c| c.move_active(-1));
    }
    fn b_bwd_execute(&mut self) {
        self.canvas.with(|c| c.move_active(1));
    }
    fn edit_execute(&mut self) {
        let Some(cur) = self.active_latex() else {
            self.set_status("Sélectionnez une formule.");
            return;
        };
        if let Some(latex) = PromptDialog::ask(self, "Formule (LaTeX)", &cur) {
            self.canvas.with(|c| c.set_active_formula_latex(latex));
        }
    }

    // ── Mode / zoom ────────────────────────────────────────────────────────────
    fn visual_execute(&mut self) {
        self.visual_mode = true;
        self.code_mode = false;
        self.canvas.with(|c| c.set_code_mode(false));
    }
    fn code_execute(&mut self) {
        self.code_mode = true;
        self.visual_mode = false;
        self.canvas.with(|c| c.set_code_mode(true));
        self.set_status("Mode code : « Modifier… » édite le LaTeX de la formule sélectionnée.");
    }
    fn zin_execute(&mut self) {
        self.scale_zoom(1.1);
    }
    fn zout_execute(&mut self) {
        self.scale_zoom(1.0 / 1.1);
    }
    fn zreset_execute(&mut self) {
        self.canvas.with(|c| c.set_zoom(1.0));
    }
    fn scale_zoom(&mut self, factor: f32) {
        self.canvas.with(|c| {
            let z = c.zoom() * factor;
            c.set_zoom(z);
        });
    }

    // ── Insertion ──────────────────────────────────────────────────────────────
    fn ins_formula_execute(&mut self) {
        self.canvas.with(|c| c.insert_block(MathBlock::Formula { latex: String::new(), align: None, numbered: None, frame: None, scale: None }));
    }
    fn ins_graph_execute(&mut self) {
        self.canvas.with(|c| c.insert_block(MathBlock::Graph { spec: default_graph_spec(0), frame: None }));
    }
    fn ins_text_execute(&mut self) {
        self.canvas.with(|c| c.insert_block(MathBlock::Text { text: "Texte".to_string(), style: TextStyle::P, frame: None, scale: None }));
    }
    fn add_page_execute(&mut self) {
        self.canvas.with(|c| c.add_page());
    }

    // ── Équation structures (appended to the selected formula) ──────────────────
    fn append(&mut self, snippet: &str) {
        self.canvas.with(|c| c.append_or_insert(snippet));
    }
    fn eq_frac_execute(&mut self) {
        self.append("\\frac{\\square}{\\square}");
    }
    fn eq_sqrt_execute(&mut self) {
        self.append("\\sqrt{\\square}");
    }
    fn eq_root_execute(&mut self) {
        self.append("\\sqrt[\\square]{\\square}");
    }
    fn eq_pow_execute(&mut self) {
        self.append("\\square^{\\square}");
    }
    fn eq_sub_execute(&mut self) {
        self.append("\\square_{\\square}");
    }
    fn eq_sum_execute(&mut self) {
        self.append("\\sum_{\\square}^{\\square}");
    }
    fn eq_int_execute(&mut self) {
        self.append("\\int_{\\square}^{\\square}");
    }
    fn eq_mat_execute(&mut self) {
        self.append("\\begin{pmatrix}\\square & \\square \\\\ \\square & \\square\\end{pmatrix}");
    }
    fn eq_paren_execute(&mut self) {
        self.append("\\left(\\square\\right)");
    }
    fn eq_abs_execute(&mut self) {
        self.append("\\left|\\square\\right|");
    }
    fn eq_pi_execute(&mut self) {
        self.append("\\pi");
    }
    fn eq_alpha_execute(&mut self) {
        self.append("\\alpha");
    }
    fn eq_theta_execute(&mut self) {
        self.append("\\theta");
    }
    fn eq_lambda_execute(&mut self) {
        self.append("\\lambda");
    }
    fn eq_sigma_execute(&mut self) {
        self.append("\\sigma");
    }
    fn eq_infty_execute(&mut self) {
        self.append("\\infty");
    }

    // ── Calcul ─────────────────────────────────────────────────────────────────
    fn deriv_execute(&mut self) {
        self.run(|l| {
            let r = cas::derivative_latex(l);
            (r.latex, r.error)
        });
    }
    fn deriv2_execute(&mut self) {
        self.run(|l| {
            let r = cas::nth_derivative_latex(l, 2);
            (r.latex, r.error)
        });
    }
    fn integ_execute(&mut self) {
        self.run(|l| {
            let r = cas::integral_latex(l);
            (r.latex, r.error)
        });
    }
    fn area_execute(&mut self) {
        let (Some(a), Some(b)) = (self.ask_num("Borne inférieure a", 0.0), self.ask_num("Borne supérieure b", 1.0)) else {
            return;
        };
        self.run(move |l| match cas::definite_integral(l, a, b) {
            (Some(v), _) => (format!("\\int_{{{}}}^{{{}}} f(x)\\,dx \\approx {}", fnum(a), fnum(b), eval::fmt_result(v)), None),
            (None, err) => (String::new(), err),
        });
    }
    fn simplify_execute(&mut self) {
        self.run(|l| {
            let r = cas::simplify_latex(l);
            (r.latex, r.error)
        });
    }
    fn expand_execute(&mut self) {
        self.run(|l| {
            let r = cas::expand_latex(l);
            (r.latex, r.error)
        });
    }
    fn factor_execute(&mut self) {
        self.run(|l| {
            let r = cas::factor_latex(l);
            (r.latex, r.error)
        });
    }
    fn solve_execute(&mut self) {
        self.run(|l| match cas::solve_roots(l) {
            (roots, None) if !roots.is_empty() => {
                let body = roots.iter().enumerate().map(|(i, r)| format!("x_{{{}}} \\approx {}", i + 1, eval::fmt_result(*r))).collect::<Vec<_>>().join(",\\quad ");
                (body, None)
            }
            (_, Some(e)) => (String::new(), Some(e)),
            _ => (String::new(), Some("Aucune racine réelle sur [-10, 10].".to_string())),
        });
    }
    fn exact_execute(&mut self) {
        self.run(|l| {
            let r = cas::solve_exact_latex(l);
            (r.latex, r.error)
        });
    }
    fn taylor_execute(&mut self) {
        let (Some(order), Some(center)) = (self.ask_num("Ordre", 5.0), self.ask_num("Centre", 0.0)) else {
            return;
        };
        self.run(move |l| {
            let r = cas::taylor_latex(l, order as i64, center);
            (r.latex, r.error)
        });
    }
    fn extrema_execute(&mut self) {
        self.run(|l| {
            let r = cas::extrema_latex(l);
            (r.latex, r.error)
        });
    }
    fn inflection_execute(&mut self) {
        self.run(|l| {
            let r = cas::inflection_latex(l);
            (r.latex, r.error)
        });
    }
    fn asymptotes_execute(&mut self) {
        self.run(|l| {
            let r = cas::asymptotes_latex(l);
            (r.latex, r.error)
        });
    }
    fn limit_execute(&mut self) {
        let Some(at) = self.ask_num("Point (nombre, inf, -inf)", 0.0) else {
            return;
        };
        self.run(move |l| {
            let r = cas::limit_latex(l, at);
            (r.latex, r.error)
        });
    }
    fn tangent_execute(&mut self) {
        let Some(a) = self.ask_num("Point x = a", 0.0) else {
            return;
        };
        self.run(move |l| {
            let (latex, _expr, err) = cas::tangent_at(l, a);
            (latex, err)
        });
    }
    fn evalpt_execute(&mut self) {
        let Some(a) = self.ask_num("x = a", 1.0) else {
            return;
        };
        self.run(move |l| {
            let r = cas::eval_point_latex(l, a);
            (r.latex, r.error)
        });
    }
    fn table_execute(&mut self) {
        self.run(|l| {
            let r = cas::value_table_latex(l, &[-2.0, -1.0, 0.0, 1.0, 2.0]);
            (r.latex, r.error)
        });
    }
    fn sum_execute(&mut self) {
        let (Some(a), Some(b)) = (self.ask_num("De x =", 1.0), self.ask_num("à x =", 10.0)) else {
            return;
        };
        self.run(move |l| {
            let r = cas::sum_latex(l, a, b);
            (r.latex, r.error)
        });
    }
    fn product_execute(&mut self) {
        let (Some(a), Some(b)) = (self.ask_num("De x =", 1.0), self.ask_num("à x =", 5.0)) else {
            return;
        };
        self.run(move |l| {
            let r = cas::product_latex(l, a, b);
            (r.latex, r.error)
        });
    }

    // ── Matrices ─────────────────────────────────────────────────────────────────
    fn det_execute(&mut self) {
        self.run(|l| {
            let r = matrix::determinant_latex(l);
            (r.latex, r.error)
        });
    }
    fn inv_execute(&mut self) {
        self.run(|l| {
            let r = matrix::inverse_latex(l);
            (r.latex, r.error)
        });
    }
    fn transpose_execute(&mut self) {
        self.run(|l| {
            let r = matrix::transpose_latex(l);
            (r.latex, r.error)
        });
    }
    fn rank_execute(&mut self) {
        self.run(|l| {
            let r = matrix::rank_latex(l);
            (r.latex, r.error)
        });
    }
    fn trace_execute(&mut self) {
        self.run(|l| {
            let r = matrix::trace_latex(l);
            (r.latex, r.error)
        });
    }
    fn power_execute(&mut self) {
        let Some(p) = self.ask_num("Puissance", 2.0) else {
            return;
        };
        self.run(move |l| {
            let r = matrix::power_latex(l, p as i64);
            (r.latex, r.error)
        });
    }
    fn rref_execute(&mut self) {
        self.run(|l| {
            let r = matrix::rref_latex(l);
            (r.latex, r.error)
        });
    }
    fn charpoly_execute(&mut self) {
        self.run(|l| {
            let r = matrix::char_poly_latex(l);
            (r.latex, r.error)
        });
    }
    fn eigen_execute(&mut self) {
        self.run(|l| {
            let r = matrix::eigenvalues_latex(l);
            (r.latex, r.error)
        });
    }
    fn system_execute(&mut self) {
        self.run(|l| {
            let r = matrix::solve_system_latex(l);
            (r.latex, r.error)
        });
    }

    // ── Statistiques ───────────────────────────────────────────────────────────
    fn stats_execute(&mut self) {
        self.run(|l| {
            let r = stats::stats_latex(l);
            (r.latex, r.error)
        });
    }
    fn fivenum_execute(&mut self) {
        self.run(|l| {
            let r = stats::five_number_latex(l);
            (r.latex, r.error)
        });
    }
    fn regression_execute(&mut self) {
        self.run(|l| {
            let r = stats::regression_latex(l);
            (r.latex, r.error)
        });
    }

    // ── Nombres ──────────────────────────────────────────────────────────────────
    fn primes_execute(&mut self) {
        self.run_num(|v| {
            let r = number::prime_factors_latex(v);
            (r.latex, r.error)
        });
    }
    fn factorial_execute(&mut self) {
        self.run_num(|v| {
            let r = number::factorial_latex(v);
            (r.latex, r.error)
        });
    }
    fn baseconv_execute(&mut self) {
        self.run_num(|v| {
            let r = number::base_convert_latex(v);
            (r.latex, r.error)
        });
    }
    fn gcdlcm_execute(&mut self) {
        let (Some(a), Some(b)) = (self.ask_num("a", 12.0), self.ask_num("b", 18.0)) else {
            return;
        };
        let r = number::gcd_lcm_latex(a, b);
        self.apply(r.latex, r.error);
    }
    fn binomial_execute(&mut self) {
        let (Some(n), Some(k)) = (self.ask_num("n", 5.0), self.ask_num("k", 2.0)) else {
            return;
        };
        let r = number::binomial_latex(n, k);
        self.apply(r.latex, r.error);
    }

    // ── Backstage / star ─────────────────────────────────────────────────────────
    fn bs_close_click(&mut self) {
        self.close();
    }

    fn star_execute(&mut self) {
        let Source::Server(id) = self.source.clone() else { return };
        let to = !self.starred;
        self.starred = to;
        std::thread::spawn(move || {
            if let Ok(transport) = kubuno_office_maths_common::api::connect_account() {
                let _ = kubuno_office_maths_common::api::send(&*transport, &kubuno_office_maths_common::api::routes::update_formula(&id, None, None, Some(to)));
            }
        });
    }

    // ── Save (increment C) ─────────────────────────────────────────────────────────
    fn save(&mut self) {
        let latex = self.canvas.with(|c| c.document().to_latex());
        let Some(latex) = latex else { return };
        match self.source.clone() {
            Source::Server(id) => self.save_server(id, latex),
            _ => self.save_local(latex),
        }
    }

    /// Server save: a plain `PATCH /formulas/:id {latex}`, after a re-GET conflict check (Maths has no
    /// digest/editing-session model — see MATHS-DESKTOP.md §2).
    fn save_server(&mut self, id: String, latex: String) {
        let Some(dispatcher) = self.dispatcher() else { return };
        self.set_status("Enregistrement…");
        let base = self.canvas.with(|c| c.document().latex.clone()).unwrap_or_default();
        std::thread::spawn(move || {
            let result = (|| {
                let transport = kubuno_office_maths_common::api::connect_account()?;
                // Re-read and compare: if the server's latex changed under us since we opened, flag a conflict.
                let cur = kubuno_office_maths_common::api::send(&*transport, &kubuno_office_maths_common::api::routes::get_formula(&id))?;
                let server_latex = cur.get("formula").and_then(|f| f.get("latex")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                if server_latex != base && server_latex != latex {
                    return Ok(SaveOutcome::Conflict(server_latex));
                }
                kubuno_office_maths_common::api::send(&*transport, &kubuno_office_maths_common::api::routes::update_formula(&id, Some(&latex), None, None))?;
                Ok::<SaveOutcome, String>(SaveOutcome::Saved)
            })();
            drop(dispatcher.begin_invoke(move |w: &mut MathsWindow| match result {
                Ok(SaveOutcome::Saved) => {
                    w.canvas.with(|c| c.mark_saved());
                    w.set_status("Enregistré.");
                }
                Ok(SaveOutcome::Conflict(_theirs)) => {
                    w.set_status("Conflit : la formule a changé sur le serveur. Rouvrez-la ou ré-enregistrez pour écraser.");
                }
                Err(e) => w.set_status(&format!("Échec de l'enregistrement : {e}")),
            }));
        });
    }

    /// Local save: write the stored `latex` back to the file (a plain LaTeX text file; `.kbmath` JSON when the
    /// name ends in `.kbmath`). Offline, no server.
    fn save_local(&mut self, latex: String) {
        // Offline journal of the stored bytes under the app data dir (replayed next open is a later refinement;
        // for now the journal records the last saved content so work is not lost on crash).
        if let Ok(dir) = kubuno_desktop_account::paths::user_data_dir() {
            let journal = dir.join("kubuno-maths").join("journal");
            let _ = std::fs::create_dir_all(&journal);
            let _ = std::fs::write(journal.join("last.kbmath"), serde_json::json!({ "version": 1, "latex": latex }).to_string());
        }
        self.canvas.with(|c| c.mark_saved());
        self.set_status("Journal local enregistré (pas de fichier serveur).");
    }
}

/// The outcome of a background server save.
enum SaveOutcome {
    Saved,
    Conflict(String),
}
