//! Team logic: common sets, the type chart, level 50 stats, Showdown export, teammate
//! suggestions and the suggested meta teams. Everything here is plain functions over the
//! usage data so it can be tested without the network.

use crate::dex::Dex;
use crate::smogon::{Meta, Mon, Spread};

pub const TEAM_SIZE: usize = 6;
/// Generated teams carry at most this many Mega Evolutions. Only one can Mega Evolve in a
/// battle, so more than two is rarely worth a slot.
const MAX_MEGAS: usize = 2;
/// How many suggested teams the meta teams page shows.
const MAX_META_TEAMS: usize = 6;
/// Generated teams may share at most this many members, so the list stays varied.
const MAX_SHARED_MEMBERS: usize = 3;
/// A move or ability needs at least this share of a Pokémon's sets to tag its team.
const TAG_THRESHOLD: f64 = 40.0;

pub const TYPES: [&str; 18] = [
    "Normal", "Fire", "Water", "Electric", "Grass", "Ice", "Fighting", "Poison", "Ground",
    "Flying", "Psychic", "Bug", "Rock", "Ghost", "Dragon", "Dark", "Steel", "Fairy",
];

/// Attacking type -> (super effective against, not very effective against, no effect on).
fn chart_row(
    attack: &str,
) -> (
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
) {
    match attack {
        "Normal" => (&[], &["Rock", "Steel"], &["Ghost"]),
        "Fire" => (
            &["Grass", "Ice", "Bug", "Steel"],
            &["Fire", "Water", "Rock", "Dragon"],
            &[],
        ),
        "Water" => (
            &["Fire", "Ground", "Rock"],
            &["Water", "Grass", "Dragon"],
            &[],
        ),
        "Electric" => (
            &["Water", "Flying"],
            &["Electric", "Grass", "Dragon"],
            &["Ground"],
        ),
        "Grass" => (
            &["Water", "Ground", "Rock"],
            &[
                "Fire", "Grass", "Poison", "Flying", "Bug", "Dragon", "Steel",
            ],
            &[],
        ),
        "Ice" => (
            &["Grass", "Ground", "Flying", "Dragon"],
            &["Fire", "Water", "Ice", "Steel"],
            &[],
        ),
        "Fighting" => (
            &["Normal", "Ice", "Rock", "Dark", "Steel"],
            &["Poison", "Flying", "Psychic", "Bug", "Fairy"],
            &["Ghost"],
        ),
        "Poison" => (
            &["Grass", "Fairy"],
            &["Poison", "Ground", "Rock", "Ghost"],
            &["Steel"],
        ),
        "Ground" => (
            &["Fire", "Electric", "Poison", "Rock", "Steel"],
            &["Grass", "Bug"],
            &["Flying"],
        ),
        "Flying" => (
            &["Grass", "Fighting", "Bug"],
            &["Electric", "Rock", "Steel"],
            &[],
        ),
        "Psychic" => (&["Fighting", "Poison"], &["Psychic", "Steel"], &["Dark"]),
        "Bug" => (
            &["Grass", "Psychic", "Dark"],
            &[
                "Fire", "Fighting", "Poison", "Flying", "Ghost", "Steel", "Fairy",
            ],
            &[],
        ),
        "Rock" => (
            &["Fire", "Ice", "Flying", "Bug"],
            &["Fighting", "Ground", "Steel"],
            &[],
        ),
        "Ghost" => (&["Psychic", "Ghost"], &["Dark"], &["Normal"]),
        "Dragon" => (&["Dragon"], &["Steel"], &["Fairy"]),
        "Dark" => (&["Psychic", "Ghost"], &["Fighting", "Dark", "Fairy"], &[]),
        "Steel" => (
            &["Ice", "Rock", "Fairy"],
            &["Fire", "Water", "Electric", "Steel"],
            &[],
        ),
        "Fairy" => (
            &["Fighting", "Dragon", "Dark"],
            &["Fire", "Poison", "Steel"],
            &[],
        ),
        _ => (&[], &[], &[]),
    }
}

