//! Team logic: common sets, the type chart, level 50 stats, Showdown export, teammate
//! suggestions and the suggested meta teams. Everything here is plain functions over the
//! usage data so it can be tested without the network.

use crate::dex::Dex;
use crate::smogon::{Meta, Mon};

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
pub(crate) fn nature_effect(nature: &str) -> Option<(usize, usize)> {
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

pub const NATURES: [&str; 25] = [
    "Adamant", "Bashful", "Bold", "Brave", "Calm", "Careful", "Docile", "Gentle", "Hardy", "Hasty",
    "Impish", "Jolly", "Lax", "Lonely", "Mild", "Modest", "Naive", "Naughty", "Quiet", "Quirky",
    "Rash", "Relaxed", "Sassy", "Serious", "Timid",
];

/// `Adamant (+Atk −SpA)`, or `Hardy (neutral)`.
pub fn nature_label(nature: &str) -> String {
    match nature_effect(nature) {
        Some((up, down)) => format!("{nature} (+{} −{})", STAT_LABELS[up], STAT_LABELS[down]),
        None => format!("{nature} (neutral)"),
    }
}

/// Champions gives each stat 0–32 points, 66 in total.
pub const MAX_POINTS_PER_STAT: u16 = 32;
pub const MAX_POINTS_TOTAL: u16 = 66;
pub const MOVE_SLOTS: usize = 4;

/// A set a visitor chose for one team member, replacing its usage defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Custom {
    pub item: Option<String>,
    pub ability: Option<String>,
    pub nature: String,
    pub points: [u16; 6],
    pub moves: Vec<String>,
}

impl Custom {
    /// URL form: `item~ability~nature~32/32/0/0/2/0~Move|Move|Move|Move`.
    fn encode(&self) -> String {
        let points: Vec<String> = self.points.iter().map(u16::to_string).collect();
        [
            self.item.clone().unwrap_or_default(),
            self.ability.clone().unwrap_or_default(),
            self.nature.clone(),
            points.join("/"),
            self.moves.join("|"),
        ]
        .join("~")
    }

    fn decode(fields: &[&str]) -> Option<Self> {
        let [item, ability, nature, points, moves] = fields else {
            return None;
        };
        let non_empty = |s: &str| (!s.is_empty()).then(|| s.to_owned());
        let mut parsed = [0u16; 6];
        let mut parts = points.split('/');
        for p in &mut parsed {
            *p = parts.next()?.parse().ok()?;
        }
        let custom = Self {
            item: non_empty(item),
            ability: non_empty(ability),
            nature: (*nature).to_owned(),
            points: parsed,
            moves: moves
                .split('|')
                .filter(|m| !m.is_empty())
                .take(MOVE_SLOTS)
                .map(str::to_owned)
                .collect(),
        };
        custom.validate().ok().map(|()| custom)
    }

    /// Checks the Champions stat point limits and that the nature exists.
    pub fn validate(&self) -> Result<(), String> {
        if !NATURES.contains(&self.nature.as_str()) {
            return Err(format!("“{}” isn’t a nature.", self.nature));
        }
        if let Some(i) = self.points.iter().position(|p| *p > MAX_POINTS_PER_STAT) {
            return Err(format!(
                "{} has {} stat points; the most one stat can have is {MAX_POINTS_PER_STAT}.",
                STAT_LABELS[i], self.points[i]
            ));
        }
        let total: u16 = self.points.iter().sum();
        if total > MAX_POINTS_TOTAL {
            return Err(format!(
                "These stat points add up to {total}; the limit is {MAX_POINTS_TOTAL}. Lower some stats and save again."
            ));
        }
        Ok(())
    }

    /// Builds a set from a tournament team list, which has no stat points: those come from
    /// the most common spread with the same nature, or the most common spread overall.
    pub fn from_team_list(
        mon: Option<&Mon>,
        item: Option<String>,
        ability: Option<String>,
        nature: Option<String>,
        moves: Vec<String>,
    ) -> Self {
        let spreads = mon.map(|m| m.spreads.as_slice()).unwrap_or_default();
        let nature = nature
            .filter(|n| NATURES.contains(&n.as_str()))
            .or_else(|| spreads.first().map(|s| s.nature.clone()))
            .unwrap_or_else(|| "Hardy".to_owned());
        let points = spreads
            .iter()
            .find(|s| s.nature == nature)
            .or(spreads.first())
            .map_or([0; 6], |s| s.points);
        Self {
            item,
            ability,
            nature,
            points,
            moves: moves.into_iter().take(MOVE_SLOTS).collect(),
        }
    }
}

