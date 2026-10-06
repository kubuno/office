// Colored ribbon icons, styled after Microsoft Word's flat multi-color icon set.
//
// lucide-react gives us clean monochrome glyphs, but Word draws its content-
// insertion commands (table, picture, shapes, chart, symbol, date…) in the
// Office palette so the eye finds them instantly. These inline SVGs reproduce
// that language: a restrained set of Office hues over a neutral paper base.
//
// Each icon is a self-contained component taking a `size` (px). The viewBox is
// always 24x24 so glyphs stay visually aligned whatever the requested size.
// Colors are literal (Word icons keep their hue in both light and dark themes);
// only the faint "paper" and hairline strokes lean on neutral greys that read
// on either background.
import type { CSSProperties } from 'react'

// Office palette — kept in one place so the family stays coherent.
const BLUE = '#2b579a' // Word blue (primary)
const BLUE_LT = '#41a5ee' // Office light blue (accents, sky)
const GOLD = '#ffb900' // amber (sun, highlight, stars)
const GOLD_LT = '#ffd23f'
const GREEN = '#107c41' // Office green (hills, checks)
const RED = '#d83b01' // Office orange-red
const PURPLE = '#8764b8' // Office purple (comments)
const PAPER = '#ffffff'
const INK = '#c8c6c4' // hairline on paper
const GREY = '#605e5c' // neutral glyph grey

type IconProps = { size?: number; style?: CSSProperties; className?: string }

function svg(size: number | undefined, children: React.ReactNode, extra?: IconProps) {
  const s = size ?? 22
  return (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none"
      xmlns="http://www.w3.org/2000/svg" style={extra?.style} className={extra?.className}
      aria-hidden focusable={false}>
      {children}
    </svg>
  )
}

// A sheet of paper — the recurring base of many Word icons.
function Sheet({ x = 5, y = 3, w = 14, h = 18, fold = 5 }: { x?: number; y?: number; w?: number; h?: number; fold?: number }) {
  const rx = x + w
  return (
    <>
      <path d={`M${x} ${y} h${w - fold} l${fold} ${fold} v${h - fold} h${-w} Z`} fill={PAPER} stroke={INK} strokeWidth="1" strokeLinejoin="round" />
      <path d={`M${rx - fold} ${y} v${fold} h${fold}`} fill="none" stroke={INK} strokeWidth="1" strokeLinejoin="round" />
    </>
  )
}

// ── Insert · Pages ──────────────────────────────────────────────────────────
export function CoverPageIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="5" y="3" width="14" height="18" rx="1.5" fill={BLUE} />
    <rect x="7" y="5.5" width="10" height="4" rx="0.7" fill={PAPER} opacity="0.9" />
    <rect x="7" y="12" width="10" height="1.4" rx="0.7" fill={PAPER} opacity="0.55" />
    <rect x="7" y="15" width="10" height="1.4" rx="0.7" fill={PAPER} opacity="0.55" />
    <rect x="7" y="18" width="6" height="1.4" rx="0.7" fill={PAPER} opacity="0.55" />
  </>, p)
}

export function PageBreakIcon(p: IconProps) {
  return svg(p.size, <>
    <Sheet />
    <path d="M4 12 h16" stroke={RED} strokeWidth="1.6" strokeDasharray="2.4 1.8" strokeLinecap="round" />
    <path d="M15.5 9.5 L18 12 L15.5 14.5" fill="none" stroke={RED} strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
  </>, p)
}

// ── Insert · Tables ─────────────────────────────────────────────────────────
export function TableIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="3.5" y="4.5" width="17" height="15" rx="1.4" fill={PAPER} stroke={BLUE} strokeWidth="1.3" />
    <rect x="3.5" y="4.5" width="17" height="4" rx="1.4" fill={BLUE} />
    <rect x="3.5" y="7.5" width="17" height="1" fill={BLUE} opacity="0.9" />
    <path d="M9 8.5 V19.5 M15 8.5 V19.5 M3.5 13 H20.5" stroke={BLUE_LT} strokeWidth="1.1" />
  </>, p)
}

// ── Insert · Illustrations ──────────────────────────────────────────────────
export function ImageIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="3" y="4.5" width="18" height="15" rx="1.6" fill={PAPER} stroke={INK} strokeWidth="1.1" />
    <circle cx="8" cy="9" r="1.9" fill={GOLD} />
    <path d="M4 18 L9.5 12 L13 15.5 L16 12.5 L20 18 Z" fill={GREEN} />
    <path d="M12.5 18 L16 12.5 L20 18 Z" fill={BLUE_LT} />
  </>, p)
}

