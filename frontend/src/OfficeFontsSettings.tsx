import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Type, Plus, Trash2, ExternalLink, HardDrive } from 'lucide-react'
import { fontsApi } from './api'
import { ModuleServiceRegistry } from '@kubuno/sdk'
import { Button, Input, Spinner } from '@ui'

// ── Google Fonts URL builder ──────────────────────────────────────────────────

function googleFontsUrl(family: string): string {
  const encoded = family.trim().replace(/ /g, '+')
  return `https://fonts.googleapis.com/css2?family=${encoded}:ital,wght@0,400;0,700;1,400&display=swap`
}

const FONT_EXTS = ['ttf', 'otf', 'woff', 'woff2', 'eot']

function isFontFile(url: string): boolean {
  const ext = url.split('?')[0].split('.').pop()?.toLowerCase() ?? ''
  return FONT_EXTS.includes(ext)
}

function injectFont(importUrl: string, cssFamily: string) {
  const id = `kfont-${cssFamily.replace(/[^a-z0-9]/gi, '-')}`
  if (document.getElementById(id)) return

  if (isFontFile(importUrl)) {
    // Raw font file — inject via @font-face
    const style  = document.createElement('style')
    style.id     = id
    style.textContent = `@font-face { font-family: "${cssFamily}"; src: url("${importUrl}"); }`
    document.head.appendChild(style)
  } else {
    // CSS stylesheet (Google Fonts or custom CSS URL)
    const link = document.createElement('link')
    link.id    = id
    link.rel   = 'stylesheet'
    link.href  = importUrl
    document.head.appendChild(link)
  }
}

// ── Add font form ─────────────────────────────────────────────────────────────

type FontSource = 'google' | 'url' | 'drive'

function AddFontForm({ onAdded }: { onAdded: () => void }) {
  const { t } = useTranslation('office')
  const qc = useQueryClient()
  const [source,    setSource]    = useState<FontSource>('google')
  const [fontName,  setFontName]  = useState('')
  const [customUrl, setCustomUrl] = useState('')
  const [driveUrl,  setDriveUrl]  = useState('')
  const [preview,   setPreview]   = useState(false)

  const importUrl = source === 'google'
    ? googleFontsUrl(fontName)
    : source === 'url'
      ? customUrl
      : driveUrl
  const cssFamily = fontName.trim()

  useEffect(() => {
    if (!preview || !fontName.trim()) return
    injectFont(importUrl, cssFamily)
  }, [preview, fontName, importUrl, cssFamily])

  const openDriveDialog = async () => {
    const openFilePicker = ModuleServiceRegistry.get<(opts: object) => Promise<{ id: string; name: string } | null>>('drive', 'openFilePicker')
    if (!openFilePicker) return
    const file = await openFilePicker({
      title:            t('fonts_picker_title'),
      acceptExtensions: ['ttf', 'otf', 'woff', 'woff2', 'eot'],
    })
    if (!file) return
    const url = `/api/v1/drive/${file.id}/download`
    setDriveUrl(url)
    // Pre-fill font name from filename (strip extension)
    const base = file.name.replace(/\.[^.]+$/, '')
    if (!fontName.trim()) setFontName(base)
    setPreview(false)
  }

  const { mutate, isPending, error } = useMutation({
    mutationFn: () => fontsApi.add({
      name:       fontName.trim(),
      css_family: cssFamily,
      source:     source === 'drive' ? 'url' : source,
      import_url: importUrl,
    }),
    onSuccess: () => {
      injectFont(importUrl, cssFamily)
      qc.invalidateQueries({ queryKey: ['office-fonts'] })
      setFontName('')
      setCustomUrl('')
      setDriveUrl('')
      setPreview(false)
      onAdded()
    },
  })

  const canAdd = fontName.trim().length > 0 && (
    source === 'google' ||
    (source === 'url' && customUrl.trim().length > 0) ||
    (source === 'drive' && driveUrl.length > 0)
  )

  const SOURCE_LABELS: Record<FontSource, string> = {
    google: 'Google Fonts',
    url:    t('fonts_source_url'),
    drive:  t('fonts_source_drive'),
  }

  return (
    <div className="space-y-3">
      {/* Source selector */}
      <div className="flex gap-2">
        {(['google', 'url', 'drive'] as FontSource[]).map(s => (
          <button
            key={s}
            onClick={() => { setSource(s); setPreview(false) }}
            className={`flex-1 py-1.5 text-xs rounded-lg border transition-colors ${
              source === s
                ? 'border-primary bg-primary/5 text-primary font-medium'
                : 'border-border text-text-secondary hover:bg-surface-1'
            }`}
          >
            {s === 'drive' && <HardDrive size={10} className="inline mr-1" />}
            {SOURCE_LABELS[s]}
          </button>
        ))}
      </div>

      {/* Files browser */}
      {source === 'drive' && (
        <div className="space-y-2">
          <button
            type="button"
            onClick={openDriveDialog}
            className="w-full flex items-center justify-center gap-2 px-3 py-2 border border-dashed border-border rounded-lg text-sm text-text-secondary hover:bg-surface-1 transition-colors"
          >
            <HardDrive size={14} />
            {driveUrl ? t('fonts_change_file') : t('fonts_browse_files')}
          </button>
          {driveUrl && (
            <div className="text-[10px] text-text-tertiary px-1 font-mono break-all">
              {driveUrl}
            </div>
          )}
        </div>
      )}

      {/* Font name */}
      <div>
        <label className="text-xs text-text-tertiary mb-1 block">
          {source === 'google' ? t('fonts_name_label_google') : t('fonts_name_label_display')}
        </label>
        <Input
          type="text"
          value={fontName}
          onChange={e => { setFontName(e.target.value); setPreview(false) }}
          placeholder={source === 'google' ? t('fonts_name_placeholder_google') : t('fonts_name_placeholder_display')}
        />
      </div>

      {/* Custom URL */}
      {source === 'url' && (
        <div>
          <label className="text-xs text-text-tertiary mb-1 block flex items-center gap-1">
            {t('fonts_css_url_label')}
            <ExternalLink size={10} />
          </label>
          <Input
            type="url"
            value={customUrl}
            onChange={e => { setCustomUrl(e.target.value); setPreview(false) }}
            placeholder="https://fonts.example.com/font.css"
          />
        </div>
      )}

      {/* Google Fonts link */}
      {source === 'google' && fontName.trim() && (
        <div className="text-[10px] text-text-tertiary px-1">
          {t('fonts_generated_url')}{' '}
          <span className="font-mono break-all">{googleFontsUrl(fontName)}</span>
        </div>
      )}

      {/* Preview */}
      {preview && fontName.trim() && (
        <div
          className="px-3 py-2.5 border border-border rounded-lg bg-surface-1 text-sm"
          style={{ fontFamily: `"${cssFamily}", serif` }}
        >
          {t('fonts_preview_pangram')}
          <span className="italic ml-2">The quick brown fox jumps over the lazy dog.</span>
        </div>
      )}

      {error && (
        <p className="text-xs text-danger">{(error as Error).message}</p>
      )}

      <div className="flex gap-2">
        {canAdd && !preview && (
          <Button
            type="button"
            variant="secondary"
            onClick={() => setPreview(true)}
            className="flex-1"
          >
            {t('fonts_preview_button')}
          </Button>
        )}
        <Button
          className="flex-1"
          icon={<Plus size={13} />}
          onClick={() => { if (canAdd) mutate() }}
          disabled={!canAdd}
          loading={isPending}
        >
          {isPending ? t('fonts_adding') : t('fonts_add_button')}
        </Button>
      </div>
    </div>
  )
}

