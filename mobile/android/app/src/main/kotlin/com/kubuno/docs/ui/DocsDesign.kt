package com.kubuno.docs.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.kubuno.android.account.SharedAccount

/**
 * Design tokens for the documents app, transcribed from the office web module.
 *
 * Only what is *specific to office/documents* lives here: the shared Kubuno
 * palette, radii and type scale already come from
 * `com.kubuno.android.ui.theme.KubunoTheme`, and duplicating them would let the
 * two drift apart. Everything below is sourced from one of:
 *   - office/frontend/src/theme.css                (module palette)
 *   - office/frontend/src/ribbon/officeThemes.ts   (per-editor tone)
 *   - core/frontend/src/core/shell/workspace/theme.ts (WORKSPACE_OFFICE chrome)
 *   - office/frontend/src/DocumentEditorPage.tsx   (paper geometry)
 *   - office/frontend/src/canvas-engine.ts         (text metrics of the page)
 */

// ─────────────────────────────────────────────────────────────────────────────
// Colours
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The office palette. The `Dark*` values have no web counterpart — the office
 * shell is light-only on the desktop — so they are aligned with the core-ui
 * dark reference (`core-ui/.../theme/Theme.kt`, `Ref.Dark*`) rather than
 * invented, which keeps the app consistent with every other Kubuno app in
 * `values-night`. Assumed divergence, documented on each affected value.
 */
object DocsColors {
    // ── Documents tone ───────────────────────────────────────────────────────
    // OFFICE_TONE.documents: the tab-strip colour of the documents editor, which
    // doubles as its accent. officeThemes.ts also special-cases the lighter
    // variant used for the File tab and the backstage rail.
    val Tone = Color(0xFF1557B0)
    val ToneLight = Color(0xFF3F7DD0)

    // On a dark surface #1557B0 loses almost all contrast, so night mode promotes
    // the web's OWN lighter variant to the accent role instead of picking a new
    // hue: the brand stays recognisable and no value is invented.
    val DarkTone = ToneLight
    val DarkToneLight = Color(0xFF8AB4F8) // core-ui Ref.DarkPrimary

    /** Text/icon colour laid over [Tone]; officeTheme() sets topbarText to white. */
    val OnTone = Color(0xFFFFFFFF)

    // ── Primary (theme.css --color-primary*) ─────────────────────────────────
    val Primary = Color(0xFF1A73E8)
    val PrimaryHover = Color(0xFF1557B0)
    val PrimaryLight = Color(0xFFD3E3FD)

    val DarkPrimary = Color(0xFF8AB4F8)
    val DarkPrimaryContainer = Color(0xFF1A3A5C)

    // ── Surfaces (theme.css --color-surface-*) ───────────────────────────────
    val Surface0 = Color(0xFFFFFFFF)
    val Surface1 = Color(0xFFF8F9FA)
    val Surface2 = Color(0xFFF1F3F4)
    val Surface3 = Color(0xFFE8EAED)

    /** theme.css --color-hover; identical to surface-2 by design. */
    val Hover = Color(0xFFF1F3F4)

    /** theme.css --color-search-bg — the tinted search field of the office shell. */
    val SearchBg = Color(0xFFE9EEF6)

    /** WORKSPACE_OFFICE.active — the row/tab selection tint of the office chrome. */
    val Selected = Color(0xFFE8F0FE)

    /** theme.css --body-bg: the page behind the module card. */
    val BodyBg = Color(0xFFF8FAFD)

    val DarkSurface0 = Color(0xFF202124)
    val DarkSurface1 = Color(0xFF292A2D)
    val DarkSurface2 = Color(0xFF35363A)
    val DarkSurface3 = Color(0xFF444746)
    val DarkHover = Color(0xFF2B2C30)
    val DarkSelected = Color(0xFF22344D)
    val DarkBodyBg = Color(0xFF17181B)

    // ── Text (theme.css --color-text-*) ──────────────────────────────────────
    // Office deliberately uses a softer ink (#444444) than the core shell
    // (#202124) for its chrome. Kept as-is: the difference is visible on the
    // ribbon labels, which is what this token dresses.
    val TextPrimary = Color(0xFF444444)
    val TextSecondary = Color(0xFF5F6368)
    val TextTertiary = Color(0xFF80868B)

