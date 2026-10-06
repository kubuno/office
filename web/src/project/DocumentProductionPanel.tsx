import React, { useMemo, useState } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  AlertTriangle, ClipboardCheck, FileCheck2, FileText, GraduationCap,
  Layers, ListTree, ShieldAlert, Sparkles,
} from 'lucide-react'
import { Badge, Button, Callout, EmptyState, Tooltip, useIsMobile } from '@ui'
import { projectsApi, type ProducibleDoc, type ProducibleDocKind } from '../api'

// "File → Export" for a project. The registers a project keeps — charter, risks,
// requirements, WBS, lessons… — are turned into REAL Kubuno documents dropped in
// Drive: editable in the Word editor and exportable to .docx/.odt/PDF, not frozen
// snapshots. That is the whole point, and what sets this apart from a plain
// "Export to PDF" button, so the panel says it out loud once at the top.
//
// Each production is a fresh document (the server is not idempotent: reproducing
// the status report gives a snapshot dated today), which is why the intro warns
// the reader they may end up with two "Status report" files in Drive.

interface DocMeta {
  /** Kept in a stable, editorial order — not alphabetical. */
  icon: React.ReactNode
  title: string
  description: string
}

interface Props {
  projectId: string
  onOpenDocument?: (documentId: string) => void
}

