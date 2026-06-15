import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '@kubuno/sdk'
import { FileText, Save, ChevronLeft, ExternalLink } from 'lucide-react'
import { Link } from 'react-router-dom'
import OfficeFontsSettings from './OfficeFontsSettings'
import { Toggle, Button, Tabs } from '@ui'

type Tab = 'fonts' | 'documents' | 'spreadsheets' | 'about'

interface OfficeSettings {
  'office.default_format': string
  'office.autosave_interval_s': number
  'office.track_changes_default': boolean
  'office.default_margins': string
  'office.spreadsheet_default_format': string
  'office.spreadsheet_autosave_s': number
  'office.spreadsheet_header_row': boolean
  'office.spreadsheet_decimal_sep': string
}

const FORMAT_OPTIONS = [
  { value: 'docx', label: '.docx', descKey: 'settings_format_docx_desc' },
  { value: 'odt',  label: '.odt',  descKey: 'settings_format_odt_desc' },
]

const AUTOSAVE_OPTIONS = [
  { value: 5,  labelKey: 'settings_autosave_5s' },
  { value: 30, labelKey: 'settings_autosave_30s' },
  { value: 60, labelKey: 'settings_autosave_1min' },
  { value: 0,  labelKey: 'settings_autosave_off' },
]

const MARGIN_OPTIONS = [
  { value: 'narrow',  labelKey: 'settings_margin_narrow',  desc: '1.27 cm' },
  { value: 'normal',  labelKey: 'settings_margin_normal',  desc: '2.54 cm' },
  { value: 'wide',    labelKey: 'settings_margin_wide',    desc: '3.81 cm' },
]

function useAdminSettings() {
  return useQuery({
    queryKey: ['admin-settings'],
    queryFn: () =>
      api.get<{ settings: { key: string; value: unknown }[] }>('/admin/settings').then((r) => {
        const map: Record<string, unknown> = {}
        r.data.settings.forEach((s) => { map[s.key] = s.value })
        return map as unknown as OfficeSettings
      }),
  })
}

