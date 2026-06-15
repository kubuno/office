import { useState, useEffect, useRef, useCallback, useLayoutEffect, useMemo } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { Awareness } from 'y-protocols/awareness'
import { useCollab } from './collab/collabProvider'
import { userColor, PresenceAvatars } from './collab/presence'
import { useAuthStore } from '@kubuno/sdk'
import { useTranslation } from 'react-i18next'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  Plus, Trash2, Star,
  Hand, MousePointer2, Square, Type, Minus, ZoomIn, ZoomOut,
  Maximize2, StickyNote, Pen, Eraser, ArrowRight, RotateCcw,
  RotateCw, Share2, ExternalLink, Copy, Circle, Diamond, Triangle, ChevronRight,
} from 'lucide-react'
import clsx from 'clsx'
import { format } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import { Button, Input, Textarea, Dropdown } from '@ui'
import type { StartPageRecentItem } from '@ui'
import { ModuleStartPage } from '@kubuno/drive'
import type { FileItem } from '@kubuno/drive'
import * as Y from 'yjs'

import { boardsApi } from './whiteboard-api'
import { officeApi } from './api'
import CollaboratorsDialog from './CollaboratorsDialog'
import { OfficeShell } from './shell/OfficeShell'
import { THEME_WHITEBOARD } from './ribbon/officeThemes'
import { fileGroup } from './ribbon/common'
import type {
  WbElement, StickyNote as StickyNoteEl, ShapeElement, TextBox,
  ArrowElement, FrameElement, Stroke, ToolType, Background,
} from './whiteboard-types'
import { STICKY_COLORS, STICKY_COLOR_KEYS } from './whiteboard-types'
import type { ShapeKind } from './whiteboard-types'
import {
  Viewport,
  renderStickyNote, renderTextBox, renderShape, renderArrow,
  renderFrame, renderStroke, renderSelectionHandles, renderBackground,
  hitTest, hitHandle, handleCursor,
} from './whiteboard-engine'
import type { ResizeHandle } from './whiteboard-engine'
import { getStroke } from 'perfect-freehand'

// ── Identifiant unique pour cet onglet ────────────────────────────────────────

const CLIENT_ID = `${Date.now()}-${Math.random().toString(36).slice(2)}`

// ── Main App ──────────────────────────────────────────────────────────────────

export default function WhiteboardApp() {
  const { id: routeId } = useParams<{ id: string }>()
  const navigate = useNavigate()
  // L'URL est la source de vérité (comme les autres modules Office) : ouvrir un
  // board pousse /office/whiteboard/:id → rechargeable / partageable par URL.
  const open = useCallback((id: string) => navigate(`/office/whiteboard/${id}`), [navigate])

  if (routeId) {
    return <WhiteboardEditor boardId={routeId} onBack={() => navigate('/office/whiteboard')} onOpen={open} />
  }
  return <BoardDashboard onOpen={open} />
}

// ── Dashboard ─────────────────────────────────────────────────────────────────

function BoardDashboard({ onOpen }: { onOpen: (id: string) => void }) {
  const { t, i18n } = useTranslation('office')
  const qc = useQueryClient()

  const { data } = useQuery({
    queryKey: ['wb-boards', 'all', ''],
    queryFn: () => boardsApi.list({}),
  })

  const createMut = useMutation({
    mutationFn: () => boardsApi.create({ title: t('wb_new_board') }),
    onSuccess: (d) => { qc.invalidateQueries({ queryKey: ['wb-boards'] }); onOpen(d.board.id) },
  })
  const trashMut  = useMutation({ mutationFn: (id: string) => boardsApi.trash(id),   onSuccess: () => qc.invalidateQueries({ queryKey: ['wb-boards'] }) })
  const dupMut    = useMutation({ mutationFn: (id: string) => boardsApi.duplicate(id), onSuccess: (d) => { qc.invalidateQueries({ queryKey: ['wb-boards'] }); onOpen(d.board.id) } })

  const boards = data?.boards ?? []

  // Ouverture d'un fichier .kbwbd depuis le navigateur → éditeur.
  const handleOpenFile = (file: FileItem): boolean => {
    boardsApi.openByFile(file.id).then(({ board }) => onOpen(board.id)).catch(() => {})
    return true
  }

  const recentItems: StartPageRecentItem[] = boards.slice(0, 12).map(b => ({
    id:       b.id,
    name:     b.title,
    subtitle: format(new Date(b.updated_at), 'd MMM', { locale: getDateLocale(i18n.language) }),
    icon:     <StickyNote size={18} className="text-text-tertiary" strokeWidth={1.5} />,
    onClick:  () => onOpen(b.id),
    actions: [
      { id: 'open',  label: t('common_open', { defaultValue: 'Ouvrir' }), icon: <ExternalLink size={15} />, onClick: () => onOpen(b.id) },
      { id: 'dup',   label: t('common_duplicate'),                        icon: <Copy size={15} />,         onClick: () => dupMut.mutate(b.id) },
      { id: 'trash', label: t('wb_move_to_trash', { defaultValue: 'Mettre à la corbeille' }), icon: <Trash2 size={15} />, danger: true, onClick: () => trashMut.mutate(b.id) },
    ],
  }))

  return (
    <ModuleStartPage
      recentTitle={t('wb_recent', { defaultValue: 'Récents' })}
      recentItems={recentItems}
      recentEmpty={
        <div className="flex flex-col items-center gap-2">
          <StickyNote size={32} className="text-text-tertiary opacity-30" strokeWidth={1.5} />
          <p className="text-text-tertiary text-xs">{t('wb_create_first_board')}</p>
        </div>
      }
      browse={{
        folderPathPrefix: 'Office/Whiteboards',
        title: t('wb_whiteboards'),
        fileTypeModuleId: 'office-whiteboard',
        onOpenFile: handleOpenFile,
        toolbarContent: (
          <Button icon={<Plus size={15} />} onClick={() => createMut.mutate()} loading={createMut.isPending}>
            {t('wb_new_board')}
          </Button>
        ),
      }}
    />
  )
}

// ── Editor ────────────────────────────────────────────────────────────────────

