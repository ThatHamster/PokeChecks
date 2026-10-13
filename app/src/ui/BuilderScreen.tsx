// The team builder: up to six Pokémon on their most common sets (or the player's own),
// with level 50 stats, type matchups, Speed order, teammate suggestions and a Showdown
// export. All the working-out happens in the WebAssembly engine, the same code as the
// website's builder.

import { Share } from '@capacitor/share';
import { useEffect, useMemo, useState } from 'react';

import { STAT_LABELS, TEAM_SIZE, isMega, teamOf, type Custom } from '../data/engineTypes';
import { setFormat, useData } from '../data/store';
import { addMember, removeMember, setCustom, setTeam, useTeams, type SavedTeam } from '../data/teams';
import type { Mon } from '../data/types';
import { ItemIcon, MonIcon, TypeBadge, Types, monthName, pct } from './bits';
import { MoveName } from './MoveName';
import { Picker } from './Picker';
import { SavedTeams } from './SavedTeams';
import { SetEditor } from './SetEditor';

type Sheet = { kind: 'pick' } | { kind: 'edit'; slot: number } | { kind: 'saved' } | null;

function multLabel(m: number): string {
  if (m === 0) return '0';
  if (m === 0.25) return '¼';
  if (m === 0.5) return '½';
  return `${m}×`;
}

function multClass(m: number): string {
  if (m === 0) return 'immune';
  if (m < 1) return 'resist';
  if (m > 1) return m >= 4 ? 'weak4' : 'weak';
  return '';
}

