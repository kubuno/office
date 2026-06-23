import { useState, useEffect, useRef, useCallback, useLayoutEffect, useMemo } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { Awareness } from 'y-protocols/awareness'
import { useCollab } from './collab/collabProvider'
import { userColor, PresenceAvatars } from './collab/presence'
import { useAuthStore, DockArea, type DockPanel } from '@kubuno/sdk'
import { useTranslation } from 'react-i18next'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  Plus, Trash2, Star,
  Hand, MousePointer2, Square, Type, Minus, ZoomIn, ZoomOut,
  Maximize2, StickyNote, Pen, Eraser, ArrowRight, RotateCcw,
  RotateCw, Share2, ExternalLink, Copy, Circle, Diamond, Triangle, ChevronRight,
  Upload, Lock, Unlock, BringToFront, SendToBack, Download, Image as ImageIcon, Grid3x3, ClipboardPaste,
  Bold, AlignLeft, AlignCenter, AlignRight, Frame as FrameIcon,
  AlignStartVertical, AlignCenterVertical, AlignEndVertical,
  AlignStartHorizontal, AlignCenterHorizontal, AlignEndHorizontal,
  AlignHorizontalDistributeCenter, AlignVerticalDistributeCenter, BoxSelect,
  Group, Ungroup, FilePlus, CopyPlus,
} from 'lucide-react'
import clsx from 'clsx'
import { format } from 'date-fns'
import { getDateLocale } from '@kubuno/sdk'
import { Button, Input, Textarea, Dropdown, RangeSlider, MenuDropdown } from '@ui'
import type { MenuItem, MenuDropdownPos } from '@ui'
import { excalidrawToWhiteboard } from './whiteboard-excalidraw'
import type { StartPageRecentItem } from '@ui'
import { ModuleStartPage } from '@kubuno/drive'
import type { FileItem } from '@kubuno/drive'
import * as Y from 'yjs'

