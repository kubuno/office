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
import { BackstageInfo } from './ribbon/ModuleBackstage'
import type { RibbonTab } from './ribbon/types'
import { DocumentsStartContent } from './DocumentsApp'

export interface DocBackstageDoc {
  title:     string
  onTitleChange?: (v: string) => void   // renommage depuis la section Informations
  onTitleCommit?: () => void
  pages:     number
  words:     number
  chars:     number
  createdAt?: string
  updatedAt?: string
  onPrint:        () => void
  onExportPdf:    () => void
  onExportTxt:    () => void
  onExportServer: (f: 'docx' | 'odt') => void
  /** Format the document was opened from, when it came from a foreign file. */
  sourceFormat?:  'docx' | 'odt' | 'doc' | null
  /** Write back into the very file the document was opened from. */
  onSaveToSource?: () => void
  onClose:        () => void
}

/// Format proposed by default when saving. A `.doc` has no writer, so the
/// nearest faithful target is `.docx` — the same fallback the backend applies.
function defaultSaveFormat(origin?: string | null): 'docx' | 'odt' {
  return origin === 'odt' ? 'odt' : 'docx'
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

  const ExportBtn = ({ icon, label, sub, onClick }: { icon: React.ReactNode; label: string; sub: string; onClick: () => void }) => (
    <button onClick={onClick} className="flex items-center gap-3 w-full max-w-md text-left px-4 py-3 rounded-lg border border-border hover:border-primary hover:bg-primary/5 transition-colors">
      <span className="text-primary">{icon}</span>
      <span className="flex flex-col"><span className="text-sm font-medium text-text-primary">{label}</span><span className="text-sm text-text-tertiary">{sub}</span></span>
    </button>
  )

  sections.push(
    { id: 'info', label: t('doc_bs_info', { defaultValue: 'Informations' }), icon: <Info size={17} />, separated: true,
      content: (
        <BackstageInfo
          title={doc.title || t('common_untitled')}
          onTitleChange={doc.onTitleChange}
          onTitleCommit={doc.onTitleCommit}
          extension=".kbdoc"
          subtitle={t('doc_document_details')}
          general={[
            [t('doc_details_created', { defaultValue: 'Créé le' }), fmt(doc.createdAt)],
            [t('doc_details_updated', { defaultValue: 'Modifié le' }), fmt(doc.updatedAt)],
          ]}
          stats={[
            [t('doc_pages_count', { defaultValue: 'Pages' }), doc.pages],
            [t('doc_words'), doc.words],
            [t('doc_characters'), doc.chars],
          ]}
        />
      ) },
    { id: 'export', label: t('doc_bs_export', { defaultValue: 'Exporter' }), icon: <FileDown size={17} />,
      content: (
        <div className="p-8">
          <h2 className="text-xl font-semibold text-text-primary mb-6">{t('doc_bs_export', { defaultValue: 'Exporter' })}</h2>
          <div className="flex flex-col gap-3">
            {/* A document opened from a foreign file saves back to it, in ITS
                format — what a user expects after opening a .docx. Shown first
                because it is the default action for such a document. */}
            {doc.onSaveToSource && doc.sourceFormat && (
              <ExportBtn
                icon={<FileText size={20} />}
                label={t('doc_bs_save_source', {
                  defaultValue: 'Enregistrer ({{fmt}})',
                  fmt: defaultSaveFormat(doc.sourceFormat).toUpperCase(),
                })}
                sub={
                  doc.sourceFormat === 'doc'
                    ? t('doc_bs_save_source_doc_sub', {
                        defaultValue: 'Ce document vient d’un .doc, que nous ne savons pas réécrire : il sera enregistré en DOCX',
                      })
                    : t('doc_bs_save_source_sub', {
                        defaultValue: 'Met à jour le fichier d’origine',
                      })
                }
                onClick={doc.onSaveToSource}
              />
            )}
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
