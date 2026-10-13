//! Damage calculation at level 50 with Champions stats. Follows Pokémon Showdown's Gen 9
//! formula and its 4096-based modifier rounding, applied in the same order: base power,
//! stats, base damage, spread, weather, critical hit, random roll, STAB, type, burn, then
//! the final modifiers.

use serde::{Deserialize, Serialize};

use crate::dex::{Category, Dex, MoveData};
use crate::smogon::Mon;
use crate::team::{level50_stat, multiplier};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weather {
    Sun,
    Rain,
    Sand,
    Snow,
}

impl Weather {
    pub const ALL: [Weather; 4] = [Weather::Sun, Weather::Rain, Weather::Sand, Weather::Snow];

    pub fn key(self) -> &'static str {
        match self {
            Self::Sun => "sun",
            Self::Rain => "rain",
            Self::Sand => "sand",
            Self::Snow => "snow",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sun => "Sun",
            Self::Rain => "Rain",
            Self::Sand => "Sand",
            Self::Snow => "Snow",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|w| w.key() == s)
    }

    /// The weather an ability sets on entry.
    pub fn from_ability(a: &str) -> Option<Self> {
        Some(match a {
            "Drought" | "Orichalcum Pulse" => Self::Sun,
            "Drizzle" => Self::Rain,
            "Sand Stream" => Self::Sand,
            "Snow Warning" => Self::Snow,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terrain {
    Electric,
    Grassy,
    Psychic,
    Misty,
}

impl Terrain {
    pub const ALL: [Terrain; 4] = [
        Terrain::Electric,
        Terrain::Grassy,
        Terrain::Psychic,
        Terrain::Misty,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Electric => "electric",
            Self::Grassy => "grassy",
            Self::Psychic => "psychic",
            Self::Misty => "misty",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Electric => "Electric Terrain",
            Self::Grassy => "Grassy Terrain",
            Self::Psychic => "Psychic Terrain",
            Self::Misty => "Misty Terrain",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == s)
    }

    pub fn from_ability(a: &str) -> Option<Self> {
        Some(match a {
            "Electric Surge" | "Hadron Engine" => Self::Electric,
            "Grassy Surge" => Self::Grassy,
            "Psychic Surge" => Self::Psychic,
            "Misty Surge" => Self::Misty,
            _ => return None,
        })
    }

    /// The move type this terrain powers up for grounded attackers.
    fn boosted_type(self) -> Option<&'static str> {
        match self {
            Self::Electric => Some("Electric"),
            Self::Grassy => Some("Grass"),
            Self::Psychic => Some("Psychic"),
            Self::Misty => None,
        }
    }
}

/// One side of the calculation.
#[derive(Clone, Debug)]
pub struct Combatant {
    pub name: String,
    pub types: Vec<String>,
    /// Base stats, HP/Atk/Def/SpA/SpD/Spe.
    pub base: [u16; 6],
    pub weight_kg: f32,
    pub item: Option<String>,
    pub ability: Option<String>,
    pub nature: String,
    pub points: [u16; 6],
    /// Stat stage of the stat this side uses in the calc (attacking or defending), −6 to +6.
    pub boost: i8,
    pub burned: bool,
}

impl Combatant {
    pub fn stat(&self, i: usize) -> u32 {
        u32::from(level50_stat(self.base[i], self.points[i], i, &self.nature))
    }

    pub fn hp(&self) -> u32 {
        self.stat(0)
    }

    fn item_is(&self, item: &str) -> bool {
        self.item.as_deref() == Some(item)
    }

    fn ability_is(&self, names: &[&str]) -> bool {
        self.ability.as_deref().is_some_and(|a| names.contains(&a))
    }

    fn has_type(&self, t: &str) -> bool {
        self.types.iter().any(|x| x == t)
    }

    /// On the ground, so terrain affects it.
    fn grounded(&self) -> bool {
        !self.has_type("Flying") && !self.ability_is(&["Levitate"]) && !self.item_is("Air Balloon")
    }
}

/// One side's set as chosen in the calc, before the Dex fills in types and base stats.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SideInput {
    pub name: String,
    pub item: Option<String>,
    pub ability: Option<String>,
    pub nature: String,
    pub points: [u16; 6],
    /// Stat stage of the stat this side uses in the calc, −6 to +6.
    pub boost: i8,
    pub burned: bool,
}