/// One team slot: a Pokémon, plus the set the visitor chose if they changed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub custom: Option<Custom>,
}

impl Member {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            custom: None,
        }
    }

    /// `Name`, or `Name~item~ability~nature~points~moves` for a changed set.
    pub fn encode(&self) -> String {
        match &self.custom {
            Some(c) => format!("{}~{}", self.name, c.encode()),
            None => self.name.clone(),
        }
    }

    /// A set that fails to parse or breaks the stat point rules falls back to the defaults.
    pub fn decode(token: &str) -> Self {
        let fields: Vec<&str> = token.split('~').map(str::trim).collect();
        Self {
            name: fields[0].to_owned(),
            custom: fields.get(1..).and_then(Custom::decode),
        }
    }
}

/// One team member's set, either the visitor's or the most common one on the ladder.
pub struct Set<'a> {
    pub mon: &'a Mon,
    pub item: Option<String>,
    pub ability: Option<String>,
    pub nature: Option<String>,
    pub points: Option<[u16; 6]>,
    pub moves: Vec<String>,
    /// The visitor changed this set.
    pub custom: bool,
}

impl Set<'_> {
    /// Level 50 stats in HP/Atk/Def/SpA/SpD/Spe order.
    pub fn stats(&self, dex: &Dex) -> [u16; 6] {
        let base = dex.entry(&self.mon.name).map_or([0; 6], |e| e.stats);
        let points = self.points.unwrap_or([0; 6]);
        let nature = self.nature.as_deref().unwrap_or("");
        std::array::from_fn(|i| level50_stat(base[i], points[i], i, nature))
    }

    pub fn speed(&self, dex: &Dex) -> u16 {
        self.stats(dex)[5]
    }

    /// Share of ladder sets with exactly this nature and spread.
    pub fn spread_pct(&self) -> Option<f64> {
        let (nature, points) = (self.nature.as_deref()?, self.points?);
        self.mon
            .spreads
            .iter()
            .find(|s| s.nature == nature && s.points == points)
            .map(|s| s.pct)
    }

    /// The set as editable values, for prefilling the edit form.
    pub fn to_custom(&self) -> Custom {
        Custom {
            item: self.item.clone(),
            ability: self.ability.clone(),
            nature: self.nature.clone().unwrap_or_else(|| "Hardy".to_owned()),
            points: self.points.unwrap_or([0; 6]),
            moves: self.moves.clone(),
        }
    }
}

/// Each member's set: the visitor's where they changed it, otherwise the most common one.
/// Default items follow the item clause, skipping items a teammate already holds.
pub fn resolve_sets<'a>(members: &[(&'a Mon, Option<&Custom>)]) -> Vec<Set<'a>> {
    let mut used: Vec<String> = members
        .iter()
        .filter_map(|(_, c)| c.and_then(|c| c.item.clone()))
        .collect();
    members
        .iter()
        .map(|(m, custom)| match custom {
            Some(c) => Set {
                mon: m,
                item: c.item.clone(),
                ability: c.ability.clone(),
                nature: Some(c.nature.clone()),
                points: Some(c.points),
                moves: c.moves.clone(),
                custom: true,
            },
            None => {
                let item = m
                    .items
                    .iter()
                    .find(|i| !used.contains(&i.name))
                    .or(m.items.first())
                    .map(|i| i.name.clone());
                if let Some(i) = &item {
                    used.push(i.clone());
                }
                let spread = m.top_spread();
                Set {
                    mon: m,
                    item,
                    ability: m.abilities.first().map(|a| a.name.clone()),
                    nature: spread.map(|s| s.nature.clone()),
                    points: spread.map(|s| s.points),
                    moves: m
                        .moves
                        .iter()
                        .take(MOVE_SLOTS)
                        .map(|s| s.name.clone())
                        .collect(),
                    custom: false,
                }
            }
        })
        .collect()
}

/// Most common sets for a whole team.
pub fn common_sets<'a>(team: &[&'a Mon]) -> Vec<Set<'a>> {
    let members: Vec<(&Mon, Option<&Custom>)> = team.iter().map(|m| (*m, None)).collect();
    resolve_sets(&members)
}

/// Items held by more than one member, which the item clause doesn't allow.
pub fn repeated_items(sets: &[Set]) -> Vec<String> {
    let mut seen: Vec<&str> = Vec::new();
    let mut repeated: Vec<String> = Vec::new();
    for item in sets.iter().filter_map(|s| s.item.as_deref()) {
        if seen.contains(&item) {
            if !repeated.iter().any(|r| r == item) {
                repeated.push(item.to_owned());
            }
        } else {
            seen.push(item);
        }
    }
    repeated
}

