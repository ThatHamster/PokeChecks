//! PokeChecks' battle logic for the mobile app, compiled to WebAssembly.
//!
//! The app downloads the `/api/v1` data, hands the JSON to an [`Engine`], and asks it for
//! damage calcs, team stats and game plans, all on the device so they work offline. Every
//! answer comes from `pokechecks-core`, the same code the website runs, so the two always
//! agree.
//!
//! Each method has a plain Rust version (testable with `cargo test`) and a `js_` wrapper that
//! converts to and from JavaScript values. Results are plain JSON-style objects.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use pokechecks_core::calc::{self, Field, Outcome, SideInput, Terrain, Weather};
use pokechecks_core::dex::{Category, Dex, MoveData};
use pokechecks_core::gameplan::{self, GamePlan, PlanMon};
use pokechecks_core::smogon::{Meta, Mon};
use pokechecks_core::speed::{self, MatchState, Runner};
use pokechecks_core::team::{self, Custom, Matchup, Member};
use pokechecks_core::tournaments::{self, ListMon};

/// The data the app has downloaded, ready to answer questions about it.
#[wasm_bindgen]
pub struct Engine {
    dex: Dex,
    /// Usage keyed by format (`vgc`, `vgc-bo3`, `bss`).
    usage: HashMap<String, Meta>,
}

/// `/api/v1/usage/{format}` as the server sends it.
#[derive(Deserialize)]
struct UsageFile {
    format: String,
    #[serde(flatten)]
    meta: Meta,
}

// ---------- Answers ----------

/// How the calc opens: both sides on their most common sets, the attacker's main attack,
/// and weather and terrain from their abilities.
#[derive(Serialize)]
pub struct CalcStart {
    pub attacker: SideInput,
    pub defender: SideInput,
    pub move_name: Option<String>,
    pub weather: Option<&'static str>,
    pub terrain: Option<&'static str>,
}

#[derive(Deserialize)]
pub struct CalcRequest {
    pub attacker: SideInput,
    pub defender: SideInput,
    pub move_name: String,
    #[serde(default)]
    pub field: FieldInput,
}

/// The battle conditions, with weather and terrain as their keys (`rain`, `electric`).
#[derive(Deserialize)]
pub struct FieldInput {
    pub weather: Option<String>,
    pub terrain: Option<String>,
    #[serde(default)]
    pub crit: bool,
    #[serde(default)]
    pub helping_hand: bool,
    #[serde(default)]
    pub screen: bool,
    /// In doubles, a spread move hits both foes. On unless turned off, like the website.
    #[serde(default = "yes")]
    pub hits_both: bool,
}

fn yes() -> bool {
    true
}

impl Default for FieldInput {
    fn default() -> Self {
        Self {
            weather: None,
            terrain: None,
            crit: false,
            helping_hand: false,
            screen: false,
            hits_both: true,
        }
    }
}

#[derive(Serialize)]
pub struct CalcResult {
    #[serde(flatten)]
    pub outcome: Outcome,
    pub min_percent: f64,
    pub max_percent: f64,
    /// "Guaranteed OHKO", "43.8% chance to 2HKO" and so on.
    pub ko: String,
    /// The calc in one line, as on the website: `32+ Atk Miracle Seed Rillaboom Wood Hammer
    /// vs. 2 HP / 0 Def Psychic Seed Sneasler in Grassy Terrain`.
    pub description: String,
    /// Level 50 stats, HP/Atk/Def/SpA/SpD/Spe.
    pub attacker_stats: [u32; 6],
    pub defender_stats: [u32; 6],
    /// The attacker's other common attacks against the same defender.
    pub other_moves: Vec<MoveSummary>,
}

#[derive(Serialize)]
pub struct MoveSummary {
    pub name: String,
    /// Share of the attacker's sets that carry it.
    pub usage: f64,
    pub min_percent: f64,
    pub max_percent: f64,
    pub ko: String,
}

#[derive(Serialize)]
pub struct TeamResult {
    /// The members kept: unknown Pokémon, a second form of one species and anything past
    /// six are dropped, as on the website.
    pub members: Vec<Member>,
    pub sets: Vec<SetResult>,
    pub matchups: Vec<MatchupResult>,
    /// Items more than one member holds, which the item clause forbids.
    pub repeated_items: Vec<String>,
    /// Showdown / Poképaste import text.
    pub paste: String,
    /// Pokémon often used alongside this team, best first.
    pub suggestions: Vec<Suggestion>,
}

#[derive(Serialize)]
pub struct SetResult {
    pub name: String,
    pub item: Option<String>,
    pub ability: Option<String>,
    pub nature: Option<String>,
    pub points: Option<[u16; 6]>,
    pub moves: Vec<String>,
    /// The player changed this set; otherwise it's the most common one.
    pub custom: bool,
    pub stats: [u16; 6],
    /// Share of ladder sets with exactly this nature and spread.
    pub spread_percent: Option<f64>,
}

