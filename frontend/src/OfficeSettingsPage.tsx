import React, { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useAuthStore } from '@kubuno/sdk'
import { FileText, ArrowLeft, ExternalLink, Check } from 'lucide-react'
import { Link, useSearchParams } from 'react-router-dom'
import OfficeFontsSettings from './OfficeFontsSettings'
import { Toggle, Button, Radio } from '@ui'
import { useModulePrefs } from './userPrefs'

type Tab = 'preferences' | 'fonts' | 'about'

// ── Per-user preferences (backend, cross-device via core users.preferences) ─────

interface OfficePrefs {
  [key: string]: unknown // satisfies useModulePrefs<T extends Record<string, unknown>>
  editorTheme:  string   // 'light' | 'dark' | 'sepia'
  showRuler:    boolean  // documents ruler
  showGrid:     boolean  // spreadsheets / diagrams grid
  autoSave:     boolean  // client-side autosave on/off
  defaultFont:  string   // default document font family
  defaultZoom:  string   // '90' | '100' | '125' | '150'
}

const DEFAULT_PREFS: OfficePrefs = {
  editorTheme: 'light', showRuler: true, showGrid: true,
  autoSave: true, defaultFont: 'sans', defaultZoom: '100',
}

// ── Mail-style layout helpers ───────────────────────────────────────────────────

function SettingsRow({ label, description, children }: {
  label: string; description?: string; children: React.ReactNode
}) {
  return (
    <div className="flex items-start gap-8 py-4 border-b border-[#e8eaed] last:border-0">
      <div className="w-60 flex-shrink-0">
        <p className="text-sm text-[#202124] font-normal">{label}</p>
        {description && <p className="text-xs text-text-tertiary mt-0.5 leading-relaxed">{description}</p>}
      </div>
      <div className="flex-1">{children}</div>
    </div>
  )
}

function RadioGroup({ options, value, onChange }: {
  options: { value: string; label: string }[]; value: string; onChange: (v: string) => void
}) {
  return (
    <div className="flex flex-col items-start gap-2">
      {options.map(opt => (
        <Radio key={opt.value} checked={value === opt.value} onChange={() => onChange(opt.value)} label={opt.label} />
      ))}
    </div>
  )
}

// ── Préférences tab (per-user) ──────────────────────────────────────────────────

