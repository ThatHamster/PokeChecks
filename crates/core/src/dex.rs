//! Base stats, types and display names from Pokémon Showdown's public data files.

use std::collections::HashMap;

use regex::Regex;
use serde_json::Value;

/// Showdown's ID format: lowercase ASCII letters and digits only.
pub fn to_id(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

pub struct DexEntry {
    /// Display name, e.g. `Charizard-Mega-Y`.
    pub name: String,
    pub types: Vec<String>,
    /// HP, Atk, Def, SpA, SpD, Spe.
    pub stats: [u16; 6],
    /// The species a form belongs to, e.g. `Salamence` for `Salamence-Mega`. Two forms of
    /// one species can't share a team.
    pub base_species: String,
    /// Abilities this form can have.
    pub abilities: Vec<String>,
    /// For Mega forms, the stone that triggers it, e.g. `Charizardite Y`.
    pub required_item: Option<String>,
    /// For Low Kick, Grass Knot, Heavy Slam and Heat Crash.
    pub weight_kg: f32,
    /// National Dex number, which is also the base form's place on the icon sheet.
    pub num: i32,
}

impl DexEntry {
    pub fn bst(&self) -> u16 {
        self.stats.iter().sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Physical,
    Special,
    Status,
}

/// What the damage calculator needs to know about a move.
#[derive(Clone, Debug)]
pub struct MoveData {
    pub name: String,
    pub base_power: u16,
    pub move_type: String,
    pub category: Category,
    /// Hits both opponents (`allAdjacentFoes`) or everyone else (`allAdjacent`).
    pub spread: bool,
    pub contact: bool,
    pub punch: bool,
    pub bite: bool,
    pub slicing: bool,
    pub pulse: bool,
    pub sound: bool,
    /// Fixed hits (Dragon Darts: 2), or the range for moves like Bullet Seed.
    pub hits: Option<(u8, u8)>,
    pub always_crits: bool,
    /// Has a secondary effect, for Sheer Force.
    pub secondary: bool,
    /// Has recoil, for Reckless.
    pub recoil: bool,
    /// Body Press attacks with Defense.
    pub uses_defense: bool,
    /// Psyshock and similar hit the target's Defense.
    pub targets_defense: bool,
    /// Foul Play uses the target's Attack.
    pub uses_target_attack: bool,
}

pub struct Dex {
    entries: HashMap<String, DexEntry>,
    /// Move, item and ability IDs mapped to display names.
    names: HashMap<String, String>,
    moves: HashMap<String, MoveData>,
    /// Icon sheet positions for forms whose icon isn't at their Dex number.
    icon_indexes: HashMap<String, u16>,
    /// Item IDs mapped to their place on the item icon sheet.
    item_sprites: HashMap<String, u16>,
}

impl Dex {
    /// Builds the Dex from Showdown's `pokedex.json`, `moves.json`, `items.js` and
    /// `abilities.js`, plus the client code holding `BattlePokemonIconIndexes` (empty if it
    /// couldn't be downloaded, which only costs form-specific icons).
    pub fn from_sources(
        pokedex: &Value,
        moves: &Value,
        items: &str,
        abilities: &str,
        icon_indexes: &str,
    ) -> Self {
        let entries = parse_pokedex(pokedex);
        let mut names = HashMap::new();
        let mut move_data = HashMap::new();
        if let Value::Object(map) = moves {
            for (id, m) in map {
                if let Some(name) = m.get("name").and_then(Value::as_str) {
                    names.insert(id.clone(), name.to_owned());
                }
                if let Some(data) = parse_move(m) {
                    move_data.insert(id.clone(), data);
                }
            }
        }
        parse_js_names(items, &mut names);
        parse_js_names(abilities, &mut names);
        Self {
            entries,
            names,
            moves: move_data,
            icon_indexes: parse_icon_indexes(icon_indexes),
            item_sprites: parse_item_sprites(items),
        }
    }

    pub fn pokemon_count(&self) -> usize {
        self.entries.len()
    }

    pub fn name_count(&self) -> usize {
        self.names.len()
    }

    pub fn entry(&self, name: &str) -> Option<&DexEntry> {
        self.entries.get(&to_id(name))
    }

    pub fn move_data(&self, name: &str) -> Option<&MoveData> {
        self.moves.get(&to_id(name))
    }

    /// Base species for the duplicate-species rule; falls back to the name itself.
    pub fn species<'a>(&'a self, name: &'a str) -> &'a str {
        self.entry(name).map_or(name, |e| e.base_species.as_str())
    }

    /// The form a team list entry battles as. Team lists name Megas by their base species
    /// plus stone (`Charizard` @ `Charizardite Y`), while usage stats use the Mega form's
    /// name (`Charizard-Mega-Y`).
    pub fn battle_form(&self, species: &str, item: Option<&str>) -> String {
        let Some(entry) = self.entry(species) else {
            return species.to_owned();
        };
        if let Some(item) = item {
            let mega = self.entries.values().find(|e| {
                e.base_species == entry.base_species && e.required_item.as_deref() == Some(item)
            });
            if let Some(m) = mega {
                return m.name.clone();
            }
        }
        entry.name.clone()
    }

    /// Where a Pokémon's icon sits on the mini icon sheet, as a CSS background offset
    /// `(x, y)`.
    /// Unknown names get Showdown's blank icon (index 0), like Showdown itself does.
    pub fn icon_offset(&self, name: &str) -> (u32, u32) {
        let id = to_id(name);
        let index = self.icon_indexes.get(&id).copied().unwrap_or_else(|| {
            self.entries
                .get(&id)
                .and_then(|e| u16::try_from(e.num).ok())
                .filter(|&n| n <= 1025)
                .unwrap_or(0)
        });
        let index = u32::from(index);
        ((index % 12) * 40, (index / 12) * 30)
    }

    /// Where an item's icon sits on the item icon sheet, as a CSS background offset
    /// `(x, y)`, or `None` for an item Showdown doesn't know.
    pub fn item_offset(&self, item: &str) -> Option<(u32, u32)> {
        let index = u32::from(*self.item_sprites.get(&to_id(item))?);
        Some(((index % 16) * 24, (index / 16) * 24))
    }

    /// Turns a stats-file ID like `heavydutyboots` into `Heavy-Duty Boots`.
    pub fn display_name(&self, id: &str) -> String {
        self.names.get(id).cloned().unwrap_or_else(|| id.to_owned())
    }
}

/// Reads each item's `spritenum` from Showdown's `items.js`. Entries look like
/// `miracleseed:{name:"Miracle Seed",fling:{basePower:30},spritenum:292,…}`, so the number
/// is looked for anywhere between one entry's start and the next.
fn parse_item_sprites(src: &str) -> HashMap<String, u16> {
    let entry = Regex::new(r#"(\w+):\{name:""#).expect("valid regex");
    let sprite = Regex::new(r"spritenum:(\d+)").expect("valid regex");
    let starts: Vec<(usize, &str)> = entry
        .captures_iter(src)
        .map(|c| (c.get(0).unwrap().start(), c.get(1).unwrap().as_str()))
        .collect();
    starts
        .iter()
        .enumerate()
        .filter_map(|(i, (start, id))| {
            let end = starts.get(i + 1).map_or(src.len(), |(next, _)| *next);
            let num = sprite.captures(&src[*start..end])?[1].parse().ok()?;
            Some(((*id).to_owned(), num))
        })
        .collect()
}

fn parse_pokedex(v: &Value) -> HashMap<String, DexEntry> {
    let Value::Object(map) = v else {
        return HashMap::new();
    };
    map.iter()
        .filter_map(|(id, e)| {
            let types = e
                .get("types")?
                .as_array()?
                .iter()
                .filter_map(|t| t.as_str().map(str::to_owned))
                .collect();
            let bs = e.get("baseStats")?;
            let stat = |k: &str| bs.get(k).and_then(Value::as_u64).map(|n| n as u16);
            let stats = [
                stat("hp")?,
                stat("atk")?,
                stat("def")?,
                stat("spa")?,
                stat("spd")?,
                stat("spe")?,
            ];
            let name = e
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_owned();
            let base_species = e
                .get("baseSpecies")
                .and_then(Value::as_str)
                .unwrap_or(&name)
                .to_owned();
            let abilities = e
                .get("abilities")
                .and_then(Value::as_object)
                .map(|a| {
                    a.values()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            let required_item = e
                .get("requiredItem")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let weight_kg = e.get("weightkg").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            let num = e.get("num").and_then(Value::as_i64).unwrap_or(0) as i32;
            Some((
                id.clone(),
                DexEntry {
                    name,
                    types,
                    stats,
                    base_species,
                    abilities,
                    required_item,
                    weight_kg,
                    num,
                },
            ))
        })
        .collect()
}

/// Reads `BattlePokemonIconIndexes={egg:1032+1,pikachubelle:1032+2,...}` from Showdown's
/// client code. Values are sums of integers.
fn parse_icon_indexes(src: &str) -> HashMap<String, u16> {
    let Some(start) = src.find("BattlePokemonIconIndexes={") else {
        return HashMap::new();
    };
    let body = &src[start + "BattlePokemonIconIndexes={".len()..];
    let body = &body[..body.find('}').unwrap_or(body.len())];
    body.split(',')
        .filter_map(|pair| {
            let (key, value) = pair.split_once(':')?;
            let sum = value
                .split('+')
                .map(|n| n.trim().parse::<u16>().ok())
                .sum::<Option<u16>>()?;
            Some((key.trim().trim_matches(['"', '\'']).to_owned(), sum))
        })
        .collect()
}

fn parse_move(m: &Value) -> Option<MoveData> {
    let category = match m.get("category")?.as_str()? {
        "Physical" => Category::Physical,
        "Special" => Category::Special,
        _ => Category::Status,
    };
    let flag = |f: &str| m.get("flags").and_then(|fl| fl.get(f)).is_some();
    let target = m.get("target").and_then(Value::as_str).unwrap_or("");
    let hits = match m.get("multihit") {
        Some(Value::Number(n)) => n.as_u64().map(|n| (n as u8, n as u8)),
        Some(Value::Array(a)) => match (
            a.first().and_then(Value::as_u64),
            a.get(1).and_then(Value::as_u64),
        ) {
            (Some(lo), Some(hi)) => Some((lo as u8, hi as u8)),
            _ => None,
        },
        _ => None,
    };
    Some(MoveData {
        name: m.get("name")?.as_str()?.to_owned(),
        base_power: m.get("basePower").and_then(Value::as_u64).unwrap_or(0) as u16,
        move_type: m.get("type")?.as_str()?.to_owned(),
        category,
        spread: matches!(target, "allAdjacentFoes" | "allAdjacent"),
        contact: flag("contact"),
        punch: flag("punch"),
        bite: flag("bite"),
        slicing: flag("slicing"),
        pulse: flag("pulse"),
        sound: flag("sound"),
        hits,
        always_crits: m.get("willCrit").and_then(Value::as_bool).unwrap_or(false),
        secondary: m.get("secondary").is_some_and(|s| !s.is_null())
            || m.get("secondaries").is_some_and(|s| !s.is_null()),
        recoil: m.get("recoil").is_some_and(|r| !r.is_null()),
        uses_defense: m.get("overrideOffensiveStat").and_then(Value::as_str) == Some("def"),
        targets_defense: m.get("overrideDefensiveStat").and_then(Value::as_str) == Some("def"),
        uses_target_attack: m.get("overrideOffensivePokemon").and_then(Value::as_str)
            == Some("target"),
    })
}

/// `items.js` and `abilities.js` are JS object literals rather than JSON, so pull out
/// each top-level `id:{ ... name:"Display Name"` pair.
fn parse_js_names(src: &str, out: &mut HashMap<String, String>) {
    let key_re = Regex::new(r"[,{](\w+):\{").expect("valid regex");
    let name_re = Regex::new(r#"name:"([^"]+)""#).expect("valid regex");
    let names: Vec<(usize, &str)> = name_re
        .captures_iter(src)
        .map(|c| (c.get(0).unwrap().start(), c.get(1).unwrap().as_str()))
        .collect();

    for key in key_re.captures_iter(src) {
        let end = key.get(0).unwrap().end();
        let next = names.partition_point(|(pos, _)| *pos < end);
        if let Some((pos, name)) = names.get(next)
            && pos - end <= 1500
        {
            out.entry(key[1].to_owned())
                .or_insert_with(|| (*name).to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_match_showdown() {
        assert_eq!(to_id("Ogerpon-Wellspring"), "ogerponwellspring");
        assert_eq!(to_id("Mr. Mime"), "mrmime");
    }

    #[test]
    fn reads_names_from_js_literals() {
        let src = r#"exports.BattleAbilities={intimidate:{flags:{},name:"Intimidate",rating:3.5},heavydutyboots:{num:1120,fling:{basePower:80},spritenum:715,name:"Heavy-Duty Boots"}}"#;
        let mut names = HashMap::new();
        parse_js_names(src, &mut names);
        assert_eq!(names["intimidate"], "Intimidate");
        assert_eq!(names["heavydutyboots"], "Heavy-Duty Boots");
    }

    #[test]
    fn reads_item_sprite_numbers() {
        let src = r#"exports.BattleItems = {abilityshield:{name:"Ability Shield",spritenum:746,fling:{basePower:30},num:1881},miracleseed:{name:"Miracle Seed",fling:{basePower:30},spritenum:292,num:239},raichunitey:{name:"Raichunite Y",spritenum:497,megaStone:{Raichu:"Raichu-Mega-Y"}},nosprite:{name:"No Sprite",num:1}};"#;
        let sprites = parse_item_sprites(src);
        assert_eq!(sprites.len(), 3);
        assert_eq!(sprites["abilityshield"], 746);
        assert_eq!(sprites["miracleseed"], 292, "spritenum after another field");
        assert_eq!(sprites["raichunitey"], 497);
    }

    #[test]
    fn reads_icon_indexes() {
        let src = "var BattlePokemonIconIndexes={

egg:1032+1,
clefablemega:1320+84,
obliteryx:1560+81
};
var BattlePokemonIconIndexesLeft={
pikachu:1
};";
        let indexes = parse_icon_indexes(src);
        assert_eq!(indexes.len(), 3);
        assert_eq!(indexes["egg"], 1033);
        assert_eq!(indexes["clefablemega"], 1404);
        assert_eq!(indexes["obliteryx"], 1641);
    }
}