function DocumentsTab() {
  const { t } = useTranslation('office')
  const queryClient = useQueryClient()
  const { data: settings } = useAdminSettings()

  const [format, setFormat]         = useState<string | null>(null)
  const [autosave, setAutosave]     = useState<number | null>(null)
  const [trackChanges, setTrack]    = useState<boolean | null>(null)
  const [margins, setMargins]       = useState<string | null>(null)

  const currentFormat       = format       ?? (settings?.['office.default_format']          ?? 'docx')
  const currentAutosave     = autosave     ?? (settings?.['office.autosave_interval_s']      ?? 30)
  const currentTrackChanges = trackChanges ?? (settings?.['office.track_changes_default']    ?? false)
  const currentMargins      = margins      ?? (settings?.['office.default_margins']          ?? 'normal')

  const isDirty = format !== null || autosave !== null || trackChanges !== null || margins !== null

  const save = useMutation({
    mutationFn: (updates: Record<string, unknown>) => api.patch('/admin/settings', updates),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['admin-settings'] })
      setFormat(null)
      setAutosave(null)
      setTrack(null)
      setMargins(null)
    },
  })

  function handleSave() {
    const updates: Record<string, unknown> = {}
    if (format       !== null) updates['office.default_format']       = format
    if (autosave     !== null) updates['office.autosave_interval_s']  = autosave
    if (trackChanges !== null) updates['office.track_changes_default'] = trackChanges
    if (margins      !== null) updates['office.default_margins']      = margins
    if (Object.keys(updates).length > 0) save.mutate(updates)
  }

  return (
    <div>
      <div className="bg-white rounded-xl border border-border divide-y divide-border">
        {/* Default format */}
        <div className="p-5">
          <p className="text-sm font-medium text-text-primary mb-1">{t('settings_doc_format_title')}</p>
          <p className="text-xs text-text-secondary mb-3">
            {t('settings_doc_format_help')}
          </p>
          <div className="flex gap-3">
            {FORMAT_OPTIONS.map(opt => (
              <button
                key={opt.value}
                onClick={() => setFormat(opt.value)}
                className={`flex-1 max-w-[160px] py-3 rounded-xl border text-center transition-colors ${
                  currentFormat === opt.value
                    ? 'border-primary bg-primary-light text-primary'
                    : 'border-border hover:bg-surface-1 text-text-secondary'
                }`}
              >
                <p className="text-sm font-semibold font-mono">{opt.label}</p>
                <p className="text-xs mt-0.5 opacity-70">{t(opt.descKey)}</p>
              </button>
            ))}
          </div>
        </div>

        {/* Autosave */}
        <div className="p-5">
          <p className="text-sm font-medium text-text-primary mb-1">
            {t('settings_autosave_title')}
          </p>
          <p className="text-xs text-text-secondary mb-3">
            {t('settings_doc_autosave_help')}
          </p>
          <div className="flex flex-wrap gap-2">
            {AUTOSAVE_OPTIONS.map(opt => (
              <button
                key={opt.value}
                onClick={() => setAutosave(opt.value)}
                className={`px-4 py-1.5 rounded-full text-sm border transition-colors ${
                  currentAutosave === opt.value
                    ? 'border-primary bg-primary-light text-primary font-medium'
                    : 'border-border hover:bg-surface-1 text-text-secondary'
                }`}
              >
                {t(opt.labelKey)}
              </button>
            ))}
          </div>
        </div>

        {/* Default margins */}
        <div className="p-5">
          <p className="text-sm font-medium text-text-primary mb-1">{t('settings_margins_title')}</p>
          <p className="text-xs text-text-secondary mb-3">
            {t('settings_margins_help')}
          </p>
          <div className="flex gap-3">
            {MARGIN_OPTIONS.map(opt => (
              <button
                key={opt.value}
                onClick={() => setMargins(opt.value)}
                className={`flex-1 max-w-[120px] py-3 rounded-xl border text-center transition-colors ${
                  currentMargins === opt.value
                    ? 'border-primary bg-primary-light text-primary'
                    : 'border-border hover:bg-surface-1 text-text-secondary'
                }`}
              >
                <p className="text-sm font-semibold">{t(opt.labelKey)}</p>
                <p className="text-xs mt-0.5 opacity-70 font-mono">{opt.desc}</p>
              </button>
            ))}
          </div>
        </div>

        {/* Track changes toggle */}
        <div className="p-5 flex items-start justify-between gap-4">
          <div>
            <p className="text-sm font-medium text-text-primary">{t('settings_track_changes_title')}</p>
            <p className="text-xs text-text-secondary mt-0.5">
              {t('settings_track_changes_help')}
            </p>
          </div>
          <Toggle checked={currentTrackChanges} onChange={() => setTrack(!currentTrackChanges)} />
        </div>
      </div>

      <div className="mt-4 flex justify-end">
        <Button onClick={handleSave} disabled={!isDirty || save.isPending}>
          <Save size={15} />
          {save.isPending ? t('settings_saving') : t('common_save')}
        </Button>
      </div>
    </div>
  )
}

const SPREADSHEET_FORMAT_OPTIONS = [
  { value: 'xlsx', label: '.xlsx', descKey: 'settings_format_xlsx_desc' },
  { value: 'ods',  label: '.ods',  descKey: 'settings_format_ods_desc' },
  { value: 'csv',  label: '.csv',  descKey: 'settings_format_csv_desc' },
]

const DECIMAL_SEP_OPTIONS = [
  { value: ',', labelKey: 'settings_decimal_comma', example: '1 234,56' },
  { value: '.', labelKey: 'settings_decimal_dot',   example: '1,234.56' },
]

