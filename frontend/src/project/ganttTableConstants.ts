// Task table columns (fixed widths, MS-Project style) + priority colours. Shared by
// the table header (ProjectEditorPage) and the task row (TaskRow).
export const COL_W = { idx: 34, mode: 34, name: 168, dur: 64, progress: 52, priority: 80, start: 92, end: 92, variance: 68, pred: 96, res: 120 }
export const PRIO_COLOR: Record<string, string> = { low: '#34a853', medium: '#fbbc04', high: '#ea4335', critical: '#b80672' }
export const TABLE_W = Object.values(COL_W).reduce((a, b) => a + b, 0)
