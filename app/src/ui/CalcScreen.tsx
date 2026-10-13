// The damage calc: level 50 damage between any two Pokémon in the format, worked out on
// the phone by the same code as the website's calc (Showdown's Gen 9 formula and rounding).
// Both sides open on their most common sets; every move, set, stat stage and field
// condition can be changed, and each result shows the range, all 16 rolls and the KO chance.

import { useEffect, useMemo, useState } from 'react';

import { setCalc, useCalcState } from '../data/calc';
import {
  STAT_LABELS,
  calcStart,
  moveInfo,
  runCalc,
  type CalcRequest,
  type FieldInput,
  type SideInput,
  type TerrainKey,
  type WeatherKey,
} from '../data/engineTypes';
import { setFormat, useData } from '../data/store';
import type { FormatKey, Mon } from '../data/types';
import { ItemIcon, MonIcon, TypeBadge, Types, monthName } from './bits';
import { Picker } from './Picker';
import { SetEditor } from './SetEditor';

const WEATHERS: [WeatherKey | null, string][] = [
  [null, 'None'],
  ['sun', 'Sun'],
  ['rain', 'Rain'],
  ['sand', 'Sand'],
  ['snow', 'Snow'],
];
const TERRAINS: [TerrainKey | null, string][] = [
  [null, 'None'],
  ['electric', 'Electric'],
  ['grassy', 'Grassy'],
  ['psychic', 'Psychic'],
  ['misty', 'Misty'],
];

type Side = 'attacker' | 'defender';
/** Most Pokémon run a handful of attacks; the rest wait behind "Show all". */
const MOVES_SHOWN = 8;
type Sheet = { kind: 'pick'; side: Side } | { kind: 'edit'; side: Side } | null;

const blankField = (start: { weather: WeatherKey | null; terrain: TerrainKey | null }): FieldInput => ({
  weather: start.weather,
  terrain: start.terrain,
  crit: false,
  helping_hand: false,
  screen: false,
  hits_both: true,
});

/** The opening calc for a format: the top two Pokémon on their most common sets. */
function opening(engine: NonNullable<ReturnType<typeof useData>['engine']>, format: FormatKey): CalcRequest | null {
  try {
    const s = calcStart(engine, format);
    return s.move_name ? { attacker: s.attacker, defender: s.defender, move_name: s.move_name, field: blankField(s) } : null;
  } catch {
    return null;
  }
}

