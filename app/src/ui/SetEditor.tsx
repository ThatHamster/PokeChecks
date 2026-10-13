// Edits one team member's set: item, ability, moves, nature and stat points. Every option
// shows how often the ladder uses it, and common spreads are one tap away. The engine
// checks the Champions limits (32 points per stat, 66 in total) before saving.

import { useMemo, useState } from 'react';

import { checkSet, natures as allNatures } from '../wasm/pokechecks_wasm.js';
import {
  MAX_POINTS_PER_STAT,
  MAX_POINTS_TOTAL,
  MOVE_SLOTS,
  STAT_LABELS,
  type Custom,
  type Nature,
  type SetResult,
} from '../data/engineTypes';
import { useData } from '../data/store';
import type { Mon, Share } from '../data/types';
import { ItemIcon, MonIcon, pct } from './bits';
import { Sheet } from './Sheet';

function draftFrom(set: SetResult): Custom {
  return {
    item: set.item,
    ability: set.ability,
    nature: set.nature ?? 'Hardy',
    points: set.points ? [...set.points] : [0, 0, 0, 0, 0, 0],
    moves: [...set.moves],
  };
}

/** Options the ladder uses, best first, plus the current choice if the ladder never used it. */
function ranked(shares: Share[], current: string | null): { name: string; pct: number | null }[] {
  const list: { name: string; pct: number | null }[] = shares.map((s) => ({ name: s.name, pct: s.pct }));
  if (current && !shares.some((s) => s.name === current)) list.unshift({ name: current, pct: null });
  return list;
}

