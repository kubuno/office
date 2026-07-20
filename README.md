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
| 📄 **Documents** | `/office/documents` | Word processor (sections, styles gallery, comments, footnotes, table of contents, advanced tables, PDF export) |
| 📊 **Spreadsheets** | `/office/spreadsheets` | Spreadsheet with a 300+ function formula engine, pivot tables, protection & encryption |
| 🖼️ **Presentations** | `/office/presentations` | Slide decks |
| 📅 **Projects** | `/office/projects` | Project management & Gantt charts |
| 🔗 **Diagrams** | `/office/diagrams` | Diagramming (shapes, connectors) |
| 📈 **Data** | `/office/data` | BI / reporting (SQL+JSON query engine, native charts) |
| ⚡ **Script** | `/office/script` | Code / scripting editor |
| ∑ **Maths** | `/office/maths` | Formula editor (KaTeX) with a built-in symbolic engine |
| 🗒️ **Whiteboard** | `/office/whiteboard` | Collaborative whiteboard |

All editors share real-time collaboration (Yjs) and store their content as Kubuno files.

## Highlights

- **Documents** — a paginated, canvas-rendered word processor in the spirit of Word: margin-anchored
  comments, footnotes, a regenerable table of contents, heading numbering, advanced font controls
  (small caps, letter spacing), drop caps, per-section margins, and full-featured tables (repeated
  header rows, sorting, column distribution, split, custom borders, `SUM` formulas). Selections can
  be dragged & dropped or resized by their edges; rulers follow the page under the caret; spell
  checking ships with a per-language dictionary picker; find & replace plugs into the platform's
  standard search bar.
- **Spreadsheets** — a formula engine with **300+ functions** across math, statistics, text, date,
  logical, financial, engineering and lookup families, including dynamic arrays (`FILTER`, `SORT`,
  `UNIQUE`, `XLOOKUP`, …) and the modern `GROUPBY` / `PIVOTBY` aggregations. On top of that:
  persistent pivot tables, cell comments, Goal Seek, a print dialog (area, scaling, paper formats),
  sheet password protection (OOXML-compatible hashes) and **whole-workbook encryption** — cells are
  stored encrypted at rest and never travel in clear.
- **Maths** — a WYSIWYG + LaTeX formula editor backed by a small symbolic engine: derivatives,
  simplification, root finding, tangents, extrema, Taylor expansions, function tables and plots, plus
  matrix operations, descriptive statistics & regression, and number-theory helpers. The code editor
  offers rich LaTeX autocompletion, and documents print cleanly to any paper format. Formulas can be
  copied as Kubuno data envelopes and pasted as live cards into other modules (chat, notes,
  documents…).
- **A shared ribbon**, Office-style: a File backstage, contextual tabs, a clipboard group in every
  editor, and responsive behaviour — when the window narrows, ribbon groups collapse one by one into
  dropdown buttons instead of overflowing.
- **Local-first sync** — every sub-module (documents, spreadsheets, presentations, diagrams,
  whiteboard) exposes a cursor-based `/delta` endpoint (change sequences + tombstones) so desktop
  and offline clients can pull incremental changes and replay local creations with client-minted ids.

## Architecture

A standalone Rust process that registers with the [core](https://github.com/kubuno/core) at startup; the core proxies its routes (`/api/v1/office/*`) and serves its runtime-loaded React frontend bundle.

- **Backend** — `src/`: Axum + SQLx (PostgreSQL, schema `office`); migrations in `migrations/`.
- **Frontend** — `frontend/`: a React bundle built to `entry.js`, consuming `@kubuno/sdk`, `@kubuno/ui` and `@kubuno/drive` from npm (provided by the host at runtime via the import map).

## Install

This module ships in the **all-in-one [Kubuno](https://github.com/kubuno/core) Docker image** (`ghcr.io/kubuno/kubuno`) — the easiest way to self-host a full Kubuno instance (core + every module). See **[kubuno/docker](https://github.com/kubuno/docker)** for `docker compose` instructions.

Native packages are also built by CI and attached to each tagged [GitHub Release](https://github.com/kubuno/office/releases):

- **Debian/Ubuntu** — `kubuno-office_*.deb`
- **Fedora/RHEL/openSUSE** — `kubuno-office-*.rpm`
- **Windows** — `kubuno-office-setup-*.exe` (NSIS installer)
- **macOS** — `kubuno-office-*.pkg`

Each package installs the module into an existing Kubuno core installation and restarts the service.

To build this module from source, see below.

## Build

**Requirements:** Rust ≥ 1.82, Node.js ≥ 24, PostgreSQL 16.

```bash
cargo build --release                     # → target/release/kubuno-office
cd frontend && npm ci && npm run build     # → dist/{entry.js, entry.css}
bash build_deb.sh                          # → dist/kubuno-office_*.deb
```

Other platforms use the matching script: `build_rpm.sh` (Fedora/RHEL/openSUSE),
`build_windows.sh` (NSIS installer, cross-compilable from Linux with `cargo-xwin`) and
`build_macos.sh` (`.pkg`, run on a Mac).

> Shared dependencies come from Kubuno — no `kubuno/core` checkout required:
> - **Rust** — shared crates via tagged git dependencies on `kubuno/core`.
> - **Frontend** — `@kubuno/sdk`, `@kubuno/ui`, `@kubuno/drive` from the `@kubuno` npm scope.

## License

[AGPL-3.0-or-later](LICENSE) © Kubuno contributors.