function WhiteboardEditor({ boardId, onBack, onOpen }: { boardId: string; onBack: () => void; onOpen: (id: string) => void }) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()
  const { data: boardData } = useQuery({ queryKey: ['wb-board', boardId], queryFn: () => boardsApi.get(boardId) })
  const board = boardData?.board

  // ── Titre éditable (standard WorkspaceShell) — synchronisé depuis le board ────
  const [titleDraft, setTitleDraft] = useState('')
  useEffect(() => { if (board?.title != null) setTitleDraft(board.title) }, [board?.title])
  const renameMut = useMutation({
    mutationFn: (title: string) => boardsApi.update(boardId, { title }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['wb-board', boardId] }); qc.invalidateQueries({ queryKey: ['wb-boards'] }) },
  })
  const starBoardMut = useMutation({
    mutationFn: (is_starred: boolean) => boardsApi.update(boardId, { is_starred }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['wb-board', boardId] }); qc.invalidateQueries({ queryKey: ['wb-boards'] }) },
  })
  const trashBoardMut = useMutation({
    mutationFn: () => boardsApi.trash(boardId),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['wb-boards'] }); onBack() },
  })
  const createBoardMut = useMutation({
    mutationFn: () => boardsApi.create({ title: t('wb_new_board') }),
    onSuccess: (d) => { qc.invalidateQueries({ queryKey: ['wb-boards'] }); onOpen(d.board.id) },
  })
  const dupBoardMut = useMutation({
    mutationFn: () => boardsApi.duplicate(boardId),
    onSuccess: (d) => { qc.invalidateQueries({ queryKey: ['wb-boards'] }); onOpen(d.board.id) },
  })
  const commitTitle = () => {
    const v = titleDraft.trim()
    if (v && v !== board?.title) renameMut.mutate(v)
    else if (!v && board?.title) setTitleDraft(board.title)
  }

  // Yjs : un doc + une awareness PAR board (recréés au changement de board).
  const doc = useMemo(() => new Y.Doc(), [boardId])
  const docRef = useRef(doc)
  docRef.current = doc
  const awareness = useMemo(() => new Awareness(doc), [doc])
  useEffect(() => () => awareness.destroy(), [awareness])
  const authUser = useAuthStore(s => s.user)
  const lastCursorRef = useRef(0)
  // Position animée (lissée) des curseurs distants : on comble les sauts entre 2
  // positions reçues par une interpolation par frame → mouvement naturel.
  const remoteCursorAnimRef = useRef<Map<number, { x: number; y: number }>>(new Map())

  // Canvas
  const canvasRef   = useRef<HTMLCanvasElement>(null)
  const viewportRef = useRef(new Viewport())
  const rafRef      = useRef<number>(0)

  // State local (UI uniquement)
  const [tool,      setTool]      = useState<ToolType>('select')
  const [shapeKind, setShapeKind] = useState<ShapeKind>('rect')
  const [shapeMenuOpen, setShapeMenuOpen] = useState(false)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  // Édition de texte en place (double-clic) — façon presentations : textarea
  // superposée à l'élément, stylée comme le texte rendu. editText = source synchrone.
  const [editingId, setEditingId] = useState<string | null>(null)
  const [editText,  setEditText]  = useState('')
  useEffect(() => { setEditingId(null) }, [boardId])
  const [zoom,       setZoom]      = useState(100)
  const [background, setBackground] = useState<Background>('dots')
  const [showProps,  setShowProps]  = useState(false)
  const [_editingTitleId, _setEditingTitleId] = useState<string | null>(null)
  const [stickyColor, _setStickyColor] = useState('yellow')
  const [elements, setElements] = useState<WbElement[]>([])
  const [strokes,  setStrokes]  = useState<Stroke[]>([])
  const [undoStack, setUndoStack] = useState<WbElement[][]>([[]])
  const [redoStack, setRedoStack] = useState<WbElement[][]>([])

  // Dessin en cours
  const penRef = useRef<{ points: number[]; id: string } | null>(null)
  const dragRef = useRef<{ startX: number; startY: number; origX: number; origY: number; id: string } | null>(null)
  const panRef  = useRef<{ startX: number; startY: number } | null>(null)
  const newShapeRef = useRef<{ startX: number; startY: number; curX: number; curY: number } | null>(null)
  const resizeRef = useRef<{ id: string; handle: ResizeHandle; startX: number; startY: number; orig: { x: number; y: number; width: number; height: number } } | null>(null)
  const [hoverCursor, setHoverCursor] = useState<string | null>(null)
  const [shareOpen, setShareOpen] = useState(false)

  // ── Yjs setup ──────────────────────────────────────────────────────────────

  // Observateurs : Y.Map/Y.Array → state local de rendu.
  useEffect(() => {
    const yElements = doc.getMap<WbElement>('elements')
    const yStrokes  = doc.getArray<Stroke>('strokes')
    const sync = () => {
      setElements(Array.from(yElements.values()).sort((a, b) => ('zIndex' in a ? a.zIndex : 0) - ('zIndex' in b ? b.zIndex : 0)))
      setStrokes(yStrokes.toArray())
    }
    yElements.observe(sync)
    yStrokes.observe(sync)
    sync()
    return () => { yElements.unobserve(sync); yStrokes.unobserve(sync) }
  }, [doc])

  // Identité de présence (curseurs/avatars).
  useEffect(() => {
    if (!authUser) return
    awareness.setLocalStateField('user', {
      id:     authUser.id,
      name:   authUser.display_name || authUser.username || authUser.email,
      color:  userColor(authUser.id),
      avatar: authUser.avatar_url,
    })
  }, [awareness, authUser])

  // Diffuser l'objet sélectionné (présence par élément).
  useEffect(() => {
    awareness.setLocalStateField('sel', { id: selectedId })
  }, [awareness, selectedId])

  // Collaboration temps réel : provider générique (curseurs/awareness/reconnexion)
  // pointé sur la route WS du whiteboard, qui conserve la persistance .kbwbd.
  useCollab(`office-whiteboard:${boardId}`, doc, !!boardId, {
    awareness,
    urlBuilder: (_room, token) => {
      const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
      return `${proto}//${window.location.host}/api/v1/office/whiteboard/boards/${boardId}/sync?token=${encodeURIComponent(token)}`
    },
  })

  // Sync background from board
  useEffect(() => {
    if (board?.background) setBackground(board.background as Background)
  }, [board?.background])

  // ── Canvas render loop ─────────────────────────────────────────────────────

  useLayoutEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const ctx    = canvas.getContext('2d')
    if (!ctx) return
    const vp     = viewportRef.current

    const render = () => {
      const { width: W, height: H } = canvas

      // Fond
      ctx.save()
      ctx.setTransform(1, 0, 0, 1, 0, 0)
      ctx.fillStyle = '#f8f9fa'
      ctx.fillRect(0, 0, W, H)
      ctx.restore()

      // Background pattern
      renderBackground(ctx, background, vp, W, H)

      // Éléments
      vp.apply(ctx)
      for (const el of elements) {
        if (el.id === editingId) continue // remplacé par la textarea d'édition
        ctx.save()
        ctx.globalAlpha = ('opacity' in el ? (el as StickyNoteEl).opacity : 1) ?? 1
        if ('rotation' in el && (el as StickyNoteEl).rotation) {
          const cx = ('x' in el ? (el as StickyNoteEl).x : 0) + ('width' in el ? (el as StickyNoteEl).width / 2 : 0)
          const cy = ('y' in el ? (el as StickyNoteEl).y : 0) + ('height' in el ? (el as StickyNoteEl).height / 2 : 0)
          ctx.translate(cx, cy)
          ctx.rotate(((el as StickyNoteEl).rotation * Math.PI) / 180)
          ctx.translate(-cx, -cy)
        }
        switch (el.type) {
          case 'sticky':  renderStickyNote(ctx, el as StickyNoteEl, vp.scale); break
          case 'text':    renderTextBox(ctx,    el as TextBox,       vp.scale); break
          case 'shape':   renderShape(ctx,      el as ShapeElement);            break
          case 'arrow':   renderArrow(ctx,      el as ArrowElement, new Map(elements.map(e => [e.id, e]))); break
          case 'frame':   renderFrame(ctx,      el as FrameElement, vp.scale); break
        }
        ctx.restore()
      }

      // Traits
      for (const s of strokes) renderStroke(ctx, s)

      // Trait en cours
      if (penRef.current && penRef.current.points.length >= 4) {
        const pts = penRef.current.points
        const smoothed = getStroke(
          Array.from({ length: pts.length / 2 }, (_, i) => [pts[i * 2], pts[i * 2 + 1], 0.5] as [number, number, number]),
          { size: tool === 'pen' ? 4 : 20, smoothing: 0.5, thinning: tool === 'pen' ? 0.4 : 0, streamline: 0.5, last: false }
        )
        if (smoothed.length > 1) {
          ctx.save()
          ctx.fillStyle = tool === 'pen' ? '#202124' : 'rgba(255, 255, 0, 0.5)'
          ctx.beginPath()
          ctx.moveTo(smoothed[0][0], smoothed[0][1])
          for (const [x, y] of smoothed.slice(1)) ctx.lineTo(x, y)
          ctx.closePath()
          ctx.fill()
          ctx.restore()
        }
      }

      // Forme en cours de dessin (aperçu live de la vraie forme)
      if (newShapeRef.current && tool === 'shape') {
        const { startX, startY, curX, curY } = newShapeRef.current
        const rx = Math.min(startX, curX), ry = Math.min(startY, curY)
        const rw = Math.abs(curX - startX), rh = Math.abs(curY - startY)
        if (rw > 0 && rh > 0) {
          ctx.globalAlpha = 0.7
          renderShape(ctx, {
            id: '__preview', type: 'shape', kind: shapeKind,
            x: rx, y: ry, width: rw, height: rh,
            fill: 'rgba(187, 222, 251, 0.5)', stroke: '#1a73e8', strokeWidth: 2,
            rotation: 0, opacity: 1, zIndex: 0, locked: false,
          } as ShapeElement)
          ctx.globalAlpha = 1
        }
      }

      // Sélection
      if (selectedId) {
        const sel = elements.find(e => e.id === selectedId)
        if (sel) renderSelectionHandles(ctx, sel, vp.scale)
      }

      // Sélections distantes (présence par élément) : cadre coloré + nom.
      awareness.getStates().forEach((st, cid) => {
        if (cid === awareness.clientID) return
        const s = st as { user?: { name: string; color: string }; sel?: { id?: string | null } }
        if (!s.user || !s.sel?.id) return
        const el = elements.find(e => e.id === s.sel!.id)
        if (!el || !('width' in el)) return
        const b = el as StickyNoteEl
        ctx.save()
        ctx.strokeStyle = s.user.color
        ctx.lineWidth = 2 / vp.scale
        ctx.strokeRect(b.x, b.y, b.width, b.height)
        ctx.restore()
      })

      // Curseurs distants (présence temps réel).
      awareness.getStates().forEach((st, cid) => {
        if (cid === awareness.clientID) return
        const s = st as { user?: { name: string; color: string }; cursor?: { x: number; y: number } }
        if (!s.user || !s.cursor) return
        // Interpolation vers la dernière position reçue (glissement fluide).
        const anim = remoteCursorAnimRef.current
        let a = anim.get(cid)
        if (!a) { a = { x: s.cursor.x, y: s.cursor.y }; anim.set(cid, a) }
        else { a.x += (s.cursor.x - a.x) * 0.2; a.y += (s.cursor.y - a.y) * 0.2 }
        const ux = a.x, uy = a.y
        const k = 1 / vp.scale
        ctx.save()
        ctx.fillStyle = s.user.color
        ctx.beginPath()
        ctx.moveTo(ux, uy)
        ctx.lineTo(ux + 11 * k, uy + 4 * k)
        ctx.lineTo(ux + 4 * k, uy + 11 * k)
        ctx.closePath()
        ctx.fill()
        const label = s.user.name || '?'
        ctx.font = `${11 * k}px system-ui, sans-serif`
        const tw = ctx.measureText(label).width
        ctx.fillStyle = s.user.color
        ctx.fillRect(ux + 10 * k, uy + 10 * k, tw + 8 * k, 16 * k)
        ctx.fillStyle = '#ffffff'
        ctx.fillText(label, ux + 14 * k, uy + 21 * k)
        ctx.restore()
      })

      ctx.setTransform(1, 0, 0, 1, 0, 0)
      rafRef.current = requestAnimationFrame(render)
    }

    rafRef.current = requestAnimationFrame(render)
    return () => cancelAnimationFrame(rafRef.current)
  }, [elements, strokes, background, selectedId, tool, shapeKind, awareness, editingId])

  // ── Resize ─────────────────────────────────────────────────────────────────

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const obs = new ResizeObserver(() => {
      canvas.width  = canvas.offsetWidth
      canvas.height = canvas.offsetHeight
    })
    obs.observe(canvas)
    canvas.width  = canvas.offsetWidth
    canvas.height = canvas.offsetHeight
    return () => obs.disconnect()
  }, [])

  // ── Helpers Yjs ───────────────────────────────────────────────────────────

  const addElement = useCallback((el: WbElement) => {
    docRef.current.transact(() => {
      docRef.current.getMap<WbElement>('elements').set(el.id, el)
    })
    setUndoStack(s => [...s, [...elements, el]])
    setRedoStack([])
  }, [elements])

  const updateElement = useCallback((id: string, patch: Partial<WbElement>) => {
    const yMap = docRef.current.getMap<WbElement>('elements')
    const existing = yMap.get(id)
    if (!existing) return
    docRef.current.transact(() => {
      yMap.set(id, { ...existing, ...patch } as WbElement)
    })
  }, [])

  const deleteSelected = useCallback(() => {
    if (!selectedId) return
    docRef.current.transact(() => {
      docRef.current.getMap<WbElement>('elements').delete(selectedId)
    })
    setSelectedId(null)
  }, [selectedId])

  const undo = useCallback(() => {
    if (undoStack.length <= 1) return
    const prev = undoStack[undoStack.length - 2]
    setRedoStack(r => [...r, undoStack[undoStack.length - 1]])
    setUndoStack(s => s.slice(0, -1))
    const yMap = docRef.current.getMap<WbElement>('elements')
    docRef.current.transact(() => {
      yMap.clear()
      for (const el of prev) yMap.set(el.id, el)
    })
  }, [undoStack])

  const redo = useCallback(() => {
    if (redoStack.length === 0) return
    const next = redoStack[redoStack.length - 1]
    setUndoStack(s => [...s, next])
    setRedoStack(r => r.slice(0, -1))
    const yMap = docRef.current.getMap<WbElement>('elements')
    docRef.current.transact(() => {
      yMap.clear()
      for (const el of next) yMap.set(el.id, el)
    })
  }, [redoStack])

  // ── Pointer events ─────────────────────────────────────────────────────────

  // Coordonnées canvas depuis un évènement pointeur : il FAUT retirer l'offset
  // écran du <canvas> (sidebar + topbar le décalent), sinon les clics sont
  // décalés d'autant et la sélection/hit-test rate l'élément qu'on vise.
  const eventToCanvas = useCallback((e: { clientX: number; clientY: number }) => {
    const rect = canvasRef.current?.getBoundingClientRect()
    return viewportRef.current.screenToCanvas(e.clientX - (rect?.left ?? 0), e.clientY - (rect?.top ?? 0))
  }, [])

  // Édition de texte en place — texte écrit en direct dans l'élément (Yjs).
  const setEditingText = useCallback((val: string) => {
    setEditText(val)
    if (editingId) updateElement(editingId, { text: val } as Partial<WbElement>)
  }, [editingId, updateElement])

  const enterTextEdit = useCallback((el: TextBox) => {
    setSelectedId(el.id); setShowProps(true)
    setEditText(el.text ?? '')
    setEditingId(el.id)
  }, [])

  // Double-clic sur un texte → édition en place.
  const onDoubleClick = useCallback((e: React.MouseEvent) => {
    const { x: cx, y: cy } = eventToCanvas(e)
    const hit = hitTest(cx, cy, elements, 4 / viewportRef.current.scale)
    if (!hit) return
    const el = elements.find(x => x.id === hit)
    if (el && el.type === 'text') enterTextEdit(el as TextBox)
  }, [elements, eventToCanvas, enterTextEdit])

  const onPointerDown = useCallback((e: React.PointerEvent) => {
    e.currentTarget.setPointerCapture(e.pointerId)
    const { x: cx, y: cy } = eventToCanvas(e)
    const vp = viewportRef.current

    if (tool === 'hand' || (tool === 'select' && e.button === 1)) {
      panRef.current = { startX: e.clientX, startY: e.clientY }
      return
    }

    if (tool === 'select') {
      // 1) Poignée de redimensionnement de l'élément déjà sélectionné ?
      if (selectedId) {
        const selEl = elements.find(e2 => e2.id === selectedId)
        if (selEl && 'width' in selEl) {
          const h = hitHandle(cx, cy, selEl, vp.scale)
          if (h) {
            const s = selEl as StickyNoteEl
            resizeRef.current = { id: selectedId, handle: h, startX: e.clientX, startY: e.clientY, orig: { x: s.x, y: s.y, width: s.width, height: s.height } }
            return
          }
        }
      }
      // 2) Sinon : sélection / déplacement
      const hit = hitTest(cx, cy, elements, 4 / vp.scale)
      if (hit) {
        const el = elements.find(e2 => e2.id === hit)
        if (el && 'x' in el) {
          dragRef.current = { startX: e.clientX, startY: e.clientY, origX: (el as StickyNoteEl).x, origY: (el as StickyNoteEl).y, id: hit }
        }
        setSelectedId(hit)
        setShowProps(true)
      } else {
        setSelectedId(null)
        setShowProps(false)
      }
      return
    }

    if (tool === 'sticky') {
      const id = genId()
      addElement({
        id, type: 'sticky', x: cx - 100, y: cy - 75, width: 200, height: 150,
        text: '', color: stickyColor, fontSize: 14, rotation: 0, opacity: 1,
        zIndex: elements.length, locked: false, textAlign: 'left',
      } as StickyNoteEl)
      setSelectedId(id)
      setShowProps(true)
      setTool('select')
      return
    }

    if (tool === 'text') {
      const id = genId()
      const el = {
        id, type: 'text', x: cx, y: cy, width: 200, height: 40,
        text: '', color: '#202124', fontSize: 16, fontWeight: 'normal',
        rotation: 0, opacity: 1, zIndex: elements.length, locked: false, textAlign: 'left',
      } as TextBox
      addElement(el)
      setTool('select')
      setSelectedId(id)
      // Différé : sinon le pointerup/click de création (canvas) blurre aussitôt la
      // textarea fraîchement focus → l'édition se fermerait immédiatement.
      setTimeout(() => enterTextEdit(el), 0)
      return
    }

    if (tool === 'shape') {
      newShapeRef.current = { startX: cx, startY: cy, curX: cx, curY: cy }
      return
    }

    if (tool === 'pen' || tool === 'eraser') {
      penRef.current = { points: [cx, cy], id: genId() }
      return
    }
  }, [tool, elements, stickyColor, addElement, eventToCanvas, selectedId, enterTextEdit])

  const onPointerMove = useCallback((e: React.PointerEvent) => {
    const vp = viewportRef.current
    const { x: cx, y: cy } = eventToCanvas(e)

    // Présence : diffuser la position du curseur (en coords canvas), throttlé.
    const now = performance.now()
    if (now - lastCursorRef.current > 40) {
      lastCursorRef.current = now
      awareness.setLocalStateField('cursor', { x: Math.round(cx), y: Math.round(cy) })
    }

    // Curseur de survol (sans bouton) : poignée de resize sur l'élément sélectionné.
    if (!e.buttons) {
      let cur: string | null = null
      if (tool === 'select' && selectedId) {
        const selEl = elements.find(e2 => e2.id === selectedId)
        if (selEl && 'width' in selEl) {
          const h = hitHandle(cx, cy, selEl, vp.scale)
          if (h) cur = handleCursor(h)
        }
      }
      setHoverCursor(cur)
      return
    }

    if (resizeRef.current) {
      const r = resizeRef.current
      const dx = (e.clientX - r.startX) / vp.scale
      const dy = (e.clientY - r.startY) / vp.scale
      let { x, y, width, height } = r.orig
      const MIN = 10
      if (r.handle.includes('e')) width  = r.orig.width  + dx
      if (r.handle.includes('s')) height = r.orig.height + dy
      if (r.handle.includes('w')) { width  = r.orig.width  - dx; x = r.orig.x + dx }
      if (r.handle.includes('n')) { height = r.orig.height - dy; y = r.orig.y + dy }
      if (width  < MIN) { if (r.handle.includes('w')) x = r.orig.x + r.orig.width  - MIN; width  = MIN }
      if (height < MIN) { if (r.handle.includes('n')) y = r.orig.y + r.orig.height - MIN; height = MIN }
      updateElement(r.id, { x: Math.round(x), y: Math.round(y), width: Math.round(width), height: Math.round(height) } as Partial<WbElement>)
      return
    }

    if (panRef.current) {
      vp.pan(e.clientX - panRef.current.startX, e.clientY - panRef.current.startY)
      panRef.current = { startX: e.clientX, startY: e.clientY }
      setZoom(vp.zoomPercent)
      return
    }

    if (dragRef.current) {
      const dx = (e.clientX - dragRef.current.startX) / vp.scale
      const dy = (e.clientY - dragRef.current.startY) / vp.scale
      updateElement(dragRef.current.id, { x: dragRef.current.origX + dx, y: dragRef.current.origY + dy } as Partial<WbElement>)
      return
    }

    if (newShapeRef.current) {
      newShapeRef.current.curX = cx
      newShapeRef.current.curY = cy
      return
    }

    if (penRef.current) {
      penRef.current.points.push(cx, cy)
      return
    }
  }, [updateElement, eventToCanvas, tool, selectedId, elements, awareness])

  const onPointerUp = useCallback((e: React.PointerEvent) => {
    panRef.current  = null

    if (resizeRef.current) {
      resizeRef.current = null
      setUndoStack(s => [...s, elements])
      setRedoStack([])
      return
    }

    if (dragRef.current) {
      const dx = (e.clientX - dragRef.current.startX) / viewportRef.current.scale
      const dy = (e.clientY - dragRef.current.startY) / viewportRef.current.scale
      const { origX, origY, id } = dragRef.current
      updateElement(id, { x: Math.round(origX + dx), y: Math.round(origY + dy) } as Partial<WbElement>)
      dragRef.current = null
      setUndoStack(s => [...s, elements])
      setRedoStack([])
      return
    }

    if (tool === 'shape' && newShapeRef.current) {
      const { x: cx, y: cy } = eventToCanvas(e)
      const { startX, startY } = newShapeRef.current
      newShapeRef.current = null
      const w = Math.abs(cx - startX), h = Math.abs(cy - startY)
      if (w < 10 || h < 10) return
      const id = genId()
      addElement({
        id, type: 'shape', kind: shapeKind,
        x: Math.min(startX, cx), y: Math.min(startY, cy), width: w, height: h,
        fill: '#BBDEFB', stroke: '#1a73e8', strokeWidth: 2,
        rotation: 0, opacity: 1, zIndex: elements.length, locked: false,
      } as ShapeElement)
      setSelectedId(id)
      setShowProps(true)
      setTool('select')
      return
    }

    if (penRef.current) {
      const { points, id } = penRef.current
      penRef.current = null
      if (points.length < 4) return

      const smoothed = getStroke(
        Array.from({ length: points.length / 2 }, (_, i) => [points[i * 2], points[i * 2 + 1], 0.5] as [number, number, number]),
        { size: tool === 'pen' ? 4 : 20, smoothing: 0.5, thinning: tool === 'pen' ? 0.4 : 0, streamline: 0.5, last: true }
      )
      const flat = smoothed.flatMap(([x, y]) => [x, y])

      docRef.current.transact(() => {
        docRef.current.getArray<Stroke>('strokes').push([{
          id, points: flat,
          color:   tool === 'eraser' ? '#f8f9fa' : '#202124',
          width:   tool === 'pen'    ? 2         : 20,
          opacity: tool === 'pen'    ? 1         : 0.5,
          tool:    tool === 'eraser' ? 'pen'     : tool as 'pen' | 'highlighter',
          userId: CLIENT_ID,
          createdAt: Date.now(),
        }])
      })
      return
    }
  }, [tool, elements, addElement, updateElement, eventToCanvas, shapeKind])

  const onWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault()
    const vp = viewportRef.current
    if (e.ctrlKey || e.metaKey) {
      // Ancre de zoom en coordonnées locales au canvas (retirer son offset écran).
      const rect = canvasRef.current?.getBoundingClientRect()
      vp.zoomAt(e.deltaY < 0 ? 1.1 : 0.9, e.clientX - (rect?.left ?? 0), e.clientY - (rect?.top ?? 0))
    } else {
      vp.pan(-e.deltaX, -e.deltaY)
    }
    setZoom(vp.zoomPercent)
  }, [])

  // Keyboard shortcuts
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Delete' || e.key === 'Backspace') {
        if (document.activeElement === document.body) deleteSelected()
      }
      if ((e.metaKey || e.ctrlKey) && e.key === 'z') { e.preventDefault(); undo() }
      if ((e.metaKey || e.ctrlKey) && e.key === 'y') { e.preventDefault(); redo() }
      // Raccourcis d'outils à une touche : ignorés si on tape dans un champ.
      const ae = document.activeElement as HTMLElement | null
      if (e.metaKey || e.ctrlKey || e.altKey) return
      if (ae && (ae.tagName === 'INPUT' || ae.tagName === 'TEXTAREA' || ae.isContentEditable)) return
      if (e.key === 'v') setTool('select')
      if (e.key === 'h') setTool('hand')
      if (e.key === 'p') setTool('pen')
      if (e.key === 't') setTool('text')
      if (e.key === 'r') { setShapeKind('rect');   setTool('shape') }
      if (e.key === 'o') { setShapeKind('circle'); setTool('shape') }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [deleteSelected, undo, redo])

  const selectedEl = elements.find(e => e.id === selectedId)

  return (
    <OfficeShell
      ribbon={[{ id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }),
        groups: [fileGroup(t, { onNew: () => createBoardMut.mutate(), onDuplicate: () => dupBoardMut.mutate() })] }]}
      theme={THEME_WHITEBOARD}
      chromeless
      topbarHeight={64}
      onBack={onBack}
      titleIcon={<StickyNote size={16} className="text-white/90 flex-shrink-0" />}
      title={titleDraft}
      onTitleChange={setTitleDraft}
      onTitleCommit={commitTitle}
      titlePlaceholder={t('common_untitled', { defaultValue: 'Sans titre' })}
      titleActions={(
        <button
          onClick={() => starBoardMut.mutate(!board?.is_starred)}
          title={board?.is_starred ? t('wb_unstar', { defaultValue: 'Retirer des favoris' }) : t('wb_star', { defaultValue: 'Ajouter aux favoris' })}
          className={clsx('p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0', board?.is_starred ? 'text-warning' : 'text-white/90')}
        >
          <Star size={15} fill={board?.is_starred ? 'currentColor' : 'none'} />
        </button>
      )}
      onDelete={() => trashBoardMut.mutate()}
      deleteTitle={t('wb_move_to_trash', { defaultValue: 'Mettre à la corbeille' })}
      deleteConfirm={{
        title: t('wb_delete_confirm_title', { defaultValue: 'Supprimer ce tableau ?' }),
        message: t('wb_delete_confirm_msg', { defaultValue: 'Le tableau sera déplacé dans la corbeille.' }),
        confirmLabel: t('common_delete', { defaultValue: 'Supprimer' }),
        variant: 'danger',
      }}
      topbarActions={<>
        <PresenceAvatars awareness={awareness} selfClientId={awareness.clientID} />
        <button onClick={undo} className="p-1.5 rounded hover:bg-white/10 text-white/90" title={t('wb_undo_shortcut')}>
          <RotateCcw size={16} />
        </button>
        <button onClick={redo} className="p-1.5 rounded hover:bg-white/10 text-white/90" title={t('wb_redo_shortcut')}>
          <RotateCw size={16} />
        </button>
        <div className="w-px h-5 bg-white/20 mx-1" />
        <Button variant="secondary" size="sm" icon={<Share2 size={15} />} onClick={() => setShareOpen(true)}>
          {t('wb_share')}
        </Button>
      </>}
    >
    <div className="flex flex-col flex-1 min-w-0 min-h-0 overflow-hidden" style={{ background: '#f8f9fa' }}>
      <div className="flex flex-1 overflow-hidden relative min-h-0">
        {/* Toolbar gauche */}
        <div className="w-12 bg-white border-r border-[#e8eaed] flex flex-col items-center py-2 gap-1 shrink-0 z-10">
          {TOOLS.map(({ id, Icon, titleKey, shortcut }) => {
            // Le bouton « Forme » ouvre un menu de formes (façon Google/Miro).
            if (id === 'shape') {
              const CurIcon = SHAPE_KINDS.find(s => s.kind === shapeKind)?.Icon ?? Square
              return (
                <div key={id} className="relative">
                  <button
                    onClick={() => { setTool('shape'); setShapeMenuOpen(o => !o) }}
                    title={`${t(titleKey)} (${shortcut})`}
                    className={clsx('w-9 h-9 rounded-lg flex items-center justify-center transition-colors relative',
                      tool === 'shape' ? 'bg-[#e8f0fe] text-[#1a73e8]' : 'text-[#5f6368] hover:bg-[#f1f3f4]')}>
                    <CurIcon size={18} />
                    <ChevronRight size={9} className="absolute bottom-0.5 right-0.5 opacity-50" />
                  </button>
                  {shapeMenuOpen && (
                    <>
                      <div className="fixed inset-0 z-20" onClick={() => setShapeMenuOpen(false)} />
                      <div className="absolute left-11 top-0 z-30 bg-white rounded-xl shadow-xl border border-[#e8eaed] py-1.5 w-56">
                        {SHAPE_KINDS.map(s => (
                          <button key={s.kind}
                            onClick={() => { setShapeKind(s.kind); setTool('shape'); setShapeMenuOpen(false) }}
                            className={clsx('flex items-center gap-3 w-full px-3 py-2 text-left text-sm transition-colors hover:bg-[#f1f3f4]',
                              shapeKind === s.kind ? 'text-[#1a73e8] bg-[#e8f0fe]' : 'text-[#202124]')}>
                            <s.Icon size={16} className="shrink-0" />
                            <span className="flex-1">{t(s.labelKey, { defaultValue: s.label })}</span>
                            {s.shortcut && <span className="text-xs text-[#80868b]">{s.shortcut}</span>}
                          </button>
                        ))}
                      </div>
                    </>
                  )}
                </div>
              )
            }
            return (
              <button key={id} onClick={() => setTool(id as ToolType)} title={`${t(titleKey)} (${shortcut})`}
                className={clsx('w-9 h-9 rounded-lg flex items-center justify-center transition-colors',
                  tool === id ? 'bg-[#e8f0fe] text-[#1a73e8]' : 'text-[#5f6368] hover:bg-[#f1f3f4]')}>
                <Icon size={18} />
              </button>
            )
          })}
        </div>

        {/* Canvas */}
        <div className="flex-1 relative overflow-hidden">
          <canvas
            ref={canvasRef}
            className="absolute inset-0 w-full h-full"
            style={{ cursor: hoverCursor ?? (tool === 'hand' ? 'grab' : tool === 'pen' ? 'crosshair' : tool === 'eraser' ? 'cell' : 'default') }}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onDoubleClick={onDoubleClick}
            onWheel={onWheel}
          />
          {/* Édition de texte en place (façon presentations) */}
          {editingId && (() => {
            const el = elements.find(e => e.id === editingId)
            if (!el || el.type !== 'text') return null
            const te = el as TextBox
            const vp = viewportRef.current
            const sp = vp.canvasToScreen(te.x, te.y)
            return (
              <textarea
                autoFocus
                value={editText}
                placeholder={t('wb_text_placeholder')}
                onChange={ev => setEditingText(ev.target.value)}
                onBlur={() => setEditingId(null)}
                onPointerDown={ev => ev.stopPropagation()}
                onKeyDown={ev => { if (ev.key === 'Escape') { ev.preventDefault(); (ev.target as HTMLTextAreaElement).blur() } ev.stopPropagation() }}
                style={{
                  position: 'absolute',
                  left: sp.x,
                  top: sp.y,
                  width: te.width * vp.scale,
                  height: te.height * vp.scale,
                  fontSize: (te.fontSize ?? 16) * vp.scale,
                  lineHeight: 1.4,
                  textAlign: te.textAlign ?? 'left',
                  color: te.color ?? '#202124',
                  fontFamily: 'Arial, sans-serif',
                  fontWeight: te.fontWeight === 'bold' ? 'bold' : 'normal',
                  background: 'transparent',
                  border: 'none',
                  outline: '2px solid #1a73e8',
                  borderRadius: 2,
                  resize: 'none',
                  padding: 0,
                  margin: 0,
                  overflow: 'hidden',
                  boxSizing: 'border-box',
                }}
              />
            )
          })()}
        </div>

        {/* Panneau propriétés */}
        {showProps && selectedEl && (
          <PropertiesPanel
            element={selectedEl}
            onUpdate={(patch) => updateElement(selectedId!, patch as Partial<WbElement>)}
            onDelete={deleteSelected}
            onClose={() => { setShowProps(false); setSelectedId(null) }}
          />
        )}
      </div>

      {/* Bottombar */}
      <div className="flex items-center h-9 bg-white border-t border-[#e8eaed] px-3 gap-2 text-xs text-[#5f6368] shrink-0">
        <button onClick={() => { viewportRef.current.zoomAt(0.8, window.innerWidth / 2, window.innerHeight / 2); setZoom(viewportRef.current.zoomPercent) }}
          className="p-1 rounded hover:bg-[#f1f3f4]">
          <ZoomOut size={14} />
        </button>
        <span className="w-12 text-center">{zoom}%</span>
        <button onClick={() => { viewportRef.current.zoomAt(1.25, window.innerWidth / 2, window.innerHeight / 2); setZoom(viewportRef.current.zoomPercent) }}
          className="p-1 rounded hover:bg-[#f1f3f4]">
          <ZoomIn size={14} />
        </button>
        <button onClick={() => { viewportRef.current.fitToElements(elements, canvasRef.current?.width ?? 800, canvasRef.current?.height ?? 600); setZoom(viewportRef.current.zoomPercent) }}
          className="p-1 rounded hover:bg-[#f1f3f4]">
          <Maximize2 size={14} />
        </button>
        <div className="flex-1" />
        <span>{t('wb_element_count', { count: elements.length })}</span>
        <Dropdown
          value={background}
          onChange={v => { const bg = v as Background; setBackground(bg); boardsApi.update(boardId, { background: bg }) }}
          height={24}
          fontSize={12}
          options={[
            { value: 'white', label: t('wb_bg_white') },
            { value: 'dots',  label: t('wb_bg_dots') },
            { value: 'grid',  label: t('wb_bg_grid') },
            { value: 'lines', label: t('wb_bg_lines') },
          ]}
        />
      </div>
    </div>

    {shareOpen && (
      <CollaboratorsDialog
        entityId={boardId}
        cacheKey="wb-collab"
        title={t('wb_share', { defaultValue: 'Partager le tableau' })}
        onClose={() => setShareOpen(false)}
        api={{
          listCollaborators:  boardsApi.listCollaborators,
          addCollaborator:    boardsApi.addCollaborator,
          updateCollaborator: boardsApi.updateCollaborator,
          removeCollaborator: boardsApi.removeCollaborator,
          searchRecipients:   officeApi.searchRecipients,
        }}
      />
    )}
    </OfficeShell>
  )
}