import { boardsApi } from './whiteboard-api'
import { officeApi } from './api'
import CollaboratorsDialog from './CollaboratorsDialog'
import { OfficeShell } from './shell/OfficeShell'
import { SaveButton } from './ribbon/SaveButton'
import { StatusBar, StatusButton, StatusSep, StatusSpacer, StatusZoom } from './shell/StatusBar'
import { MacrosMenu } from './macros/MacrosMenu'
import { THEME_WHITEBOARD } from './ribbon/officeThemes'
import { ModuleHome, useFileTab, backstageLabels, InfoPanel } from './ribbon/ModuleBackstage'
import type {
  WbElement, StickyNote as StickyNoteEl, ShapeElement, TextBox,
  ArrowElement, FrameElement, ImageElement, Stroke, ToolType, Background,
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

// ── Start content (shared) ──────────────────────────────────────────────────
// Whiteboard start page (recents + browse), reused by BOTH the landing dashboard
// and the editor's « Fichier » backstage. Self-contained: it fetches its own
// boards list and owns its mutations, so it can render in either context.
function WhiteboardStartContent({ onOpen }: { onOpen: (id: string) => void }) {
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

// ── Dashboard ─────────────────────────────────────────────────────────────────

function BoardDashboard({ onOpen }: { onOpen: (id: string) => void }) {
  const { t } = useTranslation('office')
  const navigate = useNavigate()

  return (
    <ModuleHome
      theme={THEME_WHITEBOARD}
      title={t('wb_whiteboards', { defaultValue: 'Whiteboard' })}
      titleIcon={<StickyNote size={16} className="text-white/90 flex-shrink-0" />}
      fileLabel={t('doc_bs_file', { defaultValue: 'Fichier' })}
      homeLabel={t('doc_bs_home', { defaultValue: 'Accueil' })}
      onBack={() => navigate('/office')}
      startContent={<WhiteboardStartContent onOpen={onOpen} />}
    />
  )
}

// ── Editor ────────────────────────────────────────────────────────────────────

function WhiteboardEditor({ boardId, onBack, onOpen }: { boardId: string; onBack: () => void; onOpen: (id: string) => void }) {
  const { t, i18n } = useTranslation('office')
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
  // Force-save: the board content lives in Yjs and is persisted continuously by the
  // sync service, so a nop transaction flushes any pending local update to the server,
  // then a metadata PATCH (same REST path as title/star/background) bumps `updated_at`
  // and gives the SaveButton a real network round-trip for its "saving" feedback.
  const saveBoardMut = useMutation({
    mutationFn: async () => {
      docRef.current.transact(() => {}) // flush pending Yjs updates
      await boardsApi.update(boardId, { title: titleDraft.trim() || board?.title })
    },
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['wb-board', boardId] }); qc.invalidateQueries({ queryKey: ['wb-boards'] }) },
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
  // Multi-selection: `selectedIds` is the source of truth; `selectedId` is the
  // derived single selection (single-element UI: resize handles, properties).
  const [selectedIds, setSelectedIds] = useState<string[]>([])
  const selectedId = selectedIds.length === 1 ? selectedIds[0] : null
  const setSelectedId = useCallback((id: string | null) => setSelectedIds(id ? [id] : []), [])
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
  const dragRef = useRef<{ startX: number; startY: number; origs: { id: string; x: number; y: number }[] } | null>(null)
  const boxSelRef = useRef<{ startX: number; startY: number; curX: number; curY: number } | null>(null)
  const panRef  = useRef<{ startX: number; startY: number } | null>(null)
  const newShapeRef = useRef<{ startX: number; startY: number; curX: number; curY: number } | null>(null)
  const newArrowRef = useRef<{ startX: number; startY: number; curX: number; curY: number } | null>(null)
  const newFrameRef = useRef<{ startX: number; startY: number; curX: number; curY: number } | null>(null)
  const resizeRef = useRef<{ id: string; handle: ResizeHandle; startX: number; startY: number; orig: { x: number; y: number; width: number; height: number } } | null>(null)
  const [hoverCursor, setHoverCursor] = useState<string | null>(null)
  const [shareOpen, setShareOpen] = useState(false)
  // Number of remote collaborators currently connected (presence), for the status bar.
  const [remotePeers, setRemotePeers] = useState(0)
  // Import file input (Excalidraw) + canvas context menu.
  const fileInputRef = useRef<HTMLInputElement>(null)
  const [ctxMenu, setCtxMenu] = useState<{ pos: MenuDropdownPos; items: MenuItem[] } | null>(null)
  // Image insertion (file input + decoded-image cache for the render loop) + clipboard + grid snap.
  const imgFileInputRef = useRef<HTMLInputElement>(null)
  const imgCacheRef = useRef<Map<string, HTMLImageElement>>(new Map())
  const clipboardRef = useRef<WbElement | null>(null)
  const [snapGrid, setSnapGrid] = useState(false)

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

  // Track the count of remote peers (everyone but us) for the status bar.
  useEffect(() => {
    const update = () => setRemotePeers(Math.max(0, awareness.getStates().size - 1))
    awareness.on('change', update)
    update()
    return () => awareness.off('change', update)
  }, [awareness])

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
          case 'image': {
            const im = el as ImageElement
            let cached = imgCacheRef.current.get(im.id)
            if (!cached) { cached = new Image(); cached.src = im.src; imgCacheRef.current.set(im.id, cached) }
            if (cached.complete && cached.naturalWidth) ctx.drawImage(cached, im.x, im.y, im.width, im.height)
            else { ctx.fillStyle = '#e8eaed'; ctx.fillRect(im.x, im.y, im.width, im.height) }
            break
          }
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

      // Flèche en cours (live preview)
      if (newArrowRef.current && tool === 'arrow') {
        const { startX, startY, curX, curY } = newArrowRef.current
        if (Math.hypot(curX - startX, curY - startY) > 1) {
          ctx.globalAlpha = 0.85
          renderArrow(ctx, {
            id: '__preview_arrow', type: 'arrow', startX, startY, endX: curX, endY: curY,
            color: '#1a73e8', width: 2, style: 'straight', startArrow: 'none', endArrow: 'triangle', zIndex: 0, opacity: 1,
          } as ArrowElement, new Map())
          ctx.globalAlpha = 1
        }
      }

      // Cadre en cours (live preview)
      if (newFrameRef.current && tool === 'frame') {
        const { startX, startY, curX, curY } = newFrameRef.current
        const rx = Math.min(startX, curX), ry = Math.min(startY, curY), rw = Math.abs(curX - startX), rh = Math.abs(curY - startY)
        if (rw > 1 && rh > 1) {
          ctx.globalAlpha = 0.85
          renderFrame(ctx, { id: '__preview_frame', type: 'frame', x: rx, y: ry, width: rw, height: rh, title: '', color: '#1a73e8', rotation: 0, opacity: 1, zIndex: 0, locked: false } as FrameElement, vp.scale)
          ctx.globalAlpha = 1
        }
      }

      // Sélection : poignées si 1 seul élément, contours pointillés si plusieurs.
      if (selectedIds.length === 1) {
        const sel = elements.find(e => e.id === selectedIds[0])
        if (sel) renderSelectionHandles(ctx, sel, vp.scale)
      } else if (selectedIds.length > 1) {
        for (const id of selectedIds) {
          const el = elements.find(e => e.id === id)
          if (el && 'width' in el) {
            const b = el as StickyNoteEl
            ctx.save(); ctx.strokeStyle = '#1a73e8'; ctx.lineWidth = 1.5 / vp.scale; ctx.setLineDash([4 / vp.scale, 3 / vp.scale]); ctx.strokeRect(b.x, b.y, b.width, b.height); ctx.restore()
          }
        }
      }

      // Lasso de sélection (rectangle de sélection multiple).
      if (boxSelRef.current) {
        const { startX, startY, curX, curY } = boxSelRef.current
        const bx = Math.min(startX, curX), by = Math.min(startY, curY), bw = Math.abs(curX - startX), bh = Math.abs(curY - startY)
        ctx.save()
        ctx.fillStyle = 'rgba(26,115,232,0.10)'; ctx.fillRect(bx, by, bw, bh)
        ctx.strokeStyle = '#1a73e8'; ctx.lineWidth = 1 / vp.scale; ctx.strokeRect(bx, by, bw, bh)
        ctx.restore()
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
  }, [elements, strokes, background, selectedIds, tool, shapeKind, awareness, editingId])

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

  // `arg` may be an explicit id list, or — when wired straight to onClick — the
  // DOM event; fall back to the current selection unless given a real array.
  const deleteSelected = useCallback((arg?: unknown) => {
    const ids = Array.isArray(arg) ? arg as string[] : selectedIds
    if (ids.length === 0) return
    docRef.current.transact(() => {
      const m = docRef.current.getMap<WbElement>('elements')
      for (const id of ids) m.delete(id)
    })
    setSelectedIds([])
  }, [selectedIds])

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

  // Coordonnées canvas depuis un évènement pointeur : il FAUT retirer l'offset
  // écran du <canvas> (sidebar + topbar le décalent), sinon les clics sont
  // décalés d'autant et la sélection/hit-test rate l'élément qu'on vise.
  const eventToCanvas = useCallback((e: { clientX: number; clientY: number }) => {
    const rect = canvasRef.current?.getBoundingClientRect()
    return viewportRef.current.screenToCanvas(e.clientX - (rect?.left ?? 0), e.clientY - (rect?.top ?? 0))
  }, [])

  // ── Element operations (used by context menu, ribbon, keyboard) ─────────────

  const maxZ = useCallback(() => elements.reduce((m, e) => Math.max(m, 'zIndex' in e ? e.zIndex : 0), 0), [elements])
  const minZ = useCallback(() => elements.reduce((m, e) => Math.min(m, 'zIndex' in e ? e.zIndex : 0), 0), [elements])
  const bringToFront = useCallback((id: string) => updateElement(id, { zIndex: maxZ() + 1 } as Partial<WbElement>), [updateElement, maxZ])
  const sendToBack   = useCallback((id: string) => updateElement(id, { zIndex: minZ() - 1 } as Partial<WbElement>), [updateElement, minZ])
  const setLocked    = useCallback((id: string, locked: boolean) => updateElement(id, { locked } as Partial<WbElement>), [updateElement])
  const deleteById   = useCallback((id: string) => {
    docRef.current.transact(() => docRef.current.getMap<WbElement>('elements').delete(id))
    setSelectedIds(ids => ids.filter(i => i !== id))
  }, [])
  const duplicateById = useCallback((id: string, groupId?: string) => {
    const el = docRef.current.getMap<WbElement>('elements').get(id)
    if (!el) return
    const copy = { ...el, id: genId(), zIndex: maxZ() + 1 } as WbElement & { x?: number; y?: number; groupId?: string }
    // A lone duplicate leaves its source group; a group duplicate gets the
    // caller-provided fresh groupId so the copies stay grouped together.
    if (groupId) copy.groupId = groupId; else delete copy.groupId
    if (copy.type === 'arrow') {
      const a = copy as unknown as ArrowElement
      a.startX += 20; a.startY += 20; a.endX += 20; a.endY += 20
    } else {
      ;(copy as { x: number; y: number }).x += 20
      ;(copy as { x: number; y: number }).y += 20
    }
    addElement(copy as WbElement)
    setSelectedId(copy.id)
  }, [addElement, maxZ])
  // Duplicate a set; if they all share one group, the copies stay grouped under
  // a fresh groupId. (Use this instead of `ids.forEach(duplicateById)` — forEach
  // would pass the index as the second arg.)
  const duplicateMany = useCallback((ids: string[]) => {
    const gids = new Set(ids.map(id => elements.find(e => e.id === id)?.groupId).filter(Boolean))
    const newGid = ids.length > 1 && gids.size === 1 ? genId() : undefined
    ids.forEach(id => duplicateById(id, newGid))
  }, [elements, duplicateById])

  // ── Multi-selection operations (align / distribute / select-all) ────────────
  const selectAll = useCallback(() => setSelectedIds(elements.filter(e => 'x' in e).map(e => e.id)), [elements])
  const alignSelected = useCallback((mode: 'left' | 'hcenter' | 'right' | 'top' | 'vmiddle' | 'bottom', ids?: string[]) => {
    const sel = (ids ?? selectedIds).map(id => elements.find(e => e.id === id)).filter((e): e is StickyNoteEl => !!e && 'x' in e)
    if (sel.length < 2) return
    const minX = Math.min(...sel.map(e => e.x)), maxR = Math.max(...sel.map(e => e.x + e.width))
    const minY = Math.min(...sel.map(e => e.y)), maxB = Math.max(...sel.map(e => e.y + e.height))
    docRef.current.transact(() => {
      const m = docRef.current.getMap<WbElement>('elements')
      for (const e of sel) {
        const patch: Partial<StickyNoteEl> = {}
        if (mode === 'left') patch.x = minX
        else if (mode === 'right') patch.x = maxR - e.width
        else if (mode === 'hcenter') patch.x = (minX + maxR) / 2 - e.width / 2
        else if (mode === 'top') patch.y = minY
        else if (mode === 'bottom') patch.y = maxB - e.height
        else if (mode === 'vmiddle') patch.y = (minY + maxB) / 2 - e.height / 2
        m.set(e.id, { ...e, ...patch } as WbElement)
      }
    })
    setUndoStack(s => [...s, elements]); setRedoStack([])
  }, [selectedIds, elements])
  const distributeSelected = useCallback((axis: 'h' | 'v', ids?: string[]) => {
    const sel = (ids ?? selectedIds).map(id => elements.find(e => e.id === id)).filter((e): e is StickyNoteEl => !!e && 'x' in e)
    if (sel.length < 3) return
    const sorted = [...sel].sort((a, b) => (axis === 'h' ? a.x - b.x : a.y - b.y))
    const first = sorted[0], last = sorted[sorted.length - 1]
    const step = ((axis === 'h' ? last.x - first.x : last.y - first.y)) / (sorted.length - 1)
    docRef.current.transact(() => {
      const m = docRef.current.getMap<WbElement>('elements')
      sorted.forEach((e, i) => { if (i > 0 && i < sorted.length - 1) m.set(e.id, { ...e, ...(axis === 'h' ? { x: first.x + step * i } : { y: first.y + step * i }) } as WbElement) })
    })
    setUndoStack(s => [...s, elements]); setRedoStack([])
  }, [selectedIds, elements])

  // ── Grouping ────────────────────────────────────────────────────────────────
  // Expand a set of ids to include every sibling that shares a groupId, so a
  // group is always selected / moved / deleted as a single unit.
  const expandGroups = useCallback((ids: string[]) => {
    const gids = new Set(ids.map(id => elements.find(e => e.id === id)?.groupId).filter(Boolean) as string[])
    if (gids.size === 0) return ids
    const set = new Set(ids)
    for (const e of elements) if (e.groupId && gids.has(e.groupId)) set.add(e.id)
    return [...set]
  }, [elements])
  const canGroup = selectedIds.length >= 2
  const canUngroup = selectedIds.some(id => !!elements.find(e => e.id === id)?.groupId)
  const groupSelected = useCallback((arg?: unknown) => {
    const ids = Array.isArray(arg) ? arg as string[] : selectedIds
    if (ids.length < 2) return
    const gid = genId()
    docRef.current.transact(() => {
      const m = docRef.current.getMap<WbElement>('elements')
      for (const id of ids) { const e = m.get(id); if (e) m.set(id, { ...e, groupId: gid } as WbElement) }
    })
    setSelectedIds(ids)
    setUndoStack(s => [...s, elements]); setRedoStack([])
  }, [selectedIds, elements])
  const ungroupSelected = useCallback((arg?: unknown) => {
    const ids = Array.isArray(arg) ? arg as string[] : selectedIds
    const gids = new Set(ids.map(id => elements.find(e => e.id === id)?.groupId).filter(Boolean) as string[])
    if (gids.size === 0) return
    docRef.current.transact(() => {
      const m = docRef.current.getMap<WbElement>('elements')
      for (const e of elements) {
        if (e.groupId && gids.has(e.groupId)) {
          const next = { ...e } as WbElement & { groupId?: string }
          delete next.groupId
          m.set(e.id, next as WbElement)
        }
      }
    })
    setUndoStack(s => [...s, elements]); setRedoStack([])
  }, [selectedIds, elements])

  // ── Excalidraw import ───────────────────────────────────────────────────────
  // Parse a .excalidraw file and drop its elements/strokes centred in the current
  // viewport, on top of the existing content. Source of truth stays Yjs.
  const importExcalidrawFile = useCallback(async (file: File) => {
    let text: string
    try { text = await file.text() } catch { return }
    const { elements: els, strokes: strs, error } = excalidrawToWhiteboard(text, genId)
    if (error || (els.length === 0 && strs.length === 0)) return
    const vp = viewportRef.current, canvas = canvasRef.current
    const cc = canvas ? vp.screenToCanvas(canvas.width / 2, canvas.height / 2) : { x: 0, y: 0 }
    let nx = Infinity, ny = Infinity, xx = -Infinity, xy = -Infinity
    const acc = (x: number, y: number) => { nx = Math.min(nx, x); ny = Math.min(ny, y); xx = Math.max(xx, x); xy = Math.max(xy, y) }
    for (const e of els) {
      if (e.type === 'arrow') { const a = e as ArrowElement; acc(a.startX, a.startY); acc(a.endX, a.endY) }
      else { const b = e as unknown as { x: number; y: number; width: number; height: number }; acc(b.x, b.y); acc(b.x + b.width, b.y + b.height) }
    }
    for (const s of strs) for (let i = 0; i + 1 < s.points.length; i += 2) acc(s.points[i], s.points[i + 1])
    const dx = nx === Infinity ? 0 : cc.x - (nx + xx) / 2
    const dy = ny === Infinity ? 0 : cc.y - (ny + xy) / 2
    const baseZ = maxZ()
    const shifted = els.map((e, i) => {
      if (e.type === 'arrow') { const a = e as ArrowElement; return { ...a, startX: a.startX + dx, startY: a.startY + dy, endX: a.endX + dx, endY: a.endY + dy, zIndex: baseZ + 1 + i } }
      const b = e as unknown as { x: number; y: number; zIndex: number }
      return { ...b, x: b.x + dx, y: b.y + dy, zIndex: baseZ + 1 + i } as unknown as WbElement
    }) as WbElement[]
    docRef.current.transact(() => {
      const yMap = docRef.current.getMap<WbElement>('elements')
      for (const e of shifted) yMap.set(e.id, e)
      if (strs.length) {
        docRef.current.getArray<Stroke>('strokes').push(strs.map(s => ({ ...s, points: s.points.map((v, i) => (i % 2 === 0 ? v + dx : v + dy)) })))
      }
    })
    setUndoStack(s => [...s, [...elements, ...shifted]])
    setRedoStack([])
    setSelectedId(shifted[0]?.id ?? null)
  }, [elements, maxZ])

  // Snap a coordinate to the 10px grid when grid-snap is on.
  const snap = useCallback((v: number) => (snapGrid ? Math.round(v / 10) * 10 : v), [snapGrid])

  // ── Clipboard (copy / cut / paste a single element) ─────────────────────────
  const copySelected = useCallback(() => {
    const el = selectedId ? docRef.current.getMap<WbElement>('elements').get(selectedId) : null
    if (el) clipboardRef.current = el
  }, [selectedId])
  const pasteClipboard = useCallback(() => {
    const el = clipboardRef.current; if (!el) return
    const copy = { ...el, id: genId(), zIndex: maxZ() + 1 } as WbElement
    if (copy.type === 'arrow') { const a = copy as unknown as ArrowElement; a.startX += 20; a.startY += 20; a.endX += 20; a.endY += 20 }
    else { (copy as unknown as { x: number; y: number }).x += 20; (copy as unknown as { x: number; y: number }).y += 20 }
    addElement(copy); setSelectedId(copy.id); setShowProps(true)
  }, [addElement, maxZ])

  // ── Image insertion (decode → ImageElement centred in the viewport) ─────────
  const insertImageFile = useCallback((file: File) => {
    const reader = new FileReader()
    reader.onload = () => {
      const src = reader.result as string
      const im = new Image()
      im.onload = () => {
        const sc = Math.min(1, 400 / (im.naturalWidth || 400))
        const w = (im.naturalWidth || 200) * sc, h = (im.naturalHeight || 200) * sc
        const vp = viewportRef.current, canvas = canvasRef.current
        const cc = canvas ? vp.screenToCanvas(canvas.width / 2, canvas.height / 2) : { x: 0, y: 0 }
        const id = genId()
        imgCacheRef.current.set(id, im)
        addElement({ id, type: 'image', src, x: cc.x - w / 2, y: cc.y - h / 2, width: w, height: h, natural_width: im.naturalWidth, natural_height: im.naturalHeight, rotation: 0, opacity: 1, zIndex: maxZ() + 1, locked: false } as ImageElement)
        setSelectedId(id); setShowProps(true)
      }
      im.src = src
    }
    reader.readAsDataURL(file)
  }, [addElement, maxZ])

  // Export the whole board (all elements + strokes) to a PNG, rendered off-screen
  // at 2× from the content bounding box (not just the visible viewport).
  const exportPng = useCallback(() => {
    if (elements.length === 0 && strokes.length === 0) return
    let nx = Infinity, ny = Infinity, xx = -Infinity, xy = -Infinity
    const acc = (x: number, y: number) => { nx = Math.min(nx, x); ny = Math.min(ny, y); xx = Math.max(xx, x); xy = Math.max(xy, y) }
    for (const el of elements) {
      if (el.type === 'arrow') { const a = el as ArrowElement; acc(a.startX, a.startY); acc(a.endX, a.endY) }
      else { const b = el as unknown as { x: number; y: number; width: number; height: number }; acc(b.x, b.y); acc(b.x + b.width, b.y + b.height) }
    }
    for (const s of strokes) for (let i = 0; i + 1 < s.points.length; i += 2) acc(s.points[i], s.points[i + 1])
    if (nx === Infinity) return
    const pad = 24, scale = 2
    const w = Math.max(1, Math.ceil(xx - nx) + pad * 2), h = Math.max(1, Math.ceil(xy - ny) + pad * 2)
    const oc = document.createElement('canvas'); oc.width = w * scale; oc.height = h * scale
    const ctx = oc.getContext('2d'); if (!ctx) return
    ctx.scale(scale, scale)
    ctx.fillStyle = '#ffffff'; ctx.fillRect(0, 0, w, h)
    ctx.translate(pad - nx, pad - ny)
    const map = new Map(elements.map(e => [e.id, e]))
    for (const el of [...elements].sort((a, b) => ('zIndex' in a ? a.zIndex : 0) - ('zIndex' in b ? b.zIndex : 0))) {
      ctx.save()
      ctx.globalAlpha = ('opacity' in el ? (el as StickyNoteEl).opacity : 1) ?? 1
      if ('rotation' in el && (el as StickyNoteEl).rotation) {
        const cx = (el as StickyNoteEl).x + (el as StickyNoteEl).width / 2, cy = (el as StickyNoteEl).y + (el as StickyNoteEl).height / 2
        ctx.translate(cx, cy); ctx.rotate(((el as StickyNoteEl).rotation * Math.PI) / 180); ctx.translate(-cx, -cy)
      }
      switch (el.type) {
        case 'sticky': renderStickyNote(ctx, el as StickyNoteEl, 1); break
        case 'text':   renderTextBox(ctx, el as TextBox, 1); break
        case 'shape':  renderShape(ctx, el as ShapeElement); break
        case 'arrow':  renderArrow(ctx, el as ArrowElement, map); break
        case 'frame':  renderFrame(ctx, el as FrameElement, 1); break
        case 'image': { const im = el as ImageElement; const c = imgCacheRef.current.get(im.id); if (c && c.complete && c.naturalWidth) ctx.drawImage(c, im.x, im.y, im.width, im.height); break }
      }
      ctx.restore()
    }
    for (const s of strokes) renderStroke(ctx, s)
    oc.toBlob(blob => {
      if (!blob) return
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a'); a.href = url; a.download = `${(board?.title || 'tableau').replace(/[^\w.-]+/g, '_')}.png`; a.click()
      URL.revokeObjectURL(url)
    }, 'image/png')
  }, [elements, strokes, board?.title])

  // Export the whole board to a vector SVG (shapes, text, arrows, images, strokes).
  const exportSvg = useCallback(() => {
    if (elements.length === 0 && strokes.length === 0) return
    let nx = Infinity, ny = Infinity, xx = -Infinity, xy = -Infinity
    const acc = (x: number, y: number) => { nx = Math.min(nx, x); ny = Math.min(ny, y); xx = Math.max(xx, x); xy = Math.max(xy, y) }
    for (const el of elements) { if (el.type === 'arrow') { const a = el as ArrowElement; acc(a.startX, a.startY); acc(a.endX, a.endY) } else { const b = el as unknown as { x: number; y: number; width: number; height: number }; acc(b.x, b.y); acc(b.x + b.width, b.y + b.height) } }
    for (const s of strokes) for (let i = 0; i + 1 < s.points.length; i += 2) acc(s.points[i], s.points[i + 1])
    if (nx === Infinity) return
    const pad = 24, w = Math.ceil(xx - nx) + pad * 2, h = Math.ceil(xy - ny) + pad * 2
    const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    const out: string[] = [`<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">`,
      `<defs><marker id="wbarrow" markerWidth="10" markerHeight="10" refX="8" refY="3" orient="auto" markerUnits="strokeWidth"><path d="M0,0 L8,3 L0,6 z" fill="#202124"/></marker></defs>`,
      `<rect width="${w}" height="${h}" fill="#ffffff"/>`, `<g transform="translate(${pad - nx},${pad - ny})">`]
    for (const el of [...elements].sort((a, b) => ('zIndex' in a ? a.zIndex : 0) - ('zIndex' in b ? b.zIndex : 0))) {
      const op = ('opacity' in el ? (el as StickyNoteEl).opacity : 1) ?? 1
      const rot = 'rotation' in el ? (el as StickyNoteEl).rotation || 0 : 0
      const tr = rot && 'x' in el ? ` transform="rotate(${rot} ${(el as StickyNoteEl).x + (el as StickyNoteEl).width / 2} ${(el as StickyNoteEl).y + (el as StickyNoteEl).height / 2})"` : ''
      if (el.type === 'shape') {
        const s = el as ShapeElement, fill = s.fill ?? '#BBDEFB', st = s.stroke ?? '#1a73e8', sw = s.strokeWidth ?? 2
        if (s.kind === 'rect') out.push(`<rect x="${s.x}" y="${s.y}" width="${s.width}" height="${s.height}" rx="6" fill="${fill}" stroke="${st}" stroke-width="${sw}" opacity="${op}"${tr}/>`)
        else if (s.kind === 'circle') out.push(`<ellipse cx="${s.x + s.width / 2}" cy="${s.y + s.height / 2}" rx="${s.width / 2}" ry="${s.height / 2}" fill="${fill}" stroke="${st}" stroke-width="${sw}" opacity="${op}"${tr}/>`)
        else { let pts = ''
          if (s.kind === 'triangle') pts = `${s.x + s.width / 2},${s.y} ${s.x + s.width},${s.y + s.height} ${s.x},${s.y + s.height}`
          else if (s.kind === 'diamond') pts = `${s.x + s.width / 2},${s.y} ${s.x + s.width},${s.y + s.height / 2} ${s.x + s.width / 2},${s.y + s.height} ${s.x},${s.y + s.height / 2}`
          else { const ox = s.x + s.width / 2, oy = s.y + s.height / 2, oR = Math.min(s.width, s.height) / 2, iR = oR * 0.4, ar: string[] = []; for (let i = 0; i < 10; i++) { const r = i % 2 === 0 ? oR : iR, an = i * Math.PI / 5 - Math.PI / 2; ar.push(`${ox + r * Math.cos(an)},${oy + r * Math.sin(an)}`) } pts = ar.join(' ') }
          out.push(`<polygon points="${pts}" fill="${fill}" stroke="${st}" stroke-width="${sw}" opacity="${op}"${tr}/>`) }
      } else if (el.type === 'sticky') { const s = el as StickyNoteEl, bg = STICKY_COLORS[s.color as keyof typeof STICKY_COLORS] ?? '#fff59d'
        out.push(`<rect x="${s.x}" y="${s.y}" width="${s.width}" height="${s.height}" rx="2" fill="${bg}" opacity="${op}"${tr}/>`)
        if (s.text) out.push(`<text x="${s.x + 10}" y="${s.y + 24}" font-family="Arial" font-size="${s.fontSize ?? 14}" fill="#202124">${esc(s.text).slice(0, 200)}</text>`)
      } else if (el.type === 'text') { const tb = el as TextBox, anchor = tb.textAlign === 'center' ? 'middle' : tb.textAlign === 'right' ? 'end' : 'start', tx = tb.textAlign === 'center' ? tb.x + tb.width / 2 : tb.textAlign === 'right' ? tb.x + tb.width : tb.x
        out.push(`<text x="${tx}" y="${tb.y + (tb.fontSize ?? 16)}" font-family="Arial" font-size="${tb.fontSize ?? 16}" fill="${tb.color ?? '#202124'}" text-anchor="${anchor}" font-weight="${tb.fontWeight === 'bold' ? 'bold' : 'normal'}" opacity="${op}"${tr}>${esc(tb.text ?? '')}</text>`)
      } else if (el.type === 'arrow') { const a = el as ArrowElement
        out.push(`<line x1="${a.startX}" y1="${a.startY}" x2="${a.endX}" y2="${a.endY}" stroke="${a.color ?? '#202124'}" stroke-width="${a.width ?? 2}" marker-end="url(#wbarrow)" opacity="${op}"/>`)
      } else if (el.type === 'image') { const im = el as ImageElement; out.push(`<image href="${im.src}" x="${im.x}" y="${im.y}" width="${im.width}" height="${im.height}" opacity="${op}"${tr}/>`)
      } else if (el.type === 'frame') { const f = el as FrameElement; out.push(`<rect x="${f.x}" y="${f.y}" width="${f.width}" height="${f.height}" fill="none" stroke="${f.color ?? '#9aa0a6'}" stroke-width="2" stroke-dasharray="6 4"/>`); if (f.title) out.push(`<text x="${f.x}" y="${f.y - 6}" font-family="Arial" font-size="12" fill="#5f6368">${esc(f.title)}</text>`) }
    }
    for (const s of strokes) { if (s.points.length < 4) continue; let d = `M ${s.points[0]} ${s.points[1]}`; for (let i = 2; i + 1 < s.points.length; i += 2) d += ` L ${s.points[i]} ${s.points[i + 1]}`; out.push(`<path d="${d} Z" fill="${s.color ?? '#202124'}" opacity="${s.opacity ?? 1}"/>`) }
    out.push('</g></svg>')
    const blob = new Blob([out.join('')], { type: 'image/svg+xml' })
    const url = URL.createObjectURL(blob); const a = document.createElement('a'); a.href = url; a.download = `${(board?.title || 'tableau').replace(/[^\w.-]+/g, '_')}.svg`; a.click(); URL.revokeObjectURL(url)
  }, [elements, strokes, board?.title])

  // Right-click anywhere on the canvas → contextual actions (element or board).
  const openContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    const { x: cx, y: cy } = eventToCanvas(e)
    const hit = hitTest(cx, cy, elements, 4 / viewportRef.current.scale)
    const items: MenuItem[] = []
    if (hit) {
      // Effective selection for the menu: a whole group if the clicked element
      // belongs to one, an existing multi-selection if the click is inside it,
      // otherwise just the clicked element.
      const grouped = elements.find(x => x.id === hit)?.groupId ? expandGroups([hit]) : null
      const sel = grouped && grouped.length > 1
        ? grouped
        : (selectedIds.includes(hit) && selectedIds.length > 1 ? selectedIds : [hit])
      const multi = sel.length > 1
      setSelectedIds(sel)
      setShowProps(true)
      if (multi) {
        const isGrouped = sel.some(id => !!elements.find(x => x.id === id)?.groupId)
        const alignItem = (mode: 'left' | 'hcenter' | 'right' | 'top' | 'vmiddle' | 'bottom', label: string) =>
          ({ type: 'action' as const, label, onClick: () => alignSelected(mode, sel) })
        items.push(
          { type: 'action', label: t('wb_ctx_duplicate', { defaultValue: 'Dupliquer' }), shortcut: 'Ctrl+D', icon: <ClipboardPaste size={14} />, onClick: () => duplicateMany(sel) },
          { type: 'action', label: t('wb_group', { defaultValue: 'Grouper' }), shortcut: 'Ctrl+G', icon: <Group size={14} />, onClick: () => groupSelected(sel) },
          ...(isGrouped ? [{ type: 'action' as const, label: t('wb_ungroup', { defaultValue: 'Dissocier' }), shortcut: 'Ctrl+Maj+G', icon: <Ungroup size={14} />, onClick: () => ungroupSelected(sel) }] : []),
          { type: 'separator' },
          { type: 'submenu', label: t('wb_grp_align', { defaultValue: 'Alignement' }), icon: <AlignCenterVertical size={14} />, items: [
            alignItem('left', t('wb_align_left', { defaultValue: 'Aligner à gauche' })),
            alignItem('hcenter', t('wb_align_hcenter', { defaultValue: 'Centrer horizontalement' })),
            alignItem('right', t('wb_align_right', { defaultValue: 'Aligner à droite' })),
            alignItem('top', t('wb_align_top', { defaultValue: 'Aligner en haut' })),
            alignItem('vmiddle', t('wb_align_vmiddle', { defaultValue: 'Centrer verticalement' })),
            alignItem('bottom', t('wb_align_bottom', { defaultValue: 'Aligner en bas' })),
          ] },
          { type: 'submenu', label: t('wb_grp_distribute', { defaultValue: 'Répartition' }), icon: <AlignHorizontalDistributeCenter size={14} />, disabled: sel.length < 3, items: [
            { type: 'action', label: t('wb_distribute_h', { defaultValue: 'Répartir horizontalement' }), onClick: () => distributeSelected('h', sel) },
            { type: 'action', label: t('wb_distribute_v', { defaultValue: 'Répartir verticalement' }), onClick: () => distributeSelected('v', sel) },
          ] },
          { type: 'separator' },
          { type: 'action', label: t('wb_ctx_front', { defaultValue: 'Mettre au premier plan' }), icon: <BringToFront size={14} />, onClick: () => sel.forEach(bringToFront) },
          { type: 'action', label: t('wb_ctx_back', { defaultValue: 'Mettre à l’arrière-plan' }), icon: <SendToBack size={14} />, onClick: () => sel.forEach(sendToBack) },
          { type: 'separator' },
          { type: 'action', label: t('wb_ctx_delete_n', { defaultValue: 'Supprimer ({{count}})', count: sel.length }), danger: true, icon: <Trash2 size={14} />, onClick: () => deleteSelected(sel) },
        )
      } else {
      const el = elements.find(x => x.id === hit)
      const locked = !!(el && 'locked' in el && (el as { locked?: boolean }).locked)
      items.push(
        { type: 'action', label: t('wb_ctx_copy', { defaultValue: 'Copier' }), shortcut: 'Ctrl+C', icon: <Copy size={14} />, onClick: () => { const el = docRef.current.getMap<WbElement>('elements').get(hit); if (el) clipboardRef.current = el } },
        { type: 'action', label: t('wb_ctx_duplicate', { defaultValue: 'Dupliquer' }), shortcut: 'Ctrl+D', icon: <ClipboardPaste size={14} />, onClick: () => duplicateById(hit) },
        { type: 'action', label: t('wb_ctx_rotate', { defaultValue: 'Pivoter de 90°' }), icon: <RotateCw size={14} />, onClick: () => { const el = elements.find(x => x.id === hit); if (el && 'rotation' in el) updateElement(hit, { rotation: (((el as StickyNoteEl).rotation || 0) + 90) % 360 } as Partial<WbElement>) } },
        { type: 'separator' },
        { type: 'action', label: t('wb_ctx_front', { defaultValue: 'Mettre au premier plan' }), icon: <BringToFront size={14} />, onClick: () => bringToFront(hit) },
        { type: 'action', label: t('wb_ctx_back', { defaultValue: 'Mettre à l’arrière-plan' }), icon: <SendToBack size={14} />, onClick: () => sendToBack(hit) },
        { type: 'separator' },
        { type: 'action', label: locked ? t('wb_ctx_unlock', { defaultValue: 'Déverrouiller' }) : t('wb_ctx_lock', { defaultValue: 'Verrouiller' }), icon: locked ? <Unlock size={14} /> : <Lock size={14} />, onClick: () => setLocked(hit, !locked) },
        { type: 'separator' },
        { type: 'action', label: t('common_delete', { defaultValue: 'Supprimer' }), danger: true, icon: <Trash2 size={14} />, onClick: () => deleteById(hit) },
      )
      }
    } else {
      setSelectedId(null)
      items.push(
        { type: 'action', label: t('wb_ctx_paste', { defaultValue: 'Coller' }), shortcut: 'Ctrl+V', disabled: !clipboardRef.current, icon: <ClipboardPaste size={14} />, onClick: pasteClipboard },
        { type: 'action', label: t('wb_insert_image', { defaultValue: 'Insérer une image…' }), icon: <ImageIcon size={14} />, onClick: () => imgFileInputRef.current?.click() },
        { type: 'action', label: t('wb_ctx_import_excalidraw', { defaultValue: 'Importer un fichier Excalidraw…' }), icon: <Upload size={14} />, onClick: () => fileInputRef.current?.click() },
        { type: 'action', label: t('wb_ctx_export_png', { defaultValue: 'Exporter en PNG' }), icon: <Download size={14} />, onClick: exportPng },
        { type: 'action', label: t('wb_ctx_export_svg', { defaultValue: 'Exporter en SVG' }), icon: <Download size={14} />, onClick: exportSvg },
        { type: 'separator' },
        { type: 'submenu', label: t('wb_ctx_background', { defaultValue: 'Arrière-plan' }), items: (['dots', 'grid', 'lines', 'white'] as Background[]).map(bg => ({ type: 'action' as const, label: t(`wb_bg_${bg}`, { defaultValue: bg }), checked: background === bg, onClick: () => { setBackground(bg); boardsApi.update(boardId, { background: bg }) } })) },
        { type: 'separator' },
        { type: 'action', label: t('wb_ctx_fit', { defaultValue: 'Ajuster à l’écran' }), icon: <Maximize2 size={14} />, onClick: () => { const c = canvasRef.current; if (c) { viewportRef.current.fitToElements(elements as never, c.width, c.height); setZoom(viewportRef.current.zoomPercent) } } },
      )
    }
    setCtxMenu({ pos: { top: e.clientY, left: e.clientX, minWidth: 220 }, items })
  }, [elements, background, boardId, eventToCanvas, t, selectedIds, expandGroups, groupSelected, ungroupSelected, duplicateById, duplicateMany, bringToFront, sendToBack, setLocked, deleteById, deleteSelected, alignSelected, distributeSelected, exportPng, exportSvg, updateElement, pasteClipboard])

  // ── Pointer events ─────────────────────────────────────────────────────────

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
      // 2) Sinon : sélection / déplacement (multi-sélection avec Maj + lasso)
      const hit = hitTest(cx, cy, elements, 4 / vp.scale)
      if (hit) {
        const already = selectedIds.includes(hit)
        let nextSel: string[]
        if (e.shiftKey) nextSel = already ? selectedIds.filter(i => i !== hit) : [...selectedIds, hit]
        else nextSel = already ? selectedIds : [hit]   // keep an existing multi-selection to drag it
        nextSel = expandGroups(nextSel)                 // selecting one member selects the whole group
        setSelectedIds(nextSel)
        setShowProps(true)
        // Move every selected (positionable) element together.
        const origs = nextSel
          .map(id => elements.find(e2 => e2.id === id))
          .filter((el): el is StickyNoteEl => !!el && 'x' in el)
          .map(el => ({ id: el.id, x: el.x, y: el.y }))
        if (origs.length) dragRef.current = { startX: e.clientX, startY: e.clientY, origs }
      } else {
        if (!e.shiftKey) { setSelectedIds([]); setShowProps(false) }
        boxSelRef.current = { startX: cx, startY: cy, curX: cx, curY: cy }   // rubber-band select
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

    if (tool === 'arrow') {
      newArrowRef.current = { startX: cx, startY: cy, curX: cx, curY: cy }
      return
    }

    if (tool === 'frame') {
      newFrameRef.current = { startX: cx, startY: cy, curX: cx, curY: cy }
      return
    }

    if (tool === 'pen' || tool === 'eraser') {
      penRef.current = { points: [cx, cy], id: genId() }
      return
    }
  }, [tool, elements, stickyColor, addElement, eventToCanvas, selectedId, selectedIds, expandGroups, enterTextEdit])

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
      for (const o of dragRef.current.origs) updateElement(o.id, { x: snap(o.x + dx), y: snap(o.y + dy) } as Partial<WbElement>)
      return
    }

    if (boxSelRef.current) {
      boxSelRef.current.curX = cx
      boxSelRef.current.curY = cy
      return
    }

    if (newShapeRef.current) {
      newShapeRef.current.curX = cx
      newShapeRef.current.curY = cy
      return
    }

    if (newArrowRef.current) {
      newArrowRef.current.curX = cx
      newArrowRef.current.curY = cy
      return
    }

    if (newFrameRef.current) {
      newFrameRef.current.curX = cx
      newFrameRef.current.curY = cy
      return
    }

    if (penRef.current) {
      penRef.current.points.push(cx, cy)
      return
    }
  }, [updateElement, eventToCanvas, tool, selectedId, elements, awareness, snap, expandGroups])

  const onPointerUp = useCallback((e: React.PointerEvent) => {
    panRef.current  = null

    // Rubber-band lasso: select every element whose box intersects the rectangle.
    if (boxSelRef.current) {
      const { startX, startY, curX, curY } = boxSelRef.current
      boxSelRef.current = null
      const x0 = Math.min(startX, curX), y0 = Math.min(startY, curY), x1 = Math.max(startX, curX), y1 = Math.max(startY, curY)
      if (Math.abs(x1 - x0) > 4 && Math.abs(y1 - y0) > 4) {
        const inBox = elements.filter(el => 'x' in el && (el as StickyNoteEl).x < x1 && (el as StickyNoteEl).x + (el as StickyNoteEl).width > x0 && (el as StickyNoteEl).y < y1 && (el as StickyNoteEl).y + (el as StickyNoteEl).height > y0).map(el => el.id)
        setSelectedIds(prev => expandGroups(e.shiftKey ? Array.from(new Set([...prev, ...inBox])) : inBox))
        if (inBox.length) setShowProps(true)
      }
      return
    }

    if (resizeRef.current) {
      resizeRef.current = null
      setUndoStack(s => [...s, elements])
      setRedoStack([])
      return
    }

    if (dragRef.current) {
      const dx = (e.clientX - dragRef.current.startX) / viewportRef.current.scale
      const dy = (e.clientY - dragRef.current.startY) / viewportRef.current.scale
      for (const o of dragRef.current.origs) updateElement(o.id, { x: snap(Math.round(o.x + dx)), y: snap(Math.round(o.y + dy)) } as Partial<WbElement>)
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
        x: snap(Math.min(startX, cx)), y: snap(Math.min(startY, cy)), width: snap(w), height: snap(h),
        fill: '#BBDEFB', stroke: '#1a73e8', strokeWidth: 2,
        rotation: 0, opacity: 1, zIndex: elements.length, locked: false,
      } as ShapeElement)
      setSelectedId(id)
      setShowProps(true)
      setTool('select')
      return
    }

    if (tool === 'arrow' && newArrowRef.current) {
      const { x: cx, y: cy } = eventToCanvas(e)
      const { startX, startY } = newArrowRef.current
      newArrowRef.current = null
      if (Math.hypot(cx - startX, cy - startY) < 10) return
      const id = genId()
      addElement({
        id, type: 'arrow', startX: snap(startX), startY: snap(startY), endX: snap(cx), endY: snap(cy),
        color: '#202124', width: 2, style: 'straight', startArrow: 'none', endArrow: 'triangle',
        zIndex: elements.length, opacity: 1,
      } as ArrowElement)
      setSelectedId(id)
      setShowProps(true)
      setTool('select')
      return
    }

    if (tool === 'frame' && newFrameRef.current) {
      const { x: cx, y: cy } = eventToCanvas(e)
      const { startX, startY } = newFrameRef.current
      newFrameRef.current = null
      const w = Math.abs(cx - startX), h = Math.abs(cy - startY)
      if (w < 20 || h < 20) return
      const id = genId()
      addElement({
        id, type: 'frame',
        x: snap(Math.min(startX, cx)), y: snap(Math.min(startY, cy)), width: snap(w), height: snap(h),
        title: t('wb_frame_default', { defaultValue: 'Cadre' }), color: '#9aa0a6',
        rotation: 0, opacity: 1, zIndex: elements.length, locked: false,
      } as FrameElement)
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
  }, [tool, elements, addElement, updateElement, eventToCanvas, shapeKind, snap, t])

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
      if ((e.metaKey || e.ctrlKey) && (e.key === 'd' || e.key === 'D')) { e.preventDefault(); duplicateMany(selectedIds) }
      if ((e.metaKey || e.ctrlKey) && e.key === ']') { e.preventDefault(); selectedIds.forEach(bringToFront) }
      if ((e.metaKey || e.ctrlKey) && e.key === '[') { e.preventDefault(); selectedIds.forEach(sendToBack) }
      if ((e.metaKey || e.ctrlKey) && (e.key === 'g' || e.key === 'G')) { e.preventDefault(); if (e.shiftKey) ungroupSelected(); else groupSelected() }
      // Clipboard — only when focus is on the board (never while editing text).
      const onBody = document.activeElement === document.body
      if ((e.metaKey || e.ctrlKey) && (e.key === 'a' || e.key === 'A') && onBody) { e.preventDefault(); selectAll() }
      if ((e.metaKey || e.ctrlKey) && (e.key === 'c' || e.key === 'C') && onBody) { e.preventDefault(); copySelected() }
      if ((e.metaKey || e.ctrlKey) && (e.key === 'x' || e.key === 'X') && onBody) { e.preventDefault(); copySelected(); deleteSelected() }
      if ((e.metaKey || e.ctrlKey) && (e.key === 'v' || e.key === 'V') && onBody) { e.preventDefault(); pasteClipboard() }
      // Raccourcis d'outils à une touche : ignorés si on tape dans un champ.
      const ae = document.activeElement as HTMLElement | null
      if (e.metaKey || e.ctrlKey || e.altKey) return
      if (ae && (ae.tagName === 'INPUT' || ae.tagName === 'TEXTAREA' || ae.isContentEditable)) return
      if (e.key === 'v') setTool('select')
      if (e.key === 'h') setTool('hand')
      if (e.key === 'p') setTool('pen')
      if (e.key === 't') setTool('text')
      if (e.key === 's') setTool('sticky')
      if (e.key === 'a') setTool('arrow')
      if (e.key === 'e') setTool('eraser')
      if (e.key === 'f') setTool('frame')
      if (e.key === 'r') { setShapeKind('rect');   setTool('shape') }
      if (e.key === 'o') { setShapeKind('circle'); setTool('shape') }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [deleteSelected, undo, redo, selectedIds, selectAll, duplicateById, duplicateMany, bringToFront, sendToBack, copySelected, pasteClipboard, groupSelected, ungroupSelected])

  const selectedEl = elements.find(e => e.id === selectedId)

  // API exposée aux macros (objet global `Kubuno`). Lecture seule pour cette
  // première version : on ne mute pas encore le tableau depuis une macro.
  const makeApi = () => {
    const Board = {
      /** Nombre d'objets sur le tableau. */
      getObjectCount: () => elements.length,
      /** Identifiants sélectionnés (0 ou 1 pour l'instant). */
      getSelection: () => [...selectedIds],
      /** Liste des objets ({ id, type }). */
      getObjects: () => elements.map(e => ({ id: e.id, type: e.type })),
    }
    const App = {
      getType: () => 'whiteboard',
      getId: () => boardId,
      toast: (msg: unknown) => console.log(String(msg)),
      log: (msg: unknown) => console.log(String(msg)),
    }
    return { Board, App }
  }

  // Docked panels (replace the old home-made right panel): Properties + Layers.
  const wbPanels: Record<string, DockPanel> = {
    properties: {
      label: t('wb_panel_properties', { defaultValue: 'Propriétés' }),
      render: () => selectedEl
        ? <PropertiesPanel element={selectedEl} onUpdate={p => updateElement(selectedId!, p as Partial<WbElement>)} onDelete={deleteSelected} />
        : <div className="p-4 text-sm text-[#80868b]">{t('wb_no_selection', { defaultValue: 'Sélectionnez un élément pour voir ses propriétés.' })}</div>,
    },
    layers: {
      label: t('wb_panel_layers', { defaultValue: 'Calques' }),
      render: () => <LayersPanel
        elements={elements} selectedIds={selectedIds}
        onSelect={(id, additive) => {
          setSelectedIds(prev => additive ? (prev.includes(id) ? prev.filter(i => i !== id) : [...prev, id]) : [id])
          setShowProps(true)
        }}
        onToggleLock={setLocked} onDelete={deleteById} onFront={bringToFront} onBack={sendToBack} />,
    },
  }

  // Onglet « Fichier » (backstage façon Office) — TOUJOURS en 1ʳᵉ position du ruban.
  const { fileTab, activeTabId, onTabChange } = useFileTab({
    theme: THEME_WHITEBOARD,
    labels: backstageLabels(t),
    startContent: <WhiteboardStartContent onOpen={onOpen} />,
    defaultTab: 'home',
    doc: {
      info: (
        <InfoPanel
          title={board?.title || t('common_untitled', { defaultValue: 'Sans titre' })}
          subtitle={t('wb_whiteboards', { defaultValue: 'Tableau blanc' })}
          rows={[
            [t('office_bs_info_type', { defaultValue: 'Type' }), t('wb_whiteboards', { defaultValue: 'Tableau blanc' })],
            [t('wb_grp_objects', { defaultValue: 'Objets' }), elements.length],
            ...(board?.updated_at
              ? [[t('office_bs_info_modified', { defaultValue: 'Modifié le' }), format(new Date(board.updated_at), 'd MMM yyyy', { locale: getDateLocale(i18n.language) })] as [string, string]]
              : []),
          ]}
        />
      ),
      onPrint: () => window.print(),
      onClose: onBack,
    },
  })

  return (
    <OfficeShell
      ribbon={[
        fileTab,
        { id: 'home', label: t('doc_tab_home', { defaultValue: 'Accueil' }), groups: [
          { id: 'board', label: t('wb_grp_board', { defaultValue: 'Tableau' }), items: [
            { id: 'new', kind: 'button', size: 'small', label: t('doc_new', { defaultValue: 'Nouveau' }), icon: <FilePlus size={16} />, onClick: () => createBoardMut.mutate() },
            { id: 'dup-board', kind: 'button', size: 'small', label: t('doc_duplicate', { defaultValue: 'Dupliquer' }), icon: <CopyPlus size={16} />, onClick: () => dupBoardMut.mutate() },
          ] },
          { id: 'edit', label: t('wb_grp_edit', { defaultValue: 'Édition' }), items: [
            { id: 'undo', kind: 'button', size: 'small', label: t('wb_undo', { defaultValue: 'Annuler' }), icon: <RotateCcw size={16} />, onClick: undo },
            { id: 'redo', kind: 'button', size: 'small', label: t('wb_redo', { defaultValue: 'Rétablir' }), icon: <RotateCw size={16} />, onClick: redo },
            { id: 'dup', kind: 'button', size: 'small', label: t('wb_ctx_duplicate', { defaultValue: 'Dupliquer' }), icon: <Copy size={16} />, disabled: selectedIds.length === 0, onClick: () => duplicateMany(selectedIds) },
            { id: 'del', kind: 'button', size: 'small', label: t('common_delete', { defaultValue: 'Supprimer' }), icon: <Trash2 size={16} />, disabled: selectedIds.length === 0, onClick: deleteSelected },
          ] },
        ] },
        { id: 'insert', label: t('tab_insert', { defaultValue: 'Insertion' }), groups: [
          { id: 'wb-tools', label: t('wb_grp_objects', { defaultValue: 'Objets' }), items: [
            { id: 'i-sticky', kind: 'button', size: 'large', label: t('wb_tool_sticky', { defaultValue: 'Post-it' }), icon: <StickyNote size={18} />, active: tool === 'sticky', onClick: () => setTool('sticky') },
            { id: 'i-text', kind: 'button', size: 'large', label: t('wb_tool_text', { defaultValue: 'Texte' }), icon: <Type size={18} />, active: tool === 'text', onClick: () => setTool('text') },
            { id: 'i-shape', kind: 'button', size: 'large', label: t('wb_tool_shape', { defaultValue: 'Forme' }), icon: <Square size={18} />, active: tool === 'shape', onClick: () => setTool('shape') },
            { id: 'i-arrow', kind: 'button', size: 'large', label: t('wb_tool_arrow', { defaultValue: 'Flèche' }), icon: <ArrowRight size={18} />, active: tool === 'arrow', onClick: () => setTool('arrow') },
            { id: 'i-pen', kind: 'button', size: 'large', label: t('wb_tool_pen', { defaultValue: 'Stylo' }), icon: <Pen size={18} />, active: tool === 'pen', onClick: () => setTool('pen') },
            { id: 'i-frame', kind: 'button', size: 'large', label: t('wb_tool_frame', { defaultValue: 'Cadre' }), icon: <FrameIcon size={18} />, active: tool === 'frame', onClick: () => setTool('frame') },
          ] },
          { id: 'wb-media', label: t('wb_grp_media', { defaultValue: 'Média' }), items: [
            { id: 'ins-image', kind: 'button', size: 'large', label: t('wb_insert_image', { defaultValue: 'Image' }), icon: <ImageIcon size={18} />, onClick: () => imgFileInputRef.current?.click() },
          ] },
          { id: 'wb-io', label: t('wb_grp_io', { defaultValue: 'Importer / Exporter' }), items: [
            { id: 'imp-excalidraw', kind: 'button', size: 'large', label: 'Excalidraw', icon: <Upload size={18} />, tooltip: t('wb_ctx_import_excalidraw', { defaultValue: 'Importer un fichier Excalidraw…' }), onClick: () => fileInputRef.current?.click() },
            { id: 'exp-png', kind: 'button', size: 'large', label: t('wb_export_png', { defaultValue: 'Exporter PNG' }), icon: <Download size={18} />, onClick: exportPng },
            { id: 'exp-svg', kind: 'button', size: 'large', label: t('wb_export_svg', { defaultValue: 'Exporter SVG' }), icon: <Download size={18} />, onClick: exportSvg },
          ] },
        ] },
        { id: 'arrange', label: t('wb_tab_arrange', { defaultValue: 'Disposition' }), groups: [
          { id: 'order', label: t('wb_grp_order', { defaultValue: 'Ordre' }), items: [
            { id: 'front', kind: 'button', size: 'small', label: t('wb_ctx_front', { defaultValue: 'Premier plan' }), icon: <BringToFront size={16} />, disabled: selectedIds.length === 0, onClick: () => selectedIds.forEach(bringToFront) },
            { id: 'back', kind: 'button', size: 'small', label: t('wb_ctx_back', { defaultValue: 'Arrière-plan' }), icon: <SendToBack size={16} />, disabled: selectedIds.length === 0, onClick: () => selectedIds.forEach(sendToBack) },
            { id: 'rot', kind: 'button', size: 'small', label: t('wb_ctx_rotate', { defaultValue: 'Pivoter 90°' }), icon: <RotateCw size={16} />, disabled: !selectedId, onClick: () => { const el = selectedEl; if (el && 'rotation' in el) updateElement(selectedId!, { rotation: (((el as StickyNoteEl).rotation || 0) + 90) % 360 } as Partial<WbElement>) } },
          ] },
          { id: 'align', label: t('wb_grp_align', { defaultValue: 'Alignement' }), items: [
            { id: 'al-left', kind: 'button', size: 'small', label: t('wb_align_left', { defaultValue: 'Gauche' }), icon: <AlignStartVertical size={16} />, disabled: selectedIds.length < 2, onClick: () => alignSelected('left') },
            { id: 'al-hcenter', kind: 'button', size: 'small', label: t('wb_align_hcenter', { defaultValue: 'Centre' }), icon: <AlignCenterVertical size={16} />, disabled: selectedIds.length < 2, onClick: () => alignSelected('hcenter') },
            { id: 'al-right', kind: 'button', size: 'small', label: t('wb_align_right', { defaultValue: 'Droite' }), icon: <AlignEndVertical size={16} />, disabled: selectedIds.length < 2, onClick: () => alignSelected('right') },
            { id: 'al-top', kind: 'button', size: 'small', label: t('wb_align_top', { defaultValue: 'Haut' }), icon: <AlignStartHorizontal size={16} />, disabled: selectedIds.length < 2, onClick: () => alignSelected('top') },
            { id: 'al-vmiddle', kind: 'button', size: 'small', label: t('wb_align_vmiddle', { defaultValue: 'Milieu' }), icon: <AlignCenterHorizontal size={16} />, disabled: selectedIds.length < 2, onClick: () => alignSelected('vmiddle') },
            { id: 'al-bottom', kind: 'button', size: 'small', label: t('wb_align_bottom', { defaultValue: 'Bas' }), icon: <AlignEndHorizontal size={16} />, disabled: selectedIds.length < 2, onClick: () => alignSelected('bottom') },
          ] },
          { id: 'distribute', label: t('wb_grp_distribute', { defaultValue: 'Répartition' }), items: [
            { id: 'dist-h', kind: 'button', size: 'small', label: t('wb_distribute_h', { defaultValue: 'Horizontale' }), icon: <AlignHorizontalDistributeCenter size={16} />, disabled: selectedIds.length < 3, onClick: () => distributeSelected('h') },
            { id: 'dist-v', kind: 'button', size: 'small', label: t('wb_distribute_v', { defaultValue: 'Verticale' }), icon: <AlignVerticalDistributeCenter size={16} />, disabled: selectedIds.length < 3, onClick: () => distributeSelected('v') },
          ] },
          { id: 'group', label: t('wb_grp_group', { defaultValue: 'Grouper' }), items: [
            { id: 'grp', kind: 'button', size: 'small', label: t('wb_group', { defaultValue: 'Grouper' }), icon: <Group size={16} />, disabled: !canGroup, onClick: groupSelected },
            { id: 'ungrp', kind: 'button', size: 'small', label: t('wb_ungroup', { defaultValue: 'Dissocier' }), icon: <Ungroup size={16} />, disabled: !canUngroup, onClick: ungroupSelected },
          ] },
        ] },
        { id: 'view', label: t('wb_tab_view', { defaultValue: 'Affichage' }), groups: [
          { id: 'zoom', label: t('wb_grp_zoom', { defaultValue: 'Zoom' }), items: [
            { id: 'zin', kind: 'button', size: 'small', label: t('wb_zoom_in', { defaultValue: 'Zoom avant' }), icon: <ZoomIn size={16} />, onClick: () => { viewportRef.current.zoomAt(1.25, window.innerWidth / 2, window.innerHeight / 2); setZoom(viewportRef.current.zoomPercent) } },
            { id: 'zout', kind: 'button', size: 'small', label: t('wb_zoom_out', { defaultValue: 'Zoom arrière' }), icon: <ZoomOut size={16} />, onClick: () => { viewportRef.current.zoomAt(0.8, window.innerWidth / 2, window.innerHeight / 2); setZoom(viewportRef.current.zoomPercent) } },
            { id: 'fit', kind: 'button', size: 'small', label: t('wb_ctx_fit', { defaultValue: 'Ajuster' }), icon: <Maximize2 size={16} />, onClick: () => { const c = canvasRef.current; if (c) { viewportRef.current.fitToElements(elements, c.width, c.height); setZoom(viewportRef.current.zoomPercent) } } },
          ] },
          { id: 'grid', label: t('wb_grp_grid', { defaultValue: 'Grille' }), items: [
            { id: 'snap', kind: 'toggle', size: 'small', label: t('wb_snap_grid', { defaultValue: 'Aligner sur la grille' }), icon: <Grid3x3 size={16} />, active: snapGrid, onClick: () => setSnapGrid(s => !s) },
          ] },
        ] },
      ]}
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
        <>
          <SaveButton onSave={() => saveBoardMut.mutate()} saving={saveBoardMut.isPending} label={t('doc_save', { defaultValue: 'Enregistrer' })} />
          <button
            onClick={() => starBoardMut.mutate(!board?.is_starred)}
            title={board?.is_starred ? t('wb_unstar', { defaultValue: 'Retirer des favoris' }) : t('wb_star', { defaultValue: 'Ajouter aux favoris' })}
            className={clsx('p-1.5 rounded hover:bg-white/10 transition-colors flex-shrink-0', board?.is_starred ? 'text-warning' : 'text-white/90')}
          >
            <Star size={15} fill={board?.is_starred ? 'currentColor' : 'none'} />
          </button>
        </>
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

        {/* Canvas + docked panels (Propriétés / Calques) */}
        <DockArea
          panels={wbPanels}
          storageKey="kubuno:office:whiteboardDock"
          defaultArrangement={{ right: [['properties'], ['layers']] }}
          theme={WB_DOCK_THEME}
          viewportBg="#f8f9fa"
          className="flex flex-1 min-w-0 overflow-hidden"
        >
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
            onContextMenu={openContextMenu}
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
        </DockArea>
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
        {/* Macros (sous-module Script) */}
        <MacrosMenu docType="whiteboard" docId={boardId} buildApi={makeApi} defaultLabel={titleDraft} />
        <div className="w-px h-5 bg-[#dadce0] mx-1" />
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

      {/* Barre de statut partagée (façon Documents/Tableur) */}
      <StatusBar>
        <StatusButton title={t('wb_status_objects', { defaultValue: 'Objets sur le tableau' })}>
          {t('wb_status_objects_n', { count: elements.length, defaultValue: `${elements.length} objet(s)` })}
        </StatusButton>
        {selectedId && (
          <>
            <StatusSep />
            <StatusButton title={t('wb_status_selected', { defaultValue: 'Sélection' })}>
              {t('wb_status_selected_n', { count: 1, defaultValue: '1 sélectionné(s)' })}
            </StatusButton>
          </>
        )}
        {remotePeers > 0 && (
          <>
            <StatusSep />
            <StatusButton title={t('wb_status_collaborators', { defaultValue: 'Collaborateurs connectés' })}>
              {t('wb_status_collaborators_n', { count: remotePeers, defaultValue: `${remotePeers} collaborateur(s)` })}
            </StatusButton>
          </>
        )}
        <StatusSpacer />
        <StatusZoom
          zoom={zoom / 100}
          onZoom={z => { viewportRef.current.setZoom(z); setZoom(viewportRef.current.zoomPercent) }}
          min={viewportRef.current.MIN_ZOOM}
          max={viewportRef.current.MAX_ZOOM}
        />
      </StatusBar>
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

    {/* Canvas context menu */}
    {ctxMenu && <MenuDropdown items={ctxMenu.items} pos={ctxMenu.pos} onClose={() => setCtxMenu(null)} />}

    {/* Hidden file input for Excalidraw import */}
    <input
      ref={fileInputRef}
      type="file"
      accept=".excalidraw,application/json,application/vnd.excalidraw+json"
      className="hidden"
      onChange={e => { const f = e.target.files?.[0]; if (f) importExcalidrawFile(f); e.target.value = '' }}
    />
    {/* Hidden file input for image insertion */}
    <input
      ref={imgFileInputRef}
      type="file"
      accept="image/*"
      className="hidden"
      onChange={e => { const f = e.target.files?.[0]; if (f) insertImageFile(f); e.target.value = '' }}
    />
    </OfficeShell>
  )
}

