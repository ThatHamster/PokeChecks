//! Real tournament teams from the Limitless online tournament platform's public API
//! (<https://docs.limitlesstcg.com/developer.html>). Limitless runs VGC (doubles) events and
//! names formats after the regulation, e.g. `M-C`.

use serde::Deserialize;
use serde_json::Value;

use crate::dex::Dex;
use crate::smogon::Meta;
use crate::team::{Custom, Member};

/// Only the top cut's teams are kept.
pub const TOP_PLACINGS: u32 = 8;

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
pub struct LTournament {
    pub id: String,
    pub name: String,
    pub date: String,
    #[serde(default)]
    pub players: u32,
}

#[derive(Deserialize)]
pub struct LStanding {
    #[serde(default)]
    pub name: String,
    pub placing: Option<u32>,
    pub record: Option<LRecord>,
    #[serde(default)]
    pub decklist: Value,
}

#[derive(Deserialize)]
pub struct LRecord {
    pub wins: u32,
    pub losses: u32,
    pub ties: u32,
}

#[derive(Deserialize)]
pub struct LMon {
    pub id: String,
    pub name: String,
    pub item: Option<String>,
    pub ability: Option<String>,
    pub nature: Option<String>,
    #[serde(default)]
    pub attacks: Vec<String>,
}

/// Tournament teams plus the regulation they were played under.
pub struct Tournaments {
    pub regulation: String,
    pub events: Vec<Event>,
}

#[derive(Deserialize)]
pub struct LGame {
    pub id: String,
    #[serde(default)]
    pub formats: serde_json::Map<String, Value>,
}

/// Champions regulations are named like `M-C`; older Scarlet and Violet formats (`SVI`,
/// `VGC23`) are skipped. Newer regulations sort later alphabetically.
pub fn newest_regulations(formats: Vec<String>) -> Vec<String> {
    let mut regs: Vec<String> = formats
        .into_iter()
        .filter(|f| {
            let b = f.as_bytes();
            b.len() == 3 && b[0].is_ascii_uppercase() && b[1] == b'-' && b[2].is_ascii_uppercase()
        })
        .collect();
    regs.sort_unstable_by(|a, b| b.cmp(a));
    regs
}

/// The top cut's team lists from one event's standings, best placing first. Players who
/// dropped or shared no list are skipped.
pub fn top_teams(standings: Vec<LStanding>) -> Vec<EntryTeam> {
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
    teams
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
    fn picks_champions_regulations_newest_first() {
        let formats = ["M-B", "SVI", "M-C", "VGC23", "23S2", "M-A", "N-A"]
            .map(str::to_owned)
            .to_vec();
        assert_eq!(
            newest_regulations(formats),
            vec!["N-A", "M-C", "M-B", "M-A"]
        );
    }

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