    val DarkTextPrimary = Color(0xFFE8EAED)
    val DarkTextSecondary = Color(0xFF9AA0A6)
    val DarkTextTertiary = Color(0xFF80868B)

    // ── Borders ──────────────────────────────────────────────────────────────
    val Border = Color(0xFFE0E0E0)        // theme.css --color-border
    val BorderStrong = Color(0xFFBDC1C6)  // theme.css --color-border-strong

    /** WORKSPACE_OFFICE.border — the chrome (ribbon, panels) uses its own hairline. */
    val ChromeBorder = Color(0xFFDADCE0)

    val DarkBorder = Color(0xFF5F6368)
    val DarkBorderStrong = Color(0xFF80868B)

    // ── Semantic states (theme.css) ──────────────────────────────────────────
    val Danger = Color(0xFFD93025)
    val DangerLight = Color(0xFFFCE8E6)
    val Success = Color(0xFF1E8E3E)
    val SuccessLight = Color(0xFFE6F4EA)
    val Warning = Color(0xFFF9AB00)
    val WarningLight = Color(0xFFFEF7E0)

    val DarkDanger = Color(0xFFF28B82)
    val DarkDangerLight = Color(0xFF3D1C1C)
    val DarkSuccess = Color(0xFF81C995)
    val DarkWarning = Color(0xFFFDD663)

    // ── Document sheet ───────────────────────────────────────────────────────
    // canvas-engine.ts paints document text in PURE black, not the UI ink: the
    // module chose it on purpose so a printed page matches a desktop word
    // processor's density. Never substitute the chrome's #444444 here.
    val PageInk = Color(0xFF000000)
    val PageSurface = Color(0xFFFFFFFF)

    /** The workspace behind the sheet (WORKSPACE_OFFICE.bg is white; the sheet needs contrast). */
    val PageBackdrop = Color(0xFFF1F3F4)

    val DarkPageInk = Color(0xFFE8EAED)
    val DarkPageSurface = Color(0xFF202124)
    val DarkPageBackdrop = Color(0xFF17181B)

    /** canvas-engine.ts: the horizontal rule / placeholder frame hairline. */
    val PageRule = Color(0xFFDADCE0)

    /** canvas-engine.ts CHANGE_BAR_CLR — the revision bar in the left margin. */
    val ChangeBar = Color(0xFF3C4043)
}

// Theme-aware accessors, so screens never branch on the theme themselves.

@Composable @ReadOnlyComposable
fun docsTone(): Color = if (isSystemInDarkTheme()) DocsColors.DarkTone else DocsColors.Tone

@Composable @ReadOnlyComposable
fun docsToneLight(): Color = if (isSystemInDarkTheme()) DocsColors.DarkToneLight else DocsColors.ToneLight

@Composable @ReadOnlyComposable
fun docsPageInk(): Color = if (isSystemInDarkTheme()) DocsColors.DarkPageInk else DocsColors.PageInk

@Composable @ReadOnlyComposable
fun docsPageSurface(): Color = if (isSystemInDarkTheme()) DocsColors.DarkPageSurface else DocsColors.PageSurface

@Composable @ReadOnlyComposable
fun docsPageBackdrop(): Color = if (isSystemInDarkTheme()) DocsColors.DarkPageBackdrop else DocsColors.PageBackdrop

@Composable @ReadOnlyComposable
fun docsSelected(): Color = if (isSystemInDarkTheme()) DocsColors.DarkSelected else DocsColors.Selected

@Composable @ReadOnlyComposable
fun docsHover(): Color = if (isSystemInDarkTheme()) DocsColors.DarkHover else DocsColors.Hover

// ─────────────────────────────────────────────────────────────────────────────
// Shapes
// ─────────────────────────────────────────────────────────────────────────────

/**
 * theme.css radii, named by the usage they were defined for rather than by size,
 * so a screen picks a role and cannot drift. Note the web scale keeps 12/16 for
 * large overlays, while core-ui's Material `Shapes` caps at 8dp — use
 * [Sheet] / [Overlay] only for full-width surfaces, which is exactly where the
 * web still spends them.
 */
