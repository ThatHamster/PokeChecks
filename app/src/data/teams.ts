// The team being built in each format, and teams the player saved. Both live on the phone:
// the current team in localStorage (small, needed at once) and saved teams in IndexedDB.

import { get, set } from 'idb-keyval';
import { useSyncExternalStore } from 'react';

import type { Custom, Member } from './engineTypes';
import { TEAM_SIZE } from './engineTypes';
import type { FormatKey } from './types';

export interface SavedTeam {
  id: string;
  name: string;
  format: FormatKey;
  members: Member[];
  savedAt: number;
}

interface TeamsState {
  current: Partial<Record<FormatKey, Member[]>>;
  saved: SavedTeam[];
}

const FORMATS: FormatKey[] = ['vgc', 'vgc-bo3', 'bss'];
const teamKey = (format: FormatKey) => `pokechecks:team:${format}`;
const SAVED_KEY = 'saved-teams';

function readCurrent(format: FormatKey): Member[] {
  try {
    const raw = localStorage.getItem(teamKey(format));
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? (parsed as Member[]).slice(0, TEAM_SIZE) : [];
  } catch {
    return [];
  }
}

let state: TeamsState = {
  current: Object.fromEntries(FORMATS.map((f) => [f, readCurrent(f)])),
  saved: [],
};
const listeners = new Set<() => void>();

function update(next: TeamsState) {
  state = next;
  listeners.forEach((l) => l());
}

export function useTeams(): TeamsState {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => state,
  );
}

// Saved teams load in the background; the builder works without them.
void get(SAVED_KEY)
  .then((saved) => {
    if (Array.isArray(saved)) update({ ...state, saved: saved as SavedTeam[] });
  })
  .catch(() => undefined);

export function setTeam(format: FormatKey, members: Member[]) {
  const team = members.slice(0, TEAM_SIZE);
  try {
    localStorage.setItem(teamKey(format), JSON.stringify(team));
  } catch {
    // Not remembering the team across launches is a nuisance, not a failure.
  }
  update({ ...state, current: { ...state.current, [format]: team } });
}

export function teamFor(format: FormatKey): Member[] {
  return state.current[format] ?? [];
}

export function addMember(format: FormatKey, name: string) {
  setTeam(format, [...teamFor(format), { name, custom: null }]);
}

export function removeMember(format: FormatKey, index: number) {
  setTeam(format, teamFor(format).filter((_, i) => i !== index));
}

/** Replaces one member's set; `null` goes back to the most common set. */
export function setCustom(format: FormatKey, index: number, custom: Custom | null) {
  setTeam(
    format,
    teamFor(format).map((m, i) => (i === index ? { ...m, custom } : m)),
  );
}

/** A message for the builder to show when it next opens, e.g. after importing a team. */
let pendingNotice: string | null = null;

export function noticeForBuilder(message: string) {
  pendingNotice = message;
}

export function takeBuilderNotice(): string | null {
  const message = pendingNotice;
  pendingNotice = null;
  return message;
}

async function writeSaved(saved: SavedTeam[]) {
  update({ ...state, saved });
  await set(SAVED_KEY, saved);
}

/** Saves the team under `name`, replacing a saved team of the same name and format. */
export async function saveTeam(name: string, format: FormatKey, members: Member[]) {
  const trimmed = name.trim() || 'My team';
  const existing = state.saved.find((t) => t.format === format && t.name === trimmed);
  const team: SavedTeam = {
    id: existing?.id ?? `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    name: trimmed,
    format,
    members,
    savedAt: Date.now(),
  };
  await writeSaved([team, ...state.saved.filter((t) => t.id !== team.id)]);
}

export async function deleteSaved(id: string) {
  await writeSaved(state.saved.filter((t) => t.id !== id));
}
