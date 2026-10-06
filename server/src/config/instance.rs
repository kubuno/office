//! Instance-wide settings of the office module, as the administrator left them in
//! the console.
//!
//! Declared by `module.toml`'s `[[settings]]` blocks, stored in `core.settings`,
//! and read back here through `/internal/modules/office/settings` — a module owns
//! its own schema and cannot read the core's tables, and a background refresher
//! has no user token for the public config route. The module is named in the URL
//! (not derived from the secret) so the read works whether the instance shares
//! one master secret between modules or issues a derived one per module.
//!
//! Only the settings APPLIED BY THE BACKEND live here — the defaults stamped onto
//! a new document or sheet at creation. The editor-side defaults (margins, track
//! changes, autosave intervals, decimal separator) are read by the frontend from
//! `/api/v1/config`, since they act inside the running editor, not at creation.
//!
//! Every field here is read by code that acts on it: a knob that changes nothing
//! is worse than an absent one.

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct InstanceConfig {
    /// Save format stamped on a new text document (`docx` | `odt`).
    pub default_format: String,
    /// Save format stamped on a new spreadsheet (`xlsx` | `ods` | `csv`).
    pub spreadsheet_default_format: String,
    /// Whether a new spreadsheet freezes its first row as a header.
    pub spreadsheet_header_row: bool,
    /// Slide shape of a new presentation (`16:9` | `4:3`).
    pub presentation_aspect: String,
    /// Whether a new diagram snaps shapes to the grid.
    pub diagram_snap_to_grid: bool,
    /// Accent colour of a new project (hex).
    pub project_default_color: String,
    /// Currency a new project's budgets, expenses and contracts are kept in
    /// (ISO 4217). Existing projects keep theirs: a change of policy must not
    /// rewrite amounts already entered.
    pub project_default_currency: String,
    pub project_default_include_weekends: bool,
    pub project_default_exclude_holidays: bool,
    /// Background of a new whiteboard (`white` | `grid` | `dots` | `lines`).
    pub whiteboard_background: String,
    /// Colour palette of a new data report's visuals (a palette id).
    pub data_default_palette: String,

    // ── Document layout stamped at creation ──────────────────────────────────
    // These three used to be listed as "frontend defaults we cannot reach". They
    // are reachable after all, and from the BACKEND: a document's layout lives in
    // the `multi-page` envelope of its content file, which the editor parses on
    // load. Writing the envelope at creation is therefore all it takes — no
    // editor surgery, and the value is captured in the document instead of being
    // re-read from the console on every open.
    /// Page margins of a new document, in millimetres. `0` = the factory value
    /// (25.4 mm, one inch), left untouched.
    pub default_margins_mm: i64,
    /// Paper size of a new document (`a4` | `a5` | `a3` | `letter` | `legal`).
    pub default_paper_size: String,
    /// Whether a new document opens with change tracking already on.
    pub track_changes_default: bool,

    // ── Public document links ────────────────────────────────────────────────
    /// Whether a document may be published behind a public token link at all.
    /// Off also stops serving the links already created.
    pub document_public_links_enabled: bool,
    /// Ceiling on a public document link's lifetime, in days. `0` = none.
    pub document_share_max_expiry_days: i64,
    /// Lifetime given to a public document link created without an expiry date,
    /// in days. `0` = none.
    pub document_share_default_expiry_days: i64,
    /// The most a public document link may grant (`view` | `comment` | `edit`).
    /// A request for more is brought back to this.
    pub document_share_max_permission: String,

    // ── Retention ────────────────────────────────────────────────────────────
    /// Revisions kept per document/spreadsheet; the oldest beyond this is pruned
    /// on every new one.
    pub max_versions: i64,
    /// Days an item stays in the trash before the cleaner deletes it for good.
    /// `0` = never — the trash keeps everything until someone empties it.
    pub trash_retention_days: i64,
}

/// The most a public link may grant, ordered from least to most. Used to clamp
/// what a user asks for down to what the administrator allows.
const PERMISSION_RANK: &[&str] = &["view", "comment", "edit"];

/// Millimetres → CSS pixels at 96 dpi, the unit the editor's `SectionDef`
/// margins are expressed in.
pub fn mm_to_px(mm: i64) -> i64 {
    ((mm as f64) * 96.0 / 25.4).round() as i64
}