object DocsShape {
    /** --radius-sm 4px: chips, inline badges, small icon buttons. */
    val Chip = RoundedCornerShape(4.dp)

    /** 6dp: the radius KubunoButton/KubunoListRow already use; kept for adjacency. */
    val Row = RoundedCornerShape(6.dp)

    /** --radius / --radius-md 8px: cards, document tiles, menus. */
    val Card = RoundedCornerShape(8.dp)

    /** --radius-lg 12px: dialogs and popovers. */
    val Dialog = RoundedCornerShape(12.dp)

    /** --radius-xl / --radius-2xl 16px: bottom sheets and full-width overlays. */
    val Sheet = RoundedCornerShape(topStart = 16.dp, topEnd = 16.dp, bottomStart = 0.dp, bottomEnd = 0.dp)

    /** --radius-xl 16px on every corner: floating overlay panels. */
    val Overlay = RoundedCornerShape(16.dp)

    /** The document sheet itself is square-cornered, like paper. */
    val Page = RoundedCornerShape(0.dp)
}

// ─────────────────────────────────────────────────────────────────────────────
// Page geometry
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The paper geometry of the web editor, in "document px" — the unit
 * DocumentEditorPage.tsx lays out in, i.e. CSS px at 96 dpi
 * (`PX_PER_CM = 96 / 2.54`).
 *
 * The phone does NOT reproduce the paginated sheet. Rationale, and the assumed
 * divergence from the web: an A4 page is 794 document px wide, of which only
 * 602 carry text; fitting 794 px onto a ~360 dp phone means a 0.45 scale, which
 * renders 11 pt body text at roughly 6.6 sp — unreadable, and unusable for
 * touch caret placement. So the mobile renderer drops pagination and reflows the
 * document into a single fluid column at [PhoneTypeScale] = 1:1, i.e. the text
 * keeps the apparent size it has on a desktop at 100 % zoom. Page-anchored
 * artefacts that only exist because of pagination (headers, footers, page
 * numbers, page borders, watermarks, column breaks) therefore have no place in
 * the fluid column and are surfaced separately.
 */
object DocsPage {
    /** DocumentEditorPage.tsx PX_PER_CM. */
    const val PxPerCm: Float = 96f / 2.54f

    /** canvas-engine.ts PT_PX: 1 pt = 1.3333 document px at 96 dpi. */
    const val PtToPx: Float = 96f / 72f

    /**
     * PAPER_SIZES of DocumentEditorPage.tsx, in cm, portrait. The paper size is a
     * document-wide setting (`paperSize`, default a4); orientation stays per
     * section. Kept so the export/print sheet can name the real format even
     * though the phone reflows.
     */
    enum class PaperSize(val id: String, val widthCm: Float, val heightCm: Float, val label: String) {
        A4("a4", 21f, 29.7f, "A4 (21 × 29,7 cm)"),
        A5("a5", 14.8f, 21f, "A5 (14,8 × 21 cm)"),
        A3("a3", 29.7f, 42f, "A3 (29,7 × 42 cm)"),
        LETTER("letter", 21.59f, 27.94f, "Letter (21,6 × 27,9 cm)"),
        LEGAL("legal", 21.59f, 35.56f, "Legal (21,6 × 35,6 cm)"),
        ;

        val widthPx: Float get() = widthCm * PxPerCm
        val heightPx: Float get() = heightCm * PxPerCm

        companion object {
            /** Tolerates an unknown id from `paperSize`, like getGeometry()'s `?? PAPER_SIZES.a4`. */
            fun fromId(id: String?): PaperSize =
                entries.firstOrNull { it.id == id?.lowercase() } ?: A4
        }
    }

    val DefaultPaper: PaperSize = PaperSize.A4

    /** A4 portrait as laid out by getGeometry(): round(21 × PX_PER_CM). */
    const val DefaultPageWidthPx: Int = 794

    /** round(29.7 × PX_PER_CM). */
    const val DefaultPageHeightPx: Int = 1123

