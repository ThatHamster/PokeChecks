// Tournament teams: the top 8 open team lists from the largest recent events on Limitless,
// best finishes first, with the website's filters (required Pokémon, archetypes, finish).
// Each team explains how it plays and opens in the builder with common spreads filled in.

import { Share } from '@capacitor/share';
import { useDeferredValue, useEffect, useMemo, useState } from 'react';

import { archetypes as allArchetypes } from '../wasm/pokechecks_wasm.js';
import { moveInfo, type GamePlan, type ImportResult, type TeamInfo } from '../data/engineTypes';
import { setFormat, useData } from '../data/store';
import { noticeForBuilder, setTeam } from '../data/teams';
import type { EntryTeam, TournamentEvent } from '../data/types';
import { ItemIcon, MonIcon, TypeBadge, Types } from './bits';

/** Teams per screenful; "Show more" adds another batch. */
const PAGE = 24;
const FINISHES: [number | null, string][] = [
  [null, 'Any'],
  [1, 'Winners'],
  [4, 'Top 4'],
  [8, 'Top 8'],
];

interface Row {
  event: TournamentEvent;
  team: EntryTeam;
  info: TeamInfo;
}

interface Filter {
  mons: string[];
  archetypes: string[];
  finish: number | null;
}

const winRate = (t: EntryTeam) => {
  const [w, l, d] = t.record ?? [0, 0, 0];
  return w + l + d > 0 ? w / (w + l + d) : 0;
};

/** A base species ("Charizard") matches any of its forms; a form only matches itself. */
function matches(row: Row, f: Filter, speciesNames: Set<string>): boolean {
  return (
    (f.finish === null || row.team.placing <= f.finish) &&
    f.mons.every((m) => row.info.forms.includes(m) || (speciesNames.has(m) && row.info.species.includes(m))) &&
    f.archetypes.every((a) => row.info.archetypes.includes(a))
  );
}

function ordinal(n: number): string {
  const s = n % 100 >= 11 && n % 100 <= 13 ? 'th' : ({ 1: 'st', 2: 'nd', 3: 'rd' } as Record<number, string>)[n % 10] ?? 'th';
  return `${n}${s}`;
}

function formatDate(date: string): string {
  const d = new Date(`${date}T00:00:00Z`);
  return Number.isNaN(d.getTime()) ? date : d.toLocaleDateString('en', { day: 'numeric', month: 'short', timeZone: 'UTC' });
}