export default function DocumentProductionPanel({ projectId, onOpenDocument }: Props) {
  const { t } = useTranslation('office')
  const isMobile = useIsMobile()

  // Which kind failed, and its message — flattened at the root by the API client.
  const [error, setError] = useState<{ kind: ProducibleDocKind; message: string } | null>(null)

  const { data: docs, isLoading, isError, refetch } = useQuery({
    queryKey: ['producible-docs', projectId],
    queryFn: () => projectsApi.getProducibleDocs(projectId),
  })

  const produce = useMutation({
    mutationFn: (kind: ProducibleDocKind) => projectsApi.produceDocument(projectId, kind),
    onMutate: () => setError(null),
    onSuccess: (res) => {
      onOpenDocument?.(res.document_id)
    },
    onError: (err: unknown, kind) => {
      const message = err instanceof Error && err.message
        ? err.message
        : t('docprod.error.generic', { defaultValue: 'La production du document a échoué.' })
      setError({ kind, message })
    },
  })

  // The eight producible documents, each described in one sentence — what it
  // contains — in the intended reading order.
  const META = useMemo<Record<ProducibleDocKind, DocMeta>>(() => ({
    charter: {
      icon: <FileText size={20} />,
      title: t('docprod.charter.title', { defaultValue: 'Charte du projet' }),
      description: t('docprod.charter.desc', { defaultValue: 'Objet, sponsor, objectifs et critères de succès du projet.' }),
    },
    status_report: {
      icon: <ClipboardCheck size={20} />,
      title: t('docprod.status_report.title', { defaultValue: 'Rapport d’avancement' }),
      description: t('docprod.status_report.desc', { defaultValue: 'L’état à aujourd’hui : avancement, retards, risques et incidents.' }),
    },
    closure_report: {
      icon: <FileCheck2 size={20} />,
      title: t('docprod.closure_report.title', { defaultValue: 'Rapport de clôture' }),
      description: t('docprod.closure_report.desc', { defaultValue: 'Objectifs atteints, réception, transfert et points en suspens.' }),
    },
    risk_register: {
      icon: <ShieldAlert size={20} />,
      title: t('docprod.risk_register.title', { defaultValue: 'Registre des risques' }),
      description: t('docprod.risk_register.desc', { defaultValue: 'Le tableau des risques, cotés et triés.' }),
    },
    lessons_register: {
      icon: <GraduationCap size={20} />,
      title: t('docprod.lessons_register.title', { defaultValue: 'Enseignements tirés' }),
      description: t('docprod.lessons_register.desc', { defaultValue: 'Pour chaque enseignement : situation, ce qui s’est passé, recommandation.' }),
    },
    traceability_matrix: {
      icon: <ListTree size={20} />,
      title: t('docprod.traceability_matrix.title', { defaultValue: 'Matrice de traçabilité' }),
      description: t('docprod.traceability_matrix.desc', { defaultValue: 'Chaque exigence et ce qui la réalise.' }),
    },
    wbs_dictionary: {
      icon: <Layers size={20} />,
      title: t('docprod.wbs_dictionary.title', { defaultValue: 'Dictionnaire du WBS' }),
      description: t('docprod.wbs_dictionary.desc', { defaultValue: 'Chaque lot de travail : énoncé, critères d’acceptation, hors périmètre.' }),
    },
    management_plan: {
      icon: <FileText size={20} />,
      title: t('docprod.management_plan.title', { defaultValue: 'Plan de management du projet' }),
      description: t('docprod.management_plan.desc', { defaultValue: 'Le document intégrateur qui rassemble les plans subsidiaires.' }),
    },
  }), [t])

  const ORDER: ProducibleDocKind[] = [
    'charter', 'status_report', 'closure_report', 'risk_register',
    'lessons_register', 'traceability_matrix', 'wbs_dictionary', 'management_plan',
  ]

  // Render in the editorial order, keyed by what the server actually returned.
  const items = useMemo(() => {
    const byKind = new Map<ProducibleDocKind, ProducibleDoc>()
    for (const d of docs ?? []) byKind.set(d.kind, d)
    return ORDER
      .map(kind => byKind.get(kind))
      .filter((d): d is ProducibleDoc => d != null)
  }, [docs])

  if (isError) {
    return (
      <div className="p-4">
        <EmptyState
          variant="error"
          icon={<AlertTriangle size={26} />}
          title={t('docprod.load_error.title', { defaultValue: 'Documents productibles indisponibles' })}
          description={t('docprod.load_error.desc', { defaultValue: 'La liste des documents n’a pas pu être chargée.' })}
          action={{
            label: t('docprod.load_error.retry', { defaultValue: 'Réessayer' }),
            onClick: () => { void refetch() },
          }}
          t={t}
        />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-4 p-4">
      {/* What this screen is, and what makes it different from an "Export to PDF". */}
      <header className="flex flex-col gap-1">
        <div className="flex items-center gap-2">
          <Sparkles size={18} className="text-[var(--color-primary)]" />
          <h2 className="text-base font-semibold">
            {t('docprod.heading', { defaultValue: 'Produire des documents' })}
          </h2>
        </div>
        <p className="text-sm text-secondary">
          {t('docprod.intro', { defaultValue: 'Chaque document est construit à partir des registres du projet et déposé dans Drive comme un vrai document Kubuno : modifiable dans l’éditeur et exportable en .docx, .odt ou PDF. Ce ne sont pas des exports figés.' })}
        </p>
        <p className="text-xs text-tertiary">
          {t('docprod.intro.snapshot', { defaultValue: 'Chaque production crée un nouveau document daté : reproduire un rapport en génère une seconde copie dans Drive.' })}
        </p>
      </header>

      {error && (
        <Callout
          variant="danger"
          title={t('docprod.error.title', { defaultValue: 'Production impossible' })}
          dismissible
          onDismiss={() => setError(null)}
          t={t}
        >
          {error.message}
        </Callout>
      )}

      {isLoading ? (
        <p className="text-sm text-tertiary">
          {t('docprod.loading', { defaultValue: 'Chargement des documents productibles…' })}
        </p>
      ) : items.length === 0 ? (
        <EmptyState
          variant="unavailable"
          icon={<FileText size={26} />}
          title={t('docprod.empty.title', { defaultValue: 'Aucun document productible' })}
          description={t('docprod.empty.desc', { defaultValue: 'Ce projet ne propose encore aucun document à produire.' })}
          t={t}
        />
      ) : (
        <ul className="flex flex-col gap-2">
          {items.map((doc) => {
            const meta = META[doc.kind]
            const producing = produce.isPending && produce.variables === doc.kind
            const disabled = !doc.has_data

            const button = (
              <Button
                variant="secondary"
                size="md"
                loading={producing}
                disabled={disabled || produce.isPending}
                onClick={() => produce.mutate(doc.kind)}
              >
                {t('docprod.produce', { defaultValue: 'Produire' })}
              </Button>
            )

            return (
              <li
                key={doc.kind}
                className={`flex gap-3 rounded-xl border border-border bg-surface-0 p-3 ${isMobile ? 'flex-col' : 'items-start'}`}
              >
                <div className="mt-0.5 text-secondary">{meta.icon}</div>

                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="font-medium">{meta.title}</span>
                    {disabled && (
                      <Badge variant="neutral" size="sm">
                        {t('docprod.no_data', { defaultValue: 'Sans données' })}
                      </Badge>
                    )}
                  </div>
                  <p className="mt-0.5 text-sm text-secondary">{meta.description}</p>
                  {disabled && doc.hint && (
                    <p className="mt-1 text-xs text-tertiary">{doc.hint}</p>
                  )}
                </div>

                <div className={isMobile ? 'self-start' : 'shrink-0'}>
                  {disabled && doc.hint ? (
                    // Tooltip clones its child, so a disabled Button is wrapped
                    // in an inline-flex span to keep the pointer handlers alive.
                    <Tooltip label={doc.hint}>
                      <span className="inline-flex">{button}</span>
                    </Tooltip>
                  ) : (
                    button
                  )}
                </div>
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}
