import { Trash2, Star, Briefcase } from 'lucide-react'
import { useNavigate, useLocation } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { SidebarNavItem } from '@kubuno/sdk'

// Sidebar Office volontairement minimale : seules les entrées pertinentes sont
// gardées (Office / Favoris / Corbeille). Le commutateur de sous-modules
// (Documents/Tableurs/…) et les vues redondantes avec les onglets du haut
// (Tous les documents / Récents / Modèles) ont été retirés.
export default function OfficeSidebarBody({ collapsed = false }: { collapsed?: boolean }) {
  const { t }        = useTranslation('office')
  const navigate     = useNavigate()
  const { pathname } = useLocation()
  const isHome = pathname === '/office'

  return (
    <nav className={`flex-1 overflow-y-auto py-1 space-y-0.5 flex flex-col ${collapsed ? 'px-2' : 'px-3'}`}>
      <div className="flex-1 space-y-0.5">
        <SidebarNavItem collapsed={collapsed}
          label={t('sidebar_office')}
          icon={<Briefcase className="w-4 h-4 flex-shrink-0" />}
          active={isHome}
          onClick={() => navigate('/office')}
        />
        <SidebarNavItem collapsed={collapsed}
          label={t('sidebar_starred')}
          icon={<Star className="w-4 h-4 flex-shrink-0" />}
          active={pathname === '/office/starred'}
          onClick={() => navigate('/office/starred')}
        />
        <SidebarNavItem collapsed={collapsed}
          label={t('sidebar_trash')}
          icon={<Trash2 className="w-4 h-4 flex-shrink-0" />}
          active={pathname === '/office/trash'}
          onClick={() => navigate('/office/trash')}
        />
      </div>
    </nav>
  )
}
