// Speed tiers, built for checking turn order mid-match. Your team comes from the builder at
// its real natures, Speed points and items; add the opponent's Pokémon at team preview.
// Everyone is sorted into one turn order (reversed under Trick Room), with each opponent at
// its most common spread plus its possible range, and close calls marked. Tailwind, Icy
// Wind, weather, Trick Room, stages, paralysis, Scarf and Unburden are one tap each.

import { useMemo, useState } from 'react';

import { turnOrder, type Foe, type MatchState, type Mine, type WeatherKey } from '../data/engineTypes';
import { emptyMatch, setMatch, updateMatch, useMatches } from '../data/speed';
import { setFormat, useData } from '../data/store';
import { setTeam, useTeams, type SavedTeam } from '../data/teams';
import { MonIcon, Types } from './bits';
import { Picker } from './Picker';
import { SavedTeams } from './SavedTeams';

const WEATHERS: [WeatherKey | null, string][] = [
  [null, 'None'],
  ['sun', 'Sun'],
  ['rain', 'Rain'],
  ['sand', 'Sand'],
  ['snow', 'Snow'],
];

export function SpeedScreen({ onOpenBuilder }: { onOpenBuilder: () => void }) {
  const { engine, manifest, format, usage } = useData();
  const { current } = useTeams();
  const matches = useMatches();
  const meta = usage[format];
  const members = current[format] ?? [];
  const match = matches[format] ?? emptyMatch();
  const [sheet, setSheet] = useState<'pick' | 'saved' | null>(null);

  const order = useMemo(() => {
    if (!engine || !meta) return [];
    try {
      return turnOrder(engine, format, members, match);
    } catch {
      return [];
    }
  }, [engine, meta, format, members, match]);

  if (!engine || !meta || !manifest) {
    return (
      <main className="screen">
        <p className="note">Loading speed tiers…</p>
      </main>
    );
  }

  const monByName = new Map(meta.mons.map((m) => [m.name, m]));
  const hasUnburden = (name: string) => monByName.get(name)?.abilities.some((a) => a.name === 'Unburden') ?? false;
  // Every change starts from the latest match, not this render's copy.
  const update = (patch: Partial<MatchState>) => updateMatch(format, (m) => ({ ...m, ...patch }));
  const noMods: Mine = { stage: 0, paralyzed: false, unburden: false };
  const mineAt = (i: number): Mine => match.mine[i] ?? noMods;
  const setMine = (i: number, patch: Partial<Mine> | ((x: Mine) => Partial<Mine>)) =>
    updateMatch(format, (m) => {
      const mine = members.map((_, j) => m.mine[j] ?? noMods);
      mine[i] = { ...mine[i], ...(typeof patch === 'function' ? patch(mine[i]) : patch) };
      return { ...m, mine };
    });
  /** `patch` can be a function of the foe's latest values, for steppers. */
  const setFoe = (name: string, patch: Partial<Foe> | ((f: Foe) => Partial<Foe>)) =>
    updateMatch(format, (m) => ({
      ...m,
      foes: m.foes.map((f) => (f.name === name ? { ...f, ...(typeof patch === 'function' ? patch(f) : patch) } : f)),
    }));
  const removeFoe = (name: string) => updateMatch(format, (m) => ({ ...m, foes: m.foes.filter((f) => f.name !== name) }));

  const addFoe = (name: string) => {
    // Assume a Scarf when that's what this Pokémon usually holds, as the website does.
    const scarf = monByName.get(name)?.items[0]?.name === 'Choice Scarf';
    updateMatch(format, (m) =>
      m.foes.length < 6 && !m.foes.some((f) => f.name === name)
        ? { ...m, foes: [...m.foes, { name, stage: 0, paralyzed: false, scarf, unburden: false }] }
        : m,
    );
    setSheet(null);
  };

  const openSaved = (t: SavedTeam) => {
    if (t.format !== format) void setFormat(t.format);
    setTeam(t.format, t.members);
    setSheet(null);
  };

  return (
    <main className="screen">
      <header className="masthead">
        <h1 className="small">
          <span className="ball" aria-hidden />
          Speed{' '}<span className="accent">tiers</span>
        </h1>
        <p className="dek">Who moves first, mid-match. Add the opponent’s team at team preview, then tap effects as they happen.</p>
      </header>

      <nav className="formats" aria-label="Format">
        {manifest.formats.map((f) => (
          <button key={f.key} className="chip" aria-pressed={f.key === format} onClick={() => void setFormat(f.key)}>
            {f.label}
          </button>
        ))}
      </nav>

      <section className="pane">
        <div className="pane-head">
          <h2>Turn order</h2>
          {match.trick_room && <span className="badge">Trick Room: slowest first</span>}
        </div>
        {order.length === 0 ? (
          <p className="note">Pick your team and add the opponent’s Pokémon to see who moves first.</p>
        ) : (
          <>
            <ol className="turn-order">
              {order.map((r, i) => (
                <li key={`${r.side}-${r.name}`} className={`runner ${r.side}`} style={{ animationDelay: `${i * 35}ms` }}>
                  <span className="place num">{i + 1}</span>
                  <MonIcon name={r.name} />
                  <div className="runner-main">
                    <div className="runner-name">
                      <span className={`side-chip ${r.side}`}>{r.side === 'mine' ? 'You' : 'Opp'}</span>
                      <strong>{r.name}</strong>
                    </div>
                    <div className="runner-notes">
                      {r.effects.map((e) => (
                        <span key={e} className="note-tag">
                          {e}
                        </span>
                      ))}
                      {r.tie && <span className="note-tag tie">Speed tie</span>}
                      {r.range && (
                        <span className="muted small">
                          Could be {r.range[0]}–{r.range[1]}
                        </span>
                      )}
                    </div>
                    {r.close.length > 0 && (
                      <span className="close-call small">Could move before or after {r.close.join(', ')}, depending on its spread</span>
                    )}
                  </div>
                  <span className="runner-speed num">{r.speed}</span>
                </li>
              ))}
            </ol>
            <p className="pane-note">
              Opponents are shown at their most common nature and Speed points; “Could be” is their slowest to fastest possible Speed with the same effects. Priority moves like Fake Out and Sucker Punch go before all of this.
            </p>
          </>
        )}
      </section>

      <section className="pane">
        <div className="pane-head">
          <h2>Field</h2>
        </div>
        <div className="pane-body stack field-body">
          <div className="toggles">
            <SwitchChip label="Trick Room" on={match.trick_room} onChange={(trick_room) => update({ trick_room })} />
          </div>
          <div className="pair-row">
            <span className="label">Tailwind</span>
            <SwitchChip label="Your side" on={match.my_tailwind} onChange={(my_tailwind) => update({ my_tailwind })} />
            <SwitchChip label="Their side" on={match.foe_tailwind} onChange={(foe_tailwind) => update({ foe_tailwind })} />
          </div>
          <div className="pair-row">
            <span className="label">−1 Speed (Icy Wind)</span>
            <SwitchChip label="Your side" on={match.my_drop} onChange={(my_drop) => update({ my_drop })} />
            <SwitchChip label="Their side" on={match.foe_drop} onChange={(foe_drop) => update({ foe_drop })} />
          </div>
          <div className="seg-field">
            <span className="label">Weather (for Swift Swim, Chlorophyll and so on)</span>
            <div className="seg" role="radiogroup" aria-label="Weather">
              {WEATHERS.map(([v, text]) => (
                <button key={text} role="radio" aria-checked={match.weather === v} onClick={() => update({ weather: v })}>
                  {text}
                </button>
              ))}
            </div>
          </div>
        </div>
      </section>

      <section className="pane">
        <div className="pane-head">
          <h2>Opponent’s team</h2>
          <div className="head-actions">
            <span className="count">{match.foes.length}/6</span>
            {match.foes.length > 0 && (
              <button className="mini" onClick={() => update({ foes: [] })}>
                Clear
              </button>
            )}
          </div>
        </div>
        <ul className="speed-list">
          {match.foes.map((f) => (
            <li key={f.name}>
              <div className="speed-mon">
                <MonIcon name={f.name} />
                <span className="who">
                  <span className="name">{f.name}</span>
                  <Types name={f.name} />
                </span>
                <button className="icon-btn" aria-label={`Remove ${f.name}`} onClick={() => removeFoe(f.name)}>
                  ×
                </button>
              </div>
              <div className="speed-mods">
                <Stage value={f.stage} onChange={(delta) => setFoe(f.name, (x) => ({ stage: clampStage(x.stage + delta) }))} />
                <SwitchChip label="Scarf" on={f.scarf} onChange={(scarf) => setFoe(f.name, { scarf })} />
                <SwitchChip label="Paralyzed" on={f.paralyzed} onChange={(paralyzed) => setFoe(f.name, { paralyzed })} />
                {hasUnburden(f.name) && <SwitchChip label="Unburden" on={f.unburden} onChange={(unburden) => setFoe(f.name, { unburden })} />}
              </div>
            </li>
          ))}
        </ul>
        {match.foes.length < 6 && (
          <button className="edit-btn add-row" onClick={() => setSheet('pick')}>
            + Add an opponent
          </button>
        )}
      </section>

      <section className="pane">
        <div className="pane-head">
          <h2>Your team</h2>
          <div className="head-actions">
            <button className="mini" onClick={() => setSheet('saved')}>
              Saved
            </button>
            <button className="mini" onClick={onOpenBuilder}>
              Builder
            </button>
          </div>
        </div>
        {members.length === 0 ? (
          <p className="note">Your team comes from the team builder. Build one there, or open a saved team.</p>
        ) : (
          <ul className="speed-list">
            {members.map((m, i) => (
              <li key={`${m.name}-${i}`}>
                <div className="speed-mon">
                  <MonIcon name={m.name} />
                  <span className="who">
                    <span className="name">{m.name}</span>
                    <Types name={m.name} />
                  </span>
                </div>
                <div className="speed-mods">
                  <Stage value={mineAt(i).stage} onChange={(delta) => setMine(i, (x) => ({ stage: clampStage(x.stage + delta) }))} />
                  <SwitchChip label="Paralyzed" on={mineAt(i).paralyzed} onChange={(paralyzed) => setMine(i, { paralyzed })} />
                  {hasUnburden(m.name) && <SwitchChip label="Unburden" on={mineAt(i).unburden} onChange={(unburden) => setMine(i, { unburden })} />}
                </div>
              </li>
            ))}
          </ul>
        )}
        <p className="pane-note">Each Pokémon uses its set from the builder: nature, Speed points and Choice Scarf if it holds one.</p>
      </section>

      {match.foes.length > 0 || match.trick_room || match.my_tailwind || match.foe_tailwind || match.weather ? (
        <button className="show-all" onClick={() => setMatch(format, emptyMatch())}>
          New match: clear opponents and effects
        </button>
      ) : null}

      {sheet === 'pick' && <Picker title="Add an opponent" mons={meta.mons} onPick={addFoe} onClose={() => setSheet(null)} />}
      {sheet === 'saved' && <SavedTeams manifest={manifest} format={format} members={members} onOpen={openSaved} onClose={() => setSheet(null)} />}
    </main>
  );
}

const clampStage = (s: number) => Math.max(-6, Math.min(6, s));

/** A −/+ stepper; `onChange` gets the step (−1 or +1). */
function Stage({ value, onChange }: { value: number; onChange: (delta: number) => void }) {
  return (
    <div className="stage" aria-label="Speed stage">
      <button aria-label="Lower Speed stage" disabled={value <= -6} onClick={() => onChange(-1)}>
        −
      </button>
      <span className="num strong">{value > 0 ? `+${value}` : value < 0 ? `−${-value}` : '±0'}</span>
      <button aria-label="Raise Speed stage" disabled={value >= 6} onClick={() => onChange(1)}>
        +
      </button>
    </div>
  );
}

function SwitchChip({ label, on, onChange }: { label: string; on: boolean; onChange: (on: boolean) => void }) {
  return (
    <button className="toggle" role="switch" aria-checked={on} onClick={() => onChange(!on)}>
      <span className="knob" aria-hidden />
      {label}
    </button>
  );
}