// ── Properties Panel ──────────────────────────────────────────────────────────

function PropertiesPanel({ element: el, onUpdate, onDelete }: {
  element: WbElement
  onUpdate: (patch: Partial<WbElement>) => void
  onDelete: () => void
}) {
  const { t } = useTranslation('office')
  return (
    <div className="flex flex-col h-full bg-white overflow-y-auto">
      <div className="flex items-center px-3 py-2 border-b border-[#e8eaed]">
        <span className="text-xs font-medium text-[#5f6368] uppercase tracking-wide">
          {el.type === 'sticky' ? t('wb_type_sticky') : el.type === 'text' ? t('wb_type_text') : el.type === 'shape' ? t('wb_type_shape') : el.type === 'arrow' ? t('wb_type_arrow') : t('wb_type_element')}
        </span>
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

        {/* Shape style: fill + stroke + width */}
        {el.type === 'shape' && (
          <div className="space-y-3">
            <div>
              <label className="text-xs text-[#5f6368] font-medium block mb-1">{t('wb_fill_color', { defaultValue: 'Remplissage' })}</label>
              <input type="color" value={(el as ShapeElement).fill ?? '#BBDEFB'}
                onChange={e => onUpdate({ fill: e.target.value } as Partial<WbElement>)}
                className="w-full h-8 rounded cursor-pointer border border-[#dadce0]" />
            </div>
            <div>
              <label className="text-xs text-[#5f6368] font-medium block mb-1">{t('wb_stroke_color', { defaultValue: 'Contour' })}</label>
              <input type="color" value={(el as ShapeElement).stroke ?? '#1a73e8'}
                onChange={e => onUpdate({ stroke: e.target.value } as Partial<WbElement>)}
                className="w-full h-8 rounded cursor-pointer border border-[#dadce0]" />
            </div>
            <div>
              <label className="text-xs text-[#5f6368] font-medium block mb-1">{t('wb_stroke_width', { defaultValue: 'Épaisseur du contour' })}: {(el as ShapeElement).strokeWidth ?? 2}</label>
              <RangeSlider min={0} max={12} step={1} value={(el as ShapeElement).strokeWidth ?? 2}
                onChange={v => onUpdate({ strokeWidth: v } as Partial<WbElement>)} className="w-full" aria-label={t('wb_stroke_width', { defaultValue: 'Épaisseur du contour' })} />
            </div>
          </div>
        )}

        {/* Text style: size + alignment (+ bold/colour for free text) */}
        {(el.type === 'text' || el.type === 'sticky') && (
          <div className="space-y-3">
            <div className="grid grid-cols-2 gap-2 items-end">
              <div>
                <label className="text-[10px] text-[#5f6368] font-medium block mb-1">{t('wb_font_size', { defaultValue: 'Taille' })}</label>
                <Input type="number" value={Math.round((el as TextBox).fontSize ?? 16)}
                  onChange={e => onUpdate({ fontSize: Math.max(6, +e.target.value) } as Partial<WbElement>)} className="h-8 text-xs" />
              </div>
              <div>
                <label className="text-[10px] text-[#5f6368] font-medium block mb-1">{t('wb_align', { defaultValue: 'Alignement' })}</label>
                <div className="flex gap-1">
                  {([['left', AlignLeft], ['center', AlignCenter], ['right', AlignRight]] as const).map(([a, Ic]) => (
                    <button key={a} onClick={() => onUpdate({ textAlign: a } as Partial<WbElement>)}
                      className={clsx('flex-1 h-8 rounded flex items-center justify-center border', ((el as TextBox).textAlign ?? 'left') === a ? 'bg-[#e8f0fe] text-[#1a73e8] border-[#1a73e8]' : 'text-[#5f6368] border-[#dadce0] hover:bg-[#f1f3f4]')}>
                      <Ic size={14} />
                    </button>
                  ))}
                </div>
              </div>
            </div>
            {el.type === 'text' && (
              <div className="flex items-center gap-2">
                <button onClick={() => onUpdate({ fontWeight: (el as TextBox).fontWeight === 'bold' ? 'normal' : 'bold' } as Partial<WbElement>)}
                  title={t('wb_bold', { defaultValue: 'Gras' })}
                  className={clsx('w-8 h-8 rounded flex items-center justify-center border', (el as TextBox).fontWeight === 'bold' ? 'bg-[#e8f0fe] text-[#1a73e8] border-[#1a73e8]' : 'text-[#5f6368] border-[#dadce0] hover:bg-[#f1f3f4]')}>
                  <Bold size={14} />
                </button>
                <input type="color" value={(el as TextBox).color ?? '#202124'}
                  onChange={e => onUpdate({ color: e.target.value } as Partial<WbElement>)}
                  className="flex-1 h-8 rounded cursor-pointer border border-[#dadce0]" title={t('wb_text_color', { defaultValue: 'Couleur du texte' })} />
              </div>
            )}
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

        {/* Rotation */}
        {'rotation' in el && (
          <div>
            <label className="text-xs text-[#5f6368] font-medium block mb-1">{t('wb_rotation', { defaultValue: 'Rotation (°)' })}</label>
            <div className="flex items-center gap-2">
              <Input type="number" value={Math.round((el as StickyNoteEl).rotation || 0)}
                onChange={e => onUpdate({ rotation: (((+e.target.value % 360) + 360) % 360) } as Partial<WbElement>)}
                className="h-8 text-xs flex-1" />
              <button onClick={() => onUpdate({ rotation: ((((el as StickyNoteEl).rotation || 0) + 90) % 360) } as Partial<WbElement>)}
                title={t('wb_ctx_rotate', { defaultValue: 'Pivoter de 90°' })}
                className="p-1.5 rounded border border-[#dadce0] hover:bg-[#f1f3f4] text-[#5f6368] shrink-0"><RotateCw size={14} /></button>
            </div>
          </div>
        )}

        {/* Opacity */}
        {'opacity' in el && (
          <div>
            <label className="text-xs text-[#5f6368] font-medium block mb-1">
              {t('wb_opacity', { value: Math.round(((el as StickyNoteEl).opacity ?? 1) * 100) })}
            </label>
            <RangeSlider min={0.1} max={1} step={0.05}
              value={(el as StickyNoteEl).opacity ?? 1}
              onChange={v => onUpdate({ opacity: v } as Partial<WbElement>)}
              className="w-full"
              aria-label={t('wb_opacity', { value: Math.round(((el as StickyNoteEl).opacity ?? 1) * 100) })} />
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

// Light dock theme for the whiteboard (matches the Google-Drive-ish chrome).
const WB_DOCK_THEME = { panel: '#ffffff', header: '#f1f3f4', border: '#e8eaed', text: '#202124', textDim: '#5f6368', accent: '#1a73e8' }

// ── Layers panel (dock) — every object on the board, top-most first ─────────────

function LayersPanel({ elements, selectedIds, onSelect, onToggleLock, onDelete, onFront, onBack }: {
  elements: WbElement[]
  selectedIds: string[]
  onSelect: (id: string, additive: boolean) => void
  onToggleLock: (id: string, locked: boolean) => void
  onDelete: (id: string) => void
  onFront: (id: string) => void
  onBack: (id: string) => void
}) {
  const { t } = useTranslation('office')
  const typeIcon = (el: WbElement) => {
    switch (el.type) {
      case 'sticky': return <StickyNote size={13} />
      case 'text':   return <Type size={13} />
      case 'shape':  return <Square size={13} />
      case 'arrow':  return <ArrowRight size={13} />
      case 'frame':  return <Square size={13} />
      default:       return <Square size={13} />
    }
  }
  const label = (el: WbElement) => {
    if (el.type === 'sticky' || el.type === 'text') { const txt = (el as StickyNoteEl).text?.trim(); if (txt) return txt.slice(0, 28) }
    if (el.type === 'shape') return t(`wb_shape_${(el as ShapeElement).kind}`, { defaultValue: (el as ShapeElement).kind })
    return t(`wb_type_${el.type}`, { defaultValue: el.type })
  }
  // Top-most first (descending zIndex).
  const ordered = [...elements].sort((a, b) => ('zIndex' in b ? b.zIndex : 0) - ('zIndex' in a ? a.zIndex : 0))
  return (
    <div className="flex flex-col h-full bg-white overflow-y-auto">
      {ordered.length === 0 && <div className="p-4 text-sm text-[#80868b]">{t('wb_layers_empty', { defaultValue: 'Aucun objet sur le tableau.' })}</div>}
      {ordered.map(el => {
        const locked = 'locked' in el && (el as { locked?: boolean }).locked
        const active = selectedIds.includes(el.id)
        return (
          <div key={el.id}
            onClick={e => onSelect(el.id, e.shiftKey || e.metaKey || e.ctrlKey)}
            className={clsx('group flex items-center gap-2 px-2.5 py-1.5 text-sm cursor-pointer border-b border-[#f1f3f4]',
              active ? 'bg-[#e8f0fe] text-[#1a73e8]' : 'text-[#202124] hover:bg-[#f8f9fa]')}>
            <span className="shrink-0 opacity-70">{typeIcon(el)}</span>
            <span className="flex-1 truncate">{label(el)}</span>
            {el.groupId && <span className="shrink-0 opacity-40" title={t('wb_group', { defaultValue: 'Grouper' })}><Group size={12} /></span>}
            <button onClick={e => { e.stopPropagation(); onFront(el.id) }} title={t('wb_ctx_front', { defaultValue: 'Premier plan' })}
              className="p-1 rounded opacity-0 group-hover:opacity-60 hover:opacity-100 hover:bg-black/5"><BringToFront size={12} /></button>
            <button onClick={e => { e.stopPropagation(); onBack(el.id) }} title={t('wb_ctx_back', { defaultValue: 'Arrière-plan' })}
              className="p-1 rounded opacity-0 group-hover:opacity-60 hover:opacity-100 hover:bg-black/5"><SendToBack size={12} /></button>
            <button onClick={e => { e.stopPropagation(); onToggleLock(el.id, !locked) }} title={locked ? t('wb_ctx_unlock', { defaultValue: 'Déverrouiller' }) : t('wb_ctx_lock', { defaultValue: 'Verrouiller' })}
              className={clsx('p-1 rounded hover:bg-black/5', locked ? 'opacity-80' : 'opacity-0 group-hover:opacity-60 hover:opacity-100')}>{locked ? <Lock size={12} /> : <Unlock size={12} />}</button>
            <button onClick={e => { e.stopPropagation(); onDelete(el.id) }} title={t('common_delete', { defaultValue: 'Supprimer' })}
              className="p-1 rounded opacity-0 group-hover:opacity-60 hover:opacity-100 hover:text-danger hover:bg-danger/10"><Trash2 size={12} /></button>
          </div>
        )
      })}
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
  { id: 'frame',   Icon: FrameIcon,     titleKey: 'wb_tool_frame',  shortcut: 'F' },
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
