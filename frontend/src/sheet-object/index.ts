// Shared layer for the spreadsheet's floating objects (charts, pictures, shapes) —
// public surface. Type-specific code lives elsewhere: sheet-chart/ for diagrams,
// sheet-shape/ for shapes; SpreadsheetApp only wires them together.
//
// The per-type MENU builders (chartMenu / imageMenu, and sheet-shape/shapeMenu) are
// deliberately NOT re-exported: each one is imported by the single call site that
// wires it, so nothing pulls in three menus to render one.
export * from './types'
export * from './clipboard'
export * from './order'
export * from './selection'
export * from './MenuIconRow'
export * from './MiniToolbar'
export * from './imageFrame'
// Generic dialogs, controlled (value in → next value out), shared by the three kinds.
export * from './AltTextDialog'
export * from './SizePropsDialog'
export * from './FormatObjectDialog'
export * from './MoveObjectDialog'
export * from './ObjectLinkDialog'
export * from './NameDialog'