function SpreadsheetsTab() {
  const { t } = useTranslation('office')
  const queryClient = useQueryClient()
  const { data: settings } = useAdminSettings()

  const [format, setFormat]       = useState<string | null>(null)
  const [autosave, setAutosave]   = useState<number | null>(null)
  const [headerRow, setHeaderRow] = useState<boolean | null>(null)
  const [decimalSep, setDecimal]  = useState<string | null>(null)

  const currentFormat    = format    ?? (settings?.['office.spreadsheet_default_format'] ?? 'xlsx')
  const currentAutosave  = autosave  ?? (settings?.['office.spreadsheet_autosave_s']     ?? 30)
  const currentHeaderRow = headerRow ?? (settings?.['office.spreadsheet_header_row']     ?? true)
  const currentDecimal   = decimalSep ?? (settings?.['office.spreadsheet_decimal_sep']   ?? ',')

  const isDirty = format !== null || autosave !== null || headerRow !== null || decimalSep !== null

  const save = useMutation({
    mutationFn: (updates: Record<string, unknown>) => api.patch('/admin/settings', updates),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['admin-settings'] })
      setFormat(null)
      setAutosave(null)
      setHeaderRow(null)
      setDecimal(null)
    },
  })

  function handleSave() {
    const updates: Record<string, unknown> = {}
    if (format    !== null) updates['office.spreadsheet_default_format'] = format
    if (autosave  !== null) updates['office.spreadsheet_autosave_s']     = autosave
    if (headerRow !== null) updates['office.spreadsheet_header_row']     = headerRow
    if (decimalSep !== null) updates['office.spreadsheet_decimal_sep']   = decimalSep
    if (Object.keys(updates).length > 0) save.mutate(updates)
  }

  return (
    <div>
      <div className="bg-white rounded-xl border border-border divide-y divide-border">
        {/* Default format */}
        <div className="p-5">
          <p className="text-sm font-medium text-text-primary mb-1">{t('settings_sheet_format_title')}</p>
          <p className="text-xs text-text-secondary mb-3">
            {t('settings_sheet_format_help')}
          </p>
          <div className="flex gap-3">
            {SPREADSHEET_FORMAT_OPTIONS.map(opt => (
              <button
                key={opt.value}
                onClick={() => setFormat(opt.value)}
                className={`flex-1 max-w-[140px] py-3 rounded-xl border text-center transition-colors ${
                  currentFormat === opt.value
                    ? 'border-primary bg-primary-light text-primary'
                    : 'border-border hover:bg-surface-1 text-text-secondary'
                }`}
              >
                <p className="text-sm font-semibold font-mono">{opt.label}</p>
                <p className="text-xs mt-0.5 opacity-70">{t(opt.descKey)}</p>
              </button>
            ))}
          </div>
        </div>

        {/* Autosave */}
        <div className="p-5">
          <p className="text-sm font-medium text-text-primary mb-1">{t('settings_autosave_title')}</p>
          <p className="text-xs text-text-secondary mb-3">
            {t('settings_sheet_autosave_help')}
          </p>
          <div className="flex flex-wrap gap-2">
            {AUTOSAVE_OPTIONS.map(opt => (
              <button
                key={opt.value}
                onClick={() => setAutosave(opt.value)}
                className={`px-4 py-1.5 rounded-full text-sm border transition-colors ${
                  currentAutosave === opt.value
                    ? 'border-primary bg-primary-light text-primary font-medium'
                    : 'border-border hover:bg-surface-1 text-text-secondary'
                }`}
              >
                {t(opt.labelKey)}
              </button>
            ))}
          </div>
        </div>

        {/* Decimal separator */}
        <div className="p-5">
          <p className="text-sm font-medium text-text-primary mb-1">{t('settings_decimal_title')}</p>
          <p className="text-xs text-text-secondary mb-3">
            {t('settings_decimal_help')}
          </p>
          <div className="flex gap-3">
            {DECIMAL_SEP_OPTIONS.map(opt => (
              <button
                key={opt.value}
                onClick={() => setDecimal(opt.value)}
                className={`flex-1 max-w-[160px] py-3 rounded-xl border text-center transition-colors ${
                  currentDecimal === opt.value
                    ? 'border-primary bg-primary-light text-primary'
                    : 'border-border hover:bg-surface-1 text-text-secondary'
                }`}
              >
                <p className="text-sm font-semibold">{t(opt.labelKey)}</p>
                <p className="text-xs mt-0.5 font-mono opacity-70">{opt.example}</p>
              </button>
            ))}
          </div>
        </div>

        {/* Header row toggle */}
        <div className="p-5 flex items-start justify-between gap-4">
          <div>
            <p className="text-sm font-medium text-text-primary">{t('settings_header_row_title')}</p>
            <p className="text-xs text-text-secondary mt-0.5">
              {t('settings_header_row_help')}
            </p>
          </div>
          <Toggle checked={currentHeaderRow} onChange={() => setHeaderRow(!currentHeaderRow)} />
        </div>
      </div>

      <div className="mt-4 flex justify-end">
        <Button onClick={handleSave} disabled={!isDirty || save.isPending}>
          <Save size={15} />
          {save.isPending ? t('settings_saving') : t('common_save')}
        </Button>
      </div>
    </div>
  )
}

