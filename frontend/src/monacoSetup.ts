import { loader } from '@monaco-editor/react'

// Explicitly pin the Monaco CDN version to match the installed package (0.55.1).
// This ensures the CDN URL is stable and matches the types used at build time.
loader.config({
  paths: { vs: 'https://cdn.jsdelivr.net/npm/monaco-editor@0.55.1/min/vs' },
})