function PreferencesTab() {
  const { t } = useTranslation('office')
  const { prefs: saved, update } = useModulePrefs<OfficePrefs>('office', DEFAULT_PREFS)
  const [prefs, setPrefs] = useState<OfficePrefs>(saved)
  const [savedFlag, setSavedFlag] = useState(false)
  const [busy, setBusy] = useState(false)

  const set = <K extends keyof OfficePrefs>(key: K, value: OfficePrefs[K]) =>
    setPrefs(p => ({ ...p, [key]: value }))

  const save = async () => {
    setBusy(true)
    try {
      await update(prefs)
      setSavedFlag(true)
      setTimeout(() => setSavedFlag(false), 2500)
    } finally { setBusy(false) }
  }

  return (
    <div>
      <SettingsRow
        label={t('office_pref_editor_theme', { defaultValue: 'Thème de l\'éditeur' })}
        description={t('office_pref_editor_theme_desc', { defaultValue: 'Apparence de la zone d\'édition.' })}
      >
        <RadioGroup
          value={prefs.editorTheme}
          onChange={v => set('editorTheme', v)}
          options={[
            { value: 'light', label: t('office_pref_theme_light', { defaultValue: 'Clair' }) },
            { value: 'dark',  label: t('office_pref_theme_dark',  { defaultValue: 'Sombre' }) },
            { value: 'sepia', label: t('office_pref_theme_sepia', { defaultValue: 'Sépia' }) },
          ]}
        />
      </SettingsRow>

      <SettingsRow
        label={t('office_pref_default_font', { defaultValue: 'Police par défaut des documents' })}
        description={t('office_pref_default_font_desc', { defaultValue: 'Police utilisée à la création d\'un document.' })}
      >
        <RadioGroup
          value={prefs.defaultFont}
          onChange={v => set('defaultFont', v)}
          options={[
            { value: 'sans',  label: t('office_pref_font_sans',  { defaultValue: 'Sans empattement (Inter)' }) },
            { value: 'serif', label: t('office_pref_font_serif', { defaultValue: 'Avec empattement (Serif)' }) },
            { value: 'mono',  label: t('office_pref_font_mono',  { defaultValue: 'Monospace' }) },
          ]}
        />
      </SettingsRow>

      <SettingsRow
        label={t('office_pref_default_zoom', { defaultValue: 'Zoom par défaut' })}
        description={t('office_pref_default_zoom_desc', { defaultValue: 'Niveau de zoom à l\'ouverture d\'un éditeur.' })}
      >
        <RadioGroup
          value={prefs.defaultZoom}
          onChange={v => set('defaultZoom', v)}
          options={[
            { value: '90',  label: '90 %' },
            { value: '100', label: '100 %' },
            { value: '125', label: '125 %' },
            { value: '150', label: '150 %' },
          ]}
        />
      </SettingsRow>

      <SettingsRow label={t('office_pref_ruler', { defaultValue: 'Règle' })}>
        <label className="flex items-center gap-2 cursor-pointer select-none">
          <Toggle checked={prefs.showRuler} onChange={() => set('showRuler', !prefs.showRuler)} />
          <span className="text-sm text-text-primary">{t('office_pref_ruler_on', { defaultValue: 'Afficher la règle dans les documents' })}</span>
        </label>
      </SettingsRow>

      <SettingsRow
        label={t('office_pref_grid', { defaultValue: 'Grille' })}
        description={t('office_pref_grid_desc', { defaultValue: 'Tableur et éditeur de diagrammes.' })}
      >
        <label className="flex items-center gap-2 cursor-pointer select-none">
          <Toggle checked={prefs.showGrid} onChange={() => set('showGrid', !prefs.showGrid)} />
          <span className="text-sm text-text-primary">{t('office_pref_grid_on', { defaultValue: 'Afficher la grille' })}</span>
        </label>
      </SettingsRow>

      <SettingsRow
        label={t('office_pref_autosave', { defaultValue: 'Sauvegarde automatique' })}
        description={t('office_pref_autosave_desc', { defaultValue: 'Enregistrer automatiquement vos modifications pendant la frappe.' })}
      >
        <label className="flex items-center gap-2 cursor-pointer select-none">
          <Toggle checked={prefs.autoSave} onChange={() => set('autoSave', !prefs.autoSave)} />
          <span className="text-sm text-text-primary">{t('office_pref_autosave_on', { defaultValue: 'Activer la sauvegarde automatique' })}</span>
        </label>
      </SettingsRow>

      <div className="pt-5 flex items-center gap-3">
        <Button onClick={save} loading={busy}>
          {savedFlag
            ? <><Check size={14} className="mr-1.5 inline" />{t('office_settings_saved', { defaultValue: 'Enregistré' })}</>
            : t('office_settings_save_changes', { defaultValue: 'Enregistrer les modifications' })}
        </Button>
        <Button variant="ghost" onClick={() => setPrefs(saved)}>
          {t('common_cancel', { defaultValue: 'Annuler' })}
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
  const isAdmin = useAuthStore(s => s.user?.role === 'admin')

  // Contexte « sous-module isolé » : une fenêtre standalone (ex. Documents desktop)
  // ouvre `/office/settings?app=<sous-module>` → on scope la page à ce sous-module
  // (onglets des AUTRES sous-modules masqués, fil d'ariane vers `/office/<app>`).
  const [searchParams] = useSearchParams()
  const scopedApp = searchParams.get('app') || null

  // Onglets propres à un sous-module (les autres onglets = généraux, toujours visibles).
  // Les réglages d'instance des sous-modules ont migré vers la console d'admin du core.
  const SUBMODULE_TABS: Tab[] = []

  // Onglet par défaut : celui du sous-module scopé s'il existe ET que l'utilisateur
  // y a accès (onglets sous-module = admin) ; sinon « Préférences » (par-utilisateur).
  const initialTab: Tab =
    scopedApp && (SUBMODULE_TABS as string[]).includes(scopedApp) && isAdmin
      ? (scopedApp as Tab)
      : 'preferences'
  const [tab, setTab] = useState<Tab>(initialTab)

  // Admin-only tabs hold instance-wide settings (read from /admin/settings) and
  // are hidden for non-admins; the per-user "Préférences" tab is always visible.
  const tabs: { id: Tab; label: string; adminOnly?: boolean }[] = [
    { id: 'preferences',  label: t('office_tab_preferences', { defaultValue: 'Préférences' }) },
    { id: 'fonts',        label: t('settings_tab_fonts'),        adminOnly: true },
    { id: 'about',        label: t('settings_tab_about') },
  ]
  const visibleTabs = tabs.filter(tb => {
    if (tb.adminOnly && !isAdmin) return false
    // Scopé : masque les onglets des AUTRES sous-modules (garde le sien + les généraux).
    if (scopedApp && SUBMODULE_TABS.includes(tb.id) && tb.id !== scopedApp) return false
    return true
  })

  return (
    <div className="flex flex-col h-full bg-white overflow-hidden">
      {/* Breadcrumb header */}
      <div className="flex items-center gap-2 px-6 py-2.5 border-b border-[#e8eaed] flex-shrink-0" style={{ background: '#f8f9fa' }}>
        <Link to={scopedApp ? `/office/${scopedApp}` : '/office'} className="flex items-center gap-1.5 text-sm text-[#1a73e8] hover:underline">
          <ArrowLeft size={14} />
          {scopedApp ? scopedApp.charAt(0).toUpperCase() + scopedApp.slice(1) : 'Office'}
        </Link>
        <span className="text-text-tertiary text-sm">/</span>
        <div className="flex items-center gap-1.5">
          <FileText size={15} className="text-text-secondary" />
          <span className="text-sm text-text-primary">{t('office_settings_title', { defaultValue: 'Réglages' })}</span>
        </div>
      </div>

      {/* Tab bar (Gmail-style) */}
      <div className="flex items-end border-b border-[#e8eaed] px-4 flex-shrink-0 overflow-x-auto overflow-y-hidden" style={{ background: '#fff' }}>
        {visibleTabs.map(tb => (
          <button key={tb.id} onClick={() => setTab(tb.id)}
            className={`px-4 py-3 text-sm border-b-2 -mb-px transition-colors whitespace-nowrap ${
              tab === tb.id ? 'border-[#1a73e8] text-[#1a73e8] font-medium' : 'border-transparent text-[#5f6368] hover:text-[#202124] hover:bg-[#f1f3f4]'}`}>
            {tb.label}
          </button>
        ))}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-3xl mx-auto px-8 py-6">
          {tab === 'preferences'              && <PreferencesTab />}
          {tab === 'fonts'        && isAdmin  && <OfficeFontsSettings />}
          {tab === 'about'                    && <AboutTab />}
        </div>
      </div>
    </div>
  )
}
