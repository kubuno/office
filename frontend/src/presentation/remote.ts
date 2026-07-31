// Télécommande de diaporama — le téléphone pilote le diaporama qui tourne sur un
// AUTRE écran (façon « Présenter sur un autre écran » de Google Slides / mode
// Présentateur de PowerPoint).
//
// Aucun aller-retour serveur : la présentation a déjà un canal collab Yjs par
// document (`office-presentation:<id>`), dont l'AWARENESS est un état partagé
// éphémère, exactement ce qu'il faut pour des commandes. Chaque session publie :
//   · `dev`  — ce qu'elle est (mobile ou non) → le téléphone sait quels écrans
//              sont pilotables ;
//   · `ctl`  — la dernière COMMANDE émise (téléphone) : { seq, cmd, arg, target } ;
//   · `show` — l'état du diaporama en cours (écran) : { current, total, step, running }.
// Le récepteur applique une commande quand `seq` augmente : rejouer un état
// d'awareness (reconnexion) ne rejoue donc pas les commandes déjà appliquées.
import { useEffect, useReducer } from 'react'
import type { Awareness } from 'y-protocols/awareness'

/** Re-rend le composant à chaque changement d'awareness (états volatils). */
export function useAwarenessTick(awareness: Awareness | null): void {
  const [, force] = useReducer((n: number) => n + 1, 0)
  useEffect(() => {
    if (!awareness) return
    awareness.on('change', force)
    return () => awareness.off('change', force)
  }, [awareness])
}

export type RemoteCmdKind = 'start' | 'next' | 'prev' | 'goto' | 'black' | 'stop'

export interface RemoteCmd {
  seq: number
  cmd: RemoteCmdKind
  /** Index de diapositive pour `start` / `goto`. */
  arg?: number
  /** clientID de l'écran visé (une seule machine réagit). */
  target: number
}

export interface ShowState {
  current: number
  total: number
  step: number
  running: boolean
  black?: boolean
}

export interface DeviceInfo {
  mobile: boolean
}

/** Pointeur LASER piloté depuis la télécommande (coordonnées en fraction de diapo). */
export interface LaserState {
  x: number
  y: number
  on: boolean
  /** clientID de l'écran visé. */
  target: number
}

/** Un écran candidat pour recevoir le diaporama (session non mobile connectée). */
export interface RemoteScreen {
  clientId: number
  name: string
  color?: string
  /** Un diaporama y tourne déjà. */
  running: boolean
}

interface AwarenessState {
  user?: { name?: string; color?: string }
  dev?: DeviceInfo
  ctl?: RemoteCmd
  show?: ShowState
  laser?: LaserState
}

/** Écrans pilotables (toutes les sessions NON mobiles autres que soi). */
export function listScreens(awareness: Awareness | null): RemoteScreen[] {
  if (!awareness) return []
  const out: RemoteScreen[] = []
  awareness.getStates().forEach((raw, clientId) => {
    if (clientId === awareness.clientID) return
    const st = raw as AwarenessState
    if (!st.dev || st.dev.mobile) return
    out.push({
      clientId,
      name: st.user?.name || `Écran ${clientId}`,
      color: st.user?.color,
      running: !!st.show?.running,
    })
  })
  return out
}

/** État publié par l'écran piloté (pour l'afficher sur la télécommande). */
export function readShow(awareness: Awareness | null, clientId: number | null): ShowState | null {
  if (!awareness || clientId == null) return null
  const st = awareness.getStates().get(clientId) as AwarenessState | undefined
  return st?.show ?? null
}

/** Dernière commande adressée à CETTE session (toutes télécommandes confondues). */
export function readCmdFor(awareness: Awareness | null): RemoteCmd | null {
  if (!awareness) return null
  let best: RemoteCmd | null = null
  awareness.getStates().forEach((raw, clientId) => {
    if (clientId === awareness.clientID) return
    const c = (raw as AwarenessState).ctl
    if (!c || c.target !== awareness.clientID) return
    if (!best || c.seq > best.seq) best = c
  })
  return best
}

/** Laser adressé à CETTE session (null si aucune télécommande ne le pointe). */
export function readLaserFor(awareness: Awareness | null): LaserState | null {
  if (!awareness) return null
  let found: LaserState | null = null
  awareness.getStates().forEach((raw, clientId) => {
    if (clientId === awareness.clientID) return
    const l = (raw as AwarenessState).laser
    if (l && l.on && l.target === awareness.clientID) found = l
  })
  return found
}