impl SideInput {
    /// The Pokémon's most common set.
    pub fn defaults(mon: &Mon) -> Self {
        let spread = mon.top_spread();
        Self {
            name: mon.name.clone(),
            item: mon.items.first().map(|i| i.name.clone()),
            ability: mon.abilities.first().map(|a| a.name.clone()),
            nature: spread.map_or_else(|| "Hardy".to_owned(), |s| s.nature.clone()),
            points: spread.map_or([0; 6], |s| s.points),
            boost: 0,
            burned: false,
        }
    }

    pub fn combatant(&self, dex: &Dex) -> Combatant {
        let entry = dex.entry(&self.name);
        Combatant {
            name: self.name.clone(),
            types: entry.map(|e| e.types.clone()).unwrap_or_default(),
            base: entry.map_or([0; 6], |e| e.stats),
            weight_kg: entry.map_or(0.0, |e| e.weight_kg),
            item: self.item.clone(),
            ability: self.ability.clone(),
            nature: self.nature.clone(),
            points: self.points,
            boost: self.boost,
            burned: self.burned,
        }
    }

    pub fn total_points(&self) -> u16 {
        self.points.iter().sum()
    }
}

/// Damaging moves the ladder uses on this Pokémon, most used first.
pub fn damaging_moves<'a>(mon: &'a Mon, dex: &'a Dex) -> Vec<(&'a str, f64, &'a MoveData)> {
    mon.moves
        .iter()
        .filter_map(|m| {
            dex.move_data(&m.name)
                .filter(|d| d.category != Category::Status)
                .map(|d| (m.name.as_str(), m.pct, d))
        })
        .collect()
}

/// Share of sets a move needs before it can be the default.
const COMMON_MOVE: f64 = 25.0;

/// The strongest commonly used attack, so the calc opens on a main attack rather than a
/// utility move like Fake Out. Falls back to the most used damaging move.
pub fn default_move<'a>(moves: &[(&'a str, f64, &MoveData)]) -> Option<&'a str> {
    moves
        .iter()
        .filter(|(_, pct, _)| *pct >= COMMON_MOVE)
        .max_by_key(|(_, _, d)| d.base_power)
        .or(moves.first())
        .map(|(n, _, _)| *n)
}

#[derive(Clone, Debug, Default)]
pub struct Field {
    pub doubles: bool,
    /// In doubles, a spread move is hitting two targets (so it does 75% damage).
    pub hits_both: bool,
    pub weather: Option<Weather>,
    pub terrain: Option<Terrain>,
    pub crit: bool,
    pub helping_hand: bool,
    /// Reflect against physical moves, Light Screen against special ones.
    pub screen: bool,
}

#[derive(Serialize)]
pub struct Outcome {
    /// Total damage for each of the 16 random rolls, lowest first.
    pub rolls: [u32; 16],
    pub defender_hp: u32,
    pub move_type: String,
    pub effectiveness: f32,
    pub hits: u8,
    pub base_power: u32,
    pub crit: bool,
    pub spread: bool,
    /// Effects that changed the result, in plain words.
    pub notes: Vec<String>,
}

impl Outcome {
    pub fn min(&self) -> u32 {
        self.rolls[0]
    }

    pub fn max(&self) -> u32 {
        self.rolls[15]
    }

    pub fn percent(&self, damage: u32) -> f64 {
        f64::from(damage) * 100.0 / f64::from(self.defender_hp.max(1))
    }
}

/// Showdown's `modify`: multiply by a 4096-based modifier, rounding halves down.
fn modify(value: u32, modifier: u32) -> u32 {
    ((u64::from(value) * u64::from(modifier) + 2047) / 4096) as u32
}

