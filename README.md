<!--
  SPDX-FileCopyrightText: 2026 Kubuno contributors
  SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Kubuno Office

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/Rust-edition_2021-orange.svg)
![React](https://img.shields.io/badge/React-19-61dafb.svg)
![Module](https://img.shields.io/badge/Kubuno-module-4D38DB.svg)

**Kubuno Office — the collaborative office suite.**

A module for [Kubuno](https://github.com/kubuno/core), the self-hosted, libre (AGPLv3) cloud platform.

## Apps

Office is a suite of collaborative editors, each reachable under `/office/<app>`:

| App | Path | What it does |
|---|---|---|
| 📄 **Documents** | `/office/documents` | Word processor (headers/footers, sections, styles, PDF export) |
| 📊 **Spreadsheets** | `/office/spreadsheets` | Spreadsheet with a ~60-function formula engine |
| 🖼️ **Presentations** | `/office/presentations` | Slide decks |
| 📅 **Projects** | `/office/projects` | Project management & Gantt charts |
| 🔗 **Diagrams** | `/office/diagrams` | Diagramming (shapes, connectors) |
| 📈 **Data** | `/office/data` | BI / reporting (SQL+JSON query engine, native charts) |
| ⚡ **Script** | `/office/script` | Code / scripting editor |
| ∑ **Maths** | `/office/maths` | LaTeX formula editor (KaTeX) |
| 🗒️ **Whiteboard** | `/office/whiteboard` | Collaborative whiteboard |

All editors share real-time collaboration (Yjs) and store their content as Kubuno files.

## Architecture

A standalone Rust process that registers with the [core](https://github.com/kubuno/core) at startup; the core proxies its routes (`/api/v1/office/*`) and serves its runtime-loaded React frontend bundle.

- **Backend** — `src/`: Axum + SQLx (PostgreSQL, schema `office`); migrations in `migrations/`.
- **Frontend** — `frontend/`: a React bundle built to `entry.js`, consuming `@kubuno/sdk`, `@kubuno/ui` and `@kubuno/drive` from npm (provided by the host at runtime via the import map).

## Build

**Requirements:** Rust ≥ 1.82, Node.js ≥ 20, PostgreSQL 16.

```bash
cargo build --release                     # → target/release/kubuno-office
cd frontend && npm ci && npm run build     # → dist/{entry.js, entry.css}
bash build_deb.sh                          # → dist/kubuno-office_*.deb
```

> Shared dependencies come from Kubuno — no `kubuno/core` checkout required:
> - **Rust** — shared crates via tagged git dependencies on `kubuno/core`.
> - **Frontend** — `@kubuno/sdk`, `@kubuno/ui`, `@kubuno/drive` from the `@kubuno` npm scope.

## License

[AGPL-3.0-or-later](LICENSE) © Kubuno contributors.
