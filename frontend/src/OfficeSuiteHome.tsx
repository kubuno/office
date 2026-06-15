import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import {
  FileText, TableProperties, LayoutTemplate,
  FolderKanban, Network, ArrowRight, Briefcase,
  BarChart3, Zap, Sigma,
} from 'lucide-react'

const SUBMODULES = [
  {
    id:    'documents',
    label: 'Documents',
    desc:  'Créez et modifiez des documents texte collaboratifs avec mise en page avancée',
    icon:  FileText,
    color: '#4285f4',
    path:  '/office/recent',
  },
  {
    id:    'spreadsheets',
    label: 'Tableurs',
    desc:  'Analysez vos données avec des feuilles de calcul puissantes',
    icon:  TableProperties,
    color: '#34a853',
    path:  '/office/spreadsheets',
  },
  {
    id:    'presentations',
    label: 'Présentations',
    desc:  'Créez des diaporamas et présentations percutantes',
    icon:  LayoutTemplate,
    color: '#fbbc04',
    path:  '/office/presentations',
  },
  {
    id:    'projects',
    label: 'Projets',
    desc:  'Gérez vos projets avec des tableaux Kanban collaboratifs',
    icon:  FolderKanban,
    color: '#ea4335',
    path:  '/office/projects',
  },
  {
    id:    'diagrams',
    label: 'Diagrammes',
    desc:  'Dessinez des diagrammes, organigrammes et schémas techniques',
    icon:  Network,
    color: '#9c27b0',
    path:  '/office/diagrams',
  },
  {
    id:    'data',
    label: 'Data',
    desc:  'Explorez et visualisez vos données : tableaux de bord, requêtes et graphiques',
    icon:  BarChart3,
    color: '#00897b',
    path:  '/office/data',
  },
  {
    id:    'script',
    label: 'Script',
    desc:  'Automatisez vos traitements avec des scripts',
    icon:  Zap,
    color: '#f57c00',
    path:  '/office/script',
  },
  {
    id:    'maths',
    label: 'Maths',
    desc:  'Composez des formules mathématiques façon LibreOffice Math (LaTeX)',
    icon:  Sigma,
    color: '#6d4c41',
    path:  '/office/maths',
  },
]

export default function OfficeSuiteHome() {
  const navigate = useNavigate()
  const { t } = useTranslation('office')

  return (
    <div className="h-full overflow-y-auto" style={{ background: 'var(--body-bg)' }}>
      <div className="max-w-4xl mx-auto px-6 py-8">
        <div className="flex items-center gap-3 mb-2">
          <div className="w-10 h-10 rounded-xl flex items-center justify-center"
               style={{ background: '#e8f0fe' }}>
            <Briefcase size={22} style={{ color: '#4285f4' }} />
          </div>
          <div>
            <h1 className="text-xl font-semibold text-text-primary">Office</h1>
            <p className="text-sm text-text-secondary">{t('subtitle')}</p>
          </div>
        </div>

        <p className="text-sm text-text-secondary mb-8 ml-1">
          {t('intro')}
        </p>

        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {SUBMODULES.map(({ id, label, desc, icon: Icon, color, path }) => (
            <button
              key={id}
              onClick={() => navigate(path)}
              className="text-left p-4 rounded-xl border transition-all group hover:shadow-md hover:border-border-strong cursor-pointer"
              style={{ background: 'var(--color-surface-0)', borderColor: 'var(--color-border)' }}
            >
              <div className="flex items-start justify-between mb-3">
                <div className="w-10 h-10 rounded-lg flex items-center justify-center"
                     style={{ background: color + '20' }}>
                  <Icon size={20} style={{ color }} />
                </div>
                <ArrowRight size={16} className="text-text-tertiary opacity-0 group-hover:opacity-100 transition-opacity mt-2" />
              </div>
              <h3 className="text-sm font-medium text-text-primary mb-1">{t('label_' + id, { defaultValue: label })}</h3>
              <p className="text-xs text-text-secondary leading-relaxed">{t('desc_' + id, { defaultValue: desc })}</p>
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}
