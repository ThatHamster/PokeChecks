//! Real tournament teams from the Limitless online tournament platform's public API
//! (<https://docs.limitlesstcg.com/developer.html>). Limitless runs VGC (doubles) events and
//! names formats after the regulation, e.g. `M-C`. This module downloads the events; the
//! team lists and their parsing live in `pokechecks-core`, re-exported here.

use anyhow::{Context, Result};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub use pokechecks_core::tournaments::*;

const API: &str = "https://play.limitlesstcg.com/api";
/// Limitless lists events 100 to a page, newest first; two pages cover about three weeks.
const PAGE_SIZE: usize = 100;
const PAGES: usize = 2;
/// How many of the largest recent events to use. With their top 8 that's up to 240 teams.
const EVENTS_USED: usize = 30;
/// Smaller events are skipped; a top 8 out of a handful of players says little.
const MIN_PLAYERS: u32 = 32;
/// Standings downloaded at once, to go easy on Limitless.
const CONCURRENT_DOWNLOADS: usize = 4;

/// How many regulations to try, newest first, before settling on one with no events.
const REGULATIONS_TRIED: usize = 3;

/// Top teams for the newest regulation that already has large events.
///
/// The regulation comes from Limitless's own format list rather than from Smogon. Smogon
/// only publishes a month's usage after it ends, so a new regulation would otherwise reach
/// this page about a month late. `fallback` (Smogon's regulation) is used if Limitless's
/// list can't be read.
pub async fn load_current(http: &reqwest::Client, fallback: &str) -> Result<Tournaments> {
    let mut regulations = match regulations(http).await {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!("couldn't read Limitless formats, using {fallback}: {err:#}");
            Vec::new()
        }
    };
    if regulations.is_empty() {
        regulations.push(fallback.to_owned());
    }
    let mut first_empty = None;
    for regulation in regulations.into_iter().take(REGULATIONS_TRIED) {
        let events = load(http, &regulation).await?;
        if !events.is_empty() {
            return Ok(Tournaments { regulation, events });
        }
        first_empty.get_or_insert(regulation);
    }
    Ok(Tournaments {
        regulation: first_empty.unwrap_or_else(|| fallback.to_owned()),
        events: Vec::new(),
    })
}

/// Pokémon Champions regulations Limitless knows about, newest first.
async fn regulations(http: &reqwest::Client) -> Result<Vec<String>> {
    let url = format!("{API}/games");
    let games: Vec<LGame> = http
        .get(&url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .with_context(|| format!("downloading {url}"))?
        .json()
        .await
        .with_context(|| format!("parsing {url}"))?;
    let formats = games
        .into_iter()
        .find(|g| g.id == "VGC")
        .map(|g| g.formats.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    Ok(newest_regulations(formats))
}

/// Top teams from the largest recent events for a regulation, newest event first.
pub async fn load(http: &reqwest::Client, regulation: &str) -> Result<Vec<Event>> {
    let mut events: Vec<LTournament> = Vec::new();
    for page in 1..=PAGES {
        let query = serde_urlencoded::to_string([
            ("game", "VGC"),
            ("format", regulation),
            ("limit", &PAGE_SIZE.to_string()),
            ("page", &page.to_string()),
        ])?;
        let url = format!("{API}/tournaments?{query}");
        let batch: Vec<LTournament> = http
            .get(&url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .with_context(|| format!("downloading {url}"))?
            .json()
            .await
            .with_context(|| format!("parsing {url}"))?;
        let last_page = batch.len() < PAGE_SIZE;
        events.extend(batch);
        if last_page {
            break;
        }
    }
    events.retain(|e| e.players >= MIN_PLAYERS);
    events.sort_by_key(|e| std::cmp::Reverse(e.players));
    events.dedup_by(|a, b| a.id == b.id);
    events.truncate(EVENTS_USED);

    let limit = std::sync::Arc::new(Semaphore::new(CONCURRENT_DOWNLOADS));
    let mut jobs = JoinSet::new();
    for e in events {
        let http = http.clone();
        let limit = limit.clone();
        jobs.spawn(async move {
            let _slot = limit.acquire_owned().await;
            let url = format!("{API}/tournaments/{}/standings", e.id);
            let standings: Result<Vec<LStanding>> = async {
                http.get(&url)
                    .send()
                    .await
                    .and_then(reqwest::Response::error_for_status)?
                    .json()
                    .await
                    .map_err(Into::into)
            }
            .await;
            (e, standings)
        });
    }

    let mut out = Vec::new();
    while let Some(joined) = jobs.join_next().await {
        let (e, standings) = joined?;
        let standings = match standings {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!(event = e.id, "skipping tournament: {err:#}");
                continue;
            }
        };
        let teams = top_teams(standings);
        if !teams.is_empty() {
            out.push(Event {
                date: e.date.chars().take(10).collect(),
                id: e.id,
                name: e.name,
                players: e.players,
                teams,
            });
        }
    }
    out.sort_by(|a, b| b.date.cmp(&a.date).then(b.players.cmp(&a.players)));
    tracing::info!(regulation, events = out.len(), "loaded tournament teams");
    Ok(out)
}