export function CalcScreen() {
  const { engine, manifest, format, usage } = useData();
  const calcs = useCalcState();
  const meta = usage[format];
  const info = manifest?.formats.find((f) => f.key === format);
  const doubles = info?.style !== 'Singles';
  const [sheet, setSheet] = useState<Sheet>(null);
  const [allMoves, setAllMoves] = useState(false);

  const req = calcs[format];
  // Open on the format's default calc the first time it's shown.
  useEffect(() => {
    if (engine && meta && !req) {
      const first = opening(engine, format);
      if (first) setCalc(format, first);
    }
  }, [engine, meta, req, format]);

  const result = useMemo(() => {
    if (!engine || !req) return null;
    try {
      return runCalc(engine, format, req);
    } catch {
      return null;
    }
  }, [engine, format, req]);

  if (!engine || !meta || !manifest || !req) {
    return (
      <main className="screen">
        <p className="note">Loading the damage calc…</p>
      </main>
    );
  }

  const monByName = new Map(meta.mons.map((m) => [m.name, m]));
  const update = (patch: Partial<CalcRequest>) => setCalc(format, { ...req, ...patch });
  const setField = (patch: Partial<FieldInput>) => update({ field: { ...req.field, ...patch } });
  const setSide = (side: Side, patch: Partial<SideInput>) => update({ [side]: { ...req[side], ...patch } });

  /** A new Pokémon on one side opens on its most common set; weather and terrain follow its ability. */
  const pick = (side: Side, name: string) => {
    try {
      const s = calcStart(engine, format, side === 'attacker' ? name : req.attacker.name, side === 'defender' ? name : req.defender.name);
      const next: CalcRequest = {
        ...req,
        [side]: s[side],
        move_name: side === 'attacker' ? (s.move_name ?? req.move_name) : req.move_name,
        field: { ...req.field, weather: s.weather ?? req.field.weather, terrain: s.terrain ?? req.field.terrain },
      };
      setCalc(format, next);
    } catch {
      // Not in this format's usage; the picker only lists ones that are.
    }
    setSheet(null);
  };

  const swap = () => {
    const att = monByName.get(req.defender.name);
    const moves = att ? damaging(engine, att) : [];
    update({
      attacker: { ...req.defender, boost: 0, burned: false },
      defender: { ...req.attacker, boost: 0, burned: false },
      move_name: moves.find((m) => m.name === req.move_name)?.name ?? moves[0]?.name ?? req.move_name,
    });
  };

  const attMon = monByName.get(req.attacker.name);
  const moves = attMon ? damaging(engine, attMon) : [];
  const others = new Map(result?.other_moves.map((m) => [m.name, m]));
  const editingSide = sheet?.kind === 'edit' ? sheet.side : null;
  const editingMon = editingSide ? monByName.get(req[editingSide].name) : undefined;

  return (
    <main className="screen">
      <header className="masthead">
        <h1 className="small">
          <span className="ball" aria-hidden />
          Damage{' '}<span className="accent">calc</span>
        </h1>
        {info && (
          <p className="dek">
            <strong>
              {info.label} · {info.regulation}
            </strong>{' '}
            · Level 50 · Stats from {monthName(manifest.month)}
          </p>
        )}
      </header>

      <nav className="formats" aria-label="Format">
        {manifest.formats.map((f) => (
          <button key={f.key} className="chip" aria-pressed={f.key === format} onClick={() => void setFormat(f.key)}>
            {f.label}
          </button>
        ))}
      </nav>

      <div className="sides">
        <SideCard
          label="Attacker"
          side={req.attacker}
          stats={result?.attacker_stats}
          onPick={() => setSheet({ kind: 'pick', side: 'attacker' })}
          onEdit={() => setSheet({ kind: 'edit', side: 'attacker' })}
          onBoost={(boost) => setSide('attacker', { boost })}
          onBurn={(burned) => setSide('attacker', { burned })}
        />
        <button className="swap" aria-label="Swap attacker and defender" onClick={swap}>
          ⇅
        </button>
        <SideCard
          label="Defender"
          side={req.defender}
          stats={result?.defender_stats}
          onPick={() => setSheet({ kind: 'pick', side: 'defender' })}
          onEdit={() => setSheet({ kind: 'edit', side: 'defender' })}
          onBoost={(boost) => setSide('defender', { boost })}
        />
      </div>

      {result && <ResultCard result={result} />}

      <section className="pane">
        <div className="pane-head">
          <h2>Moves</h2>
          <span className="label">{req.attacker.name}’s common attacks</span>
        </div>
        <ul className="move-picks">
          {(allMoves ? moves : moves.slice(0, MOVES_SHOWN).concat(moves.slice(MOVES_SHOWN).filter((m) => m.name === req.move_name))).map((m) => {
            const o = others.get(m.name);
            const on = m.name === req.move_name;
            return (
              <li key={m.name}>
                <button aria-pressed={on} onClick={() => update({ move_name: m.name })}>
                  <span className="mv">{m.name}</span>
                  <TypeBadge type={m.type} mini />
                  <span className="num small muted">
                    {on && result
                      ? `${result.min_percent.toFixed(1)}–${result.max_percent.toFixed(1)}%`
                      : o
                        ? `${o.min_percent.toFixed(1)}–${o.max_percent.toFixed(1)}%`
                        : ''}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
        {moves.length > MOVES_SHOWN && (
          <button className="show-all" onClick={() => setAllMoves(!allMoves)}>
            {allMoves ? 'Show fewer' : `Show all ${moves.length} attacks`}
          </button>
        )}
        {moves.length === 0 && <p className="note">{req.attacker.name} has no damaging moves in this month’s stats.</p>}
      </section>

      <section className="pane">
        <div className="pane-head">
          <h2>Field</h2>
        </div>
        <div className="pane-body stack field-body">
          <Segmented label="Weather" options={WEATHERS} value={req.field.weather} onChange={(weather) => setField({ weather })} />
          <Segmented label="Terrain" options={TERRAINS} value={req.field.terrain} onChange={(terrain) => setField({ terrain })} />
          <div className="toggles">
            <Toggle label="Critical hit" on={req.field.crit} onChange={(crit) => setField({ crit })} />
            {doubles && <Toggle label="Helping Hand" on={req.field.helping_hand} onChange={(helping_hand) => setField({ helping_hand })} />}
            <Toggle label="Reflect / Light Screen" on={req.field.screen} onChange={(screen) => setField({ screen })} />
            {doubles && <Toggle label="Spread move hits both" on={req.field.hits_both} onChange={(hits_both) => setField({ hits_both })} />}
          </div>
        </div>
      </section>

      {sheet?.kind === 'pick' && (
        <Picker
          title={sheet.side === 'attacker' ? 'Choose the attacker' : 'Choose the defender'}
          mons={meta.mons}
          onPick={(name) => pick(sheet.side, name)}
          onClose={() => setSheet(null)}
        />
      )}
      {editingSide && editingMon && (
        <SetEditor
          showMoves={false}
          resetLabel="Most common set"
          set={{
            name: req[editingSide].name,
            item: req[editingSide].item,
            ability: req[editingSide].ability,
            nature: req[editingSide].nature,
            points: req[editingSide].points,
            moves: [],
            custom: true,
            stats: [],
            spread_percent: null,
          }}
          mon={editingMon}
          onClose={() => setSheet(null)}
          onSave={(c) => {
            setSide(editingSide, { item: c.item, ability: c.ability, nature: c.nature, points: c.points });
            setSheet(null);
          }}
          onReset={() => {
            const s = calcStart(engine, format, req.attacker.name, req.defender.name);
            setSide(editingSide, { ...s[editingSide], boost: req[editingSide].boost, burned: req[editingSide].burned });
            setSheet(null);
          }}
        />
      )}
    </main>
  );
}

/** The attacker's damaging moves from the ladder, most used first, with their types. */
function damaging(engine: NonNullable<ReturnType<typeof useData>['engine']>, mon: Mon) {
  return mon.moves.flatMap((m) => {
    const info = moveInfo(engine, m.name);
    return info && info.category !== 'Status' ? [{ name: m.name, type: info.move_type }] : [];
  });
}

function SideCard({
  label,
  side,
  stats,
  onPick,
  onEdit,
  onBoost,
  onBurn,
}: {
  label: string;
  side: SideInput;
  stats?: number[];
  onPick: () => void;
  onEdit: () => void;
  onBoost: (boost: number) => void;
  onBurn?: (burned: boolean) => void;
}) {
  const points = side.points.map((p, i) => (p ? `${p} ${STAT_LABELS[i]}` : null)).filter(Boolean).join(' / ') || 'No points';
  return (
    <article className={`side-card ${label.toLowerCase()}`}>
      <span className="side-label">{label}</span>
      <button className="side-mon" onClick={onPick} aria-label={`Change the ${label.toLowerCase()}`}>
        <MonIcon name={side.name} />
        <span className="who">
          <span className="name">{side.name}</span>
          <Types name={side.name} />
        </span>
        <span className="change" aria-hidden>
          ⇄
        </span>
      </button>
      <p className="side-set">
        {side.item ? (
          <>
            <ItemIcon item={side.item} />
            {side.item}
          </>
        ) : (
          'No item'
        )}
        {side.ability && <> · {side.ability}</>}
      </p>
      <p className="side-set muted small">
        {side.nature} · {points}
        {stats && stats.length === 6 && <> · {stats[0]} HP</>}
      </p>
      <div className="side-actions">
        <div className="stage" aria-label={`${label} stat stage`}>
          <button aria-label="Lower stat stage" disabled={side.boost <= -6} onClick={() => onBoost(side.boost - 1)}>
            −
          </button>
          <span className="num strong">{side.boost > 0 ? `+${side.boost}` : side.boost < 0 ? `−${-side.boost}` : '±0'}</span>
          <button aria-label="Raise stat stage" disabled={side.boost >= 6} onClick={() => onBoost(side.boost + 1)}>
            +
          </button>
        </div>
        {onBurn && (
          <button className="mini" aria-pressed={side.burned} onClick={() => onBurn(!side.burned)}>
            {side.burned ? 'Burned' : 'Burn'}
          </button>
        )}
        <button className="mini" onClick={onEdit}>
          Edit set
        </button>
      </div>
    </article>
  );
}

function ResultCard({ result: r }: { result: ReturnType<typeof runCalc> }) {
  const min = r.rolls[0];
  const max = r.rolls[15];
  // The bar can't show more than 100%; the numbers show the real range.
  const lo = Math.min(r.min_percent, 100);
  const hi = Math.min(r.max_percent, 100);
  return (
    <section className="pane result">
      <div className="pane-body">
        <p className="calc-line">{r.description}</p>
        {max === 0 ? (
          <p className="big-result">No damage</p>
        ) : (
          <>
            <div className="result-row">
              <p className="big-result num">
                {min}–{max}
                <span className="muted"> / {r.defender_hp} HP</span>
              </p>
              <p className="pct num">
                {r.min_percent.toFixed(1)}–{r.max_percent.toFixed(1)}%
              </p>
            </div>
            <p className={`ko${/OHKO/.test(r.ko) ? ' ohko' : ''}`}>{r.ko}</p>
            <div
              className="hpbar"
              role="img"
              aria-label={`Leaves ${(100 - hi).toFixed(0)}–${(100 - lo).toFixed(0)}% HP`}
              key={`${min}-${max}-${r.defender_hp}`}
            >
              <span className="hp-left" style={{ width: `${100 - hi}%` }} />
              <span className="hp-maybe" style={{ width: `${hi - lo}%` }} />
            </div>
            <p className="rolls num small muted">
              Rolls: {r.rolls.join(', ')}
              {r.hits > 1 ? ` (${r.hits} hits)` : ''}
            </p>
          </>
        )}
        {r.notes.length > 0 && (
          <div className="notes">
            {r.notes.map((n) => (
              <span key={n} className="note-tag">
                {n}
              </span>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

function Segmented<T extends string | null>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: [T, string][];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div className="seg-field">
      <span className="label">{label}</span>
      <div className="seg" role="radiogroup" aria-label={label}>
        {options.map(([v, text]) => (
          <button key={text} role="radio" aria-checked={v === value} onClick={() => onChange(v)}>
            {text}
          </button>
        ))}
      </div>
    </div>
  );
}

function Toggle({ label, on, onChange }: { label: string; on: boolean; onChange: (on: boolean) => void }) {
  return (
    <button className="toggle" role="switch" aria-checked={on} onClick={() => onChange(!on)}>
      <span className="knob" aria-hidden />
      {label}
    </button>
  );
}
