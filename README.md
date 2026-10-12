# PokeChecks

Pokémon Champions ranked usage, side by side with base stats. Shows the newest Champions
regulation that Smogon has published stats for (currently Regulation M-C) across VGC,
VGC Best of 3 and Battle Stadium Singles.

Written in Rust (edition 2024, Rust 1.99) with Axum and Maud. Pages are rendered on the
server and need no JavaScript. A System / Light / Dark switch in the header lets each
visitor pick a theme, remembered in a cookie; System follows the device setting.

Three pages share one URL, so links keep the format, comparison and team:

- **Usage:** the ladder, sortable and filterable, with up to four Pokémon compared side by
  side on base stats, items, moves and teammates.
- **Team builder:** pick up to six Pokémon (one per species). Each gets its most common set,
  with items following the item clause. The page shows type matchups (counting abilities
  like Levitate), level 50 Speed from the common nature and stat points, teammate
  suggestions, and a Showdown / Poképaste export.
- **Meta teams:** teams assembled from teammate statistics. Each starts from a popular
  Pokémon and adds whichever Pokémon most often appear alongside the ones already picked,
  without mixing weathers or terrains. They show common cores, not specific players' teams.

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

## Test

```
cargo test
cargo clippy --all-targets -- -D warnings
```

## Data sources

- Usage: [Smogon usage stats](https://www.smogon.com/stats/), the monthly "chaos" files.
- Base stats, types, and move, item and ability names: [Pokémon Showdown](https://play.pokemonshowdown.com/data/).

## License

MIT. See [LICENSE](LICENSE).