    /** DocumentEditorPage.tsx defaultSection(): 96 px on all four sides = 1 inch. */
    const val DefaultMarginPx: Int = 96

    /** Text column of a default A4 page: 794 − 96 − 96. */
    const val DefaultContentWidthPx: Int = DefaultPageWidthPx - 2 * DefaultMarginPx

    /** DocumentEditorPage.tsx COL_GAP — gutter between multi-column sections. */
    const val ColumnGapPx: Int = 36

    /** getGeometry() clamps a section to 1..3 columns. */
    const val MaxColumns: Int = 3

    /**
     * Document px → dp/sp on the phone. 1:1 by decision (see the object doc):
     * the type scale is preserved and the *page* is what gets dropped, not the
     * text size. Exposed as a constant so a future "fit page width" mode can
     * change one value instead of every call site.
     */
    const val PhoneTypeScale: Float = 1f

    /** Converts a document px measurement (image width, indent, spacing) to dp. */
    fun px(documentPx: Number): Dp =
        (documentPx.toFloat() * PhoneTypeScale).dp

    /**
     * Horizontal padding of the fluid column. The web reserves 96/794 ≈ 12 % of
     * the sheet per margin; applied literally that would eat ~44 dp of a 360 dp
     * phone, so the phone uses a flat reading gutter instead and only honours
     * the real margins on a wide window.
     */
    val ColumnPaddingCompact = 16.dp
    val ColumnPaddingWide = 24.dp

    /**
     * Upper bound of the fluid column, in dp: the web's own 602 px text column.
     * On a tablet the reflowed column therefore lines up exactly with the
     * desktop measure instead of stretching into an unreadable line length.
     */
    val MaxColumnWidth = DefaultContentWidthPx.dp

    /** Window width from which [ColumnPaddingWide] and centring kick in. */
    val WideWindowBreakpoint = 640.dp

    /** canvas-engine.ts LIST_INDENT — one list nesting level. */
    val ListIndent = 32.dp

    /** Elevation of the sheet over [DocsColors.PageBackdrop] in the read-only preview. */
    val PageElevation = 1.dp
}

// ─────────────────────────────────────────────────────────────────────────────
// Type scale
// ─────────────────────────────────────────────────────────────────────────────

/**
 * The text styles of the document body, transcribed from canvas-engine.ts:
 * `DEFAULT_PT = 11`, `H_SIZE = {1:24, 2:18, 3:14, 4:13, 5:12, 6:11}` (pt),
 * headings bold up to level 4 only, and `LH_RATIO = 1.15` as the line-height
 * multiplier. Sizes are `pt × PtToPx × PhoneTypeScale` expressed in sp, so the
 * reader's font-size preference still scales them.
 *
 * Colour is deliberately left unspecified: document ink is theme-dependent
 * ([docsPageInk]) and inline `color` marks override it per span, so baking a
 * colour in would silently win over the document's own.
 *
 * The family stays the system font: the web ships DM Sans / Arial, but a prior
 * product decision keeps mobile on the platform face.
 */
object DocsType {
    private val Sans = FontFamily.Default

    /** canvas-engine.ts LH_RATIO. */
    const val LineHeightRatio: Float = 1.15f

    /** canvas-engine.ts DEFAULT_PT. */
    const val BodyPt: Float = 11f

    private fun style(pt: Float, weight: FontWeight): TextStyle {
        val size = pt * DocsPage.PtToPx * DocsPage.PhoneTypeScale
        return TextStyle(
            fontFamily = Sans,
            fontSize = size.sp,
            lineHeight = (size * LineHeightRatio).sp,
            fontWeight = weight,
        )
    }

    /** 11 pt → 14.7 sp. */
    val Body: TextStyle = style(BodyPt, FontWeight.Normal)

    /** canvas-engine.ts codeMarks: Courier New 10 pt on a #f8f9fa ground. */
    val Code: TextStyle = style(10f, FontWeight.Normal).copy(fontFamily = FontFamily.Monospace)

    /** ENDNOTE_PT = 9 — foot/endnotes and their separator label. */
    val Note: TextStyle = style(9f, FontWeight.Normal)

