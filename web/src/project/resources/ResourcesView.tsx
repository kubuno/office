import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Tabs, type TabDef } from '@ui'
import { Boxes, BarChart3, CalendarOff, TrendingUp } from 'lucide-react'
import type { ResourceViewProps } from './types'
import ResourceRegistry from './ResourceRegistry'
import ResourceHeatmap from './ResourceHeatmap'
import ResourceAvailability from './ResourceAvailability'
import ResourceCostView from './ResourceCostView'

type SubTab = 'registry' | 'load' | 'availability' | 'cost'

// The project's resource-management workspace: a registry, a workload heatmap,
// availability (time off) and a cost view — one place to manage the team.
export default function ResourcesView(props: ResourceViewProps) {
  const { t } = useTranslation('office')
  const [tab, setTab] = useState<SubTab>('registry')
  const subTabs: TabDef<SubTab>[] = [
    { id: 'registry',     icon: Boxes,       label: t('res_tab_registry', { defaultValue: 'Registre' }) },
    { id: 'load',         icon: BarChart3,   label: t('res_tab_load', { defaultValue: 'Plan de charge' }) },
    { id: 'availability', icon: CalendarOff, label: t('res_tab_avail', { defaultValue: 'Disponibilité' }) },
    { id: 'cost',         icon: TrendingUp,  label: t('res_tab_cost', { defaultValue: 'Coûts' }) },
  ]
  return (
    <div className="h-full w-full flex flex-col min-h-0 bg-surface-0">
      <div className="shrink-0 border-b border-border bg-surface-1 px-2">
        <Tabs tabs={subTabs} value={tab} onChange={setTab} size="sm" t={t} />
      </div>
      <div className="flex-1 min-h-0 overflow-hidden">
        {tab === 'registry'     && <ResourceRegistry {...props} />}
        {tab === 'load'         && <ResourceHeatmap {...props} />}
        {tab === 'availability' && <ResourceAvailability {...props} />}
        {tab === 'cost'         && <ResourceCostView {...props} />}
      </div>
    </div>
  )
}
