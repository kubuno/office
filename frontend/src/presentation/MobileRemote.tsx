// Télécommande de diaporama (mobile) — le téléphone devient la « vue
// présentateur » : diapo courante, diapo suivante, notes, minuteur et gros
// boutons ◀ ▶, pendant que le diaporama se déroule sur l'écran piloté.
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ChevronLeft, ChevronRight, Monitor, MousePointer2, Square, X } from 'lucide-react'
import type { Slide, SlideSummary, Presentation } from '../api'
import type { SlidePainter } from './MobileSlideReader'
import type { ShowState } from './remote'

function Preview({ slide, paint, label, laser, onLaser }: {
  slide: Partial<Slide> | null
  paint: SlidePainter
  label: string
  /** Mode laser actif : l'aperçu devient un pavé tactile de visée. */
  laser?: boolean
  onLaser?: (p: { x: number; y: number } | null) => void
}) {
  const boxRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const [dot, setDot] = useState<{ x: number; y: number } | null>(null)
  useEffect(() => {
    const box = boxRef.current, canvas = canvasRef.current
    if (!box || !canvas) return
    const draw = () => {
      const w = box.clientWidth
      if (w < 2) return
      if (slide) paint(canvas, w, Math.round(w * 540 / 960), slide, 'present')
      else { const ctx = canvas.getContext('2d'); if (ctx) ctx.clearRect(0, 0, canvas.width, canvas.height) }
    }
    draw()
    const ro = new ResizeObserver(draw)
    ro.observe(box)
    return () => ro.disconnect()
  }, [slide, paint])
  // Visée : le doigt sur l'aperçu donne la position du laser en FRACTION de diapo
  // (l'écran distant la reconvertit dans sa propre géométrie).
  const aim = (e: React.PointerEvent) => {
    const box = boxRef.current
    if (!box) return
    const r = box.getBoundingClientRect()
    const p = {
      x: Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)),
      y: Math.max(0, Math.min(1, (e.clientY - r.top) / r.height)),
    }
    setDot(p)
    onLaser?.(p)
  }
  const stopAim = () => { setDot(null); onLaser?.(null) }

  return (
    <div className="min-w-0 flex-1">
      <div className="text-[11px] uppercase tracking-wide text-white/50 mb-1">{label}</div>
      <div ref={boxRef}
        className={`relative w-full rounded-lg overflow-hidden bg-white/10 ${laser ? 'ring-2 ring-red-500' : ''}`}
        style={{ aspectRatio: '16 / 9', touchAction: laser ? 'none' : undefined }}
        onPointerDown={laser ? e => { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); aim(e) } : undefined}
        onPointerMove={laser ? e => { if (e.buttons || e.pointerType === 'touch') aim(e) } : undefined}
        onPointerUp={laser ? stopAim : undefined}
        onPointerCancel={laser ? stopAim : undefined}>
        <canvas ref={canvasRef} className="block w-full" />
        {laser && dot && (
          <span className="absolute pointer-events-none rounded-full"
            style={{
              left: `${dot.x * 100}%`, top: `${dot.y * 100}%`, width: 14, height: 14,
              transform: 'translate(-50%, -50%)',
              background: 'radial-gradient(circle, #ff4d4d 0%, #ff0000 55%, rgba(255,0,0,0) 72%)',
              boxShadow: '0 0 10px 3px rgba(255,0,0,0.6)',
            }} />
        )}
      </div>
    </div>
  )
}

