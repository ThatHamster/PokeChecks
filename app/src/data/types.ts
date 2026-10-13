// The shapes of the server's /api/v1 responses (see src/api.rs and pokechecks-core).

/** Bumped by the server when a response changes in a way this app can't read. */
export const SUPPORTED_SCHEMA = 1;

export type FormatKey = 'vgc' | 'vgc-bo3' | 'bss';

export interface Format {
  key: FormatKey;
  label: string;
  style: 'Doubles' | 'Singles';
  regulation: string;
  cutoffs: number[];
  default_cutoff: number;
}

export interface Manifest {
  schema: number;
  /** The month the battles were played, e.g. `2026-09`. */
  month: string;
  formats: Format[];
}

export interface Share {
  name: string;
  pct: number;
}

export interface Spread {
  nature: string;
  /** Stat points, HP/Atk/Def/SpA/SpD/Spe. */
  points: number[];
  pct: number;
}

export interface Mon {
  name: string;
  rank: number;
  /** Weighted usage, in percent. */
  usage: number;
  /** Last month's usage, when the format existed then. */
  prev: number | null;
  abilities: Share[];
  items: Share[];
  moves: Share[];
  tera: Share[];
  teammates: string[];
  spreads: Spread[];
  natures: Share[];
  pair_rates: Record<string, number>;
}

export interface Usage {
  month: string;
  format: FormatKey;
  battles: number;
  cutoff: number;
  mons: Mon[];
}

export interface ListMon {
  id: string;
  name: string;
  item: string | null;
  ability: string | null;
  nature: string | null;
  moves: string[];
}

export interface EntryTeam {
  placing: number;
  player: string;
  record: [number, number, number] | null;
  members: ListMon[];
}

export interface TournamentEvent {
  id: string;
  name: string;
  date: string;
  players: number;
  teams: EntryTeam[];
}

export interface Tournaments {
  regulation: string;
  events: TournamentEvent[];
}