impl Default for InstanceConfig {
    fn default() -> Self {
        Self {
            default_format:             "docx".to_string(),
            spreadsheet_default_format: "xlsx".to_string(),
            spreadsheet_header_row:     true,
            presentation_aspect:        "16:9".to_string(),
            diagram_snap_to_grid:       true,
            project_default_color:      "#1a73e8".to_string(),
            project_default_currency:   "EUR".to_string(),
            project_default_include_weekends: false,
            project_default_exclude_holidays: false,
            whiteboard_background:      "dots".to_string(),
            data_default_palette:       "kubuno".to_string(),
            default_margins_mm:         0,
            default_paper_size:         "a4".to_string(),
            track_changes_default:      false,
            document_public_links_enabled:      true,
            document_share_max_expiry_days:     0,
            document_share_default_expiry_days: 0,
            document_share_max_permission:      "edit".to_string(),
            max_versions:                       50,
            trash_retention_days:               0,
        }
    }
}

impl InstanceConfig {
    /// Maps the core's `{key: value}` object onto the struct. Every read falls
    /// back to the compiled default rather than to a permissive value: a payload
    /// missing a key (an older core, a failed migration) must not silently change
    /// a default. An enum value outside the declared set is treated as a mistake
    /// and ignored the same way.
    pub fn from_settings(settings: &Value) -> Self {
        let d = Self::default();
        let enum_of = |key: &str, allowed: &[&str], fallback: String| -> String {
            settings
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| allowed.contains(s))
                .map(str::to_string)
                .unwrap_or(fallback)
        };
        let bool_of = |key: &str, fallback: bool| {
            settings.get(key).and_then(Value::as_bool).unwrap_or(fallback)
        };
        // An integer outside its declared bounds is a mistake, not an intent:
        // it falls back to the shipped default rather than to the nearest edge.
        let int_of = |key: &str, min: i64, max: i64, fallback: i64| -> i64 {
            settings
                .get(key)
                .and_then(Value::as_i64)
                .filter(|n| (min..=max).contains(n))
                .unwrap_or(fallback)
        };
        // A free-form string (a colour): kept only when non-empty.
        let str_of = |key: &str, fallback: String| -> String {
            settings
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or(fallback)
        };
        Self {
            default_format: enum_of("default_format", &["docx", "odt"], d.default_format),
            spreadsheet_default_format: enum_of(
                "spreadsheet_default_format",
                &["xlsx", "ods", "csv"],
                d.spreadsheet_default_format,
            ),
            spreadsheet_header_row: bool_of("spreadsheet_header_row", d.spreadsheet_header_row),
            presentation_aspect: enum_of("presentation_aspect", &["16:9", "4:3"], d.presentation_aspect),
            diagram_snap_to_grid: bool_of("diagram_snap_to_grid", d.diagram_snap_to_grid),
            project_default_color: str_of("project_default_color", d.project_default_color),
            project_default_currency: str_of("project_default_currency", d.project_default_currency),
            project_default_include_weekends: bool_of("project_default_include_weekends", d.project_default_include_weekends),
            project_default_exclude_holidays: bool_of("project_default_exclude_holidays", d.project_default_exclude_holidays),
            whiteboard_background: enum_of(
                "whiteboard_background",
                &["white", "grid", "dots", "lines"],
                d.whiteboard_background,
            ),
            data_default_palette: enum_of(
                "data_default_palette",
                &[
                    "kubuno", "classic", "vibrant", "cool", "warm", "pastel", "earth",
                    "colorblind", "grayscale", "default", "executive", "dark",
                ],
                d.data_default_palette,
            ),
            default_margins_mm:    int_of("default_margins_mm", 0, 100, d.default_margins_mm),
            default_paper_size:    enum_of(
                "default_paper_size",
                &["a4", "a5", "a3", "letter", "legal"],
                d.default_paper_size,
            ),
            track_changes_default: bool_of("track_changes_default", d.track_changes_default),
            document_public_links_enabled: bool_of(
                "document_public_links_enabled",
                d.document_public_links_enabled,
            ),
            document_share_max_expiry_days: int_of(
                "document_share_max_expiry_days", 0, 3650, d.document_share_max_expiry_days,
            ),
            document_share_default_expiry_days: int_of(
                "document_share_default_expiry_days", 0, 3650, d.document_share_default_expiry_days,
            ),
            document_share_max_permission: enum_of(
                "document_share_max_permission",
                PERMISSION_RANK,
                d.document_share_max_permission,
            ),
            max_versions:         int_of("max_versions", 1, 1000, d.max_versions),
            trash_retention_days: int_of("trash_retention_days", 0, 3650, d.trash_retention_days),
        }
    }

    /// Brings a requested public-link permission down to what the instance
    /// allows. An unknown request is treated as the weakest, never the strongest:
    /// a typo must not hand out edit rights.
    pub fn clamp_share_permission(&self, requested: &str) -> String {
        let rank = |p: &str| PERMISSION_RANK.iter().position(|c| *c == p);
        let cap = rank(&self.document_share_max_permission).unwrap_or(0);
        match rank(requested) {
            Some(r) if r <= cap => requested.to_string(),
            Some(_)             => self.document_share_max_permission.clone(),
            None                => "view".to_string(),
        }
    }

    /// The document margins, in CSS pixels, to stamp on a new document — or
    /// `None` when the administrator left the factory value alone.
    pub fn default_margins_px(&self) -> Option<i64> {
        (self.default_margins_mm > 0).then(|| mm_to_px(self.default_margins_mm))
    }
}