export function TeamsScreen({ onOpenBuilder }: { onOpenBuilder: () => void }) {
  const { engine, manifest, format, tournaments } = useData();
  const info = manifest?.formats.find((f) => f.key === format);
  const [filter, setFilter] = useState<Filter>({ mons: [], archetypes: [], finish: null });
  const [shown, setShown] = useState(PAGE);
  const [typed, setTyped] = useState('');
  const query = useDeferredValue(typed);
  const [notice, setNotice] = useState<string | null>(null);

  const rows = useMemo<Row[]>(() => {
    if (!engine || !tournaments) return [];
    const list = tournaments.events.flatMap((event) =>
      event.teams.map((team) => ({ event, team, info: engine.teamInfo(team.members) as TeamInfo })),
    );
    // Best finish, then best record, then the bigger and newer event, as on the website.
    return list.sort(
      (a, b) =>
        a.team.placing - b.team.placing ||
        winRate(b.team) - winRate(a.team) ||
        b.event.players - a.event.players ||
        b.event.date.localeCompare(a.event.date),
    );
  }, [engine, tournaments]);

  // Every Pokémon on these teams, most common first, then base species so "Charizard" finds Megas.
  const { known, speciesNames } = useMemo(() => {
    const counts = new Map<string, number>();
    rows.forEach((r) => r.info.forms.forEach((f) => counts.set(f, (counts.get(f) ?? 0) + 1)));
    const forms = [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([n]) => n);
    const species = new Set(rows.flatMap((r) => r.info.species));
    return { known: [...forms, ...[...species].filter((s) => !counts.has(s))], speciesNames: species };
  }, [rows]);

  const archetypeList = useMemo(() => (engine ? (allArchetypes() as { key: string; label: string }[]) : []), [engine]);
  const labelOf = (key: string) => archetypeList.find((a) => a.key === key)?.label ?? key;
  const matching = rows.filter((r) => matches(r, filter, speciesNames));
  const change = (f: Filter) => {
    setFilter(f);
    setShown(PAGE);
  };

  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(() => setNotice(null), 4500);
    return () => clearTimeout(t);
  }, [notice]);

  const header = (
    <>
      <header className="masthead">
        <h1 className="small">
          <span className="ball" aria-hidden />
          Tournament{' '}<span className="accent">teams</span>
        </h1>
        <p className="dek">
          <strong>Regulation {tournaments?.regulation ?? info?.regulation}</strong> · Top 8 lists from the largest recent events on Limitless
        </p>
      </header>
      {manifest && (
        <nav className="formats" aria-label="Format">
          {manifest.formats.map((f) => (
            <button key={f.key} className="chip" aria-pressed={f.key === format} onClick={() => void setFormat(f.key)}>
              {f.label}
            </button>
          ))}
        </nav>
      )}
    </>
  );

  if (format === 'bss') {
    return (
      <main className="screen">
        {header}
        <section className="pane empty-state">
          <strong>Tournament teams are VGC only</strong>
          <p>Limitless runs doubles events, so there are no Battle Stadium Singles team lists.</p>
          <button className="go" onClick={() => void setFormat('vgc')}>
            See VGC teams
          </button>
        </section>
      </main>
    );
  }
  if (!engine || !tournaments) {
    return (
      <main className="screen">
        {header}
        <p className="note">Tournament teams are still downloading. They’ll appear here once they arrive, and work offline after that.</p>
      </main>
    );
  }

  const suggestions = query.trim()
    ? known.filter((n) => n.toLowerCase().includes(query.trim().toLowerCase()) && !filter.mons.includes(n)).slice(0, 8)
    : [];
  const countWith = (a: string) =>
    rows.filter((r) => matches(r, { ...filter, archetypes: filter.archetypes.includes(a) ? filter.archetypes : [...filter.archetypes, a] }, speciesNames)).length;

  const open = (team: EntryTeam) => {
    const result = engine.importTeam(format, team.members) as ImportResult;
    setTeam(format, result.members);
    const missing = result.missing;
    noticeForBuilder(
      missing.length
        ? `Opened ${team.player}’s team. ${missing.join(', ')} ${missing.length > 1 ? 'aren’t' : 'isn’t'} in this month’s usage stats yet, so ${missing.length > 1 ? 'they were' : 'it was'} left out.`
        : `Opened ${team.player}’s team with the most common spreads filled in.`,
    );
    onOpenBuilder();
  };

  return (
    <main className="screen">
      {header}
      {notice && (
        <p className="toast" role="status">
          {notice}
        </p>
      )}
      {info && tournaments.regulation !== info.regulation && (
        <p className="warn-note standalone">
          These events are Regulation {tournaments.regulation}, but usage stats are still Regulation {info.regulation} until Smogon publishes a month of {tournaments.regulation} data. Pokémon new to {tournaments.regulation} can’t be opened in the builder until then.
        </p>
      )}

      <section className="pane">
        <div className="pane-head">
          <h2>Filter teams</h2>
          <span className="label">
            {matching.length} of {rows.length} teams
          </span>
        </div>
        <div className="pane-body stack field-body">
          <div className="seg-field">
            <span className="label">Require Pokémon</span>
            <div className="search">
              <input
                type="search"
                placeholder={filter.mons.length >= 6 ? 'Six is the most a team can have' : 'Type a Pokémon'}
                value={typed}
                disabled={filter.mons.length >= 6}
                onChange={(e) => setTyped(e.target.value)}
                aria-label="Require a Pokémon"
              />
            </div>
            {suggestions.length > 0 && (
              <div className="suggest-chips">
                {suggestions.map((n) => (
                  <button
                    key={n}
                    className="mate-chip"
                    onClick={() => {
                      change({ ...filter, mons: [...filter.mons, n] });
                      setTyped('');
                    }}
                  >
                    <MonIcon name={n} />
                    {n}
                  </button>
                ))}
              </div>
            )}
            {query.trim() && suggestions.length === 0 && <span className="hint">None of these teams has a Pokémon matching “{query}”.</span>}
            {filter.mons.length > 0 && (
              <div className="suggest-chips">
                {filter.mons.map((m) => (
                  <button key={m} className="mate-chip on" aria-label={`Stop requiring ${m}`} onClick={() => change({ ...filter, mons: filter.mons.filter((x) => x !== m) })}>
                    <MonIcon name={m} />
                    {m}
                    <span aria-hidden> ×</span>
                  </button>
                ))}
              </div>
            )}
          </div>

          <div className="seg-field">
            <span className="label">Archetype</span>
            <div className="toggles">
              {archetypeList.map((a) => {
                const on = filter.archetypes.includes(a.key);
                const n = countWith(a.key);
                return (
                  <button
                    key={a.key}
                    className="chip small arch"
                    aria-pressed={on}
                    disabled={!on && n === 0}
                    onClick={() =>
                      change({ ...filter, archetypes: on ? filter.archetypes.filter((x) => x !== a.key) : [...filter.archetypes, a.key] })
                    }
                  >
                    {a.label} <span className="count">{n}</span>
                  </button>
                );
              })}
            </div>
          </div>

          <div className="seg-field">
            <span className="label">Finish</span>
            <div className="seg" role="radiogroup" aria-label="Finish">
              {FINISHES.map(([v, text]) => (
                <button key={text} role="radio" aria-checked={filter.finish === v} onClick={() => change({ ...filter, finish: v })}>
                  {text}
                </button>
              ))}
            </div>
          </div>

          {(filter.mons.length > 0 || filter.archetypes.length > 0 || filter.finish !== null) && (
            <button className="show-all left" onClick={() => change({ mons: [], archetypes: [], finish: null })}>
              Clear filters
            </button>
          )}
        </div>
      </section>

      <p className="pane-note standalone">
        Team lists don’t include stat points. “Open in builder” fills them in with the most common spread for each Pokémon and nature, so check them before you play.
      </p>

      {matching.length === 0 ? (
        <section className="pane empty-state">
          <strong>No teams match</strong>
          <p>Try removing a filter.</p>
        </section>
      ) : (
        <div className="tt-list">
          {matching.slice(0, shown).map((r, i) => (
            <TeamCard
              key={`${r.event.id}-${r.team.placing}-${r.team.player}`}
              row={r}
              index={i}
              labelOf={labelOf}
              onOpen={() => open(r.team)}
              onCopied={() => setNotice('Team list copied.')}
            />
          ))}
          {matching.length > shown && (
            <button className="go show-more" onClick={() => setShown(shown + PAGE)}>
              Show more teams · {shown} of {matching.length}
            </button>
          )}
        </div>
      )}
    </main>
  );
}