// ── Properties Panel ──────────────────────────────────────────────────────────

function PropertiesPanel({ element: el, onUpdate, onDelete, onClose }: {
  element: WbElement
  onUpdate: (patch: Partial<WbElement>) => void
  onDelete: () => void
  onClose: () => void
}) {
  const { t } = useTranslation('office')
  return (
    <div className="w-64 bg-white border-l border-[#e8eaed] flex flex-col shrink-0 overflow-y-auto z-10">
      <div className="flex items-center justify-between px-3 py-2 border-b border-[#e8eaed]">
        <span className="text-xs font-medium text-[#5f6368] uppercase tracking-wide">
          {el.type === 'sticky' ? t('wb_type_sticky') : el.type === 'text' ? t('wb_type_text') : el.type === 'shape' ? t('wb_type_shape') : el.type === 'arrow' ? t('wb_type_arrow') : t('wb_type_element')}
        </span>
        <button onClick={onClose} className="p-1 rounded hover:bg-[#f1f3f4] text-[#9aa0a6]">
          <Minus size={14} />
        </button>
      </div>

      <div className="p-3 space-y-4">
        {/* Sticky color */}
        {el.type === 'sticky' && (
          <div>
            <label className="text-xs text-[#5f6368] font-medium block mb-2">{t('wb_color')}</label>
            <div className="flex flex-wrap gap-1.5">
              {STICKY_COLOR_KEYS.map(k => (
                <button key={k} onClick={() => onUpdate({ color: k } as Partial<WbElement>)}
                  className={clsx('w-6 h-6 rounded border-2 transition-all',
                    (el as StickyNoteEl).color === k ? 'border-[#1a73e8] scale-110' : 'border-transparent hover:border-[#9aa0a6]')}
                  style={{ background: STICKY_COLORS[k] }} title={k} />
              ))}
            </div>
          </div>
        )}

        {/* Text */}
        {(el.type === 'sticky' || el.type === 'text') && (
          <div>
            <label className="text-xs text-[#5f6368] font-medium block mb-1">{t('wb_type_text')}</label>
            <Textarea
              value={(el as StickyNoteEl).text ?? ''}
              onChange={e => onUpdate({ text: e.target.value } as Partial<WbElement>)}
              rows={4}
              className="h-auto min-h-0 resize-none"
              placeholder={t('wb_text_placeholder')}
            />
          </div>
        )}

        {/* Shape fill */}
        {el.type === 'shape' && (
          <div>
            <label className="text-xs text-[#5f6368] font-medium block mb-1">{t('wb_fill_color')}</label>
            <input type="color" value={(el as ShapeElement).fill ?? '#BBDEFB'}
              onChange={e => onUpdate({ fill: e.target.value } as Partial<WbElement>)}
              className="w-full h-8 rounded cursor-pointer border border-[#dadce0]" />
          </div>
        )}

        {/* Position */}
        {'x' in el && (
          <div className="grid grid-cols-2 gap-2">
            <div>
              <label className="text-[10px] text-[#5f6368] font-medium block mb-1">X</label>
              <Input type="number" value={Math.round((el as StickyNoteEl).x)}
                onChange={e => onUpdate({ x: +e.target.value } as Partial<WbElement>)}
                className="h-8 text-xs" />
            </div>
            <div>
              <label className="text-[10px] text-[#5f6368] font-medium block mb-1">Y</label>
              <Input type="number" value={Math.round((el as StickyNoteEl).y)}
                onChange={e => onUpdate({ y: +e.target.value } as Partial<WbElement>)}
                className="h-8 text-xs" />
            </div>
          </div>
        )}

        {/* Opacity */}
        {'opacity' in el && (
          <div>
            <label className="text-xs text-[#5f6368] font-medium block mb-1">
              {t('wb_opacity', { value: Math.round(((el as StickyNoteEl).opacity ?? 1) * 100) })}
            </label>
            <input type="range" min={0.1} max={1} step={0.05}
              value={(el as StickyNoteEl).opacity ?? 1}
              onChange={e => onUpdate({ opacity: +e.target.value } as Partial<WbElement>)}
              className="w-full" />
          </div>
        )}

        {/* Actions */}
        <div className="flex gap-2 pt-2 border-t border-[#f1f3f4]">
          <Button variant="danger" icon={<Trash2 size={13} />} onClick={onDelete} className="flex-1 text-xs">
            {t('common_delete')}
          </Button>
        </div>
      </div>
    </div>
  )
}