/// Showdown's `chainModify`: combine two 4096-based modifiers.
fn chain(a: u32, b: u32) -> u32 {
    ((u64::from(a) * u64::from(b) + 2048) >> 12) as u32
}

fn apply_stage(stat: u32, stage: i8) -> u32 {
    let s = u32::from(stage.unsigned_abs().min(6));
    if stage >= 0 {
        stat * (2 + s) / 2
    } else {
        stat * 2 / (2 + s)
    }
}

/// The item that boosts each type by 20%.
fn type_item(item: &str) -> Option<&'static str> {
    Some(match item {
        "Charcoal" => "Fire",
        "Mystic Water" => "Water",
        "Miracle Seed" => "Grass",
        "Magnet" => "Electric",
        "Never-Melt Ice" => "Ice",
        "Black Belt" => "Fighting",
        "Poison Barb" => "Poison",
        "Soft Sand" => "Ground",
        "Sharp Beak" => "Flying",
        "Twisted Spoon" => "Psychic",
        "Silver Powder" => "Bug",
        "Hard Stone" => "Rock",
        "Spell Tag" => "Ghost",
        "Dragon Fang" => "Dragon",
        "Black Glasses" => "Dark",
        "Metal Coat" => "Steel",
        "Fairy Feather" => "Fairy",
        "Silk Scarf" => "Normal",
        _ => return None,
    })
}

/// The berry that halves a super-effective hit of each type (Chilan halves any Normal hit).
fn resist_berry(item: &str) -> Option<&'static str> {
    Some(match item {
        "Occa Berry" => "Fire",
        "Passho Berry" => "Water",
        "Wacan Berry" => "Electric",
        "Rindo Berry" => "Grass",
        "Yache Berry" => "Ice",
        "Chople Berry" => "Fighting",
        "Kebia Berry" => "Poison",
        "Shuca Berry" => "Ground",
        "Coba Berry" => "Flying",
        "Payapa Berry" => "Psychic",
        "Tanga Berry" => "Bug",
        "Charti Berry" => "Rock",
        "Kasib Berry" => "Ghost",
        "Haban Berry" => "Dragon",
        "Colbur Berry" => "Dark",
        "Babiri Berry" => "Steel",
        "Roseli Berry" => "Fairy",
        "Chilan Berry" => "Normal",
        _ => return None,
    })
}

/// Abilities that make the defender immune to a whole type.
fn ability_immunity(ability: &str, move_type: &str) -> bool {
    matches!(
        (ability, move_type),
        ("Levitate" | "Earth Eater", "Ground")
            | ("Flash Fire" | "Well-Baked Body", "Fire")
            | ("Water Absorb" | "Storm Drain" | "Dry Skin", "Water")
            | ("Volt Absorb" | "Lightning Rod" | "Motor Drive", "Electric")
            | ("Sap Sipper", "Grass")
    )
}

/// How many times a multi-hit move lands: fixed counts as listed, 2–5 hit moves at 5 with
/// Skill Link, 4 with Loaded Dice, otherwise 3.
fn hit_count(mv: &MoveData, attacker: &Combatant) -> (u8, Option<&'static str>) {
    match mv.hits {
        None => (1, None),
        Some((lo, hi)) if lo == hi => (lo, None),
        Some((_, hi)) if attacker.ability_is(&["Skill Link"]) => {
            (hi, Some("Skill Link: every hit lands"))
        }
        Some(_) if attacker.item_is("Loaded Dice") => (4, Some("Loaded Dice: assumes 4 hits")),
        Some(_) => (3, Some("Assumes 3 hits")),
    }
}

fn weight_power(target_kg: f32) -> u32 {
    match target_kg {
        w if w < 10.0 => 20,
        w if w < 25.0 => 40,
        w if w < 50.0 => 60,
        w if w < 100.0 => 80,
        w if w < 200.0 => 100,
        _ => 120,
    }
}

fn weight_ratio_power(attacker_kg: f32, target_kg: f32) -> u32 {
    let ratio = attacker_kg / target_kg.max(0.1);
    match ratio {
        r if r >= 5.0 => 120,
        r if r >= 4.0 => 100,
        r if r >= 3.0 => 80,
        r if r >= 2.0 => 60,
        _ => 40,
    }
}