// ── Main ──────────────────────────────────────────────────────────────────────

export default function OfficeFontsSettings() {
  const { t } = useTranslation('office')
  const qc = useQueryClient()

  const { data = [], isLoading } = useQuery({
    queryKey: ['office-fonts'],
    queryFn:  fontsApi.list,
  })

  // Inject all saved fonts on mount
  useEffect(() => {
    data.forEach(f => injectFont(f.import_url, f.css_family))
  }, [data])

  const { mutate: del, isPending: deleting, variables: deletingId } = useMutation({
    mutationFn: (id: string) => fontsApi.delete(id),
    onSuccess:  () => qc.invalidateQueries({ queryKey: ['office-fonts'] }),
  })

  return (
    <div className="max-w-xl mx-auto px-6 py-8">
      <div className="flex items-center gap-2 mb-6">
        <Type size={20} className="text-primary" />
        <h1 className="text-xl font-semibold text-text-primary">{t('fonts_title')}</h1>
      </div>
      <p className="text-sm text-text-secondary mb-6">
        {t('fonts_intro')}
      </p>

      {/* Installed fonts */}
      <div className="mb-8">
        <h2 className="text-xs font-semibold text-text-tertiary uppercase tracking-wider mb-3">
          {t('fonts_installed_count', { count: data.length })}
        </h2>

        {isLoading ? (
          <div className="flex justify-center py-8">
            <Spinner size="md" />
          </div>
        ) : data.length === 0 ? (
          <p className="text-sm text-text-tertiary italic py-4 text-center">
            {t('fonts_empty')}
          </p>
        ) : (
          <div className="space-y-2">
            {data.map(font => (
              <div key={font.id}
                className="flex items-center gap-3 px-4 py-3 border border-border rounded-xl bg-white hover:shadow-sm transition-shadow group">
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium text-text-primary" style={{ fontFamily: `"${font.css_family}", sans-serif` }}>
                    {font.name}
                  </div>
                  <div className="text-[10px] text-text-tertiary mt-0.5">
                    {font.source === 'google'
                      ? 'Google Fonts'
                      : font.import_url.includes('/api/v1/drive/')
                        ? t('fonts_source_drive')
                        : t('fonts_source_url_short')
                    } · {font.css_family}
                  </div>
                </div>
                {/* Preview snippet */}
                <div className="text-text-tertiary text-xs italic shrink-0 hidden sm:block" style={{ fontFamily: `"${font.css_family}", sans-serif` }}>
                  AaBbCc
                </div>
                <button
                  onClick={() => del(font.id)}
                  disabled={deleting && deletingId === font.id}
                  className="p-1.5 rounded-lg text-text-tertiary hover:text-danger hover:bg-danger/10
                             opacity-0 group-hover:opacity-100 transition-all"
                >
                  {deleting && deletingId === font.id
                    ? <Spinner size="xs" />
                    : <Trash2 size={14} />}
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Add font */}
      <div className="border border-border rounded-xl p-5 bg-surface-1/50">
        <h2 className="text-xs font-semibold text-text-tertiary uppercase tracking-wider mb-4 flex items-center gap-1.5">
          <Plus size={11} /> {t('fonts_add_section')}
        </h2>
        <AddFontForm onAdded={() => {}} />
      </div>

      <p className="text-xs text-text-tertiary mt-6 text-center">
        {t('fonts_tip')}{' '}
        <a href="https://fonts.google.com" target="_blank" rel="noopener noreferrer"
          className="text-primary hover:underline">fonts.google.com</a>
      </p>
    </div>
  )
}