function TeamCard({
  row,
  index,
  labelOf,
  onOpen,
  onCopied,
}: {
  row: Row;
  index: number;
  labelOf: (key: string) => string;
  onOpen: () => void;
  onCopied: () => void;
}) {
  const { engine } = useData();
  const { team: t, event: e, info } = row;
  const [plan, setPlan] = useState<GamePlan | null>(null);
  const [showList, setShowList] = useState(false);
  if (!engine) return null;

  const paste = () => engine.listPaste(t.members);
  const share = async () => {
    try {
      if ((await Share.canShare()).value) {
        await Share.share({ title: `${t.player}’s team`, text: paste() });
        return;
      }
    } catch {
      // Fall back to copying.
    }
    try {
      await navigator.clipboard.writeText(paste());
      onCopied();
    } catch {
      // Nothing more to try.
    }
  };

  return (
    <article className="pane team-card" style={{ animationDelay: `${Math.min(index, 8) * 40}ms` }}>
      <div className="team-head">
        <div className="placing-row">
          <span className={`placing${t.placing === 1 ? ' first' : ''}`}>{ordinal(t.placing)}</span>
          <span className="muted small">of {e.players}</span>
          {t.record && (
            <span className="num muted small record">
              {t.record[0]}–{t.record[1]}
              {t.record[2] > 0 ? `–${t.record[2]}` : ''}
            </span>
          )}
        </div>
        <h3>{t.player}</h3>
        <p className="muted small event-line">
          {e.name} · {formatDate(e.date)}
        </p>
        {info.archetypes.length > 0 && (
          <div className="notes">
            {info.archetypes.map((a) => (
              <span key={a} className="note-tag arch-tag">
                {labelOf(a)}
              </span>
            ))}
          </div>
        )}
      </div>
      <ul className="roster">
        {t.members.map((m, i) => (
          <li key={i}>
            <MonIcon name={info.forms[i]} />
            <span className="who">
              <span className="name">{info.forms[i]}</span>
              <Types name={info.forms[i]} />
            </span>
            {m.item && (
              <span className="roster-item muted small">
                <ItemIcon item={m.item} />
                <span>{m.item}</span>
              </span>
            )}
          </li>
        ))}
      </ul>

      <div className="card-actions">
        <button className="go" onClick={onOpen}>
          Open in builder
        </button>
        <button className="mini" onClick={() => void share()}>
          Share
        </button>
      </div>

      <details className="fold" onToggle={(ev) => (ev.currentTarget.open && !plan ? setPlan((engine.plan(t.members, true) as { plan: GamePlan }).plan) : null)}>
        <summary>
          How this team plays{plan?.style ? `: ${plan.style}` : ''} <span className="muted small">· for newer players</span>
        </summary>
        {plan && (
          <div className="plan">
            <ol className="plan-steps">
              {plan.steps.map((s, i) => (
                <li key={i}>{s}</li>
              ))}
            </ol>
            <h4>What each Pokémon does</h4>
            <dl className="plan-roles">
              {plan.roles.map(([name, roles]) => (
                <div key={name}>
                  <dt>
                    <MonIcon name={name} />
                    {name}
                  </dt>
                  <dd>
                    <ul>
                      {roles.map((r, i) => (
                        <li key={i}>
                          <span className="role">{r.label}</span> {r.detail}
                        </li>
                      ))}
                    </ul>
                  </dd>
                </div>
              ))}
            </dl>
          </div>
        )}
      </details>

      <details className="fold" onToggle={(ev) => setShowList(ev.currentTarget.open)}>
        <summary>Full team list</summary>
        {showList && (
          <div className="list-sets">
            {t.members.map((m, i) => (
              <div key={i} className="list-set">
                <strong>
                  {m.name}
                  {m.item ? ` @ ${m.item}` : ''}
                </strong>
                <span className="muted small">
                  {[m.ability, m.nature && `${m.nature} nature`].filter(Boolean).join(' · ')}
                </span>
                <ul className="moves">
                  {m.moves.map((mv) => {
                    const mi = moveInfo(engine, mv);
                    return (
                      <li key={mv}>
                        <span className="move-name">
                          {mi && <TypeBadge type={mi.move_type} mini />}
                          {mv}
                        </span>
                      </li>
                    );
                  })}
                </ul>
              </div>
            ))}
          </div>
        )}
      </details>
    </article>
  );
}