pub fn calculate(att: &Combatant, def: &Combatant, mv: &MoveData, field: &Field) -> Outcome {
    let mut notes: Vec<String> = Vec::new();
    let defender_hp = def.hp();
    let physical = mv.category == Category::Physical;
    let attacker_ability = att.ability.as_deref().unwrap_or("");
    let defender_ability = def.ability.as_deref().unwrap_or("");

    // Move type and base power.
    let mut move_type = mv.move_type.clone();
    let mut bp = u32::from(mv.base_power);
    let mut ate = false;
    if mv.name == "Weather Ball"
        && let Some(w) = field.weather
    {
        move_type = match w {
            Weather::Sun => "Fire",
            Weather::Rain => "Water",
            Weather::Sand => "Rock",
            Weather::Snow => "Ice",
        }
        .to_owned();
        bp = 100;
        notes.push(format!(
            "Weather Ball becomes {move_type}-type at 100 power"
        ));
    }
    if move_type == "Normal" {
        let new_type = match attacker_ability {
            "Aerilate" => Some("Flying"),
            "Pixilate" => Some("Fairy"),
            "Refrigerate" => Some("Ice"),
            "Galvanize" => Some("Electric"),
            _ => None,
        };
        if let Some(t) = new_type {
            move_type = t.to_owned();
            ate = true;
            notes.push(format!(
                "{attacker_ability} makes it {t}-type and 20% stronger"
            ));
        }
    }
    match mv.name.as_str() {
        "Low Kick" | "Grass Knot" => bp = weight_power(def.weight_kg),
        "Heavy Slam" | "Heat Crash" => bp = weight_ratio_power(att.weight_kg, def.weight_kg),
        "Acrobatics" if att.item.is_none() => bp *= 2,
        "Facade" if att.burned => bp *= 2,
        _ => {}
    }
    let mut spread_move = mv.spread;
    if mv.name == "Expanding Force" && field.terrain == Some(Terrain::Psychic) && att.grounded() {
        bp = 120;
        spread_move = true;
        notes.push("Expanding Force in Psychic Terrain: 120 power and hits both opponents".into());
    }
    let crit = field.crit || mv.always_crits;
    let spread = field.doubles && field.hits_both && spread_move;
    let (hits, hit_note) = hit_count(mv, att);
    if let Some(n) = hit_note {
        notes.push(n.to_owned());
    }

    // Type effectiveness, one defending type at a time.
    let scrappy = att.ability_is(&["Scrappy", "Mind's Eye"])
        && matches!(move_type.as_str(), "Normal" | "Fighting");
    let type_factors: Vec<f32> = def
        .types
        .iter()
        .filter(|t| !(scrappy && *t == "Ghost"))
        .map(|t| multiplier(&move_type, std::slice::from_ref(t), None))
        .collect();
    let mut effectiveness: f32 = type_factors.iter().product();
    if effectiveness > 0.0 && ability_immunity(defender_ability, &move_type) {
        effectiveness = 0.0;
        notes.push(format!("{defender_ability} makes {} immune", def.name));
    }
    if effectiveness > 0.0 && move_type == "Ground" && def.item_is("Air Balloon") {
        effectiveness = 0.0;
        notes.push("Air Balloon makes it immune to Ground moves".into());
    }
    let empty = |notes| Outcome {
        rolls: [0; 16],
        defender_hp,
        move_type: move_type.clone(),
        effectiveness: 0.0,
        hits,
        base_power: bp,
        crit,
        spread,
        notes,
    };
    if effectiveness == 0.0 || mv.category == Category::Status || bp == 0 {
        return empty(notes);
    }

    // Base power modifiers.
    let mut bp_mod = 4096;
    let mut add = |modifier: u32, note: Option<String>| {
        bp_mod = chain(bp_mod, modifier);
        if let Some(n) = note {
            notes.push(n);
        }
    };
    if att.ability_is(&["Technician"]) && bp <= 60 {
        add(6144, Some("Technician".into()));
    }
    if ate {
        add(4915, None);
    }
    if let Some(item) = att.item.as_deref()
        && type_item(item) == Some(move_type.as_str())
    {
        add(4915, Some(item.to_owned()));
    }
    if (physical && att.item_is("Muscle Band")) || (!physical && att.item_is("Wise Glasses")) {
        add(4505, att.item.clone());
    }
    let ability_bp = match attacker_ability {
        "Iron Fist" if mv.punch => Some(4915),
        "Strong Jaw" if mv.bite => Some(6144),
        "Sharpness" if mv.slicing => Some(6144),
        "Mega Launcher" if mv.pulse => Some(6144),
        "Tough Claws" if mv.contact => Some(5325),
        "Sheer Force" if mv.secondary => Some(5325),
        "Reckless" if mv.recoil => Some(4915),
        "Punk Rock" if mv.sound => Some(5325),
        "Sand Force"
            if field.weather == Some(Weather::Sand)
                && matches!(move_type.as_str(), "Rock" | "Ground" | "Steel") =>
        {
            Some(5325)
        }
        _ => None,
    };
    if let Some(m) = ability_bp {
        add(m, Some(attacker_ability.to_owned()));
    }
    // Knock Off can't remove a Mega Stone, so it gets no boost against a Mega.
    if mv.name == "Knock Off" && def.item.is_some() && !def.name.contains("-Mega") {
        add(6144, Some("Knock Off against a held item".into()));
    }
    if field.helping_hand {
        add(6144, Some("Helping Hand".into()));
    }
    if let Some(t) = field.terrain {
        if att.grounded() && t.boosted_type() == Some(move_type.as_str()) {
            add(5325, Some(t.label().to_owned()));
        }
        if t == Terrain::Misty && move_type == "Dragon" && def.grounded() {
            add(2048, Some("Misty Terrain halves Dragon damage".into()));
        }
        if t == Terrain::Grassy
            && matches!(mv.name.as_str(), "Earthquake" | "Bulldoze")
            && def.grounded()
        {
            add(2048, Some("Grassy Terrain halves Earthquake".into()));
        }
    }
    let bp_final = modify(bp, bp_mod).max(1);

    // Attacking stat.
    let (source, attack_index) = if mv.uses_target_attack {
        (def, 1)
    } else if mv.uses_defense {
        (att, 2)
    } else if physical {
        (att, 1)
    } else {
        (att, 3)
    };
    let mut stage = if mv.uses_target_attack { 0 } else { att.boost };
    if crit && stage < 0 {
        stage = 0;
    }
    let mut attack = apply_stage(source.stat(attack_index), stage);
    let mut atk_mod = 4096;
    let mut amod = |modifier: u32, note: &str| {
        atk_mod = chain(atk_mod, modifier);
        notes.push(note.to_owned());
    };
    if physical && att.item_is("Choice Band") {
        amod(6144, "Choice Band");
    }
    if !physical && att.item_is("Choice Specs") {
        amod(6144, "Choice Specs");
    }
    match attacker_ability {
        "Huge Power" | "Pure Power" if physical => amod(8192, attacker_ability),
        "Guts" if physical && att.burned => amod(6144, "Guts"),
        "Gorilla Tactics" if physical => amod(6144, "Gorilla Tactics"),
        "Hustle" if physical => amod(6144, "Hustle"),
        "Solar Power" if !physical && field.weather == Some(Weather::Sun) => {
            amod(6144, "Solar Power")
        }
        "Water Bubble" if move_type == "Water" => amod(8192, "Water Bubble"),
        "Transistor" if move_type == "Electric" => amod(5325, "Transistor"),
        "Dragon's Maw" if move_type == "Dragon" => amod(6144, "Dragon’s Maw"),
        "Steelworker" if move_type == "Steel" => amod(6144, "Steelworker"),
        "Rocky Payload" if move_type == "Rock" => amod(6144, "Rocky Payload"),
        _ => {}
    }
    match (defender_ability, move_type.as_str()) {
        ("Thick Fat", "Fire" | "Ice") => amod(2048, "Thick Fat"),
        ("Heatproof", "Fire") => amod(2048, "Heatproof"),
        ("Water Bubble", "Fire") => amod(2048, "Water Bubble"),
        ("Purifying Salt", "Ghost") => amod(2048, "Purifying Salt"),
        _ => {}
    }
    attack = modify(attack, atk_mod).max(1);

    // Defending stat.
    let defense_index = if physical || mv.targets_defense { 2 } else { 4 };
    let mut def_stage = def.boost;
    if crit && def_stage > 0 {
        def_stage = 0;
    }
    let mut defense = apply_stage(def.stat(defense_index), def_stage);
    let mut def_mod = 4096;
    if field.weather == Some(Weather::Sand) && def.has_type("Rock") && defense_index == 4 {
        def_mod = chain(def_mod, 6144);
        notes.push("Sand raises Rock types’ Special Defense".into());
    }
    if field.weather == Some(Weather::Snow) && def.has_type("Ice") && defense_index == 2 {
        def_mod = chain(def_mod, 6144);
        notes.push("Snow raises Ice types’ Defense".into());
    }
    if defense_index == 4 && def.item_is("Assault Vest") {
        def_mod = chain(def_mod, 6144);
        notes.push("Assault Vest".into());
    }
    if defense_index == 2 && def.ability_is(&["Fur Coat"]) {
        def_mod = chain(def_mod, 8192);
        notes.push("Fur Coat".into());
    }
    defense = modify(defense, def_mod).max(1);

    // Base damage at level 50: floor(2 × 50 / 5 + 2) = 22.
    let mut base =
        (22 * u64::from(bp_final) * u64::from(attack) / u64::from(defense)) as u32 / 50 + 2;

    if spread {
        base = modify(base, 3072);
        notes.push("Spread move hitting two targets: 75% damage".into());
    }
    match (field.weather, move_type.as_str()) {
        (Some(Weather::Sun), "Fire") | (Some(Weather::Rain), "Water") => {
            base = modify(base, 6144);
            notes.push(format!(
                "{} boosts it",
                field.weather.map_or("", Weather::label)
            ));
        }
        (Some(Weather::Sun), "Water") | (Some(Weather::Rain), "Fire") => {
            base = modify(base, 2048);
            notes.push(format!(
                "{} weakens it",
                field.weather.map_or("", Weather::label)
            ));
        }
        _ => {}
    }
    if crit {
        base = base * 3 / 2;
        notes.push("Critical hit".into());
    }

    // Final modifiers, combined.
    let mut final_mod = 4096;
    let mut fmod = |modifier: u32, note: String| {
        final_mod = chain(final_mod, modifier);
        notes.push(note);
    };
    if field.screen && !crit {
        let name = if physical || mv.targets_defense {
            "Reflect"
        } else {
            "Light Screen"
        };
        fmod(if field.doubles { 2732 } else { 2048 }, name.to_owned());
    }
    match defender_ability {
        "Multiscale" | "Shadow Shield" => {
            fmod(2048, format!("{defender_ability} (assumes full HP)"))
        }
        "Filter" | "Solid Rock" | "Prism Armor" if effectiveness > 1.0 => {
            fmod(3072, defender_ability.to_owned())
        }
        "Fluffy" if mv.contact && move_type != "Fire" => fmod(2048, "Fluffy".into()),
        "Fluffy" if move_type == "Fire" && !mv.contact => {
            fmod(8192, "Fluffy (weak to Fire)".into())
        }
        "Ice Scales" if !physical => fmod(2048, "Ice Scales".into()),
        "Punk Rock" if mv.sound => fmod(2048, "Punk Rock".into()),
        _ => {}
    }
    if att.item_is("Expert Belt") && effectiveness > 1.0 {
        fmod(4915, "Expert Belt".into());
    }
    if att.item_is("Life Orb") {
        fmod(5324, "Life Orb".into());
    }
    if att.ability_is(&["Tinted Lens"]) && effectiveness < 1.0 {
        fmod(8192, "Tinted Lens".into());
    }
    if att.ability_is(&["Sniper"]) && crit {
        fmod(6144, "Sniper".into());
    }
    if let Some(item) = def.item.as_deref()
        && let Some(t) = resist_berry(item)
        && t == move_type
        && (effectiveness > 1.0 || t == "Normal")
    {
        fmod(2048, item.to_owned());
    }

    let stab = att.has_type(&move_type);
    let stab_mod = if att.ability_is(&["Adaptability"]) {
        8192
    } else {
        6144
    };
    let burned = att.burned && physical && !att.ability_is(&["Guts"]) && mv.name != "Facade";
    if burned {
        notes.push("Burn halves physical damage".into());
    }

    let mut rolls = [0u32; 16];
    for (i, roll) in rolls.iter_mut().enumerate() {
        let mut d = base * (85 + i as u32) / 100;
        if stab {
            d = modify(d, stab_mod);
        }
        for f in &type_factors {
            d = if *f >= 2.0 {
                d * 2
            } else if *f < 1.0 {
                d / 2
            } else {
                d
            };
        }
        if burned {
            d = modify(d, 2048);
        }
        d = modify(d, final_mod).max(1);
        *roll = d * u32::from(hits);
    }

    Outcome {
        rolls,
        defender_hp,
        move_type,
        effectiveness,
        hits,
        base_power: bp,
        crit,
        spread,
        notes,
    }
}