#[derive(Serialize)]
pub struct MatchupResult {
    #[serde(flatten)]
    pub matchup: Matchup,
    /// Several members weak and few to switch into it.
    pub problem: bool,
}

#[derive(Serialize)]
pub struct Suggestion {
    pub name: String,
    /// Average share of the members' teams that also run it, 0 to 1.
    pub score: f64,
}

#[derive(Serialize)]
pub struct PlanResult {
    /// Archetype keys such as `rain` or `trick-room`.
    pub archetypes: Vec<&'static str>,
    pub plan: GamePlan,
}

/// What the tournament filters need about one team list, without the game plan's text.
#[derive(Serialize)]
pub struct TeamInfo {
    /// The forms each Pokémon battles as: `Charizard` holding `Charizardite Y` is
    /// `Charizard-Mega-Y`.
    pub forms: Vec<String>,
    /// Each form's base species, so "Charizard" can match its Megas.
    pub species: Vec<String>,
    /// Archetype keys such as `rain` or `trick-room`.
    pub archetypes: Vec<&'static str>,
}

#[derive(Serialize)]
pub struct ArchetypeInfo {
    pub key: &'static str,
    pub label: &'static str,
}

#[derive(Serialize)]
pub struct ImportResult {
    /// The team list as builder members, stat points filled in from common spreads.
    pub members: Vec<Member>,
    /// Pokémon missing from this format's usage, which the builder can't show.
    pub missing: Vec<String>,
}

// ---------- The engine ----------

impl Engine {
    /// Builds an engine from `/api/v1/dex`.
    pub fn from_dex_json(dex_json: &str) -> Result<Self, String> {
        let dex = serde_json::from_str(dex_json).map_err(|e| format!("reading the Dex: {e}"))?;
        Ok(Self {
            dex,
            usage: HashMap::new(),
        })
    }

    /// Adds or replaces one format's usage from `/api/v1/usage/{format}`, returning the
    /// format key.
    pub fn add_usage_json(&mut self, usage_json: &str) -> Result<String, String> {
        let file: UsageFile =
            serde_json::from_str(usage_json).map_err(|e| format!("reading usage: {e}"))?;
        self.usage.insert(file.format.clone(), file.meta);
        Ok(file.format)
    }

    fn meta(&self, format: &str) -> Result<&Meta, String> {
        self.usage
            .get(format)
            .ok_or_else(|| format!("No usage loaded for “{format}”."))
    }