    val H1: TextStyle = style(24f, FontWeight.Bold)
    val H2: TextStyle = style(18f, FontWeight.Bold)
    val H3: TextStyle = style(14f, FontWeight.Bold)
    val H4: TextStyle = style(13f, FontWeight.Bold)
    // Levels 5 and 6 are NOT bold: canvas-engine only forces bold for level <= 4.
    val H5: TextStyle = style(12f, FontWeight.Normal)
    val H6: TextStyle = style(11f, FontWeight.Normal)

    /** Style of a ProseMirror `heading` node; anything outside 1..6 falls back to body. */
    fun heading(level: Int): TextStyle = when (level) {
        1 -> H1
        2 -> H2
        3 -> H3
        4 -> H4
        5 -> H5
        6 -> H6
        else -> Body
    }

    /**
     * canvas-engine.ts H_BEFORE — space above a heading, in document px. A
     * paragraph gets 0, a list item 2.
     */
    fun spaceBefore(level: Int): Dp = when (level) {
        1 -> DocsPage.px(20)
        2 -> DocsPage.px(16)
        3 -> DocsPage.px(12)
        4 -> DocsPage.px(10)
        5, 6 -> DocsPage.px(8)
        else -> DocsPage.px(0)
    }

    /** canvas-engine.ts H_AFTER; a plain paragraph and a list item both get 2. */
    fun spaceAfter(level: Int): Dp = when (level) {
        1 -> DocsPage.px(6)
        in 2..6 -> DocsPage.px(4)
        else -> DocsPage.px(2)
    }

    /** Space around a blockquote / horizontal rule block (canvas-engine: 8 px). */
    val BlockSpacing = DocsPage.px(8)
}

// ─────────────────────────────────────────────────────────────────────────────
// Authenticated URLs
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Absolute base of the office API on an account's instance. Always goes through
 * the core proxy (`/api/v1/office`), never the module's own port.
 */
fun officeApiBase(account: SharedAccount): String =
    "${account.serverUrl.trimEnd('/')}/api/v1/office"

/**
 * Turns an image `src` found inside a document into a URL the image loader can
 * fetch for [account].
 *
 * A document carries three shapes of `src` (office/frontend/src/imagePicker.ts
 * resolves a pick to "the chosen URL kept as-is, or a data URL"):
 *   - `data:…`      — inlined by the picker, already self-contained;
 *   - absolute http(s) — an external image, left untouched;
 *   - instance-relative, typically `/api/v1/drive/<file_id>/download` — needs the
 *     account's origin prepended, and an access token, which the call factory
 *     attaches by matching this very prefix.
 *
 * Returns null for an empty or unusable `src` so callers render their placeholder
 * instead of firing a request that cannot succeed.
 */
fun documentImageUrl(account: SharedAccount, src: String?): String? {
    val raw = src?.trim().orEmpty()
    if (raw.isEmpty()) return null
    if (raw.startsWith("data:")) return raw
    if (raw.startsWith("http://") || raw.startsWith("https://")) return raw
    // A "kbshape:" alt regenerates a client-side SVG on the web; there is no such
    // generator here, so it is not a fetchable URL.
    if (raw.startsWith("kbshape:")) return null
    val origin = account.serverUrl.trimEnd('/')
    return if (raw.startsWith("/")) origin + raw else "$origin/$raw"
}

/** Binary export of a document; `format` is "docx" or "odt". */
fun documentExportUrl(account: SharedAccount, documentId: String, format: String): String =
    "${officeApiBase(account)}/documents/$documentId/export/$format"

/**
 * The account owner's avatar, served by the core. Falls through to no image when
 * the user has none, so keep an initial-letter fallback underneath.
 */
fun docsAvatarUrl(account: SharedAccount): String =
    "${account.serverUrl.trimEnd('/')}/api/v1/users/${account.userId}/avatar"

/** Avatar of an arbitrary collaborator/commenter on the same instance. */
fun docsUserAvatarUrl(account: SharedAccount, userId: String): String =
    "${account.serverUrl.trimEnd('/')}/api/v1/users/$userId/avatar"
