//! Real tournament teams from the Limitless online tournament platform's public API
//! (<https://docs.limitlesstcg.com/developer.html>). Limitless runs VGC (doubles) events and
//! names formats after the regulation, e.g. `M-C`.

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use tokio::task::JoinSet;

use crate::dex::Dex;
use crate::smogon::Meta;
use crate::team::{Custom, Member};

const API: &str = "https://play.limitlesstcg.com/api";
/// How many recent events to look through for large ones.
const RECENT_EVENTS: usize = 100;
/// How many of the largest recent events to show.
const EVENTS_SHOWN: usize = 8;
/// Smaller events are skipped; a top 4 out of a handful of players says little.
const MIN_PLAYERS: u32 = 32;
const TOP_PLACINGS: u32 = 4;

pub struct Event {
    pub id: String,
    pub name: String,
    /// `YYYY-MM-DD`
    pub date: String,
    pub players: u32,
    pub teams: Vec<EntryTeam>,
}

impl Event {
    pub fn url(&self) -> String {
        format!(
            "https://play.limitlesstcg.com/tournament/{}/standings",
            self.id
        )
    }
}

pub struct EntryTeam {
    pub placing: u32,
    pub player: String,
    pub record: Option<(u32, u32, u32)>,
    pub members: Vec<ListMon>,
}

/// One Pokémon as written on an open team list.
pub struct ListMon {
    /// Limitless ID, which matches Showdown's (`indeedeef`).
    pub id: String,
    /// Name as written on the list. Megas appear as the base species holding the stone.
    pub name: String,
    pub item: Option<String>,
    pub ability: Option<String>,
    pub nature: Option<String>,
    pub moves: Vec<String>,
}

#[derive(Deserialize)]
struct LTournament {
    id: String,
    name: String,
    date: String,
    #[serde(default)]
    players: u32,
}

#[derive(Deserialize)]
struct LStanding {
    #[serde(default)]
    name: String,
    placing: Option<u32>,
    record: Option<LRecord>,
    #[serde(default)]
    decklist: Value,
}

#[derive(Deserialize)]
struct LRecord {
    wins: u32,
    losses: u32,
    ties: u32,
}

#[derive(Deserialize)]
struct LMon {
    id: String,
    name: String,
    item: Option<String>,
    ability: Option<String>,
    nature: Option<String>,
    #[serde(default)]
    attacks: Vec<String>,
}

/// Top teams from the largest recent events for a regulation, newest event first.
pub async fn load(http: &reqwest::Client, regulation: &str) -> Result<Vec<Event>> {
    let query = serde_urlencoded::to_string([
        ("game", "VGC"),
        ("format", regulation),
        ("limit", &RECENT_EVENTS.to_string()),
    ])?;
    let url = format!("{API}/tournaments?{query}");
    let mut events: Vec<LTournament> = http
        .get(&url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .with_context(|| format!("downloading {url}"))?
        .json()
        .await
        .with_context(|| format!("parsing {url}"))?;
    events.retain(|e| e.players >= MIN_PLAYERS);
    events.sort_by_key(|e| std::cmp::Reverse(e.players));
    events.truncate(EVENTS_SHOWN);

    let mut jobs = JoinSet::new();
    for e in events {
        let http = http.clone();
        jobs.spawn(async move {
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
        let mut teams: Vec<EntryTeam> = standings
            .into_iter()
            .filter_map(|s| {
                let placing = s.placing.filter(|p| *p <= TOP_PLACINGS)?;
                let list: Vec<LMon> = serde_json::from_value(s.decklist).ok()?;
                (!list.is_empty()).then(|| EntryTeam {
                    placing,
                    player: s.name,
                    record: s.record.map(|r| (r.wins, r.losses, r.ties)),
                    members: list
                        .into_iter()
                        .map(|m| ListMon {
                            id: m.id,
                            name: m.name,
                            item: m.item.filter(|i| !i.is_empty()),
                            ability: m.ability.filter(|a| !a.is_empty()),
                            nature: m.nature.filter(|n| !n.is_empty()),
                            moves: m.attacks,
                        })
                        .collect(),
                })
            })
            .collect();
        teams.sort_by_key(|t| t.placing);
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

/// Turns a team list into team builder members. Pokémon missing from this month's usage
/// data can't be shown in the builder and are returned separately.
pub fn to_members(team: &EntryTeam, meta: &Meta, dex: &Dex) -> (Vec<Member>, Vec<String>) {
    let mut members = Vec::new();
    let mut missing = Vec::new();
    for lm in &team.members {
        let form = dex.battle_form(&lm.id, lm.item.as_deref());
        match meta.mons.iter().find(|m| m.name == form) {
            Some(mon) => members.push(Member {
                name: form,
                custom: Some(Custom::from_team_list(
                    Some(mon),
                    lm.item.clone(),
                    lm.ability.clone(),
                    lm.nature.clone(),
                    lm.moves.clone(),
                )),
            }),
            None => missing.push(form),
        }
    }
    (members, missing)
}

/// The team list as Showdown import text. Team lists have no stat points, so there is no
/// EVs line.
pub fn team_list_paste(team: &EntryTeam) -> String {
    let mut out = String::new();
    for m in &team.members {
        out.push_str(&m.name);
        if let Some(item) = &m.item {
            out.push_str(&format!(" @ {item}"));
        }
        out.push('\n');
        if let Some(a) = &m.ability {
            out.push_str(&format!("Ability: {a}\n"));
        }
        out.push_str("Level: 50\n");
        if let Some(n) = &m.nature {
            out.push_str(&format!("{n} Nature\n"));
        }
        for mv in &m.moves {
            out.push_str(&format!("- {mv}\n"));
        }
        out.push('\n');
    }
    out.trim_end().to_owned() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_limitless_standing() {
        let json = r#"[{"name":"Ruby","placing":1,"record":{"wins":9,"losses":2,"ties":0},
            "decklist":[{"id":"charizard","name":"Charizard","item":"Charizardite Y","ability":"Blaze",
            "attacks":["Heat Wave","Protect"],"nature":"Timid","tera":null}]},
            {"name":"Dropped","placing":null,"decklist":[]},
            {"name":"No list","placing":2,"decklist":{}}]"#;
        let standings: Vec<LStanding> = serde_json::from_str(json).unwrap();
        assert_eq!(standings.len(), 3);
        let list: Vec<LMon> = serde_json::from_value(standings[0].decklist.clone()).unwrap();
        assert_eq!(list[0].item.as_deref(), Some("Charizardite Y"));
        assert!(serde_json::from_value::<Vec<LMon>>(standings[2].decklist.clone()).is_err());
    }

    #[test]
    fn paste_keeps_the_team_list_as_written() {
        let team = EntryTeam {
            placing: 1,
            player: "Ruby".into(),
            record: Some((9, 2, 0)),
            members: vec![ListMon {
                id: "charizard".into(),
                name: "Charizard".into(),
                item: Some("Charizardite Y".into()),
                ability: Some("Blaze".into()),
                nature: Some("Timid".into()),
                moves: vec!["Heat Wave".into(), "Protect".into()],
            }],
        };
        assert_eq!(
            team_list_paste(&team),
            "Charizard @ Charizardite Y\nAbility: Blaze\nLevel: 50\nTimid Nature\n- Heat Wave\n- Protect\n"
        );
    }
}
