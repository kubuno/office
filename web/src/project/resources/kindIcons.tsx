import type { ComponentType, CSSProperties } from 'react'
import { UserRound, Briefcase, Wrench, Building2, Package, KeyRound, Server, Database, Landmark, Banknote } from 'lucide-react'

/** Lucide glyph for each resource kind (KIND_META carries only data, not JSX). */
export const KIND_ICON: Record<string, ComponentType<{ size?: number; className?: string; style?: CSSProperties; color?: string }>> = {
  person:         UserRound,
  contractor:     Briefcase,
  equipment:      Wrench,
  facility:       Building2,
  material:       Package,
  software:       KeyRound,
  infrastructure: Server,
  information:    Database,
  financial:      Landmark,
  cost:           Banknote,
}
