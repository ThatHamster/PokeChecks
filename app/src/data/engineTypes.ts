// What the WebAssembly engine (crates/wasm) returns. wasm-bindgen types these as `any`, so
// the wrappers below give them shapes.

import type { Engine } from './engine';
import type { FormatKey } from './types';

/** A set the player changed, replacing the usage defaults. */
export interface Custom {
  item: string | null;
  ability: string | null;
  nature: string;
  /** Stat points, HP/Atk/Def/SpA/SpD/Spe, 0–32 each and 66 in total. */
  points: number[];
  moves: string[];
}

export interface Member {
  name: string;
  custom: Custom | null;
}

export interface SetResult {
  name: string;
  item: string | null;
  ability: string | null;
  nature: string | null;
  points: number[] | null;
  moves: string[];
  /** The player changed this set; otherwise it's the most common one. */
  custom: boolean;
  /** Level 50 stats, HP/Atk/Def/SpA/SpD/Spe. */
  stats: number[];
  spread_percent: number | null;
}

export interface MatchupResult {
  attack: string;
  /** One per member, in team order. */
  multipliers: number[];
  weak: number;
  resist: number;
  problem: boolean;
}

export interface TeamResult {
  members: Member[];
  sets: SetResult[];
  matchups: MatchupResult[];
  repeated_items: string[];
  paste: string;
  suggestions: { name: string; score: number }[];
}

export interface MoveInfo {
  name: string;
  base_power: number;
  move_type: string;
  category: 'Physical' | 'Special' | 'Status';
}

export interface Nature {
  name: string;
  /** `Adamant (+Atk −SpA)` or `Hardy (neutral)`. */
  label: string;
}

export const STAT_LABELS = ['HP', 'Atk', 'Def', 'SpA', 'SpD', 'Spe'];
export const MAX_POINTS_PER_STAT = 32;
export const MAX_POINTS_TOTAL = 66;
export const TEAM_SIZE = 6;
export const MOVE_SLOTS = 4;

export function teamOf(engine: Engine, format: FormatKey, members: Member[]): TeamResult {
  return engine.team(format, members) as TeamResult;
}

export function moveInfo(engine: Engine, name: string): MoveInfo | null {
  return (engine.moveInfo(name) as MoveInfo | undefined) ?? null;
}

export const isMega = (name: string) => /-Mega(-|$)/.test(name);

// ---------- Damage calc ----------

/** One side of the calc: a Pokémon and its set, plus its stat stage and burn. */
export interface SideInput {
  name: string;
  item: string | null;
  ability: string | null;
  nature: string;
  points: number[];
  /** Stage of the stat this side uses in the calc, −6 to +6. */
  boost: number;
  burned: boolean;
}

export type WeatherKey = 'sun' | 'rain' | 'sand' | 'snow';
export type TerrainKey = 'electric' | 'grassy' | 'psychic' | 'misty';

export interface FieldInput {
  weather: WeatherKey | null;
  terrain: TerrainKey | null;
  crit: boolean;
  helping_hand: boolean;
  /** Reflect against physical moves, Light Screen against special ones. */
  screen: boolean;
  /** In doubles, a spread move is hitting both foes (75% damage). */
  hits_both: boolean;
}

export interface CalcRequest {
  attacker: SideInput;
  defender: SideInput;
  move_name: string;
  field: FieldInput;
}

export interface CalcStart {
  attacker: SideInput;
  defender: SideInput;
  move_name: string | null;
  weather: WeatherKey | null;
  terrain: TerrainKey | null;
}

export interface MoveSummary {
  name: string;
  usage: number;
  min_percent: number;
  max_percent: number;
  ko: string;
}

export interface CalcResult {
  /** All 16 damage rolls, lowest first. */
  rolls: number[];
  defender_hp: number;
  move_type: string;
  effectiveness: number;
  hits: number;
  base_power: number;
  crit: boolean;
  spread: boolean;
  notes: string[];
  min_percent: number;
  max_percent: number;
  ko: string;
  description: string;
  attacker_stats: number[];
  defender_stats: number[];
  other_moves: MoveSummary[];
}

export function calcStart(engine: Engine, format: FormatKey, attacker?: string, defender?: string): CalcStart {
  return engine.calcStart(format, attacker, defender) as CalcStart;
}

export function runCalc(engine: Engine, format: FormatKey, req: CalcRequest): CalcResult {
  return engine.calc(format, req) as CalcResult;
}

// ---------- Tournament teams ----------

export interface TeamInfo {
  /** The forms each Pokémon battles as (`Charizard` + `Charizardite Y` → `Charizard-Mega-Y`). */
  forms: string[];
  species: string[];
  archetypes: string[];
}

export interface Role {
  label: string;
  detail: string;
}

export interface GamePlan {
  /** The team's main idea, e.g. `Rain` or `Trick Room`. */
  style: string | null;
  steps: string[];
  /** Each Pokémon with its jobs, in team order. */
  roles: [string, Role[]][];
}

export interface ImportResult {
  members: Member[];
  missing: string[];
}

// ---------- Speed tiers ----------

export interface Foe {
  name: string;
  stage: number;
  paralyzed: boolean;
  /** Assume it holds a Choice Scarf. */
  scarf: boolean;
  /** Unburden has triggered. */
  unburden: boolean;
}

/** Changes marked on one of your own Pokémon, by team slot. */
export interface Mine {
  stage: number;
  paralyzed: boolean;
  unburden: boolean;
}

export interface MatchState {
  foes: Foe[];
  mine: Mine[];
  trick_room: boolean;
  my_tailwind: boolean;
  foe_tailwind: boolean;
  /** −1 Speed on that whole side, as after Icy Wind or Electroweb. */
  my_drop: boolean;
  foe_drop: boolean;
  weather: WeatherKey | null;
}

export interface Runner {
  side: 'mine' | 'foe';
  name: string;
  speed: number;
  /** An opponent's slowest and fastest possible Speed with the same effects. */
  range: [number, number] | null;
  effects: string[];
  tie: boolean;
  /** Your Pokémon this opponent could move before or after, depending on its spread. */
  close: string[];
}

export function turnOrder(engine: Engine, format: FormatKey, members: Member[], state: MatchState): Runner[] {
  return engine.turnOrder(format, members, state) as Runner[];
}