pub const STAT_LABELS: [&str; 6] = ["HP", "Atk", "Def", "SpA", "SpD", "Spe"];

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
        // Showdown checks the ability against the base form, so a Mega ability (Aerilate on
        // Salamence-Mega) is left out and Showdown fills in the default.
        let base_has_ability = |a: &str| {
            dex.entry(species)
                .is_some_and(|e| e.abilities.iter().any(|x| x == a))
        };
        if let Some(ability) = set
            .ability
            .as_deref()
            .filter(|a| !mega || base_has_ability(a))
        {
            out.push_str(&format!("Ability: {ability}\n"));
        }
        out.push_str("Level: 50\n");
        if let Some(points) = &set.points {
            let line = points_line(points);
            if !line.is_empty() {
                out.push_str(&format!("EVs: {line}\n"));
            }
        }
        if let Some(nature) = &set.nature {
            out.push_str(&format!("{nature} Nature\n"));
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
    use crate::smogon::{Share, Spread};
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
            spreads: vec![],
            natures: vec![],
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

    fn custom() -> Custom {
        Custom {
            item: Some("Life Orb".into()),
            ability: Some("Intimidate".into()),
            nature: "Jolly".into(),
            points: [2, 32, 0, 0, 0, 32],
            moves: vec!["Fake Out".into(), "U-turn".into(), "Will-O-Wisp".into()],
        }
    }

    #[test]
    fn members_round_trip_through_the_url() {
        let m = Member {
            name: "Incineroar".into(),
            custom: Some(custom()),
        };
        let token = m.encode();
        assert_eq!(
            token,
            "Incineroar~Life Orb~Intimidate~Jolly~2/32/0/0/0/32~Fake Out|U-turn|Will-O-Wisp"
        );
        assert_eq!(Member::decode(&token), m);
        assert_eq!(Member::decode("Rillaboom"), Member::new("Rillaboom"));
    }

    #[test]
    fn bad_sets_in_the_url_fall_back_to_defaults() {
        // 33 points in one stat, a total over 66, and a made-up nature.
        for token in [
            "A~~~Jolly~33/0/0/0/0/0~",
            "A~~~Jolly~32/32/32/0/0/0~",
            "A~~~Grumpy~0/0/0/0/0/0~",
            "A~~~Jolly~1/2/3~",
        ] {
            assert_eq!(Member::decode(token).custom, None, "{token}");
        }
    }

    #[test]
    fn validates_stat_point_limits() {
        let mut c = custom();
        assert!(c.validate().is_ok());
        c.points = [32, 32, 2, 0, 0, 1];
        assert!(c.validate().unwrap_err().contains("67"));
        c.points = [0, 40, 0, 0, 0, 0];
        assert!(c.validate().unwrap_err().contains("Atk"));
    }

    #[test]
    fn team_list_import_borrows_points_from_a_matching_spread() {
        let mut m = mon("Incineroar", 1, &[], &[]);
        m.spreads = vec![
            Spread {
                nature: "Careful".into(),
                points: [32, 0, 14, 0, 20, 0],
                pct: 5.0,
            },
            Spread {
                nature: "Adamant".into(),
                points: [32, 32, 0, 0, 2, 0],
                pct: 3.0,
            },
        ];
        let c = Custom::from_team_list(Some(&m), None, None, Some("Adamant".into()), vec![]);
        assert_eq!(c.points, [32, 32, 0, 0, 2, 0]);
        let c = Custom::from_team_list(Some(&m), None, None, Some("Timid".into()), vec![]);
        assert_eq!(
            (c.nature.as_str(), c.points),
            ("Timid", [32, 0, 14, 0, 20, 0])
        );
    }

    #[test]
    fn custom_items_take_priority_and_repeats_are_reported() {
        let a = mon("A", 1, &["Sitrus Berry"], &[]);
        let b = mon("B", 2, &["Life Orb", "Sitrus Berry"], &[]);
        // B's chosen Sitrus Berry is claimed first. A's only option is also Sitrus Berry, so
        // A keeps it and the clash is reported instead of silently changed.
        let mut c = custom();
        c.item = Some("Sitrus Berry".into());
        let sets = resolve_sets(&[(&a, None), (&b, Some(&c))]);
        assert_eq!(sets[1].item.as_deref(), Some("Sitrus Berry"));
        assert_eq!(repeated_items(&sets), vec!["Sitrus Berry".to_owned()]);
        assert_eq!(nature_label("Jolly"), "Jolly (+Spe −SpA)");
        assert_eq!(nature_label("Hardy"), "Hardy (neutral)");
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