export function ShapesIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="3.5" y="12.5" width="8" height="8" rx="1.2" fill={BLUE_LT} />
    <circle cx="16.5" cy="7.5" r="4" fill={RED} />
    <path d="M13 12.5 L20.5 12.5 L16.75 20 Z" fill={GOLD} />
  </>, p)
}

export function TextBoxIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="3.5" y="6" width="17" height="12" rx="1.2" fill={PAPER} stroke={BLUE} strokeWidth="1.3" strokeDasharray="2.6 1.8" />
    <path d="M8 10 H16 M10 10 V15" stroke={GREY} strokeWidth="1.5" strokeLinecap="round" />
  </>, p)
}

export function ChartIcon(p: IconProps) {
  return svg(p.size, <>
    <path d="M4 20 V4" stroke={GREY} strokeWidth="1.3" strokeLinecap="round" />
    <path d="M4 20 H21" stroke={GREY} strokeWidth="1.3" strokeLinecap="round" />
    <rect x="6.5" y="12" width="3" height="6.5" rx="0.6" fill={BLUE_LT} />
    <rect x="11" y="8" width="3" height="10.5" rx="0.6" fill={GREEN} />
    <rect x="15.5" y="5" width="3" height="13.5" rx="0.6" fill={GOLD} />
  </>, p)
}

export function SmartArtIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="9" y="3.5" width="6" height="5" rx="1" fill={BLUE} />
    <rect x="3" y="15" width="6" height="5" rx="1" fill={GREEN} />
    <rect x="15" y="15" width="6" height="5" rx="1" fill={GOLD} />
    <path d="M12 8.5 V11.5 M12 11.5 H6 V15 M12 11.5 H18 V15" stroke={GREY} strokeWidth="1.2" fill="none" />
  </>, p)
}

// ── Insert · Header/Footer ──────────────────────────────────────────────────
export function HeaderIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="4" y="3.5" width="16" height="17" rx="1.4" fill={PAPER} stroke={INK} strokeWidth="1.1" />
    <rect x="4" y="3.5" width="16" height="4.5" rx="1.4" fill={BLUE} />
    <path d="M7 12 H17 M7 15 H17 M7 18 H13" stroke={INK} strokeWidth="1.2" strokeLinecap="round" />
  </>, p)
}

export function FooterIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="4" y="3.5" width="16" height="17" rx="1.4" fill={PAPER} stroke={INK} strokeWidth="1.1" />
    <rect x="4" y="16" width="16" height="4.5" rx="1.4" fill={BLUE} />
    <path d="M7 6 H17 M7 9 H17 M7 12 H13" stroke={INK} strokeWidth="1.2" strokeLinecap="round" />
  </>, p)
}

export function PageNumberIcon(p: IconProps) {
  return svg(p.size, <>
    <Sheet />
    <path d="M8 12 H16 M8 15 H16" stroke={INK} strokeWidth="1.1" strokeLinecap="round" />
    <rect x="13.5" y="16.5" width="5" height="5" rx="1" fill={BLUE} />
    <text x="16" y="20.4" fontSize="4.6" fill={PAPER} textAnchor="middle" fontFamily="sans-serif" fontWeight="700">1</text>
  </>, p)
}

// ── Insert · Text ───────────────────────────────────────────────────────────
export function TocIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="4" y="4" width="16" height="16" rx="1.6" fill={PAPER} stroke={INK} strokeWidth="1.1" />
    <circle cx="7.5" cy="8" r="1" fill={BLUE} />
    <circle cx="7.5" cy="12" r="1" fill={GREEN} />
    <circle cx="7.5" cy="16" r="1" fill={GOLD} />
    <path d="M10 8 H17 M10 12 H17 M10 16 H14.5" stroke={GREY} strokeWidth="1.3" strokeLinecap="round" />
  </>, p)
}

export function SymbolIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="3.5" y="3.5" width="17" height="17" rx="2" fill={BLUE} />
    <text x="12" y="17.5" fontSize="14" fill={PAPER} textAnchor="middle" fontFamily="serif" fontWeight="600">Ω</text>
  </>, p)
}

