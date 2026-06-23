// Types du tableau blanc — partagés entre le canvas et Yjs

export type ToolType =
  | 'select' | 'hand' | 'sticky' | 'text' | 'shape' | 'arrow' | 'pen' | 'eraser' | 'frame'

export type ShapeKind = 'rect' | 'circle' | 'triangle' | 'diamond' | 'star'
export type ArrowStyle = 'straight' | 'curved'
export type ArrowHead = 'triangle' | 'open' | 'circle' | 'none'
export type Background = 'white' | 'grid' | 'dots' | 'lines'

export interface BaseElement {
  id:       string
  type:     string
  x:        number
  y:        number
  width:    number
  height:   number
  rotation: number
  opacity:  number
  zIndex:   number
  locked:   boolean
  // Optional group membership: every element sharing a `groupId` is selected,
  // moved and deleted as a single unit (set by group/ungroup operations).
  groupId?: string
}

export interface StickyNote extends BaseElement {
  type:       'sticky'
  text:       string
  color:      string
  fontSize:   number
  textAlign:  'left' | 'center' | 'right'
}

export interface TextBox extends BaseElement {
  type:       'text'
  text:       string
  fontSize:   number
  fontWeight: 'normal' | 'bold'
  color:      string
  textAlign:  'left' | 'center' | 'right'
}

export interface ShapeElement extends BaseElement {
  type:         'shape'
  kind:         ShapeKind
  fill:         string
  stroke:       string
  strokeWidth:  number
}

export interface ArrowElement {
  id:            string
  type:          'arrow'
  startX:        number
  startY:        number
  endX:          number
  endY:          number
  startElementId?: string
  startSide?:    'top' | 'bottom' | 'left' | 'right' | 'center'
  endElementId?: string
  endSide?:      'top' | 'bottom' | 'left' | 'right' | 'center'
  color:         string
  width:         number
  style:         ArrowStyle
  startArrow:    ArrowHead
  endArrow:      ArrowHead
  zIndex:        number
  opacity:       number
  groupId?:      string
}

export interface FrameElement extends BaseElement {
  type:   'frame'
  title:  string
  color:  string
}

export interface ImageElement extends BaseElement {
  type:    'image'
  src:     string
  natural_width: number
  natural_height: number
}

export type WbElement = StickyNote | TextBox | ShapeElement | ArrowElement | FrameElement | ImageElement

export interface Stroke {
  id:        string
  points:    number[]   // flat array [x0,y0, x1,y1, ...]
  color:     string
  width:     number
  opacity:   number
  tool:      'pen' | 'highlighter'
  userId:    string
  createdAt: number
}

// Couleurs des post-its
export const STICKY_COLORS: Record<string, string> = {
  yellow:  '#FFF9C4',
  green:   '#C8E6C9',
  blue:    '#BBDEFB',
  pink:    '#F8BBD9',
  orange:  '#FFE0B2',
  purple:  '#E1BEE7',
  red:     '#FFCDD2',
  teal:    '#B2EBF2',
  white:   '#FFFFFF',
  dark:    '#37474F',
}

export const STICKY_COLOR_KEYS = Object.keys(STICKY_COLORS)
