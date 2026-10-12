# PokeChecks

Pokémon Champions ranked usage, side by side with base stats. Shows the newest Champions
regulation that Smogon has published stats for (currently Regulation M-C) across VGC,
VGC Best of 3 and Battle Stadium Singles.

Written in Rust (edition 2024, Rust 1.99) with Axum and Maud. Pages are rendered on the
server and need no JavaScript. A System / Light / Dark switch in the header lets each
visitor pick a theme, remembered in a cookie; System follows the device setting.

Every meta team and tournament team has a "How this team plays" section for newer players:
a short starting plan (the team's main idea, a simple lead, which Pokémon to Mega Evolve,
who does the damage) and what each Pokémon is there to do. It's worked out from the team's
own moves, abilities and items, so it stays accurate as the teams change.

The pages share one URL, so links keep the format, comparison and team:

- **Usage:** the ladder, sortable and filterable, with up to four Pokémon compared side by
  side on base stats, items, moves and teammates.
- **Team builder:** pick up to six Pokémon (one per species). Each starts on its most common
  set, with items following the item clause. "Edit set" changes the item, ability, moves,
  nature and stat points; every option shows its ladder usage, common spreads are one click
  away, and custom stat points are checked against the Champions limits (32 per stat, 66
  total). The page shows level 50 stats, type matchups (counting abilities like Levitate),
  Speed order, teammate suggestions, and a Showdown / Poképaste export.
- **Meta teams:** teams assembled from teammate statistics. Each starts from a popular
  Pokémon and adds whichever Pokémon most often appear alongside the ones already picked,
  without mixing weathers or terrains. They show common cores, not specific players' teams.
- **Tournament teams:** top 4 open team lists from the largest recent events for the current
  regulation on [Limitless](https://play.limitlesstcg.com/tournaments?game=VGC), refreshed
  every three hours. Any team opens in the builder; team lists have no stat points, so the
  builder fills them in from the most common spread for each Pokémon and nature.

![The team builder with Rillaboom, Incineroar, Salamence-Mega and Sneasler, showing their common sets, teammate suggestions and type matchups](docs/team-builder.png)

![PokeChecks showing the VGC Regulation M-C usage ladder beside a base stat and set comparison of Rillaboom, Sneasler and Incineroar](docs/screenshot.png)

## Run

```
cargo run
```

Then open http://127.0.0.1:3000. Set `PORT` to use another port.

On first start it downloads Pokémon Showdown's Pokédex data, then fetches Smogon usage files
as each format and rating cutoff is first viewed. It checks for a newer month or regulation
every six hours, so a new regulation (such as M-D) appears without code changes.

## How the data stays current

While the server runs, it refreshes everything itself:

- **Usage stats:** checks Smogon every 6 hours. Smogon publishes each month's stats in the
  first days of the next month; earlier months are dropped from memory when a new one
  appears.
- **Tournament teams:** checks Limitless every 3 hours. The regulation comes from
  Limitless's own format list, so a new regulation's events show up as soon as they're
  played instead of waiting a month for Smogon.
- **Pokédex:** reloads Showdown's data every 24 hours (retrying hourly if it fails), so new
  Pokémon, forms and Megas appear without a restart.

Refreshes happen on the first visit after each interval, so that visit can take a few
seconds longer.

## Test

```
cargo test
cargo clippy --all-targets -- -D warnings
```

## Data sources

- Usage: [Smogon usage stats](https://www.smogon.com/stats/), the monthly "chaos" files.
- Base stats, types, and move, item and ability names: [Pokémon Showdown](https://play.pokemonshowdown.com/data/).
- Tournament teams: the [Limitless public API](https://docs.limitlesstcg.com/developer.html).

## License

MIT. See [LICENSE](LICENSE).