export function EquationIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="3.5" y="3.5" width="17" height="17" rx="2" fill={PAPER} stroke={BLUE} strokeWidth="1.3" />
    <text x="12" y="16.5" fontSize="12" fill={BLUE} textAnchor="middle" fontFamily="serif" fontStyle="italic" fontWeight="600">π</text>
  </>, p)
}

export function DateTimeIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="3.5" y="5" width="17" height="15" rx="1.8" fill={PAPER} stroke={INK} strokeWidth="1.1" />
    <rect x="3.5" y="5" width="17" height="4" rx="1.8" fill={RED} />
    <path d="M7.5 3.5 V6.5 M16.5 3.5 V6.5" stroke={RED} strokeWidth="1.6" strokeLinecap="round" />
    <circle cx="12" cy="14.5" r="3.6" fill={PAPER} stroke={BLUE} strokeWidth="1.2" />
    <path d="M12 12.7 V14.5 L13.4 15.4" stroke={BLUE} strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
  </>, p)
}

export function CaptionIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="3.5" y="4" width="17" height="10" rx="1.4" fill={PAPER} stroke={INK} strokeWidth="1.1" />
    <path d="M6 7 L9 10.5 L12 8 L18 13.5 H6 Z" fill={GREEN} />
    <circle cx="8" cy="7" r="1.2" fill={GOLD} />
    <rect x="4.5" y="16.5" width="15" height="3.6" rx="1" fill={BLUE} />
    <path d="M6.5 18.3 H17.5" stroke={PAPER} strokeWidth="1.1" strokeLinecap="round" opacity="0.85" />
  </>, p)
}

export function FootnoteIcon(p: IconProps) {
  return svg(p.size, <>
    <Sheet />
    <path d="M8 8 H16 M8 11 H16" stroke={INK} strokeWidth="1.2" strokeLinecap="round" />
    <path d="M7.5 14.5 H16.5" stroke={INK} strokeWidth="1" />
    <text x="8" y="19.6" fontSize="5" fill={BLUE} fontFamily="serif" fontWeight="700">1</text>
    <path d="M11 17.5 H16" stroke={INK} strokeWidth="1" strokeLinecap="round" />
  </>, p)
}

export function EndnoteIcon(p: IconProps) {
  return svg(p.size, <>
    <Sheet />
    <path d="M8 8 H16 M8 11 H16 M8 14 H16" stroke={INK} strokeWidth="1.2" strokeLinecap="round" />
    <text x="8" y="19.8" fontSize="5" fill={GREEN} fontFamily="serif" fontWeight="700">i</text>
    <path d="M11 17.8 H16" stroke={INK} strokeWidth="1" strokeLinecap="round" />
  </>, p)
}

export function LinkIcon(p: IconProps) {
  return svg(p.size, <>
    <path d="M10 14 a3 3 0 0 1 0-4 l2.5-2.5 a3 3 0 0 1 4.2 4.2 L15 13.5" fill="none" stroke={BLUE} strokeWidth="1.7" strokeLinecap="round" />
    <path d="M14 10 a3 3 0 0 1 0 4 l-2.5 2.5 a3 3 0 0 1-4.2-4.2 L9 10.5" fill="none" stroke={BLUE_LT} strokeWidth="1.7" strokeLinecap="round" />
  </>, p)
}

export function BookmarkIcon(p: IconProps) {
  return svg(p.size, <>
    <path d="M7 3.5 h10 v17 l-5-3.5 l-5 3.5 Z" fill={RED} stroke={RED} strokeWidth="1" strokeLinejoin="round" />
    <path d="M9.5 8 H14.5" stroke={PAPER} strokeWidth="1.3" strokeLinecap="round" opacity="0.8" />
  </>, p)
}

export function CommentIcon(p: IconProps) {
  return svg(p.size, <>
    <path d="M4 5.5 h16 v10 h-9 l-4 4 v-4 h-3 Z" fill={PURPLE} />
    <path d="M8 9.5 H16 M8 12.5 H13" stroke={PAPER} strokeWidth="1.3" strokeLinecap="round" opacity="0.9" />
  </>, p)
}

export function HorizontalRuleIcon(p: IconProps) {
  return svg(p.size, <>
    <path d="M5 7 H19 M5 17 H19" stroke={INK} strokeWidth="1.2" strokeLinecap="round" />
    <path d="M4 12 H20" stroke={BLUE} strokeWidth="2" strokeLinecap="round" />
  </>, p)
}