export function BuilderScreen() {
  const { engine, manifest, format, usage } = useData();
  const { current } = useTeams();
  const stored = current[format] ?? [];
  const meta = usage[format];
  const info = manifest?.formats.find((f) => f.key === format);
  const [sheet, setSheet] = useState<Sheet>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const team = useMemo(
    () => (engine && meta ? teamOf(engine, format, stored) : null),
    [engine, meta, format, stored],
  );

  // A Pokémon that left this month's stats can't be shown; drop it and say so.
  useEffect(() => {
    if (!team || team.members.length === stored.length) return;
    const kept = new Set(team.members.map((m) => m.name));
    const gone = stored.filter((m) => !kept.has(m.name)).map((m) => m.name);
    setTeam(format, team.members);
    setNotice(`${gone.join(', ')} ${gone.length > 1 ? 'aren’t' : 'isn’t'} in this month’s ${info?.label ?? ''} stats, so ${gone.length > 1 ? 'they were' : 'it was'} removed.`);
  }, [team, stored, format, info]);

  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(() => setNotice(null), 4500);
    return () => clearTimeout(t);
  }, [notice]);

  if (!engine || !meta || !team || !manifest) {
    return (
      <main className="screen">
        <p className="note">Loading the team builder…</p>
      </main>
    );
  }

  const monByName = new Map(meta.mons.map((m) => [m.name, m]));
  const megas = team.sets.filter((s) => isMega(s.name)).length;
  const bySpeed = [...team.sets].sort((a, b) => b.stats[5] - a.stats[5]);
  const fastest = Math.max(1, bySpeed[0]?.stats[5] ?? 1);
  const problems = team.matchups.filter((m) => m.problem).map((m) => m.attack);

  const add = (name: string) => {
    const tried = teamOf(engine, format, [...stored, { name, custom: null }]);
    if (tried.members.length === stored.length) {
      setNotice(
        stored.length >= TEAM_SIZE
          ? 'Your team already has six Pokémon.'
          : `Only one form of each species can be on a team, so ${name} can’t join.`,
      );
      return;
    }
    addMember(format, name);
    setSheet(null);
  };

  const editing = sheet?.kind === 'edit' ? team.sets[sheet.slot] : null;
  const editingMon: Mon | undefined = editing ? monByName.get(editing.name) : undefined;

  const openSaved = (t: SavedTeam) => {
    if (t.format !== format) void setFormat(t.format);
    setTeam(t.format, t.members);
    setSheet(null);
    setNotice(`Opened “${t.name}”.`);
  };

  return (
    <main className="screen">
      <header className="masthead">
        <h1 className="small">
          <span className="ball" aria-hidden />
          Team{' '}<span className="accent">builder</span>
        </h1>
        {info && (
          <p className="dek">
            <strong>
              {info.label} · {info.regulation}
            </strong>{' '}
            · Stats from {monthName(manifest.month)}
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

      {notice && (
        <p className="toast" role="status">
          {notice}
        </p>
      )}

      <section className="pane">
        <div className="pane-head">
          <h2>Your team</h2>
          <div className="head-actions">
            <span className="count">
              {team.sets.length}/{TEAM_SIZE}
            </span>
            <button className="mini" onClick={() => setSheet({ kind: 'saved' })}>
              Saved
            </button>
            {team.sets.length > 0 && (
              <button className="mini" onClick={() => setTeam(format, [])}>
                Clear
              </button>
            )}
          </div>
        </div>

        <div className="team-grid">
          {team.sets.map((s, slot) => {
            const mon = monByName.get(s.name);
            const itemPct = mon?.items.find((i) => i.name === s.item)?.pct;
            return (
              <article className="member" key={`${s.name}-${slot}`} style={{ animationDelay: `${slot * 50}ms` }}>
                <div className="member-top">
                  <MonIcon name={s.name} />
                  <span className="who">
                    <span className="name">{s.name}</span>
                    <Types name={s.name} />
                  </span>
                  <button className="icon-btn" aria-label={`Remove ${s.name}`} onClick={() => removeMember(format, slot)}>
                    ×
                  </button>
                </div>
                <span className={`badge${s.custom ? '' : ' plain'}`}>{s.custom ? 'Your set' : 'Most common set'}</span>
                <dl className="set">
                  <dt>Item</dt>
                  <dd>
                    {s.item ? (
                      <>
                        <ItemIcon item={s.item} />
                        {s.item}
                        {itemPct !== undefined && <span className="muted">· {pct(itemPct)}</span>}
                      </>
                    ) : (
                      'No item'
                    )}
                  </dd>
                  <dt>Ability</dt>
                  <dd>{s.ability ?? '—'}</dd>
                  {s.nature && (
                    <>
                      <dt>Nature</dt>
                      <dd>{s.nature}</dd>
                    </>
                  )}
                  {s.points && (
                    <>
                      <dt>Points</dt>
                      <dd className="num small">
                        {s.points.map((p, i) => (p ? `${p} ${STAT_LABELS[i]}` : null)).filter(Boolean).join(' / ') || 'None'}
                      </dd>
                    </>
                  )}
                </dl>
                <div className="stat-line">
                  {STAT_LABELS.map((l, i) => (
                    <span key={l}>
                      <span className="label">{l}</span>
                      <span className="num strong">{s.stats[i]}</span>
                    </span>
                  ))}
                </div>
                <ul className="moves">
                  {s.moves.map((m) => (
                    <li key={m}>
                      <MoveName name={m} />
                    </li>
                  ))}
                </ul>
                <button className="edit-btn" onClick={() => setSheet({ kind: 'edit', slot })}>
                  Edit set
                </button>
              </article>
            );
          })}
          {team.sets.length < TEAM_SIZE && (
            <button className="member add" onClick={() => setSheet({ kind: 'pick' })}>
              <span className="plus" aria-hidden>
                +
              </span>
              {team.sets.length === 0 ? 'Add your first Pokémon' : 'Add Pokémon'}
            </button>
          )}
        </div>

        {team.repeated_items.length > 0 && (
          <p className="warn-note">
            Item clause: {team.repeated_items.join(', ')} {team.repeated_items.length > 1 ? 'are' : 'is'} held by more than one Pokémon. Each item can only be used once per team.
          </p>
        )}
        {megas > 1 && (
          <p className="warn-note">Only one Pokémon can Mega Evolve per battle, and this team has {megas} Megas.</p>
        )}
      </section>

      {team.sets.length > 0 && (
        <>
          <section className="pane">
            <div className="pane-head">
              <h2>Type matchups</h2>
            </div>
            <p className="pane-note">
              {problems.length ? <>Watch out for <strong>{problems.join(', ')}</strong>.</> : 'No type hits three or more of your team super effectively.'}
            </p>
            <div className="table-scroll">
              <table className="matchups">
                <thead>
                  <tr>
                    <th />
                    {team.sets.map((s, i) => (
                      <th key={i} title={s.name}>
                        <MonIcon name={s.name} />
                      </th>
                    ))}
                    <th className="label">Weak</th>
                  </tr>
                </thead>
                <tbody>
                  {team.matchups.map((r) => (
                    <tr key={r.attack} className={r.problem ? 'warn' : ''}>
                      <td>
                        <TypeBadge type={r.attack} mini />
                      </td>
                      {r.multipliers.map((m, i) => (
                        <td key={i} className={`mult ${multClass(m)}`}>
                          {m === 1 ? '' : multLabel(m)}
                        </td>
                      ))}
                      <td className={`num strong${r.weak ? ' weak-count' : ''}`}>{r.weak}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>

          <section className="pane">
            <div className="pane-head">
              <h2>Speed at level 50</h2>
            </div>
            <ul className="speeds">
              {bySpeed.map((s, i) => (
                <li key={`${s.name}-${i}`}>
                  <MonIcon name={s.name} />
                  <span className="name">{s.name}</span>
                  <span className="track">
                    <i style={{ width: `${(s.stats[5] / fastest) * 100}%` }} />
                  </span>
                  <span className="num strong">{s.stats[5]}</span>
                </li>
              ))}
            </ul>
          </section>

          <ExportPane paste={team.paste} />
        </>
      )}

      {sheet?.kind === 'pick' && <Picker mons={meta.mons} team={team} onPick={add} onClose={() => setSheet(null)} />}
      {editing && editingMon && sheet?.kind === 'edit' && (
        <SetEditor
          set={editing}
          mon={editingMon}
          onClose={() => setSheet(null)}
          onSave={(custom: Custom) => {
            setCustom(format, sheet.slot, custom);
            setSheet(null);
          }}
          onReset={() => {
            setCustom(format, sheet.slot, null);
            setSheet(null);
          }}
        />
      )}
      {sheet?.kind === 'saved' && (
        <SavedTeams manifest={manifest} format={format} members={team.members} onOpen={openSaved} onClose={() => setSheet(null)} />
      )}
    </main>
  );
}

function ExportPane({ paste }: { paste: string }) {
  const [copied, setCopied] = useState(false);
  const [canShare, setCanShare] = useState(false);

  useEffect(() => {
    Share.canShare()
      .then((r) => setCanShare(r.value))
      .catch(() => setCanShare(false));
  }, []);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(paste);
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    } catch {
      // Clipboard access can be refused; the text is still there to select by hand.
    }
  };

  return (
    <section className="pane">
      <div className="pane-head">
        <h2>Showdown export</h2>
        <div className="head-actions">
          <button className="mini" onClick={() => void copy()}>
            {copied ? 'Copied!' : 'Copy'}
          </button>
          {canShare && (
            <button className="mini" onClick={() => void Share.share({ title: 'PokeChecks team', text: paste }).catch(() => undefined)}>
              Share
            </button>
          )}
        </div>
      </div>
      <pre className="paste">{paste}</pre>
      <p className="pane-note">
        Paste it into Pokémon Showdown’s Import or Poképaste. Stat points go on the EVs line, which Champions formats read as 0–32 points. Megas are listed as their base form holding the Mega Stone.
      </p>
    </section>
  );
}
