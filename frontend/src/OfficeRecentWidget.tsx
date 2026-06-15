import { useQuery } from '@tanstack/react-query'
import { FileText, Star } from 'lucide-react'
import { formatDistanceToNow, parseISO } from 'date-fns'
import { useTranslation } from 'react-i18next'
import { officeApi } from './api'
import { DashboardWidget } from '@kubuno/sdk'
import { getDateLocale } from '@kubuno/sdk'
import { Link } from 'react-router-dom'

export default function OfficeRecentWidget() {
  const { t, i18n } = useTranslation('office')
  const { data, isLoading } = useQuery({
    queryKey: ['widget-office-recent'],
    queryFn:  () => officeApi.list({ recent: true, limit: 6 }),
    staleTime: 60_000,
  })

  const docs = data?.documents ?? []

  return (
    <DashboardWidget
      title={t('shell_recent_documents')}
      icon={<FileText size={15} className="text-blue-500" />}
      link="/office/recent"
    >
      {isLoading ? (
        <div className="px-4 py-6 text-center text-sm text-text-tertiary">{t('common_loading')}</div>
      ) : docs.length === 0 ? (
        <div className="px-4 py-6 text-center text-sm text-text-tertiary italic">
          {t('shell_no_recent_documents')}
        </div>
      ) : (
        <ul className="divide-y divide-border">
          {docs.map(doc => (
            <li key={doc.id}>
              <Link
                to={`/office/${doc.id}`}
                className="flex items-center gap-3 px-4 py-2.5 hover:bg-surface-1 transition-colors"
              >
                <span className="text-lg shrink-0">{doc.icon ?? '📄'}</span>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-1">
                    <p className="text-sm text-text-primary truncate">{doc.title || t('common_untitled')}</p>
                    {doc.is_starred && <Star size={11} className="text-amber-400 fill-amber-400 shrink-0" />}
                  </div>
                  <p className="text-xs text-text-tertiary mt-0.5">
                    {formatDistanceToNow(parseISO(doc.updated_at), { locale: getDateLocale(i18n.language), addSuffix: true })}
                    {doc.word_count > 0 && ` · ${t('shell_word_count', { count: doc.word_count })}`}
                  </p>
                </div>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </DashboardWidget>
  )
}