/// Damage multiplier of an attacking type against a defender's types and ability.
pub fn multiplier(attack: &str, defender_types: &[String], ability: Option<&str>) -> f32 {
    let (strong, weak, none) = chart_row(attack);
    let mut m: f32 = defender_types
        .iter()
        .map(|d| {
            let d = d.as_str();
            if none.contains(&d) {
                0.0
            } else if strong.contains(&d) {
                2.0
            } else if weak.contains(&d) {
                0.5
            } else {
                1.0
            }
        })
        .product();
    // Abilities that change what a defender takes from a whole type.
    m *= match (ability.unwrap_or(""), attack) {
        ("Levitate" | "Earth Eater", "Ground")
        | ("Flash Fire" | "Well-Baked Body", "Fire")
        | ("Water Absorb" | "Storm Drain" | "Dry Skin", "Water")
        | ("Volt Absorb" | "Lightning Rod" | "Motor Drive", "Electric")
        | ("Sap Sipper", "Grass") => 0.0,
        ("Thick Fat", "Fire" | "Ice")
        | ("Heatproof" | "Water Bubble", "Fire")
        | ("Purifying Salt", "Ghost") => 0.5,
        _ => 1.0,
    };
    m
}

/// The stat a nature raises and lowers, as indexes into HP/Atk/Def/SpA/SpD/Spe.
fn nature_effect(nature: &str) -> Option<(usize, usize)> {
    let (up, down) = match nature {
        "Lonely" => (1, 2),
        "Brave" => (1, 5),
        "Adamant" => (1, 3),
        "Naughty" => (1, 4),
        "Bold" => (2, 1),
        "Relaxed" => (2, 5),
        "Impish" => (2, 3),
        "Lax" => (2, 4),
        "Timid" => (5, 1),
        "Hasty" => (5, 2),
        "Jolly" => (5, 3),
        "Naive" => (5, 4),
        "Modest" => (3, 1),
        "Mild" => (3, 2),
        "Quiet" => (3, 5),
        "Rash" => (3, 4),
        "Calm" => (4, 1),
        "Gentle" => (4, 2),
        "Sassy" => (4, 5),
        "Careful" => (4, 3),
        _ => return None,
    };
    Some((up, down))
}

/// A stat at level 50 in Champions: HP = base + 75 + points, others = (base + 20 + points) × nature.
pub fn level50_stat(base: u16, points: u16, index: usize, nature: &str) -> u16 {
    if index == 0 {
        return base + 75 + points;
    }
    let raw = u32::from(base + 20 + points);
    match nature_effect(nature) {
        Some((up, _)) if up == index => (raw * 11 / 10) as u16,
        Some((_, down)) if down == index => (raw * 9 / 10) as u16,
        _ => raw as u16,
    }
}

pub fn is_mega(name: &str) -> bool {
    name.contains("-Mega")
}

/// One team member's most common set.
pub struct Set<'a> {
    pub mon: &'a Mon,
    pub item: Option<String>,
    pub ability: Option<String>,
    pub moves: Vec<String>,
    pub spread: Option<&'a Spread>,
}

impl Set<'_> {
    /// Level 50 Speed with the common spread, or neutral with no points when unknown.
    pub fn speed(&self, dex: &Dex) -> u16 {
        let base = dex.entry(&self.mon.name).map_or(0, |e| e.stats[5]);
        match self.spread {
            Some(s) => level50_stat(base, s.points[5], 5, &s.nature),
            None => level50_stat(base, 0, 5, ""),
        }
    }
}

/// Picks each member's most common set. Items follow the item clause: if a teammate
/// earlier in the list already holds an item, the next most common one is used.
pub fn common_sets<'a>(team: &[&'a Mon]) -> Vec<Set<'a>> {
    let mut used: Vec<String> = Vec::new();
    team.iter()
        .map(|m| {
            let item = m
                .items
                .iter()
                .find(|i| !used.contains(&i.name))
                .or(m.items.first())
                .map(|i| i.name.clone());
            if let Some(i) = &item {
                used.push(i.clone());
            }
            Set {
                mon: m,
                item,
                ability: m.abilities.first().map(|a| a.name.clone()),
                moves: m.moves.iter().take(4).map(|s| s.name.clone()).collect(),
                spread: m.spread.as_ref(),
            }
        })
        .collect()
}

const STAT_LABELS: [&str; 6] = ["HP", "Atk", "Def", "SpA", "SpD", "Spe"];

/// Stat points as Showdown's `EVs` line, which Champions formats read as raw 0–32 points.
pub fn points_line(points: &[u16; 6]) -> String {
    points
        .iter()
        .zip(STAT_LABELS)
        .filter(|(p, _)| **p > 0)
        .map(|(p, l)| format!("{p} {l}"))
        .collect::<Vec<_>>()
        .join(" / ")
}