    fn mon<'a>(meta: &'a Meta, name: &str) -> Result<&'a Mon, String> {
        meta.mons
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| format!("{name} isn’t in this format’s usage."))
    }

    /// Where a Pokémon's icon sits on the icon sheet, as `[x, y]` pixels.
    pub fn icon(&self, name: &str) -> [u32; 2] {
        let (x, y) = self.dex.icon_offset(name);
        [x, y]
    }

    /// Where an item's icon sits on the item sheet, if Showdown has one.
    pub fn item_icon(&self, item: &str) -> Option<[u32; 2]> {
        self.dex.item_offset(item).map(|(x, y)| [x, y])
    }

    /// Types, base stats and abilities, for showing a Pokémon.
    pub fn types(&self, name: &str) -> Vec<String> {
        self.dex
            .entry(name)
            .map(|e| e.types.clone())
            .unwrap_or_default()
    }

    /// A move's type, category, power and flags, for showing it with its type badge.
    pub fn move_info(&self, name: &str) -> Option<&MoveData> {
        self.dex.move_data(name)
    }

    /// Every ability this form can have, including ones the ladder never uses.
    pub fn abilities(&self, name: &str) -> Vec<String> {
        self.dex
            .entry(name)
            .map(|e| e.abilities.clone())
            .unwrap_or_default()
    }

    /// The calc's opening state. Without names, the attacker is the most used Pokémon and
    /// the defender the next one, as on the website.
    pub fn calc_start(
        &self,
        format: &str,
        attacker: Option<&str>,
        defender: Option<&str>,
    ) -> Result<CalcStart, String> {
        let meta = self.meta(format)?;
        let att = match attacker {
            Some(n) => Self::mon(meta, n)?,
            None => meta.mons.first().ok_or("This format has no usage yet.")?,
        };
        let def = match defender {
            Some(n) => Self::mon(meta, n)?,
            None => meta
                .mons
                .iter()
                .find(|m| m.name != att.name)
                .ok_or("This format has no usage yet.")?,
        };
        let attacker = SideInput::defaults(att);
        let defender = SideInput::defaults(def);
        let moves = calc::damaging_moves(att, &self.dex);
        let sides = [&attacker, &defender];
        Ok(CalcStart {
            move_name: calc::default_move(&moves).map(str::to_owned),
            weather: sides
                .iter()
                .find_map(|s| s.ability.as_deref().and_then(Weather::from_ability))
                .map(Weather::key),
            terrain: sides
                .iter()
                .find_map(|s| s.ability.as_deref().and_then(Terrain::from_ability))
                .map(Terrain::key),
            attacker,
            defender,
        })
    }

    pub fn calc(&self, format: &str, req: &CalcRequest) -> Result<CalcResult, String> {
        let meta = self.meta(format)?;
        let mv = self
            .dex
            .move_data(&req.move_name)
            .filter(|m| m.category != Category::Status)
            .ok_or_else(|| format!("“{}” isn’t a damaging move.", req.move_name))?;
        let field = Field {
            doubles: self.doubles(format),
            hits_both: req.field.hits_both,
            weather: req.field.weather.as_deref().and_then(Weather::parse),
            terrain: req.field.terrain.as_deref().and_then(Terrain::parse),
            crit: req.field.crit,
            helping_hand: req.field.helping_hand,
            screen: req.field.screen,
        };
        let att = req.attacker.combatant(&self.dex);
        let def = req.defender.combatant(&self.dex);
        let summary = |o: &Outcome| {
            (
                o.percent(o.min()),
                o.percent(o.max()),
                calc::ko_chance(&o.rolls, o.defender_hp),
            )
        };
        let other_moves = meta
            .mons
            .iter()
            .find(|m| m.name == req.attacker.name)
            .map(|mon| {
                calc::damaging_moves(mon, &self.dex)
                    .into_iter()
                    .filter(|(name, _, _)| *name != mv.name)
                    .map(|(name, usage, data)| {
                        let (min_percent, max_percent, ko) =
                            summary(&calc::calculate(&att, &def, data, &field));
                        MoveSummary {
                            name: name.to_owned(),
                            usage,
                            min_percent,
                            max_percent,
                            ko,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let outcome = calc::calculate(&att, &def, mv, &field);
        let (min_percent, max_percent, ko) = summary(&outcome);
        Ok(CalcResult {
            description: calc::describe(&req.attacker, &req.defender, mv, &field, &outcome),
            outcome,
            min_percent,
            max_percent,
            ko,
            attacker_stats: std::array::from_fn(|i| att.stat(i)),
            defender_stats: std::array::from_fn(|i| def.stat(i)),
            other_moves,
        })
    }

    fn doubles(&self, format: &str) -> bool {
        format != "bss"
    }

    /// Everything the team builder shows for these members.
    pub fn team(&self, format: &str, members: &[Member]) -> Result<TeamResult, String> {
        let meta = self.meta(format)?;
        let mut mons: Vec<&Mon> = Vec::new();
        let mut kept = Vec::new();
        for member in members {
            if let Some(m) = meta.mons.iter().find(|m| m.name == member.name)
                && team::blocked(&mons, &member.name, &self.dex).is_none()
            {
                mons.push(m);
                kept.push(member.clone());
            }
        }
        let chosen: Vec<(&Mon, Option<&Custom>)> = mons
            .iter()
            .zip(&kept)
            .map(|(m, member)| (*m, member.custom.as_ref()))
            .collect();
        let sets = team::resolve_sets(&chosen);
        Ok(TeamResult {
            sets: sets
                .iter()
                .map(|s| SetResult {
                    name: s.mon.name.clone(),
                    item: s.item.clone(),
                    ability: s.ability.clone(),
                    nature: s.nature.clone(),
                    points: s.points,
                    moves: s.moves.clone(),
                    custom: s.custom,
                    stats: s.stats(&self.dex),
                    spread_percent: s.spread_pct(),
                })
                .collect(),
            matchups: team::matchups(&sets, &self.dex)
                .into_iter()
                .map(|matchup| MatchupResult {
                    problem: matchup.is_problem(),
                    matchup,
                })
                .collect(),
            repeated_items: team::repeated_items(&sets),
            paste: team::showdown_paste(&sets, &self.dex),
            suggestions: team::suggestions(&mons, meta, &self.dex, 6)
                .into_iter()
                .map(|(m, score)| Suggestion {
                    name: m.name.clone(),
                    score,
                })
                .collect(),
            members: kept,
        })
    }

    /// Who moves first: your team (on its builder sets) against the opponent's Pokémon at
    /// their most common spreads, with every Speed effect in `state` applied.
    pub fn turn_order(
        &self,
        format: &str,
        members: &[Member],
        state: &MatchState,
    ) -> Result<Vec<Runner>, String> {
        let meta = self.meta(format)?;
        let chosen: Vec<(&Mon, Option<&Custom>)> = members
            .iter()
            .filter_map(|m| {
                meta.mons
                    .iter()
                    .find(|x| x.name == m.name)
                    .map(|x| (x, m.custom.as_ref()))
            })
            .collect();
        let sets = team::resolve_sets(&chosen);
        Ok(speed::turn_order(state, &sets, meta, &self.dex))
    }

    /// The archetypes and "How this team plays" plan for a tournament team list.
    pub fn plan(&self, list: &[ListMon], doubles: bool) -> PlanResult {
        let mons: Vec<PlanMon> = list
            .iter()
            .map(|m| PlanMon::from_list(m, &self.dex))
            .collect();
        PlanResult {
            archetypes: gameplan::archetypes(&mons)
                .into_iter()
                .map(gameplan::Archetype::key)
                .collect(),
            plan: gameplan::plan(&mons, doubles),
        }
    }

    /// The battle forms, species and archetypes of a tournament team list.
    pub fn team_info(&self, list: &[ListMon]) -> TeamInfo {
        let mons: Vec<PlanMon> = list
            .iter()
            .map(|m| PlanMon::from_list(m, &self.dex))
            .collect();
        TeamInfo {
            species: mons
                .iter()
                .map(|m| self.dex.species(&m.name).to_owned())
                .collect(),
            archetypes: gameplan::archetypes(&mons)
                .into_iter()
                .map(gameplan::Archetype::key)
                .collect(),
            forms: mons.into_iter().map(|m| m.name).collect(),
        }
    }

    /// A team list as Showdown import text, exactly as written (no stat points).
    pub fn list_paste(&self, list: Vec<ListMon>) -> String {
        tournaments::team_list_paste(&tournaments::EntryTeam {
            placing: 0,
            player: String::new(),
            record: None,
            members: list,
        })
    }

    /// A tournament team list as team builder members.
    pub fn import_team(&self, format: &str, list: Vec<ListMon>) -> Result<ImportResult, String> {
        let meta = self.meta(format)?;
        let team = tournaments::EntryTeam {
            placing: 0,
            player: String::new(),
            record: None,
            members: list,
        };
        let (members, missing) = tournaments::to_members(&team, meta, &self.dex);
        Ok(ImportResult { members, missing })
    }
}

#[derive(Serialize)]
pub struct Nature {
    pub name: &'static str,
    /// `Adamant (+Atk −SpA)` or `Hardy (neutral)`.
    pub label: String,
}

/// All 25 natures with their effects, alphabetically.
pub fn natures() -> Vec<Nature> {
    team::NATURES
        .iter()
        .map(|&name| Nature {
            name,
            label: team::nature_label(name),
        })
        .collect()
}

/// The archetypes the tournament filters offer, in order, with their labels.
pub fn archetypes() -> Vec<ArchetypeInfo> {
    gameplan::Archetype::ALL
        .iter()
        .map(|a| ArchetypeInfo {
            key: a.key(),
            label: a.label(),
        })
        .collect()
}

/// Checks a changed set against the Champions rules: a real nature, at most 32 points in
/// one stat and 66 in all. Returns the problem in plain words, or nothing when it's fine.
pub fn check_set(custom: &Custom) -> Option<String> {
    custom.validate().err()
}

// ---------- JavaScript bindings ----------

/// Plain objects rather than `Map`s, so the app can read results like JSON.
fn to_js(value: &impl Serialize) -> Result<JsValue, JsError> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| JsError::new(&e.to_string()))
}

fn from_js<T: serde::de::DeserializeOwned>(value: JsValue) -> Result<T, JsError> {
    serde_wasm_bindgen::from_value(value).map_err(|e| JsError::new(&e.to_string()))
}

fn js_err(message: String) -> JsError {
    JsError::new(&message)
}

#[wasm_bindgen]
impl Engine {
    /// `new Engine(dexJson)` with the text of `/api/v1/dex`.
    #[wasm_bindgen(constructor)]
    pub fn js_new(dex_json: &str) -> Result<Engine, JsError> {
        Self::from_dex_json(dex_json).map_err(js_err)
    }

    /// Adds one format's usage (the text of `/api/v1/usage/{format}`); returns its key.
    #[wasm_bindgen(js_name = addUsage)]
    pub fn js_add_usage(&mut self, usage_json: &str) -> Result<String, JsError> {
        self.add_usage_json(usage_json).map_err(js_err)
    }

    #[wasm_bindgen(js_name = icon)]
    pub fn js_icon(&self, name: &str) -> Vec<u32> {
        self.icon(name).to_vec()
    }

    #[wasm_bindgen(js_name = itemIcon)]
    pub fn js_item_icon(&self, item: &str) -> Option<Vec<u32>> {
        self.item_icon(item).map(|p| p.to_vec())
    }

    #[wasm_bindgen(js_name = types)]
    pub fn js_types(&self, name: &str) -> Vec<String> {
        self.types(name)
    }

    #[wasm_bindgen(js_name = moveInfo)]
    pub fn js_move_info(&self, name: &str) -> Result<JsValue, JsError> {
        to_js(&self.move_info(name))
    }

    #[wasm_bindgen(js_name = abilities)]
    pub fn js_abilities(&self, name: &str) -> Vec<String> {
        self.abilities(name)
    }

    #[wasm_bindgen(js_name = calcStart)]
    pub fn js_calc_start(
        &self,
        format: &str,
        attacker: Option<String>,
        defender: Option<String>,
    ) -> Result<JsValue, JsError> {
        to_js(
            &self
                .calc_start(format, attacker.as_deref(), defender.as_deref())
                .map_err(js_err)?,
        )
    }

    #[wasm_bindgen(js_name = calc)]
    pub fn js_calc(&self, format: &str, request: JsValue) -> Result<JsValue, JsError> {
        let req: CalcRequest = from_js(request)?;
        to_js(&self.calc(format, &req).map_err(js_err)?)
    }

    #[wasm_bindgen(js_name = team)]
    pub fn js_team(&self, format: &str, members: JsValue) -> Result<JsValue, JsError> {
        let members: Vec<Member> = from_js(members)?;
        to_js(&self.team(format, &members).map_err(js_err)?)
    }

    #[wasm_bindgen(js_name = plan)]
    pub fn js_plan(&self, list: JsValue, doubles: bool) -> Result<JsValue, JsError> {
        let list: Vec<ListMon> = from_js(list)?;
        to_js(&self.plan(&list, doubles))
    }

    #[wasm_bindgen(js_name = turnOrder)]
    pub fn js_turn_order(
        &self,
        format: &str,
        members: JsValue,
        state: JsValue,
    ) -> Result<JsValue, JsError> {
        let members: Vec<Member> = from_js(members)?;
        let state: MatchState = from_js(state)?;
        to_js(&self.turn_order(format, &members, &state).map_err(js_err)?)
    }

    #[wasm_bindgen(js_name = teamInfo)]
    pub fn js_team_info(&self, list: JsValue) -> Result<JsValue, JsError> {
        let list: Vec<ListMon> = from_js(list)?;
        to_js(&self.team_info(&list))
    }

    #[wasm_bindgen(js_name = listPaste)]
    pub fn js_list_paste(&self, list: JsValue) -> Result<String, JsError> {
        Ok(self.list_paste(from_js(list)?))
    }

    #[wasm_bindgen(js_name = importTeam)]
    pub fn js_import_team(&self, format: &str, list: JsValue) -> Result<JsValue, JsError> {
        let list: Vec<ListMon> = from_js(list)?;
        to_js(&self.import_team(format, list).map_err(js_err)?)
    }
}

/// `archetypes()`: every archetype key with its label, in filter order.
#[wasm_bindgen(js_name = archetypes)]
pub fn js_archetypes() -> Result<JsValue, JsError> {
    to_js(&archetypes())
}

/// `natures()`: all 25 natures with labels like `Adamant (+Atk −SpA)`.
#[wasm_bindgen(js_name = natures)]
pub fn js_natures() -> Result<JsValue, JsError> {
    to_js(&natures())
}

/// `checkSet(custom)`: the problem with a changed set in plain words, or `undefined`.
#[wasm_bindgen(js_name = checkSet)]
pub fn js_check_set(custom: JsValue) -> Result<Option<String>, JsError> {
    Ok(check_set(&from_js(custom)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pokechecks_core::smogon::{Chaos, build_meta};

    const POKEDEX: &str = r#"{
        "incineroar": {"num": 727, "name": "Incineroar", "types": ["Fire", "Dark"],
            "baseStats": {"hp": 95, "atk": 115, "def": 90, "spa": 80, "spd": 90, "spe": 60},
            "abilities": {"0": "Blaze", "H": "Intimidate"}, "weightkg": 83},
        "rillaboom": {"num": 812, "name": "Rillaboom", "types": ["Grass"],
            "baseStats": {"hp": 100, "atk": 125, "def": 90, "spa": 60, "spd": 70, "spe": 85},
            "abilities": {"0": "Overgrow", "H": "Grassy Surge"}, "weightkg": 90},
        "pelipper": {"num": 279, "name": "Pelipper", "types": ["Water", "Flying"],
            "baseStats": {"hp": 60, "atk": 50, "def": 100, "spa": 95, "spd": 70, "spe": 65},
            "abilities": {"0": "Keen Eye", "H": "Drizzle"}, "weightkg": 28}
    }"#;
    const MOVES: &str = r#"{
        "flareblitz": {"name": "Flare Blitz", "basePower": 120, "type": "Fire",
            "category": "Physical", "flags": {"contact": 1}, "recoil": [33, 100], "target": "normal"},
        "fakeout": {"name": "Fake Out", "basePower": 40, "type": "Normal", "category": "Physical",
            "flags": {"contact": 1}, "secondary": {"chance": 100}, "target": "normal"},
        "uturn": {"name": "U-turn", "basePower": 70, "type": "Bug", "category": "Physical",
            "flags": {"contact": 1}, "target": "normal"},
        "woodhammer": {"name": "Wood Hammer", "basePower": 120, "type": "Grass",
            "category": "Physical", "flags": {"contact": 1}, "target": "normal"},
        "grassyglide": {"name": "Grassy Glide", "basePower": 55, "type": "Grass",
            "category": "Physical", "flags": {"contact": 1}, "target": "normal"},
        "hurricane": {"name": "Hurricane", "basePower": 110, "type": "Flying",
            "category": "Special", "flags": {}, "target": "any"},
        "weatherball": {"name": "Weather Ball", "basePower": 50, "type": "Normal",
            "category": "Special", "flags": {}, "target": "normal"},
        "protect": {"name": "Protect", "basePower": 0, "type": "Normal", "category": "Status",
            "flags": {}, "target": "self"}
    }"#;
    const ITEMS: &str = r#"exports.BattleItems = {sitrusberry:{name:"Sitrus Berry",spritenum:448},safetygoggles:{name:"Safety Goggles",spritenum:604},assaultvest:{name:"Assault Vest",spritenum:581},focussash:{name:"Focus Sash",spritenum:151}};"#;
    const ABILITIES: &str = r#"exports.BattleAbilities = {intimidate:{name:"Intimidate"},grassysurge:{name:"Grassy Surge"},drizzle:{name:"Drizzle"}};"#;
    const CHAOS: &str = r#"{"info": {"number of battles": 1000, "cutoff": 1630}, "data": {
        "Incineroar": {"usage": 0.5, "Abilities": {"intimidate": 100},
            "Items": {"sitrusberry": 60, "safetygoggles": 40},
            "Moves": {"fakeout": 90, "flareblitz": 80, "uturn": 50, "protect": 30},
            "Teammates": {"Rillaboom": 60, "Pelipper": 10},
            "Spreads": {"Careful:32/2/0/0/32/0": 50, "Adamant:32/32/0/0/2/0": 30}},
        "Rillaboom": {"usage": 0.4, "Abilities": {"grassysurge": 100},
            "Items": {"assaultvest": 70, "sitrusberry": 30},
            "Moves": {"fakeout": 95, "woodhammer": 85, "grassyglide": 80, "uturn": 60},
            "Teammates": {"Incineroar": 60, "Pelipper": 30},
            "Spreads": {"Adamant:32/32/0/0/0/2": 100}},
        "Pelipper": {"usage": 0.2, "Abilities": {"drizzle": 100}, "Items": {"focussash": 100},
            "Moves": {"hurricane": 90, "weatherball": 80, "protect": 70},
            "Teammates": {"Incineroar": 10, "Rillaboom": 30},
            "Spreads": {"Modest:32/0/0/32/0/2": 100}}
    }}"#;

    fn source_dex() -> Dex {
        let pokedex = serde_json::from_str(POKEDEX).unwrap();
        let moves = serde_json::from_str(MOVES).unwrap();
        Dex::from_sources(&pokedex, &moves, ITEMS, ABILITIES, "")
    }

    /// An engine fed the same JSON the server's API would send for the sample data.
    fn engine() -> Engine {
        let dex = source_dex();
        let chaos: Chaos = serde_json::from_str(CHAOS).unwrap();
        let meta = build_meta(&dex, chaos, None);
        let mut usage = serde_json::to_value(&meta).unwrap();
        usage["month"] = "2026-09".into();
        usage["format"] = "vgc".into();

        let mut engine = Engine::from_dex_json(&serde_json::to_string(&dex).unwrap()).unwrap();
        assert_eq!(engine.add_usage_json(&usage.to_string()).unwrap(), "vgc");
        engine
    }

    #[test]
    fn dex_survives_the_trip_through_json() {
        let engine = engine();
        let dex = source_dex();
        assert_eq!(engine.types("Pelipper"), ["Water", "Flying"]);
        let (x, y) = dex.icon_offset("Rillaboom");
        assert_eq!(engine.icon("Rillaboom"), [x, y]);
        assert_eq!(
            engine.item_icon("Sitrus Berry"),
            dex.item_offset("Sitrus Berry").map(|(x, y)| [x, y])
        );
        assert!(engine.item_icon("Sitrus Berry").is_some());
        assert_eq!(engine.item_icon("Not An Item"), None);
    }

    #[test]
    fn calc_opens_like_the_website() {
        let start = engine().calc_start("vgc", None, None).unwrap();
        assert_eq!(start.attacker.name, "Incineroar");
        assert_eq!(start.defender.name, "Rillaboom");
        assert_eq!(start.attacker.item.as_deref(), Some("Sitrus Berry"));
        assert_eq!(start.attacker.nature, "Careful");
        assert_eq!(start.attacker.points, [32, 2, 0, 0, 32, 0]);
        // The strongest attack on at least a quarter of sets, not the more common Fake Out.
        assert_eq!(start.move_name.as_deref(), Some("Flare Blitz"));
        assert_eq!(start.terrain, Some("grassy"), "from Grassy Surge");
        assert_eq!(start.weather, None);

        let rain = engine()
            .calc_start("vgc", Some("Pelipper"), Some("Incineroar"))
            .unwrap();
        assert_eq!(rain.weather, Some("rain"));
        assert!(engine().calc_start("bss", None, None).is_err());
        assert!(engine().calc_start("vgc", Some("Pikachu"), None).is_err());
    }

    #[test]
    fn calc_matches_the_core_calculator() {
        let engine = engine();
        let start = engine.calc_start("vgc", None, None).unwrap();
        let req = CalcRequest {
            attacker: start.attacker.clone(),
            defender: start.defender.clone(),
            move_name: "Flare Blitz".into(),
            field: FieldInput::default(),
        };
        let got = engine.calc("vgc", &req).unwrap();

        let dex = source_dex();
        let field = Field {
            doubles: true,
            hits_both: true,
            ..Field::default()
        };
        let want = calc::calculate(
            &start.attacker.combatant(&dex),
            &start.defender.combatant(&dex),
            dex.move_data("Flare Blitz").unwrap(),
            &field,
        );
        assert_eq!(got.outcome.rolls, want.rolls);
        assert_eq!(got.ko, calc::ko_chance(&want.rolls, want.defender_hp));
        assert_eq!(got.attacker_stats[0], start.attacker.combatant(&dex).hp());
        assert!(got.description.contains("Incineroar Flare Blitz vs."));
        let others: Vec<&str> = got.other_moves.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            others,
            ["Fake Out", "U-turn"],
            "damaging moves only, most used first"
        );

        let protect = CalcRequest {
            move_name: "Protect".into(),
            ..req
        };
        assert!(engine.calc("vgc", &protect).is_err());
    }

    #[test]
    fn calc_reads_the_field_from_javascript_style_json() {
        let engine = engine();
        let start = engine.calc_start("vgc", None, None).unwrap();
        let mut req = serde_json::json!({
            "attacker": start.attacker,
            "defender": start.defender,
            "move_name": "Flare Blitz",
        });
        let plain: CalcRequest = serde_json::from_value(req.clone()).unwrap();
        assert!(
            plain.field.hits_both,
            "spread moves hit both foes by default"
        );
        req["field"] = serde_json::json!({ "weather": "sun", "crit": true });
        let sunny: CalcRequest = serde_json::from_value(req).unwrap();
        assert!(sunny.field.hits_both);
        let boosted = engine.calc("vgc", &sunny).unwrap();
        let normal = engine.calc("vgc", &plain).unwrap();
        assert!(boosted.outcome.max() > normal.outcome.max());
    }

    #[test]
    fn team_follows_the_website_rules() {
        let engine = engine();
        let mut rillaboom = Member::new("Rillaboom");
        rillaboom.custom = Some(Custom {
            item: Some("Sitrus Berry".into()),
            ability: Some("Grassy Surge".into()),
            nature: "Adamant".into(),
            points: [32, 32, 0, 0, 0, 2],
            moves: vec!["Wood Hammer".into(), "Fake Out".into()],
        });
        let members = [
            Member::new("Incineroar"),
            Member::new("Incineroar"),
            rillaboom,
            Member::new("Missingno"),
        ];
        let team = engine.team("vgc", &members).unwrap();
        let names: Vec<&str> = team.members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["Incineroar", "Rillaboom"]);
        // Rillaboom's chosen Sitrus Berry pushes Incineroar to its next item.
        assert_eq!(team.sets[0].item.as_deref(), Some("Safety Goggles"));
        assert!(!team.sets[0].custom && team.sets[1].custom);
        assert_eq!(
            team.sets[0].stats[0], 202,
            "95 base HP with 32 points at level 50"
        );
        assert_eq!(team.matchups.len(), 18);
        assert!(team.paste.starts_with("Incineroar @ Safety Goggles"));
        assert_eq!(team.suggestions.len(), 1);
        assert_eq!(team.suggestions[0].name, "Pelipper");
    }

    #[test]
    fn set_editor_lookups() {
        let engine = engine();
        let hurricane = engine.move_info("Hurricane").unwrap();
        assert_eq!(hurricane.move_type, "Flying");
        assert_eq!(hurricane.category, Category::Special);
        assert!(engine.move_info("Not A Move").is_none());
        assert_eq!(engine.abilities("Pelipper"), ["Keen Eye", "Drizzle"]);
        let all = natures();
        assert_eq!(all.len(), 25);
        assert_eq!(all[0].label, "Adamant (+Atk −SpA)");
        assert!(all.iter().any(|n| n.label == "Hardy (neutral)"));
    }

    #[test]
    fn turn_order_sorts_and_marks_close_calls() {
        use pokechecks_core::speed::{Foe, Side};
        let engine = engine();
        let foe = |name: &str| Foe {
            name: name.into(),
            stage: 0,
            paralyzed: false,
            scarf: false,
            unburden: false,
        };
        let mut state = MatchState {
            foes: vec![foe("Incineroar"), foe("Pelipper"), foe("Missingno")],
            ..MatchState::default()
        };
        let team = [Member::new("Rillaboom")];
        let order = engine.turn_order("vgc", &team, &state).unwrap();
        let names: Vec<&str> = order.iter().map(|r| r.name.as_str()).collect();
        // Rillaboom (Adamant, 2 Spe) 107; Pelipper (Modest, 2 Spe) 86; Incineroar (Careful) 80.
        assert_eq!(
            names,
            ["Rillaboom", "Pelipper", "Incineroar"],
            "unknown foes dropped"
        );
        assert_eq!(order[0].side, Side::Mine);
        assert_eq!(order[0].speed, 107);
        // Pelipper could reach 128 with a Timid 32 spread, so it might outspeed Rillaboom.
        assert_eq!(order[1].close, ["Rillaboom"]);

        state.trick_room = true;
        state.weather = Some(pokechecks_core::calc::Weather::Rain);
        let tr = engine.turn_order("vgc", &team, &state).unwrap();
        assert_eq!(tr[0].name, "Incineroar", "slowest first under Trick Room");
        let json = serde_json::to_value(&tr[0]).unwrap();
        assert_eq!(json["side"], "foe");
        let back: MatchState = serde_json::from_value(
            serde_json::json!({ "foes": [], "mine": [], "trick_room": false,
                "my_tailwind": false, "foe_tailwind": false, "my_drop": false, "foe_drop": false,
                "weather": "rain" }),
        )
        .unwrap();
        assert_eq!(back.weather, Some(pokechecks_core::calc::Weather::Rain));
    }

    #[test]
    fn sets_are_checked_against_the_limits() {
        let mut set = Custom {
            item: None,
            ability: None,
            nature: "Adamant".into(),
            points: [32, 32, 2, 0, 0, 0],
            moves: vec![],
        };
        assert_eq!(check_set(&set), None);
        set.points[2] = 3;
        assert!(check_set(&set).unwrap().contains("66"));
        set.nature = "Sleepy".into();
        assert!(check_set(&set).is_some());
    }

    #[test]
    fn tournament_teams_get_plans_and_import() {
        let engine = engine();
        let list = vec![
            ListMon {
                id: "pelipper".into(),
                name: "Pelipper".into(),
                item: Some("Focus Sash".into()),
                ability: Some("Drizzle".into()),
                nature: Some("Modest".into()),
                moves: vec!["Hurricane".into(), "Weather Ball".into(), "Protect".into()],
            },
            ListMon {
                id: "incineroar".into(),
                name: "Incineroar".into(),
                item: None,
                ability: Some("Intimidate".into()),
                nature: Some("Careful".into()),
                moves: vec!["Fake Out".into(), "Flare Blitz".into()],
            },
        ];
        let info = engine.team_info(&list);
        assert_eq!(info.forms, ["Pelipper", "Incineroar"]);
        assert_eq!(info.species, ["Pelipper", "Incineroar"]);
        assert!(info.archetypes.contains(&"rain"));
        assert!(engine.list_paste(list.clone()).starts_with(
            "Pelipper @ Focus Sash
"
        ));
        assert_eq!(archetypes().len(), 7);
        assert_eq!(archetypes()[0].key, "sun");

        let plan = engine.plan(&list, true);
        assert!(plan.archetypes.contains(&"rain"));
        assert!(!plan.plan.steps.is_empty());

        let imported = engine.import_team("vgc", list).unwrap();
        assert_eq!(imported.members.len(), 2);
        assert!(imported.missing.is_empty());
        let custom = imported.members[1].custom.as_ref().unwrap();
        assert_eq!(
            custom.points,
            [32, 2, 0, 0, 32, 0],
            "spread from the usage stats"
        );
    }
}
