import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { ArrowRight } from 'lucide-react'
import OfficeLogo from './OfficeLogo'
import DocumentsLogo from './DocumentsLogo'
import SpreadsheetsLogo from './SpreadsheetsLogo'
import PresentationsLogo from './PresentationsLogo'
import ProjectsLogo from './ProjectsLogo'
import DiagramsLogo from './DiagramsLogo'
import DataLogo from './DataLogo'
import ScriptLogo from './ScriptLogo'
import MathsLogo from './MathsLogo'
import WhiteboardLogo from './WhiteboardLogo'

// A sub-module that has its own brand logo shows it as-is (no tinted tile);
// the others keep a lucide icon on a tinted square.

import type { ComponentType } from 'react'
import type { LucideIcon } from 'lucide-react'

interface SubModule {
  id:    string
  label: string
  desc:  string
  icon?: LucideIcon
  logo?: ComponentType<{ size?: number; className?: string; title?: string }>
  color: string
  path:  string
}

// `label` is the product name of the sub-module: it is deliberately NOT
// translated (module and sub-module names are the same in every language).
const SUBMODULES: SubModule[] = [
  {
    id:    'documents',
    label: 'Documents',
    desc:  'Créez et modifiez des documents texte collaboratifs avec mise en page avancée',
    logo:  DocumentsLogo,
    color: '#067fdc',
    path:  '/office/recent',
  },
  {
    id:    'spreadsheets',
    label: 'Spreadsheets',
    desc:  'Analysez vos données avec des feuilles de calcul puissantes',
    logo:  SpreadsheetsLogo,
    color: '#3ca94f',
    path:  '/office/spreadsheets',
  },
  {
    id:    'presentations',
    label: 'Presentations',
    desc:  'Créez des diaporamas et présentations percutantes',
    logo:  PresentationsLogo,
    color: '#ed6415',
    path:  '/office/presentations',
  },
  {
    id:    'projects',
    label: 'Projects',
    desc:  'Gérez vos projets avec des tableaux Kanban collaboratifs',
    logo:  ProjectsLogo,
    color: '#06a261',
    path:  '/office/projects',
  },
  {
    id:    'diagrams',
    label: 'Diagrams',
    desc:  'Dessinez des diagrammes, organigrammes et schémas techniques',
    logo:  DiagramsLogo,
    color: '#7f38bf',
    path:  '/office/diagrams',
  },
  {
    id:    'data',
    label: 'Data',
    desc:  'Explorez et visualisez vos données : tableaux de bord, requêtes et graphiques',
    logo:  DataLogo,
    color: '#3c54c6',
    path:  '/office/data',
  },
  {
    id:    'script',
    label: 'Script',
    desc:  'Automatisez vos traitements avec des scripts',
    logo:  ScriptLogo,
    color: '#f3af02',
    path:  '/office/script',
  },
  {
    id:    'maths',
    label: 'Maths',
    desc:  'Composez des formules mathématiques façon LibreOffice Math (LaTeX)',
    logo:  MathsLogo,
    color: '#f03f7f',
    path:  '/office/maths',
  },
  {
    id:    'whiteboard',
    label: 'Whiteboard',
    desc:  'Esquissez, brainstormez et collaborez sur une toile infinie',
    logo:  WhiteboardLogo,
    color: '#02aabb',
    path:  '/office/whiteboard',
  },
]

export default function OfficeSuiteHome() {
  const navigate = useNavigate()
  const { t } = useTranslation('office')

  return (
    <div className="h-full overflow-y-auto" style={{ background: 'var(--body-bg)' }}>
      <div className="max-w-4xl mx-auto px-6 py-8">
        <div className="flex items-center gap-3 mb-2">
          <OfficeLogo size={40} />
          <div>
            <h1 className="text-xl font-semibold text-text-primary">Office</h1>
            <p className="text-sm text-text-secondary">{t('subtitle')}</p>
          </div>
        </div>

        <p className="text-sm text-text-secondary mb-8 ml-1">
          {t('intro')}
        </p>

        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {SUBMODULES.map(({ id, label, desc, icon: Icon, logo: Logo, color, path }) => (
            <button
              key={id}
              onClick={() => navigate(path)}
              className="text-left p-4 rounded-xl border transition-all group hover:shadow-md hover:border-border-strong cursor-pointer"
              style={{ background: 'var(--color-surface-0)', borderColor: 'var(--color-border)' }}
            >
              <div className="flex items-start justify-between mb-3">
                {Logo ? (
                  <Logo size={40} />
                ) : (
                  <div className="w-10 h-10 rounded-lg flex items-center justify-center"
                       style={{ background: color + '20' }}>
                    {Icon && <Icon size={20} style={{ color }} />}
                  </div>
                )}
                <ArrowRight size={16} className="text-text-tertiary opacity-0 group-hover:opacity-100 transition-opacity mt-2" />
              </div>
              <h3 className="text-sm font-medium text-text-primary mb-1">{label}</h3>
              <p className="text-xs text-text-secondary leading-relaxed">{t('desc_' + id, { defaultValue: desc })}</p>
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}