/// Showdown / Poképaste import text.
pub fn showdown_paste(sets: &[Set], dex: &Dex) -> String {
    let mut out = String::new();
    for set in sets {
        let name = &set.mon.name;
        let mega = is_mega(name);
        // Mega forms are battle-only: the paste names the base species holding its stone.
        let species = if mega {
            dex.species(name)
        } else {
            name.as_str()
        };
        out.push_str(species);
        if let Some(item) = &set.item {
            out.push_str(" @ ");
            out.push_str(item);
        }
        out.push('\n');
        // A Mega's listed ability is its Mega ability, which Showdown won't accept on the base form.
        if let (Some(ability), false) = (&set.ability, mega) {
            out.push_str(&format!("Ability: {ability}\n"));
        }
        out.push_str("Level: 50\n");
        if let Some(s) = set.spread {
            let line = points_line(&s.points);
            if !line.is_empty() {
                out.push_str(&format!("EVs: {line}\n"));
            }
            out.push_str(&format!("{} Nature\n", s.nature));
        }
        for m in &set.moves {
            out.push_str(&format!("- {m}\n"));
        }
        out.push('\n');
    }
    out.trim_end().to_owned() + "\n"
}

/// How the whole team takes one attacking type.
pub struct Matchup {
    pub attack: &'static str,
    /// One multiplier per team member, in team order.
    pub multipliers: Vec<f32>,
    pub weak: usize,
    /// Resists and immunities together.
    pub resist: usize,
}

impl Matchup {
    /// Several members weak and few to switch into it.
    pub fn is_problem(&self) -> bool {
        self.weak >= 3 || (self.weak >= 2 && self.resist == 0)
    }
}

pub fn matchups(sets: &[Set], dex: &Dex) -> Vec<Matchup> {
    TYPES
        .iter()
        .map(|&attack| {
            let multipliers: Vec<f32> = sets
                .iter()
                .map(|s| {
                    let types = dex
                        .entry(&s.mon.name)
                        .map(|e| e.types.as_slice())
                        .unwrap_or_default();
                    multiplier(attack, types, s.ability.as_deref())
                })
                .collect();
            Matchup {
                attack,
                weak: multipliers.iter().filter(|m| **m > 1.0).count(),
                resist: multipliers.iter().filter(|m| **m < 1.0).count(),
                multipliers,
            }
        })
        .collect()
}

/// Why a Pokémon can't join the team right now.
#[derive(Debug, PartialEq, Eq)]
pub enum Blocked {
    OnTeam,
    Full,
    /// Another form of the same species is already on the team.
    SameSpecies(String),
}

pub fn blocked(team: &[&Mon], candidate: &str, dex: &Dex) -> Option<Blocked> {
    if team.iter().any(|m| m.name == candidate) {
        return Some(Blocked::OnTeam);
    }
    let species = dex.species(candidate);
    if let Some(m) = team.iter().find(|m| dex.species(&m.name) == species) {
        return Some(Blocked::SameSpecies(m.name.clone()));
    }
    (team.len() >= TEAM_SIZE).then_some(Blocked::Full)
}

/// Average share of the team's members' teams that also run `candidate`.
pub fn pair_score(team: &[&Mon], candidate: &str) -> f64 {
    if team.is_empty() {
        return 0.0;
    }
    team.iter()
        .map(|m| m.pair_rates.get(candidate).copied().unwrap_or(0.0))
        .sum::<f64>()
        / team.len() as f64
}

/// Pokémon that most often appear alongside the current team, best first.
/// With an empty team this is simply the most used Pokémon.
pub fn suggestions<'a>(team: &[&Mon], meta: &'a Meta, dex: &Dex, n: usize) -> Vec<(&'a Mon, f64)> {
    let mut out: Vec<(&Mon, f64)> = meta
        .mons
        .iter()
        .filter(|c| blocked(team, &c.name, dex).is_none())
        .map(|c| (c, pair_score(team, &c.name)))
        .collect();
    if !team.is_empty() {
        out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.rank.cmp(&b.0.rank)));
    }
    out.truncate(n);
    out
}

/// A team assembled from teammate statistics.
pub struct MetaTeam<'a> {
    pub members: Vec<&'a Mon>,
    /// Average share of each member's teams that include each other member.
    pub cohesion: f64,
    pub tags: Vec<&'static str>,
}