export function QuoteIcon(p: IconProps) {
  return svg(p.size, <>
    <path d="M5 6 H19" stroke={INK} strokeWidth="1.2" strokeLinecap="round" />
    <rect x="5" y="9" width="2.4" height="9" rx="1" fill={GREEN} />
    <path d="M10 10.5 H19 M10 13.5 H19 M10 16.5 H15" stroke={INK} strokeWidth="1.2" strokeLinecap="round" />
  </>, p)
}

export function CodeBlockIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="3.5" y="5" width="17" height="14" rx="1.6" fill={PAPER} stroke={INK} strokeWidth="1.1" />
    <rect x="3.5" y="5" width="17" height="3" rx="1.6" fill={GREY} />
    <path d="M9 12 L7 14 L9 16 M15 12 L17 14 L15 16 M13 11.5 L11 16.5" fill="none" stroke={BLUE} strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
  </>, p)
}

// ── Home · Clipboard ────────────────────────────────────────────────────────
export function PasteIcon(p: IconProps) {
  return svg(p.size, <>
    <rect x="5" y="4" width="12" height="15" rx="1.4" fill={GOLD} />
    <rect x="8.5" y="2.6" width="5" height="2.8" rx="0.8" fill={GREY} />
    <rect x="9" y="9" width="11" height="12" rx="1.2" fill={PAPER} stroke={BLUE} strokeWidth="1.2" />
    <path d="M11.5 12.5 H17.5 M11.5 15 H17.5 M11.5 17.5 H15" stroke={BLUE_LT} strokeWidth="1.2" strokeLinecap="round" />
  </>, p)
}

export function FormatPainterIcon(p: IconProps) {
  return svg(p.size, <>
    <path d="M4 6.5 h9 v3 a1 1 0 0 1-1 1 h-7 a1 1 0 0 1-1-1 Z" fill={BLUE} />
    <rect x="7" y="4.5" width="3" height="2.5" fill={GREY} />
    <path d="M12 9 h3 v3.5 h-2.5" fill="none" stroke={GOLD} strokeWidth="1.5" strokeLinejoin="round" />
    <path d="M11 13 h4 v6 a2 2 0 0 1-2 2 h0 a2 2 0 0 1-2-2 Z" fill={GOLD} />
    <rect x="12.2" y="15" width="1.6" height="3.5" rx="0.8" fill={GOLD_LT} />
  </>, p)
}

// ── Home · Paragraph (lists) ────────────────────────────────────────────────
export function BulletListIcon(p: IconProps) {
  return svg(p.size, <>
    <circle cx="5" cy="7" r="1.5" fill={BLUE} />
    <circle cx="5" cy="12" r="1.5" fill={BLUE} />
    <circle cx="5" cy="17" r="1.5" fill={BLUE} />
    <path d="M9 7 H20 M9 12 H20 M9 17 H20" stroke={GREY} strokeWidth="1.6" strokeLinecap="round" />
  </>, p)
}

export function NumberListIcon(p: IconProps) {
  return svg(p.size, <>
    <text x="3" y="8.7" fontSize="6" fill={BLUE} fontFamily="sans-serif" fontWeight="700">1</text>
    <text x="3" y="14" fontSize="6" fill={BLUE} fontFamily="sans-serif" fontWeight="700">2</text>
    <text x="3" y="19.3" fontSize="6" fill={BLUE} fontFamily="sans-serif" fontWeight="700">3</text>
    <path d="M9 7 H20 M9 12 H20 M9 17 H20" stroke={GREY} strokeWidth="1.6" strokeLinecap="round" />
  </>, p)
}

// Named export map used by a small on-canvas gallery / documentation preview.
export const RIBBON_ICONS = {
  CoverPageIcon, PageBreakIcon, TableIcon, ImageIcon, ShapesIcon, TextBoxIcon,
  ChartIcon, SmartArtIcon, HeaderIcon, FooterIcon, PageNumberIcon, TocIcon,
  SymbolIcon, EquationIcon, DateTimeIcon, CaptionIcon, FootnoteIcon, EndnoteIcon,
  LinkIcon, BookmarkIcon, CommentIcon, HorizontalRuleIcon, QuoteIcon, CodeBlockIcon,
  PasteIcon, FormatPainterIcon, BulletListIcon, NumberListIcon,
} as const