// ── Helpers ───────────────────────────────────────────────────────────────────

const TOOLS: { id: string; Icon: React.ComponentType<{ size: number }>; titleKey: string; shortcut: string }[] = [
  { id: 'select',  Icon: MousePointer2, titleKey: 'wb_tool_select', shortcut: 'V' },
  { id: 'hand',    Icon: Hand,          titleKey: 'wb_tool_hand',   shortcut: 'H' },
  { id: 'sticky',  Icon: StickyNote,    titleKey: 'wb_tool_sticky', shortcut: 'S' },
  { id: 'text',    Icon: Type,          titleKey: 'wb_tool_text',   shortcut: 'T' },
  { id: 'shape',   Icon: Square,        titleKey: 'wb_tool_shape',  shortcut: 'R' },
  { id: 'arrow',   Icon: ArrowRight,    titleKey: 'wb_tool_arrow',  shortcut: 'A' },
  { id: 'pen',     Icon: Pen,           titleKey: 'wb_tool_pen',    shortcut: 'P' },
  { id: 'eraser',  Icon: Eraser,        titleKey: 'wb_tool_eraser', shortcut: 'E' },
]

// Formes proposées par le menu du bouton « Forme ».
const SHAPE_KINDS: { kind: ShapeKind; Icon: React.ComponentType<{ size: number; className?: string }>; labelKey: string; label: string; shortcut?: string }[] = [
  { kind: 'rect',     Icon: Square,   labelKey: 'wb_shape_rect',     label: 'Rectangle', shortcut: 'R' },
  { kind: 'circle',   Icon: Circle,   labelKey: 'wb_shape_circle',   label: 'Ovale',     shortcut: 'O' },
  { kind: 'diamond',  Icon: Diamond,  labelKey: 'wb_shape_diamond',  label: 'Losange' },
  { kind: 'triangle', Icon: Triangle, labelKey: 'wb_shape_triangle', label: 'Triangle' },
  { kind: 'star',     Icon: Star,     labelKey: 'wb_shape_star',     label: 'Étoile' },
]

function genId() {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}
