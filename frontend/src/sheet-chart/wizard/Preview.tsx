// Live wizard preview — delegates to the real `ChartView` renderer for every
// chart kind. (The interim pie/radar fallback that used to live here was
// removed once CircularPlot/RadarPlot landed; `WizardPreview` remains the
// single branch point shared by the wizard and the grid.)

import type { SheetChart } from '../../api'
import type { CellValueGetter } from '../data'
import { ChartView } from '../ChartView'
import type { WizardTranslate } from './state'

export interface WizardPreviewProps {
  chart: SheetChart
  getCell: CellValueGetter
  width: number
  height: number
  t: WizardTranslate
}

/** Live preview of a draft chart (also used to render charts on the grid). */
export function WizardPreview({ chart, getCell, width, height, t }: WizardPreviewProps) {
  return <ChartView chart={chart} width={width} height={height} getCell={getCell} t={t} />
}