/// "Guaranteed OHKO", "43.8% chance to 2HKO" and so on.
pub fn ko_chance(rolls: &[u32; 16], hp: u32) -> String {
    if rolls[15] == 0 {
        return "No damage".into();
    }
    for n in 1..=4u32 {
        if rolls[0] * n >= hp {
            return format!("Guaranteed {}", hko(n));
        }
        if rolls[15] * n >= hp {
            // Count every combination of n rolls that adds up to a KO.
            let mut sums: Vec<(u32, u64)> = vec![(0, 1)];
            for _ in 0..n {
                let mut next: std::collections::BTreeMap<u32, u64> = Default::default();
                for (s, c) in &sums {
                    for r in rolls {
                        *next.entry((s + r).min(hp)).or_default() += c;
                    }
                }
                sums = next.into_iter().collect();
            }
            let total = 16u64.pow(n);
            let kos: u64 = sums.iter().filter(|(s, _)| *s >= hp).map(|(_, c)| c).sum();
            return format!(
                "{:.1}% chance to {}",
                kos as f64 * 100.0 / total as f64,
                hko(n)
            );
        }
    }
    "5 or more hits to KO".into()
}

fn hko(n: u32) -> String {
    if n == 1 {
        "OHKO".into()
    } else {
        format!("{n}HKO")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(
        name: &str,
        types: &[&str],
        base: [u16; 6],
        nature: &str,
        points: [u16; 6],
    ) -> Combatant {
        Combatant {
            name: name.into(),
            types: types.iter().map(|t| (*t).to_owned()).collect(),
            base,
            weight_kg: 50.0,
            item: None,
            ability: None,
            nature: nature.into(),
            points,
            boost: 0,
            burned: false,
        }
    }

    fn mv(name: &str, bp: u16, t: &str, category: Category) -> MoveData {
        MoveData {
            name: name.into(),
            base_power: bp,
            move_type: t.into(),
            category,
            spread: false,
            contact: false,
            punch: false,
            bite: false,
            slicing: false,
            pulse: false,
            sound: false,
            hits: None,
            always_crits: false,
            secondary: false,
            recoil: false,
            uses_defense: false,
            targets_defense: false,
            uses_target_attack: false,
        }
    }

    #[test]
    fn rounding_matches_showdown() {
        // 1.5× of 55 is 82.5, which Showdown rounds down to 82; 1.5× of 57 is 85.5 -> 85.
        assert_eq!(modify(55, 6144), 82);
        assert_eq!(modify(57, 6144), 85);
        // 0.75× of 50 = 37.5 -> 37.
        assert_eq!(modify(50, 3072), 37);
        assert_eq!(chain(6144, 6144), 9216);
        assert_eq!(apply_stage(100, 1), 150);
        assert_eq!(apply_stage(100, -1), 66);
        assert_eq!(apply_stage(100, 6), 400);
    }

    #[test]
    fn neutral_hit_matches_the_formula_by_hand() {
        // Attack: (100 + 20 + 32) = 152 neutral. Defense: (100 + 20 + 0) = 120.
        // Base: floor(floor(22 * 80 * 152 / 120) / 50) + 2 = floor(2229 / 50) + 2 = 46.
        // Rolls run from floor(46 * 0.85) = 39 to 46, no STAB, neutral type.
        let a = mon("A", &["Normal"], [100; 6], "Hardy", [0, 32, 0, 0, 0, 0]);
        let d = mon("D", &["Normal"], [100; 6], "Hardy", [0; 6]);
        let o = calculate(
            &a,
            &d,
            &mv("Test", 80, "Water", Category::Physical),
            &Field::default(),
        );
        assert_eq!((o.min(), o.max()), (39, 46));
        assert_eq!(o.defender_hp, 175);
    }

    #[test]
    fn stab_type_and_spread_apply_in_order() {
        let a = mon("A", &["Fire"], [100; 6], "Hardy", [0, 0, 0, 32, 0, 0]);
        let d = mon("D", &["Grass"], [100; 6], "Hardy", [0; 6]);
        let mut heat_wave = mv("Heat Wave", 95, "Fire", Category::Special);
        heat_wave.spread = true;
        let single = calculate(&a, &d, &heat_wave, &Field::default());
        let both = calculate(
            &a,
            &d,
            &heat_wave,
            &Field {
                doubles: true,
                hits_both: true,
                ..Field::default()
            },
        );
        assert!(both.spread && !single.spread);
        assert!(both.max() < single.max());
        assert_eq!(single.effectiveness, 2.0);
        // Base 54 → max roll 54 → STAB 81 → ×2 = 162.
        assert_eq!(single.max(), 162);
    }

    #[test]
    fn immunities_and_status_moves_do_nothing() {
        let a = mon("A", &["Ground"], [100; 6], "Hardy", [0; 6]);
        let mut d = mon("D", &["Ghost"], [100; 6], "Hardy", [0; 6]);
        d.ability = Some("Levitate".into());
        let o = calculate(
            &a,
            &d,
            &mv("Earthquake", 100, "Ground", Category::Physical),
            &Field::default(),
        );
        assert_eq!(o.max(), 0);
        assert_eq!(ko_chance(&o.rolls, o.defender_hp), "No damage");
    }

    #[test]
    fn ko_chances() {
        let rolls: [u32; 16] = std::array::from_fn(|i| 85 + i as u32);
        assert_eq!(ko_chance(&rolls, 80), "Guaranteed OHKO");
        assert_eq!(ko_chance(&rolls, 99), "12.5% chance to OHKO");
        assert_eq!(ko_chance(&rolls, 170), "Guaranteed 2HKO");
        assert_eq!(ko_chance(&rolls, 255), "Guaranteed 3HKO");
        assert!(ko_chance(&rolls, 190).ends_with("chance to 2HKO"));
    }

    #[test]
    fn weight_moves_and_ate_abilities() {
        assert_eq!(weight_power(83.0), 80);
        assert_eq!(weight_ratio_power(200.0, 40.0), 120);
        let mut a = mon("A", &["Flying"], [100; 6], "Hardy", [0; 6]);
        a.ability = Some("Aerilate".into());
        let d = mon("D", &["Grass"], [100; 6], "Hardy", [0; 6]);
        let o = calculate(
            &a,
            &d,
            &mv("Hyper Voice", 90, "Normal", Category::Special),
            &Field::default(),
        );
        assert_eq!(o.move_type, "Flying");
        assert_eq!(o.effectiveness, 2.0);
    }
}
