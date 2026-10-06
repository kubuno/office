// Macros of the open document, for the ribbon's « Macros » group.
//
// Documents store their macros through the script attachment (`macrosApi` +
// `scriptsApi`), the same source the existing `MacrosMenu` reads — the ribbon
// dialog and that menu therefore always show the same list.
import { useCallback } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { macrosApi, scriptsApi } from '../script-api'
import { runMacro, type MacroResult } from './runtime'

export interface DocumentMacro {
  key: string        // attachment id (what delete needs)
  scriptId: string
  label: string
}

/** Word shows the leading comment of a macro as its description. */
export function describeSource(source: string): string {
  const lines: string[] = []
  for (const raw of source.split('\n')) {
    const line = raw.trim()
    if (!line) { if (lines.length) break; continue }
    if (!line.startsWith('//')) break
    lines.push(line.replace(/^\/\/\s?/, ''))
  }
  return lines.join('\n')
}

export function useDocumentMacros(docId: string, docTitle: string) {
  const qc = useQueryClient()
  const key = ['doc-macros', 'document', docId] as const
  const invalidate = () => qc.invalidateQueries({ queryKey: key })

  const list = useQuery({
    queryKey: key,
    enabled: !!docId,
    queryFn: async (): Promise<Array<DocumentMacro & { description: string }>> => {
      const { macros } = await macrosApi.listForDocument('document', docId)
      // The description lives in the script, so each row needs its source.
      return Promise.all(macros.map(async m => {
        let description = ''
        try { description = describeSource((await scriptsApi.get(m.script_id)).script.source_code) }
        catch { /* script illisible : on affiche la macro sans description */ }
        return { key: m.id, scriptId: m.script_id, label: m.button_label, description }
      }))
    },
  })

  const sourceOf = useCallback(async (scriptId: string) => {
    return (await scriptsApi.get(scriptId)).script.source_code
  }, [])

  const run = useCallback(async (scriptId: string, buildApi: () => unknown): Promise<MacroResult> => {
    const source = await sourceOf(scriptId)
    return runMacro(source, buildApi() as Record<string, unknown>, Date.now())
  }, [sourceOf])

  /** Create a macro from `source` (recording) or from a template, and return its script id. */
  const create = useMutation({
    mutationFn: async ({ name, source }: { name: string; source: string }) => {
      const { script } = await scriptsApi.create({ name: `${name} — ${docTitle}`, source_code: source })
      await macrosApi.create({
        script_id: script.id, document_type: 'document', document_id: docId, button_label: name,
      })
      return script.id
    },
    onSuccess: invalidate,
  })

  const remove = useMutation({
    mutationFn: async (attachmentId: string) => { await macrosApi.delete(attachmentId) },
    onSuccess: invalidate,
  })

  return {
    macros: list.data ?? [],
    loading: list.isLoading,
    sourceOf,
    run,
    create,
    remove,
  }
}
