// Mobile READING view for presentations — the counterpart of the Documents
// mobile read mode: the editor chrome disappears and the deck becomes a
// full-screen, swipeable slide reader (one slide per page, native CSS scroll
// snapping so the gesture is as smooth as the platform's own pagers).
//
// The renderer itself lives in PresentationEditorPage (it owns the canonical
// SlideRenderer); this view only receives a `paint` callback, which keeps the
// two files free of any circular import.
import { useEffect, useRef, useState, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { EyeOff, StickyNote, X } from 'lucide-react'
import type { Slide, SlideSummary, Presentation } from '../api'

/** Paints a slide into a canvas already sized `w`×`h` (CSS px). */
export type SlidePainter = (
  canvas: HTMLCanvasElement,
  w: number,
  h: number,
  slide: Partial<Slide>,
  mode: 'present' | 'thumbnail',
) => void

const RATIO = 540 / 960

// ── One page of the pager ────────────────────────────────────────────────────
function ReaderPage({ slide, paint, active }: {
  slide: Partial<Slide> | null
  paint: SlidePainter
  active: boolean
}) {
  const boxRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const box = boxRef.current, canvas = canvasRef.current
    if (!box || !canvas || !slide) return
    const draw = () => {
      // Fit inside the page box, honouring both axes (landscape phones are wide
      // but short, so height is the binding constraint there).
      const bw = box.clientWidth, bh = box.clientHeight
      if (bw < 2 || bh < 2) return
      let w = bw, h = Math.round(w * RATIO)
      if (h > bh) { h = bh; w = Math.round(h / RATIO) }
      paint(canvas, w, h, slide, 'present')
    }
    draw()
    const ro = new ResizeObserver(draw)
    ro.observe(box)
    return () => ro.disconnect()
  }, [slide, paint, active])

  return (
    <div ref={boxRef} className="flex-shrink-0 w-full h-full flex items-center justify-center px-2"
      style={{ scrollSnapAlign: 'center', scrollSnapStop: 'always' }}>
      {slide
        ? <canvas ref={canvasRef} className="block rounded shadow-2xl bg-white" />
        : <div className="w-full rounded bg-white/10" style={{ aspectRatio: '16 / 9' }} />}
    </div>
  )
}

export function MobileSlideReader({
  slides, fullSlides, index, onIndexChange, paint,
}: {
  slides: SlideSummary[]
  fullSlides: Record<string, Slide>
  theme: Presentation['theme']
  index: number
  onIndexChange: (i: number) => void
  paint: SlidePainter
}) {
  const { t } = useTranslation('office')
  const scrollRef = useRef<HTMLDivElement>(null)
  const [notesOpen, setNotesOpen] = useState(false)
  // True while WE are scrolling the pager programmatically → the scroll handler
  // must not fight the animation by reporting intermediate pages.
  const settingRef = useRef(false)

  // External index change (topbar chevrons, slide added…) → scroll to that page.
  useEffect(() => {
    const el = scrollRef.current
    if (!el) return
    const target = index * el.clientWidth
    if (Math.abs(el.scrollLeft - target) < 4) return
    settingRef.current = true
    el.scrollTo({ left: target, behavior: 'auto' })
    const tm = setTimeout(() => { settingRef.current = false }, 60)
    return () => clearTimeout(tm)
  }, [index, slides.length])

  const onScroll = useCallback(() => {
    const el = scrollRef.current
    if (!el || settingRef.current || el.clientWidth < 2) return
    const i = Math.round(el.scrollLeft / el.clientWidth)
    if (i !== index && i >= 0 && i < slides.length) onIndexChange(i)
  }, [index, slides.length, onIndexChange])

  const cur = slides[index]
  const notes = (cur && fullSlides[cur.id]?.notes) || ''

  return (
    <div className="relative flex-1 min-h-0 bg-neutral-900">
      <div
        ref={scrollRef}
        onScroll={onScroll}
        className="h-full w-full flex overflow-x-auto overflow-y-hidden"
        style={{ scrollSnapType: 'x mandatory', scrollbarWidth: 'none', overscrollBehaviorX: 'contain' }}
      >
        {slides.map((s, i) => (
          <ReaderPage
            key={s.id}
            // Only the neighbouring slides are painted: a 60-slide deck would
            // otherwise build 60 canvases (and 60 renderers) at once.
            slide={Math.abs(i - index) <= 1 ? (fullSlides[s.id] ?? s) : null}
            paint={paint}
            active={i === index}
          />
        ))}
      </div>

      {/* Page indicator + hidden-slide badge + notes toggle. */}
      <div className="absolute left-0 right-0 flex items-center justify-center gap-2 pointer-events-none"
        style={{ bottom: 'calc(12px + env(safe-area-inset-bottom))' }}>
        <span className="px-2.5 h-7 flex items-center rounded-full bg-black/55 text-white text-xs tabular-nums backdrop-blur-sm">
          {slides.length ? index + 1 : 0} / {slides.length}
        </span>
        {cur?.is_hidden && (
          <span className="px-2 h-7 flex items-center gap-1 rounded-full bg-black/55 text-white/80 text-xs backdrop-blur-sm">
            <EyeOff size={13} /> {t('pres_hidden', { defaultValue: 'Masquée' })}
          </span>
        )}
        {!!notes && (
          <button onClick={() => setNotesOpen(o => !o)}
            className="pointer-events-auto px-2.5 h-7 flex items-center gap-1.5 rounded-full bg-black/55 text-white text-xs backdrop-blur-sm active:bg-black/70"
            title={t('pres_presenter_notes', { defaultValue: 'Notes du présentateur' })}>
            <StickyNote size={13} /> {t('pres_notes_short', { defaultValue: 'Notes' })}
          </button>
        )}
      </div>

      {/* Notes in a bottom sheet (read-only) — the phone has no room for the
          desktop notes pane, and they matter most while rehearsing. */}
      {notesOpen && !!notes && (
        <>
          <div className="absolute inset-0 bg-black/40" onClick={() => setNotesOpen(false)} />
          <div className="absolute left-0 right-0 bottom-0 max-h-[50%] flex flex-col rounded-t-2xl bg-surface-1 shadow-2xl"
            style={{ paddingBottom: 'env(safe-area-inset-bottom)', animation: 'kb-sheet-up .18s ease-out' }}>
            <div className="flex items-center justify-between px-4 pt-3 pb-2">
              <span className="text-xs font-medium text-text-secondary uppercase tracking-wide">
                {t('pres_presenter_notes', { defaultValue: 'Notes du présentateur' })}
              </span>
              <button onClick={() => setNotesOpen(false)} className="w-9 h-9 -mr-2 flex items-center justify-center rounded-full text-text-secondary active:bg-surface-2">
                <X size={18} />
              </button>
            </div>
            <div className="px-4 pb-4 overflow-y-auto text-xs text-text-primary whitespace-pre-wrap">{notes}</div>
          </div>
        </>
      )}
    </div>
  )
}