function AboutTab() {
  const { t } = useTranslation('office')
  return (
    <div className="space-y-4">
      <div className="bg-white rounded-xl border border-border overflow-hidden">
        <div className="flex items-center gap-3 px-5 py-4 border-b border-border bg-surface-1">
          <div className="w-10 h-10 rounded-xl bg-blue-100 flex items-center justify-center shrink-0">
            <FileText size={20} className="text-blue-600" />
          </div>
          <div>
            <p className="text-sm font-semibold text-text-primary">Kubuno Office</p>
            <p className="text-xs text-text-tertiary">{t('settings_about_version')}</p>
          </div>
          <span className="ml-auto text-xs font-medium px-2 py-0.5 rounded-full bg-orange-100 text-orange-700">
            Rust
          </span>
        </div>

        <div className="divide-y divide-border">
          <div className="px-5 py-4">
            <p className="text-xs font-semibold text-text-tertiary uppercase tracking-wider mb-2">{t('settings_about_description_label')}</p>
            <p className="text-sm text-text-secondary leading-relaxed">
              {t('settings_about_description')}
            </p>
          </div>

          <div className="px-5 py-4 grid grid-cols-2 gap-4">
            <div>
              <p className="text-xs font-semibold text-text-tertiary uppercase tracking-wider mb-2">{t('settings_about_author_label')}</p>
              <p className="text-sm text-text-primary">Kubuno Contributors</p>
            </div>
            <div>
              <p className="text-xs font-semibold text-text-tertiary uppercase tracking-wider mb-2">{t('settings_about_license_label')}</p>
              <p className="text-sm text-text-primary">AGPL-3.0</p>
            </div>
          </div>

          <div className="px-5 py-4">
            <p className="text-xs font-semibold text-text-tertiary uppercase tracking-wider mb-3">{t('settings_about_tech_label')}</p>
            <div className="flex flex-wrap gap-2">
              {['Rust', 'Axum 0.7', 'SQLx 0.8', 'PostgreSQL 16', 'TipTap', 'React 19'].map(tech => (
                <span key={tech} className="text-xs px-2 py-1 rounded-lg bg-surface-2 text-text-secondary font-mono">{tech}</span>
              ))}
            </div>
          </div>

          <div className="px-5 py-4">
            <p className="text-xs font-semibold text-text-tertiary uppercase tracking-wider mb-2">{t('settings_about_links_label')}</p>
            <a
              href="https://github.com/kubuno/kubuno"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1.5 text-sm text-primary hover:underline"
            >
              <ExternalLink size={13} />
              github.com/kubuno/kubuno
            </a>
          </div>

          {/* Crédits — icônes tierces (collection « Computer and Hardware Duotone », licence MIT) */}
          <div className="px-5 py-4">
            <p className="text-xs font-semibold text-text-tertiary uppercase tracking-wider mb-2">{t('settings_about_credits_label', { defaultValue: 'Crédits' })}</p>
            <p className="text-sm text-text-secondary leading-relaxed mb-1">
              {t('settings_about_icons_credit', { defaultValue: 'Catégorie « Ordinateur et Matériel » de l’éditeur de diagrammes — collection « Computer and Hardware Duotone » (licence MIT).' })}
            </p>
            <p className="text-sm text-text-secondary">
              Vectors and icons by{' '}
              <a href="https://www.svgrepo.com" target="_blank" rel="noopener noreferrer" className="text-primary hover:underline inline-flex items-center gap-1">
                <ExternalLink size={12} /> SVG Repo
              </a>
            </p>
          </div>
        </div>
      </div>
    </div>
  )
}

export default function OfficeSettingsPage() {
  const { t } = useTranslation('office')
  const [tab, setTab] = useState<Tab>('fonts')

  const TABS: { id: Tab; label: string }[] = [
    { id: 'fonts',        label: t('settings_tab_fonts') },
    { id: 'documents',    label: t('settings_tab_documents') },
    { id: 'spreadsheets', label: t('settings_tab_spreadsheets') },
    { id: 'about',        label: t('settings_tab_about') },
  ]

  return (
    <div className="max-w-2xl">
      <div className="flex items-center gap-3 mb-6">
        <Link to="/admin?tab=modules" className="p-1.5 rounded-lg hover:bg-surface-2 text-text-secondary hover:text-text-primary transition-colors">
          <ChevronLeft size={18} />
        </Link>
        <div className="flex items-center gap-2">
          <div className="w-8 h-8 rounded-lg bg-blue-100 flex items-center justify-center">
            <FileText size={16} className="text-blue-600" />
          </div>
          <div>
            <h1 className="text-lg font-medium text-text-primary">{t('settings_page_title')}</h1>
            <p className="text-xs text-text-tertiary">{t('settings_page_subtitle')}</p>
          </div>
        </div>
      </div>

      <Tabs tabs={TABS} value={tab} onChange={setTab} className="mb-6" />

      {tab === 'fonts'        && <OfficeFontsSettings />}
      {tab === 'documents'    && <DocumentsTab />}
      {tab === 'spreadsheets' && <SpreadsheetsTab />}
      {tab === 'about'        && <AboutTab />}
    </div>
  )
}
