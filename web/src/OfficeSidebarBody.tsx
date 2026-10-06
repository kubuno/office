import { Trash2, Star, Briefcase, Users2 } from 'lucide-react'
import { useLocation } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { SidebarNavItem } from '@kubuno/sdk'
import { useQueryClient } from '@tanstack/react-query'
import { bindQueryClient } from './queryClientBridge'

// Deliberately minimal Office sidebar: only the relevant entries are kept
// (Office / Starred / Shared with me / Trash). The sub-module switcher (Documents/Spreadsheets/…)
// and the views redundant with the top tabs (All documents / Recent / Templates)
// have been removed.
// Every entry uses `to`, so SidebarNavItem renders a real <a href> link.
export default function OfficeSidebarBody({ collapsed = false }: { collapsed?: boolean }) {
  const { t }        = useTranslation('office')
  const { pathname } = useLocation()
  const isHome = pathname === '/office'

  // Hand the host's query client to the non-React "New" menu item builders
  // (this body is mounted whenever the sidebar — hence the menu — is available).
  bindQueryClient(useQueryClient())

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
        {/* Projects someone else shared with me — the only entry point to that view. */}
        <SidebarNavItem collapsed={collapsed}
          label={t('sidebar_shared', { defaultValue: 'Partagés avec moi' })}
          icon={<Users2 className="w-4 h-4 flex-shrink-0" />}
          active={pathname === '/office/projects/shared'}
          to="/office/projects/shared"
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
