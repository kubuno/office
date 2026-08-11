import { Trash2, Star, Briefcase } from 'lucide-react'
import { useLocation } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { SidebarNavItem } from '@kubuno/sdk'

// Deliberately minimal Office sidebar: only the relevant entries are kept
// (Office / Starred / Trash). The sub-module switcher (Documents/Spreadsheets/…)
// and the views redundant with the top tabs (All documents / Recent / Templates)
// have been removed.
// Every entry uses `to`, so SidebarNavItem renders a real <a href> link.
export default function OfficeSidebarBody({ collapsed = false }: { collapsed?: boolean }) {
  const { t }        = useTranslation('office')
  const { pathname } = useLocation()
  const isHome = pathname === '/office'

  return (
    <nav className={`flex-1 overflow-y-auto py-1 space-y-0.5 flex flex-col px-2`}>
      <div className="flex-1 space-y-0.5">
        <SidebarNavItem collapsed={collapsed}
          label={t('sidebar_office')}
          icon={<Briefcase className="w-4 h-4 flex-shrink-0" />}
          active={isHome}
          to="/office"
        />
        <SidebarNavItem collapsed={collapsed}
          label={t('sidebar_starred')}
          icon={<Star className="w-4 h-4 flex-shrink-0" />}
          active={pathname === '/office/starred'}
          to="/office/starred"
        />
        <SidebarNavItem collapsed={collapsed}
          label={t('sidebar_trash')}
          icon={<Trash2 className="w-4 h-4 flex-shrink-0" />}
          active={pathname === '/office/trash'}
          to="/office/trash"
        />
      </div>
    </nav>
  )
}
