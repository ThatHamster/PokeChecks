//! Downloads Pokémon Showdown's public data files and icon sheets. The Dex itself lives in
//! `pokechecks-core`, re-exported here.

use anyhow::{Context, Result};
use axum::body::Bytes;
use serde_json::Value;

pub use pokechecks_core::dex::*;

const SHOWDOWN_DATA: &str = "https://play.pokemonshowdown.com/data/";
/// Holds `BattlePokemonIconIndexes`, the sprite sheet positions of forms and Megas.
const SHOWDOWN_ICON_INDEXES: &str = "https://play.pokemonshowdown.com/js/battle-dex-data.js";
/// Showdown's mini icon sheet: 40×30 icons, 12 to a row. PokeChecks serves its own copy.
pub const SHOWDOWN_ICON_SHEET: &str =
    "https://play.pokemonshowdown.com/sprites/pokemonicons-sheet.png";
/// Showdown's item icon sheet: 24×24 icons, 16 to a row. PokeChecks serves its own copy.
pub const SHOWDOWN_ITEM_SHEET: &str =
    "https://play.pokemonshowdown.com/sprites/itemicons-sheet.png";

/// Locally served copies of Showdown's icon sheets. Either is `None` if it couldn't be
/// downloaded, and pages then link to Showdown's copy instead.
#[derive(Default)]
pub struct Sheets {
    pub icons: Option<Bytes>,
    pub items: Option<Bytes>,
}

pub async fn load(http: &reqwest::Client) -> Result<(Dex, Sheets)> {
    let (pokedex, moves, items, abilities) = tokio::try_join!(
        fetch_json(http, "pokedex.json"),
        fetch_json(http, "moves.json"),
        fetch_text(http, "items.js"),
        fetch_text(http, "abilities.js"),
    )?;
    // Icons are decoration, so a failed download only costs form-specific icons or a
    // locally served sheet.
    let (indexes, icons, item_icons) = tokio::join!(
        fetch_url(http, SHOWDOWN_ICON_INDEXES),
        fetch_sheet(http, SHOWDOWN_ICON_SHEET),
        fetch_sheet(http, SHOWDOWN_ITEM_SHEET)
    );
    let indexes = indexes
        .inspect_err(|e| tracing::warn!(error = %e, "couldn't load Showdown icon indexes"))
        .unwrap_or_default();
    let sheets = Sheets {
        icons: icons
            .inspect_err(|e| tracing::warn!(error = %e, "couldn't download Showdown icon sheet"))
            .ok(),
        items: item_icons
            .inspect_err(|e| tracing::warn!(error = %e, "couldn't download Showdown item sheet"))
            .ok(),
    };

    let dex = Dex::from_sources(&pokedex, &moves, &items, &abilities, &indexes);
    tracing::info!(
        pokemon = dex.pokemon_count(),
        names = dex.name_count(),
        "loaded Showdown dex"
    );
    Ok((dex, sheets))
}

async fn fetch_text(http: &reqwest::Client, file: &str) -> Result<String> {
    fetch_url(http, &format!("{SHOWDOWN_DATA}{file}")).await
}

async fn fetch_url(http: &reqwest::Client, url: &str) -> Result<String> {
    http.get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .with_context(|| format!("downloading {url}"))?
        .text()
        .await
        .with_context(|| format!("reading {url}"))
}

async fn fetch_sheet(http: &reqwest::Client, url: &str) -> Result<Bytes> {
    http.get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .with_context(|| format!("downloading {url}"))?
        .bytes()
        .await
        .with_context(|| format!("reading {url}"))
}

async fn fetch_json(http: &reqwest::Client, file: &str) -> Result<Value> {
    let text = fetch_text(http, file).await?;
    serde_json::from_str(&text).with_context(|| format!("parsing {file}"))
}