/// Starts from each popular Pokémon in turn and greedily adds whichever Pokémon appears most
/// often alongside the members chosen so far. Near-duplicates of earlier teams are skipped.
pub fn meta_teams<'a>(meta: &'a Meta, dex: &Dex) -> Vec<MetaTeam<'a>> {
    let mut teams: Vec<MetaTeam> = Vec::new();
    for seed in meta.mons.iter().take(30) {
        let mut members: Vec<&Mon> = vec![seed];
        while members.len() < TEAM_SIZE {
            let megas = members.iter().filter(|m| is_mega(&m.name)).count();
            let next = meta
                .mons
                .iter()
                .filter(|c| blocked(&members, &c.name, dex).is_none())
                .filter(|c| megas < MAX_MEGAS || !is_mega(&c.name))
                .filter(|c| field_compatible(&members, c))
                .map(|c| (c, pair_score(&members, &c.name)))
                .filter(|(_, score)| *score > 0.0)
                .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.rank.cmp(&a.0.rank)));
            match next {
                Some((c, _)) => members.push(c),
                None => break,
            }
        }
        let too_similar = teams.iter().any(|t| {
            members
                .iter()
                .filter(|m| t.members.iter().any(|x| x.name == m.name))
                .count()
                > MAX_SHARED_MEMBERS
        });
        if members.len() == TEAM_SIZE && !too_similar {
            let sets = common_sets(&members);
            teams.push(MetaTeam {
                cohesion: cohesion(&members),
                tags: tags(&sets),
                members,
            });
            if teams.len() == MAX_META_TEAMS {
                break;
            }
        }
    }
    teams
}

fn cohesion(members: &[&Mon]) -> f64 {
    let mut total = 0.0;
    let mut pairs = 0;
    for a in members {
        for b in members {
            if a.name != b.name {
                total += a.pair_rates.get(&b.name).copied().unwrap_or(0.0);
                pairs += 1;
            }
        }
    }
    if pairs == 0 {
        0.0
    } else {
        total / pairs as f64
    }
}

/// Weather or terrain a Pokémon sets automatically with its usual ability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldEffect {
    pub is_weather: bool,
    pub name: &'static str,
}

pub fn field_effect(m: &Mon) -> Option<FieldEffect> {
    let ability = m.abilities.first().filter(|a| a.pct >= TAG_THRESHOLD)?;
    let (is_weather, name) = match ability.name.as_str() {
        "Drought" | "Orichalcum Pulse" => (true, "Sun"),
        "Drizzle" => (true, "Rain"),
        "Sand Stream" => (true, "Sand"),
        "Snow Warning" => (true, "Snow"),
        "Grassy Surge" => (false, "Grassy Terrain"),
        "Psychic Surge" => (false, "Psychic Terrain"),
        "Electric Surge" | "Hadron Engine" => (false, "Electric Terrain"),
        "Misty Surge" => (false, "Misty Terrain"),
        _ => return None,
    };
    Some(FieldEffect { is_weather, name })
}

/// False when `candidate` would set a different weather (or terrain) than a member already
/// does; the two would overwrite each other.
fn field_compatible(members: &[&Mon], candidate: &Mon) -> bool {
    let Some(new) = field_effect(candidate) else {
        return true;
    };
    members
        .iter()
        .filter_map(|m| field_effect(m))
        .all(|old| old.is_weather != new.is_weather || old.name == new.name)
}