export function MobileRemote({
  slides, fullSlides, show, screenName, paint, onPrev, onNext, onBlack, onStop, onLaser,
}: {
  slides: SlideSummary[]
  fullSlides: Record<string, Slide>
  theme: Presentation['theme']
  /** État publié par l'écran piloté (null tant qu'il n'a pas répondu). */
  show: ShowState | null
  screenName: string
  paint: SlidePainter
  onPrev: () => void
  onNext: () => void
  onBlack: () => void
  onStop: () => void
  /** Position du laser (fraction de diapo) ou `null` pour l'éteindre. */
  onLaser: (p: { x: number; y: number } | null) => void
}) {
  const { t } = useTranslation('office')
  const [laserOn, setLaserOn] = useState(false)
  const [elapsed, setElapsed] = useState(0)
  useEffect(() => { const iv = setInterval(() => setElapsed(e => e + 1), 1000); return () => clearInterval(iv) }, [])
  const mmss = `${String(Math.floor(elapsed / 60)).padStart(2, '0')}:${String(elapsed % 60).padStart(2, '0')}`

  // Les diapos MASQUÉES ne passent pas au diaporama : l'index publié par l'écran
  // porte sur cette même liste visible.
  const visible = slides.filter(s => !s.is_hidden)
  const idx = show?.current ?? 0
  const cur = visible[idx]
  const nxt = visible[idx + 1]
  const notes = (cur && fullSlides[cur.id]?.notes) || ''

  const Btn = ({ onClick, disabled, children, label }: {
    onClick: () => void; disabled?: boolean; children: React.ReactNode; label: string
  }) => (
    <button onClick={onClick} disabled={disabled} aria-label={label} title={label}
      className="flex-1 h-16 rounded-2xl bg-white/10 text-white flex items-center justify-center
                 active:bg-white/20 disabled:opacity-30 touch-manipulation">
      {children}
    </button>
  )

  return (
    <div className="flex-1 min-h-0 flex flex-col bg-neutral-900 text-white overflow-y-auto"
      style={{ paddingBottom: 'env(safe-area-inset-bottom)' }}>
      {/* Écran piloté + minuteur */}
      <div className="flex items-center gap-2 px-4 pt-3 pb-2 text-xs text-white/70">
        <Monitor size={14} className="flex-shrink-0" />
        <span className="truncate flex-1">{screenName}</span>
        <span className="tabular-nums">{mmss}</span>
        <span className="tabular-nums text-white/50">{visible.length ? idx + 1 : 0} / {visible.length}</span>
      </div>

      <div className="flex gap-3 px-4">
        <Preview slide={cur ? (fullSlides[cur.id] ?? cur) : null} paint={paint}
          label={laserOn
            ? t('pres_remote_aim', { defaultValue: 'Visez avec le doigt' })
            : t('pres_remote_current', { defaultValue: 'Diapo affichée' })}
          laser={laserOn} onLaser={onLaser} />
        <Preview slide={nxt ? (fullSlides[nxt.id] ?? nxt) : null} paint={paint}
          label={t('pres_remote_next', { defaultValue: 'Suivante' })} />
      </div>

      {/* Notes du présentateur — la raison d'être d'une télécommande. */}
      <div className="mx-4 mt-3 mb-2 flex-1 min-h-[4rem] rounded-lg bg-white/5 p-3 text-xs leading-relaxed whitespace-pre-wrap text-white/80">
        {notes || <span className="text-white/35">{t('pres_notes_placeholder', { defaultValue: 'Ajoutez vos notes ici…' })}</span>}
      </div>

      {/* Commandes : cibles larges, utilisables sans regarder le téléphone.
          Marge à droite : le FAB du lanceur d'apps flotte dans ce coin. */}
      <div className="flex items-stretch gap-2 pl-4 pr-[72px] pb-3">
        <Btn onClick={onPrev} disabled={idx <= 0 && !(show?.step ?? 0)} label={t('pres_prev', { defaultValue: 'Précédent' })}>
          <ChevronLeft size={30} />
        </Btn>
        <Btn onClick={onNext} label={t('pres_next', { defaultValue: 'Suivant' })}>
          <ChevronRight size={30} />
        </Btn>
      </div>
      <div className="flex items-stretch gap-2 px-4 pb-4">
        <button onClick={() => { const on = !laserOn; setLaserOn(on); if (!on) onLaser(null) }}
          className={`flex-1 h-11 rounded-xl text-xs flex items-center justify-center gap-2 touch-manipulation
            ${laserOn ? 'bg-red-600 text-white' : 'bg-white/10 text-white active:bg-white/20'}`}>
          <MousePointer2 size={15} /> {t('pres_remote_laser', { defaultValue: 'Laser' })}
        </button>
        <button onClick={onBlack}
          className="flex-1 h-11 rounded-xl bg-white/10 text-white text-xs flex items-center justify-center gap-2 active:bg-white/20 touch-manipulation">
          <Square size={15} /> {t('pres_remote_black', { defaultValue: 'Écran noir' })}
        </button>
        <button onClick={onStop}
          className="flex-1 h-11 rounded-xl bg-danger/80 text-white text-xs flex items-center justify-center gap-2 active:bg-danger touch-manipulation">
          <X size={15} /> {t('pres_remote_stop', { defaultValue: 'Terminer' })}
        </button>
      </div>
    </div>
  )
}
