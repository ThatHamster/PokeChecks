// About: what PokeChecks is, the fan-app disclaimer, and the state of the saved data.

import { refresh, useData } from '../data/store';
import { Ball, monthName } from './bits';

function ago(ms: number): string {
  const minutes = Math.round((Date.now() - ms) / 60000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  return new Date(ms).toLocaleDateString('en', { day: 'numeric', month: 'short' });
}

export function AboutScreen() {
  const { manifest, checkedAt, offline, refreshing } = useData();
  return (
    <main className="screen">
      <header className="masthead">
        <h1 className="small">
          <Ball />
          About
        </h1>
      </header>

      <section className="pane prose">
        <div className="pane-head">
          <h2>PokeChecks</h2>
        </div>
        <div className="pane-body">
          <p>
            Ranked usage for Pokémon Champions, with each Pokémon’s most common items,
            abilities, moves, spreads and teammates. Usage comes from Smogon’s monthly stats.
          </p>
          <p>
            Everything is saved on your phone, so it works offline. When you’re online, PokeChecks
            checks for newer stats and downloads only what changed.
          </p>
        </div>
      </section>

      <section className="pane">
        <div className="pane-head">
          <h2>Your data</h2>
        </div>
        <div className="pane-body stack">
          <p>
            {manifest ? (
              <>
                Stats from <strong>{monthName(manifest.month)}</strong>.{' '}
              </>
            ) : null}
            {checkedAt ? <>Last checked {ago(checkedAt)}.</> : 'Not downloaded yet.'}
            {offline && ' You’re offline, so this is the saved copy.'}
          </p>
          <button className="go" disabled={refreshing} onClick={() => void refresh()}>
            {refreshing ? 'Checking…' : 'Check for new stats'}
          </button>
        </div>
      </section>

      <p className="disclaimer">
        PokeChecks is an unofficial fan app. It isn’t affiliated with, endorsed or supported by
        Nintendo, Creatures Inc., GAME FREAK or The Pokémon Company. Pokémon and its names are
        trademarks of their respective owners. Sprites and item icons come from Pokémon Showdown.
      </p>
    </main>
  );
}