/// Weather, terrain and speed control the team's common sets set up.
pub fn tags(sets: &[Set]) -> Vec<&'static str> {
    let mut out = Vec::new();
    let mut add = |t: &'static str| {
        if !out.contains(&t) {
            out.push(t);
        }
    };
    for s in sets {
        if let Some(field) = field_effect(s.mon) {
            add(field.name);
        }
        for m in s.mon.moves.iter().filter(|m| m.pct >= TAG_THRESHOLD) {
            match m.name.as_str() {
                "Trick Room" => add("Trick Room"),
                "Tailwind" => add("Tailwind"),
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smogon::Share;
    use std::collections::HashMap;

    fn types(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn type_chart_combines_both_types() {
        assert_eq!(multiplier("Ice", &types(&["Dragon", "Flying"]), None), 4.0);
        assert_eq!(
            multiplier("Fighting", &types(&["Dark", "Steel"]), None),
            4.0
        );
        assert_eq!(multiplier("Ground", &types(&["Steel", "Ghost"]), None), 2.0);
        assert_eq!(
            multiplier("Electric", &types(&["Ground", "Water"]), None),
            0.0
        );
        assert_eq!(multiplier("Fire", &types(&["Water", "Dragon"]), None), 0.25);
        assert_eq!(multiplier("Normal", &types(&["Steel", "Ghost"]), None), 0.0);
    }

    #[test]
    fn abilities_change_matchups() {
        assert_eq!(
            multiplier("Ground", &types(&["Ghost", "Poison"]), Some("Levitate")),
            0.0
        );
        assert_eq!(
            multiplier("Fire", &types(&["Grass"]), Some("Thick Fat")),
            1.0
        );
        assert_eq!(
            multiplier("Water", &types(&["Fire"]), Some("Water Absorb")),
            0.0
        );
    }

    #[test]
    fn level_50_stats_match_champions_formula() {
        // Rillaboom: base 85 Speed, Jolly, 32 points -> (85 + 20 + 32) * 1.1 = 150.7 -> 150
        assert_eq!(level50_stat(85, 32, 5, "Jolly"), 150);
        // Adamant lowers Special Attack: (60 + 20) * 0.9 = 72
        assert_eq!(level50_stat(60, 0, 3, "Adamant"), 72);
        // HP ignores nature: 100 + 75 + 32
        assert_eq!(level50_stat(100, 32, 0, "Adamant"), 207);
        assert_eq!(level50_stat(100, 0, 5, "Hardy"), 120);
    }

    #[test]
    fn points_line_skips_zeros() {
        assert_eq!(points_line(&[32, 32, 0, 0, 2, 0]), "32 HP / 32 Atk / 2 SpD");
        assert_eq!(points_line(&[0; 6]), "");
    }

    fn mon(name: &str, rank: usize, items: &[&str], pairs: &[(&str, f64)]) -> Mon {
        Mon {
            name: name.to_owned(),
            rank,
            usage: 50.0 / rank as f64,
            prev: None,
            abilities: vec![Share {
                name: "Intimidate".into(),
                pct: 100.0,
            }],
            items: items
                .iter()
                .map(|i| Share {
                    name: (*i).to_owned(),
                    pct: 50.0,
                })
                .collect(),
            moves: vec![Share {
                name: "Protect".into(),
                pct: 90.0,
            }],
            tera: vec![],
            teammates: vec![],
            spread: None,
            pair_rates: pairs
                .iter()
                .map(|(n, r)| ((*n).to_owned(), *r))
                .collect::<HashMap<_, _>>(),
        }
    }

    #[test]
    fn rejects_a_second_different_weather() {
        let with_ability = |name: &str, ability: &str| {
            let mut m = mon(name, 1, &[], &[]);
            m.abilities = vec![Share {
                name: ability.into(),
                pct: 99.0,
            }];
            m
        };
        let pelipper = with_ability("Pelipper", "Drizzle");
        let charizard = with_ability("Charizard-Mega-Y", "Drought");
        let politoed = with_ability("Politoed", "Drizzle");
        let rillaboom = with_ability("Rillaboom", "Grassy Surge");
        assert!(!field_compatible(&[&pelipper], &charizard));
        assert!(field_compatible(&[&pelipper], &politoed));
        // Terrain and weather don't clash.
        assert!(field_compatible(&[&pelipper], &rillaboom));
    }

    #[test]
    fn sets_follow_the_item_clause() {
        let a = mon("A", 1, &["Sitrus Berry", "Leftovers"], &[]);
        let b = mon("B", 2, &["Sitrus Berry", "Focus Sash"], &[]);
        let sets = common_sets(&[&a, &b]);
        assert_eq!(sets[0].item.as_deref(), Some("Sitrus Berry"));
        assert_eq!(sets[1].item.as_deref(), Some("Focus Sash"));
    }

    #[test]
    fn pair_score_averages_over_the_team() {
        let a = mon("A", 1, &[], &[("C", 0.6)]);
        let b = mon("B", 2, &[], &[("C", 0.2)]);
        assert!((pair_score(&[&a, &b], "C") - 0.4).abs() < 1e-9);
        assert_eq!(pair_score(&[&a], "Z"), 0.0);
    }
}