export function SetEditor({
  set,
  mon,
  onSave,
  onReset,
  onClose,
  showMoves = true,
  resetLabel = 'Reset to most common',
}: {
  set: SetResult;
  mon: Mon;
  onSave: (custom: Custom) => void;
  onReset: () => void;
  onClose: () => void;
  /** The calc picks its move separately, so its editor leaves moves out. */
  showMoves?: boolean;
  resetLabel?: string;
}) {
  const { engine } = useData();
  const [draft, setDraft] = useState<Custom>(() => draftFrom(set));
  const total = draft.points.reduce((a, b) => a + b, 0);
  const problem = checkSet(draft) ?? null;

  const natures = useMemo(() => {
    const labels = new Map((allNatures() as Nature[]).map((n) => [n.name, n.label]));
    const used = mon.natures.filter((n) => labels.has(n.name));
    const rest = [...labels.keys()].filter((n) => !used.some((u) => u.name === n));
    return [
      ...used.map((n) => ({ name: n.name, label: labels.get(n.name)!, pct: n.pct as number | null })),
      ...rest.map((n) => ({ name: n, label: labels.get(n)!, pct: null })),
    ];
  }, [mon]);

  // Abilities the form can have but the ladder never uses come after the used ones.
  const abilities = useMemo(() => {
    const legal = engine?.abilities(mon.name) ?? [];
    return [
      ...mon.abilities.map((a) => ({ name: a.name, pct: a.pct as number | null })),
      ...legal.filter((a) => !mon.abilities.some((u) => u.name === a)).map((name) => ({ name, pct: null })),
    ];
  }, [engine, mon]);

  const change = (patch: Partial<Custom>) => setDraft((d) => ({ ...d, ...patch }));
  const setMove = (slot: number, name: string) => {
    const moves = [...draft.moves];
    moves[slot] = name;
    change({ moves: moves.filter((m, i) => m && moves.indexOf(m) === i) });
  };
  const setPoint = (i: number, value: number) => {
    const points = [...draft.points];
    points[i] = Math.max(0, Math.min(MAX_POINTS_PER_STAT, Number.isFinite(value) ? Math.round(value) : 0));
    change({ points });
  };

  return (
    <Sheet
      title="Edit set"
      onClose={onClose}
      footer={
        <>
          {set.custom && (
            <button className="ghost" onClick={onReset}>
              {resetLabel}
            </button>
          )}
          <button className="go" disabled={problem !== null} onClick={() => onSave(draft)}>
            Save set
          </button>
        </>
      }
    >
      <div className="editor-mon">
        <MonIcon name={mon.name} />
        <strong>{mon.name}</strong>
      </div>

      <label className="field">
        <span className="label">Item</span>
        <span className="select-wrap">
          {draft.item && <ItemIcon item={draft.item} />}
          <select value={draft.item ?? ''} onChange={(e) => change({ item: e.target.value || null })}>
            <option value="">No item</option>
            {ranked(mon.items, draft.item).map((o) => (
              <option key={o.name} value={o.name}>
                {o.name}
                {o.pct !== null ? ` · ${pct(o.pct)}` : ''}
              </option>
            ))}
          </select>
        </span>
      </label>

      <label className="field">
        <span className="label">Ability</span>
        <select value={draft.ability ?? ''} onChange={(e) => change({ ability: e.target.value || null })}>
          {abilities.map((o) => (
            <option key={o.name} value={o.name}>
              {o.name}
              {o.pct !== null ? ` · ${pct(o.pct)}` : ' · not used on ladder'}
            </option>
          ))}
        </select>
      </label>

      {showMoves && (
      <fieldset className="field">
        <legend className="label">Moves</legend>
        <div className="move-grid">
          {Array.from({ length: MOVE_SLOTS }, (_, i) => (
            <select key={i} aria-label={`Move ${i + 1}`} value={draft.moves[i] ?? ''} onChange={(e) => setMove(i, e.target.value)}>
              <option value="">No move</option>
              {ranked(mon.moves, draft.moves[i] ?? null).map((o) => (
                <option key={o.name} value={o.name} disabled={draft.moves.includes(o.name) && draft.moves[i] !== o.name}>
                  {o.name}
                  {o.pct !== null ? ` · ${pct(o.pct)}` : ''}
                </option>
              ))}
            </select>
          ))}
        </div>
      </fieldset>
      )}

      <label className="field">
        <span className="label">Nature</span>
        <select value={draft.nature} onChange={(e) => change({ nature: e.target.value })}>
          {natures.map((n) => (
            <option key={n.name} value={n.name}>
              {n.pct !== null ? `${n.label} · ${pct(n.pct)}` : n.label}
            </option>
          ))}
        </select>
      </label>

      <fieldset className="field">
        <legend className="label">
          Stat points · <span className={total > MAX_POINTS_TOTAL ? 'over' : ''}>{total}</span> / {MAX_POINTS_TOTAL}
        </legend>
        <div className="points">
          {STAT_LABELS.map((label, i) => (
            <div className="stepper" key={label}>
              <span className="label">{label}</span>
              <button aria-label={`Lower ${label}`} onClick={() => setPoint(i, draft.points[i] - 1)} disabled={draft.points[i] <= 0}>
                −
              </button>
              <input
                inputMode="numeric"
                aria-label={`${label} points`}
                value={draft.points[i]}
                onChange={(e) => setPoint(i, Number(e.target.value.replace(/\D/g, '')))}
              />
              <button aria-label={`Raise ${label}`} onClick={() => setPoint(i, draft.points[i] + 1)} disabled={draft.points[i] >= MAX_POINTS_PER_STAT || total >= MAX_POINTS_TOTAL}>
                +
              </button>
            </div>
          ))}
        </div>
        <span className="hint">0–{MAX_POINTS_PER_STAT} per stat, {MAX_POINTS_TOTAL} in total.</span>
      </fieldset>

      {mon.spreads.length > 0 && (
        <div className="field">
          <span className="label">Common spreads</span>
          <div className="spreads">
            {mon.spreads.slice(0, 6).map((s) => {
              const on = s.nature === draft.nature && s.points.every((p, i) => p === draft.points[i]);
              return (
                <button
                  key={`${s.nature}:${s.points.join('/')}`}
                  className="chip small"
                  aria-pressed={on}
                  onClick={() => change({ nature: s.nature, points: [...s.points] })}
                >
                  <strong>{s.nature}</strong>{' '}
                  {s.points.map((p, i) => (p ? `${p} ${STAT_LABELS[i]}` : null)).filter(Boolean).join(' / ') || 'none'}
                  <span className="muted"> · {pct(s.pct)}</span>
                </button>
              );
            })}
          </div>
        </div>
      )}

      {problem && (
        <p className="form-error" role="alert">
          {problem}
        </p>
      )}
    </Sheet>
  );
}
