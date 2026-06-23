// Onglet « Fichier » (Backstage façon Office) du sous-module Documents : sections
// Accueil (récents/parcourir/modèles, réutilise DocumentsStartContent) + Informations
// + Exporter + Imprimer + Fermer. `DocumentsHome` = page d'accueil SANS document
// (route /office/documents) : la chrome Office avec le backstage ouvert et verrouillé.
import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { Home, Info, FileDown, Printer, X, FilePlus, FileText, FileType2 } from 'lucide-react'
import { format } from 'date-fns'
import { getDateLocale, WORKSPACE_OFFICE } from '@kubuno/sdk'
import { OfficeShell } from './shell/OfficeShell'
import { Backstage } from './ribbon/Backstage'
import type { BackstageSection } from './ribbon/Backstage'
import type { RibbonTab } from './ribbon/types'
import { DocumentsStartContent } from './DocumentsApp'

export interface DocBackstageDoc {
  title:     string
  pages:     number
  words:     number
  chars:     number
  createdAt?: string
  updatedAt?: string
  onPrint:        () => void
  onExportPdf:    () => void
  onExportTxt:    () => void
  onExportServer: (f: 'docx' | 'odt') => void
  onClose:        () => void
}

// Construit les sections du backstage. `doc` absent = page d'accueil (Accueil seul).
export function useDocumentsBackstageSections(doc?: DocBackstageDoc): BackstageSection[] {
  const { t, i18n } = useTranslation('office')
  const fmt = (d?: string) => (d ? format(new Date(d), 'PPPp', { locale: getDateLocale(i18n.language) }) : '—')

  const sections: BackstageSection[] = [
    { id: 'home', label: t('doc_bs_home', { defaultValue: 'Accueil' }), icon: <Home size={17} />,
      content: <div className="h-full overflow-auto"><DocumentsStartContent /></div> },
  ]
  if (!doc) return sections

  const Row = ({ k, v }: { k: string; v: string | number }) => (
    <div className="flex justify-between gap-6 py-1.5 border-b border-border/60 text-sm">
      <span className="text-text-secondary">{k}</span><span className="font-medium text-text-primary text-right">{v}</span>
    </div>
  )
  const ExportBtn = ({ icon, label, sub, onClick }: { icon: React.ReactNode; label: string; sub: string; onClick: () => void }) => (
    <button onClick={onClick} className="flex items-center gap-3 w-full max-w-md text-left px-4 py-3 rounded-lg border border-border hover:border-primary hover:bg-primary/5 transition-colors">
      <span className="text-primary">{icon}</span>
      <span className="flex flex-col"><span className="text-sm font-medium text-text-primary">{label}</span><span className="text-xs text-text-tertiary">{sub}</span></span>
    </button>
  )

  sections.push(
    { id: 'info', label: t('doc_bs_info', { defaultValue: 'Informations' }), icon: <Info size={17} />, separated: true,
      content: (
        <div className="p-8 max-w-2xl">
          <h2 className="text-xl font-semibold text-text-primary mb-1">{doc.title || t('common_untitled')}</h2>
          <p className="text-sm text-text-tertiary mb-6">{t('doc_document_details')}</p>
          <Row k={t('doc_pages_count', { defaultValue: 'Pages' })} v={doc.pages} />
          <Row k={t('doc_words')} v={doc.words} />
          <Row k={t('doc_characters')} v={doc.chars} />
          <Row k={t('doc_details_created', { defaultValue: 'Créé le' })} v={fmt(doc.createdAt)} />
          <Row k={t('doc_details_updated', { defaultValue: 'Modifié le' })} v={fmt(doc.updatedAt)} />
        </div>
      ) },
    { id: 'export', label: t('doc_bs_export', { defaultValue: 'Exporter' }), icon: <FileDown size={17} />,
      content: (
        <div className="p-8">
          <h2 className="text-xl font-semibold text-text-primary mb-6">{t('doc_bs_export', { defaultValue: 'Exporter' })}</h2>
          <div className="flex flex-col gap-3">
            <ExportBtn icon={<FileType2 size={20} />} label="PDF" sub={t('doc_bs_export_pdf', { defaultValue: 'Document PDF (mise en page fidèle)' })} onClick={doc.onExportPdf} />
            <ExportBtn icon={<FileText size={20} />} label={t('doc_bs_export_docx', { defaultValue: 'Word (DOCX)' })} sub={t('doc_bs_export_docx_sub', { defaultValue: 'Format Microsoft Word' })} onClick={() => doc.onExportServer('docx')} />
            <ExportBtn icon={<FileText size={20} />} label={t('doc_bs_export_odt', { defaultValue: 'OpenDocument (ODT)' })} sub={t('doc_bs_export_odt_sub', { defaultValue: 'Format ouvert OpenDocument' })} onClick={() => doc.onExportServer('odt')} />
            <ExportBtn icon={<FileText size={20} />} label={t('doc_bs_export_txt', { defaultValue: 'Texte (TXT)' })} sub={t('doc_bs_export_txt_sub', { defaultValue: 'Texte brut sans mise en forme' })} onClick={doc.onExportTxt} />
          </div>
        </div>
      ) },
    { id: 'print', label: t('common_print'), icon: <Printer size={17} />, onSelect: doc.onPrint },
    { id: 'close', label: t('common_close'), icon: <X size={17} />, onSelect: doc.onClose, separated: true },
  )
  return sections
}

// Page d'accueil du sous-module Documents (route /office/documents, AUCUN document) :
// éditeur-chrome avec uniquement l'onglet Fichier (backstage ouvert, verrouillé).
export function DocumentsHome() {
  const { t } = useTranslation('office')
  const navigate = useNavigate()
  const sections = useDocumentsBackstageSections()  // pas de doc → Accueil seul
  const fileTab: RibbonTab = {
    id: 'file', label: t('doc_bs_file', { defaultValue: 'Fichier' }), groups: [],
    backstage: <Backstage sections={sections} theme={WORKSPACE_OFFICE} onBack={() => { /* verrouillé */ }} locked />,
    backstageLocked: true,
  }
  return (
    <div className="h-full overflow-hidden flex flex-col">
      <OfficeShell
        ribbon={[fileTab]}
        activeTabId="file"
        chromeless
        topbarHeight={64}
        titleIcon={<FilePlus size={16} className="text-white/90 flex-shrink-0" />}
        title={t('documents_browse_title', { defaultValue: 'Documents' })}
        onBack={() => navigate('/office')}
      >
        <div className="flex-1" />
      </OfficeShell>
    </div>
  )
}