/// Reads the instance settings from the core. Any failure yields `None`, so the
/// caller keeps the values it already had rather than reverting to defaults
/// because the core was briefly unreachable.
pub async fn fetch(http: &reqwest::Client, core_url: &str, secret: &str) -> Option<InstanceConfig> {
    let url = format!("{core_url}/internal/modules/office/settings");
    let resp = http
        .get(&url)
        .header("X-Internal-Secret", secret)
        .send()
        .await
        .map_err(|e| tracing::warn!(error = %e, "Lecture des réglages d'instance office"))
        .ok()?;

    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), "Réglages d'instance office refusés par le core");
        return None;
    }

    let body: Value = resp
        .json()
        .await
        .map_err(|e| tracing::warn!(error = %e, "Réglages d'instance office : réponse illisible"))
        .ok()?;

    Some(InstanceConfig::from_settings(body.get("settings")?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_keys_keep_the_compiled_defaults() {
        let cfg = InstanceConfig::from_settings(&json!({}));
        assert_eq!(cfg.default_format, "docx");
        assert_eq!(cfg.spreadsheet_default_format, "xlsx");
        assert!(cfg.spreadsheet_header_row);
    }

    #[test]
    fn values_are_read() {
        let cfg = InstanceConfig::from_settings(&json!({
            "default_format": "odt",
            "spreadsheet_default_format": "csv",
            "spreadsheet_header_row": false,
        }));
        assert_eq!(cfg.default_format, "odt");
        assert_eq!(cfg.spreadsheet_default_format, "csv");
        assert!(!cfg.spreadsheet_header_row);
    }

    #[test]
    fn unknown_enum_value_falls_back() {
        let cfg = InstanceConfig::from_settings(&json!({ "default_format": "pdf" }));
        assert_eq!(cfg.default_format, "docx");
    }

    #[test]
    fn a_public_link_never_grants_more_than_the_instance_allows() {
        let mut cfg = InstanceConfig::default();
        // Shipped default: nothing is clamped.
        assert_eq!(cfg.clamp_share_permission("edit"), "edit");
        cfg.document_share_max_permission = "comment".into();
        assert_eq!(cfg.clamp_share_permission("view"), "view");
        assert_eq!(cfg.clamp_share_permission("comment"), "comment");
        assert_eq!(cfg.clamp_share_permission("edit"), "comment");
        // A typo falls to the weakest, never to the strongest.
        assert_eq!(cfg.clamp_share_permission("owner"), "view");
    }

    #[test]
    fn margins_convert_millimetres_to_editor_pixels_at_96_dpi() {
        let mut cfg = InstanceConfig::default();
        // Untouched: the document keeps the factory inch.
        assert_eq!(cfg.default_margins_px(), None);
        cfg.default_margins_mm = 25;
        assert_eq!(cfg.default_margins_px(), Some(94));
        cfg.default_margins_mm = 20;
        assert_eq!(cfg.default_margins_px(), Some(76));
    }

    #[test]
    fn the_new_knobs_default_to_the_behaviour_shipped_before_them() {
        let cfg = InstanceConfig::from_settings(&json!({}));
        assert_eq!(cfg.default_paper_size, "a4");
        assert!(!cfg.track_changes_default);
        assert!(cfg.document_public_links_enabled);
        assert_eq!(cfg.max_versions, 50);
        assert_eq!(cfg.trash_retention_days, 0);
    }

    #[test]
    fn out_of_range_numbers_are_ignored_rather_than_applied() {
        let cfg = InstanceConfig::from_settings(&json!({
            "max_versions": 0,
            "trash_retention_days": 99999,
            "default_margins_mm": 500,
        }));
        assert_eq!(cfg.max_versions, 50);
        assert_eq!(cfg.trash_retention_days, 0);
        assert_eq!(cfg.default_margins_mm, 0);
    }
}
