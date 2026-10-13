//! Simple game plans for newer players, worked out from a team's moves, abilities and
//! items. Teams change every month, so nothing here is written by hand: each Pokémon's job
//! is recognised from its set, and the plan is assembled from those jobs.

use serde::Serialize;

use crate::dex::Dex;
use crate::team::{is_mega, nature_effect};
use crate::tournaments::ListMon;

/// What the plan needs to know about one Pokémon.
pub struct PlanMon {
    pub name: String,
    /// Base stats, HP/Atk/Def/SpA/SpD/Spe.
    pub stats: [u16; 6],
    pub item: Option<String>,
    /// The ability it battles with. For a Mega that's the Mega's ability.
    pub ability: Option<String>,
    pub nature: Option<String>,
    pub moves: Vec<String>,
    /// False for a Mega whose ability only starts once it Mega Evolves (Froslass-Mega's
    /// Snow Warning); true when the base form already has it (Tyranitar's Sand Stream).
    pub ability_on_entry: bool,
}

impl PlanMon {
    pub fn from_list(m: &ListMon, dex: &Dex) -> Self {
        let name = dex.battle_form(&m.id, m.item.as_deref());
        let entry = dex.entry(&name);
        let battle_ability = if is_mega(&name) {
            entry.and_then(|e| e.abilities.first().cloned())
        } else {
            None
        };
        let ability_on_entry = battle_ability.is_none() || battle_ability == m.ability;
        Self {
            stats: entry.map_or([0; 6], |e| e.stats),
            item: m.item.clone(),
            ability: battle_ability.or_else(|| m.ability.clone()),
            nature: m.nature.clone(),
            moves: m.moves.clone(),
            ability_on_entry,
            name,
        }
    }

    /// When its ability takes effect, for sentences like "sets rain {when}".
    fn ability_timing(&self) -> &'static str {
        if self.ability_on_entry {
            "as soon as it comes in"
        } else {
            "when it Mega Evolves"
        }
    }

    fn has_move(&self, names: &[&str]) -> Option<&str> {
        self.moves
            .iter()
            .find(|m| names.contains(&m.as_str()))
            .map(String::as_str)
    }

    fn ability_is(&self, names: &[&str]) -> bool {
        self.ability.as_deref().is_some_and(|a| names.contains(&a))
    }

    /// A nature that lowers Attack means a special set and one that lowers Special Attack
    /// means a physical set; otherwise go by whichever attacking stat is higher.
    fn physical(&self) -> bool {
        match self.nature.as_deref().and_then(nature_effect) {
            Some((_, 1)) => false,
            Some((_, 3)) => true,
            _ => self.stats[1] >= self.stats[3],
        }
    }

    fn speed(&self) -> u16 {
        self.stats[5]
    }
}

/// How a team's main effect gets set: by a move, which takes a turn, or by an ability
/// (weather, terrain), which starts by itself.
enum Setup {
    Move(&'static str),
    Ability(String),
}

/// One job a Pokémon does, explained for someone new to competitive play.
#[derive(Serialize)]
pub struct Role {
    pub label: &'static str,
    pub detail: String,
}

#[derive(Serialize)]
pub struct GamePlan {
    /// The team's main idea, e.g. `Rain` or `Trick Room`, if it has one.
    pub style: Option<&'static str>,
    pub steps: Vec<String>,
    /// Each Pokémon with its jobs, in team order.
    pub roles: Vec<(String, Vec<Role>)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Weather {
    Rain,
    Sun,
    Sand,
    Snow,
}

impl Weather {
    fn from_ability(a: &str) -> Option<Self> {
        Some(match a {
            "Drizzle" => Self::Rain,
            "Drought" | "Orichalcum Pulse" => Self::Sun,
            "Sand Stream" => Self::Sand,
            "Snow Warning" => Self::Snow,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Rain => "Rain",
            Self::Sun => "Sun",
            Self::Sand => "Sand",
            Self::Snow => "Snow",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Self::Rain => "rain",
            Self::Sun => "harsh sunlight",
            Self::Sand => "a sandstorm",
            Self::Snow => "snow",
        }
    }

    fn effect(self) -> &'static str {
        match self {
            Self::Rain => "makes Water-type moves 50% stronger and weakens Fire-type moves",
            Self::Sun => "makes Fire-type moves 50% stronger and weakens Water-type moves",
            Self::Sand => {
                "chips at non-Rock, Ground and Steel Pokémon each turn and raises Rock types' Special Defense"
            }
            Self::Snow => "raises Ice types' Defense and lets Blizzard always hit",
        }
    }

    /// Abilities that only work, or work best, in this weather.
    fn abusers(self) -> &'static [&'static str] {
        match self {
            Self::Rain => &["Swift Swim", "Rain Dish", "Dry Skin", "Hydration"],
            Self::Sun => &[
                "Chlorophyll",
                "Solar Power",
                "Protosynthesis",
                "Flower Gift",
            ],
            Self::Sand => &["Sand Rush", "Sand Force", "Sand Veil"],
            Self::Snow => &["Slush Rush", "Ice Body", "Snow Cloak"],
        }
    }

    /// Moves that get much better in this weather.
    fn moves(self) -> &'static [&'static str] {
        match self {
            Self::Rain => &["Electro Shot", "Weather Ball", "Thunder", "Hurricane"],
            Self::Sun => &["Solar Beam", "Solar Blade", "Weather Ball", "Growth"],
            Self::Sand => &["Weather Ball", "Shore Up"],
            Self::Snow => &["Blizzard", "Aurora Veil", "Weather Ball"],
        }
    }
}

fn terrain(ability: &str) -> Option<(&'static str, &'static str)> {
    Some(match ability {
        "Grassy Surge" => (
            "Grassy Terrain",
            "heals grounded Pokémon a little each turn, powers up Grass-type moves and makes Grassy Glide move first",
        ),
        "Psychic Surge" => (
            "Psychic Terrain",
            "blocks priority moves aimed at grounded Pokémon and powers up Psychic-type moves",
        ),
        "Electric Surge" | "Hadron Engine" => (
            "Electric Terrain",
            "stops grounded Pokémon from falling asleep and powers up Electric-type moves",
        ),
        "Misty Surge" => (
            "Misty Terrain",
            "protects grounded Pokémon from status conditions and halves Dragon-type damage",
        ),
        _ => return None,
    })
}

const SPREAD_ATTACKS: &[&str] = &[
    "Earthquake",
    "Rock Slide",
    "Heat Wave",
    "Hyper Voice",
    "Dazzling Gleam",
    "Surf",
    "Muddy Water",
    "Discharge",
    "Blizzard",
    "Eruption",
    "Water Spout",
    "Make It Rain",
    "Bleakwind Storm",
    "Sludge Wave",
    "Boomburst",
    "Bulldoze",
    "Lava Plume",
    "Petal Blizzard",
    "Breaking Swipe",
    "Glacial Lance",
    "Astral Barrage",
    "Precipice Blades",
    "Origin Pulse",
    "Matcha Gotcha",
    "Expanding Force",
];
const PRIORITY_ATTACKS: &[&str] = &[
    "Sucker Punch",
    "Extreme Speed",
    "Aqua Jet",
    "Grassy Glide",
    "Bullet Punch",
    "Shadow Sneak",
    "Ice Shard",
    "Quick Attack",
    "First Impression",
    "Mach Punch",
    "Jet Punch",
    "Accelerock",
    "Vacuum Wave",
];
const SETUP_MOVES: &[&str] = &[
    "Swords Dance",
    "Nasty Plot",
    "Calm Mind",
    "Dragon Dance",
    "Bulk Up",
    "Belly Drum",
    "Quiver Dance",
    "Shell Smash",
    "Coil",
    "Victory Dance",
    "Shift Gear",
    "Agility",
    "Tail Glow",
];
const SLEEP_MOVES: &[&str] = &["Spore", "Sleep Powder", "Yawn", "Hypnosis", "Dark Void"];
const PIVOT_MOVES: &[&str] = &["U-turn", "Volt Switch", "Flip Turn", "Teleport"];
const REDIRECT_MOVES: &[&str] = &["Follow Me", "Rage Powder"];
const SPEED_DROP_MOVES: &[&str] = &["Icy Wind", "Electroweb", "Bulldoze", "Thunder Wave"];

/// Most jobs listed for one Pokémon, so each card stays short.
const MAX_ROLES: usize = 3;

fn roles_for(m: &PlanMon, doubles: bool) -> Vec<Role> {
    let mut out: Vec<Role> = Vec::new();
    let mut add = |label: &'static str, detail: String| {
        if out.len() < MAX_ROLES {
            out.push(Role { label, detail });
        }
    };
    let ability = m.ability.as_deref().unwrap_or("");

    if let Some(w) = Weather::from_ability(ability) {
        add(
            "Weather",
            format!(
                "{ability} sets {} {}, which {}.",
                w.noun(),
                m.ability_timing(),
                w.effect()
            ),
        );
    }
    if let Some((name, effect)) = terrain(ability) {
        add(
            "Terrain",
            format!(
                "{ability} sets {name} {}, which {effect}.",
                m.ability_timing()
            ),
        );
    }
    if m.has_move(&["Trick Room"]).is_some() {
        add(
            "Speed control",
            "Trick Room makes slower Pokémon move first for five turns.".to_owned(),
        );
    }
    if m.has_move(&["Tailwind"]).is_some() {
        add(
            "Speed control",
            "Tailwind doubles your team’s Speed for four turns.".to_owned(),
        );
    }
    if let Some(mv) = m.has_move(SPEED_DROP_MOVES) {
        let detail = if mv == "Thunder Wave" {
            "Thunder Wave paralyzes a target, cutting its Speed in half.".to_owned()
        } else {
            format!("{mv} lowers the Speed of the Pokémon it hits, so your team can outspeed them.")
        };
        add("Speed control", detail);
    }
    if doubles && m.has_move(&["Fake Out"]).is_some() {
        add(
            "Fake Out",
            "On its first turn out, Fake Out makes one opponent flinch so it can’t move. Use it to give a partner a free turn.".to_owned(),
        );
    }
    if m.ability_is(&["Intimidate"]) {
        add(
            "Intimidate",
            "Lowers the opposing Pokémon’s Attack when it comes in, weakening physical attackers."
                .to_owned(),
        );
    }
    if doubles && let Some(mv) = m.has_move(REDIRECT_MOVES) {
        add(
            "Redirection",
            format!(
                "{mv} pulls single-target attacks onto itself so its partner is safe for a turn."
            ),
        );
    }
    if doubles && m.has_move(&["Wide Guard"]).is_some() {
        add(
            "Protection",
            "Wide Guard blocks moves that hit both of your Pokémon, like Rock Slide and Heat Wave."
                .to_owned(),
        );
    }
    if m.has_move(&["Parting Shot"]).is_some() {
        add(
            "Pivot",
            "Parting Shot lowers a target’s Attack and Special Attack, then switches out so a teammate comes in safely.".to_owned(),
        );
    } else if let Some(mv) = m.has_move(PIVOT_MOVES) {
        add(
            "Pivot",
            format!("{mv} attacks and then switches out, bringing a teammate in safely."),
        );
    }
    if let Some(mv) = m.has_move(SLEEP_MOVES) {
        add(
            "Status",
            format!("{mv} puts a target to sleep so it can’t attack."),
        );
    } else if m.has_move(&["Will-O-Wisp"]).is_some() {
        add(
            "Status",
            "Will-O-Wisp burns a target, halving the damage of its physical attacks.".to_owned(),
        );
    }
    if doubles && m.has_move(&["Helping Hand"]).is_some() {
        add(
            "Support",
            "Helping Hand makes its partner’s attack 50% stronger that turn.".to_owned(),
        );
    }
    if let Some(mv) = m.has_move(SETUP_MOVES) {
        add(
            "Setup",
            format!(
                "{mv} boosts its stats; once it has used it, it can sweep through weakened opponents."
            ),
        );
    }
    if m.ability_is(&["Unburden"]) && m.item.as_deref().is_some_and(|i| i.ends_with("Seed")) {
        add(
            "Speed boost",
            "Its terrain seed is used up when the terrain is set, and Unburden then doubles its Speed.".to_owned(),
        );
    }
    let weather_user = [Weather::Rain, Weather::Sun, Weather::Sand, Weather::Snow]
        .into_iter()
        .find(|w| m.ability_is(w.abusers()));
    if let Some(w) = weather_user {
        add(
            "Weather user",
            format!(
                "{ability} makes it much stronger or faster in {}.",
                w.noun()
            ),
        );
    }

    // Everyone else is here to deal damage; say how (`add` skips it once the card is full).
    {
        let kind = if m.physical() { "Physical" } else { "Special" };
        let spread = m
            .has_move(SPREAD_ATTACKS)
            .filter(|_| doubles)
            .map(|mv| format!(" {mv} hits both opponents at once."))
            .unwrap_or_default();
        let priority = m
            .has_move(PRIORITY_ATTACKS)
            .map(|mv| {
                format!(
                    " {mv} moves before most other attacks, good for finishing off weakened foes."
                )
            })
            .unwrap_or_default();
        let scarf = if m.item.as_deref() == Some("Choice Scarf") {
            " Choice Scarf makes it faster but locks it into one move until it switches out."
        } else {
            ""
        };
        add(
            if m.physical() {
                "Physical attacker"
            } else {
                "Special attacker"
            },
            format!("{kind} damage dealer.{spread}{priority}{scarf}")
                .trim()
                .to_owned(),
        );
    }
    out
}

fn list_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// Team styles the tournament page filters by, like Pikalytics' archetype chips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Archetype {
    Sun,
    Rain,
    Sand,
    Snow,
    TrickRoom,
    Tailwind,
    PerishTrap,
}

impl Archetype {
    pub const ALL: [Archetype; 7] = [
        Self::Sun,
        Self::Rain,
        Self::Sand,
        Self::Snow,
        Self::TrickRoom,
        Self::Tailwind,
        Self::PerishTrap,
    ];

    /// The value used in links, e.g. `trick-room`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Sun => "sun",
            Self::Rain => "rain",
            Self::Sand => "sand",
            Self::Snow => "snow",
            Self::TrickRoom => "trick-room",
            Self::Tailwind => "tailwind",
            Self::PerishTrap => "perish-trap",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sun => "Sun",
            Self::Rain => "Rain",
            Self::Sand => "Sand",
            Self::Snow => "Snow",
            Self::TrickRoom => "Trick Room",
            Self::Tailwind => "Tailwind",
            Self::PerishTrap => "Perish Trap",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.key() == key)
    }
}

/// Which archetypes a team fits. Weather counts when someone sets it, by ability (as it
/// battles, so Mega abilities count) or by move; Trick Room and Tailwind when someone runs
/// the move; Perish Trap when the team has Perish Song and a way to keep foes in.
pub fn archetypes(team: &[PlanMon]) -> Vec<Archetype> {
    let any = |abilities: &[&str], moves: &[&str]| {
        team.iter()
            .any(|m| m.ability_is(abilities) || m.has_move(moves).is_some())
    };
    let checks = [
        (
            Archetype::Sun,
            any(&["Drought", "Orichalcum Pulse"], &["Sunny Day"]),
        ),
        (Archetype::Rain, any(&["Drizzle"], &["Rain Dance"])),
        (Archetype::Sand, any(&["Sand Stream"], &["Sandstorm"])),
        (
            Archetype::Snow,
            any(&["Snow Warning"], &["Snowscape", "Chilly Reception"]),
        ),
        (Archetype::TrickRoom, any(&[], &["Trick Room"])),
        (Archetype::Tailwind, any(&[], &["Tailwind"])),
        (
            Archetype::PerishTrap,
            any(&[], &["Perish Song"])
                && any(
                    &["Shadow Tag", "Arena Trap"],
                    &[
                        "Mean Look",
                        "Block",
                        "Spirit Shackle",
                        "Anchor Shot",
                        "Jaw Lock",
                        "Fairy Lock",
                    ],
                ),
        ),
    ];
    checks
        .into_iter()
        .filter(|(_, fits)| *fits)
        .map(|(a, _)| a)
        .collect()
}

/// Builds the plan. `doubles` is true for VGC (bring 4) and false for Battle Stadium
/// Singles (bring 3).
pub fn plan(team: &[PlanMon], doubles: bool) -> GamePlan {
    let roles: Vec<(String, Vec<Role>)> = team
        .iter()
        .map(|m| (m.name.clone(), roles_for(m, doubles)))
        .collect();
    let mut steps = Vec::new();
    let mut style = None;

    let weather = team.iter().find_map(|m| {
        m.ability
            .as_deref()
            .and_then(Weather::from_ability)
            .map(|w| (m, w))
    });
    let trick_room = team.iter().find(|m| m.has_move(&["Trick Room"]).is_some());
    let tailwind = team.iter().find(|m| m.has_move(&["Tailwind"]).is_some());
    let terrain_setter = team.iter().find_map(|m| {
        m.ability
            .as_deref()
            .and_then(terrain)
            .map(|(name, effect)| (m, name, effect))
    });
    let fake_out = team
        .iter()
        .find(|m| doubles && m.has_move(&["Fake Out"]).is_some());
    let intimidate = team.iter().find(|m| m.ability_is(&["Intimidate"]));

    // 1. The team's main idea.
    if let Some((setter, w)) = weather {
        style = Some(w.name());
        let users: Vec<&str> = team
            .iter()
            .filter(|m| m.name != setter.name)
            .filter(|m| m.ability_is(w.abusers()) || m.has_move(w.moves()).is_some())
            .map(|m| m.name.as_str())
            .collect();
        let mut s = format!(
            "This is a {} team. {} sets {} {}, which {}.",
            w.name().to_lowercase(),
            setter.name,
            w.noun(),
            setter.ability_timing(),
            w.effect()
        );
        if !users.is_empty() {
            s.push_str(&format!(
                " {} {} built to take advantage of it.",
                list_names(&users),
                if users.len() == 1 { "is" } else { "are" }
            ));
        }
        steps.push(s);
    } else if let Some(setter) = trick_room {
        style = Some("Trick Room");
        let slow: Vec<&str> = team
            .iter()
            .filter(|m| m.name != setter.name && m.speed() <= 70)
            .map(|m| m.name.as_str())
            .collect();
        let mut s = format!(
            "This is a Trick Room team. {} sets up Trick Room, which makes slower Pokémon move first for five turns.",
            setter.name
        );
        if !slow.is_empty() {
            s.push_str(&format!(
                " That lets slow, hard-hitting Pokémon like {} attack before the opponent.",
                list_names(&slow)
            ));
        }
        steps.push(s);
    } else if let Some(setter) = tailwind {
        style = Some("Tailwind");
        steps.push(format!(
            "This team wins by moving first. {} sets Tailwind, which doubles your team’s Speed for four turns, so your attackers hit before the opponent can.",
            setter.name
        ));
    } else if let Some((setter, name, effect)) = terrain_setter {
        style = Some(name);
        steps.push(format!(
            "{} sets {} {}, which {}.",
            setter.name,
            name,
            setter.ability_timing(),
            effect
        ));
    } else {
        steps.push(
            "This is a balanced team: no weather or speed tricks, just strong Pokémon that cover each other’s weaknesses. Aim each attack at the opponent it hurts most."
                .to_owned(),
        );
    }

    // 2. A simple lead. Weather and terrain start on their own; Tailwind and Trick Room
    // need a turn, which is what Fake Out and Intimidate protect.
    let setter: Option<(&PlanMon, Setup)> = weather
        .map(|(m, w)| (m, Setup::Ability(w.noun().to_owned())))
        .or(trick_room.map(|m| (m, Setup::Move("Trick Room"))))
        .or(tailwind.map(|m| (m, Setup::Move("Tailwind"))))
        .or(terrain_setter.map(|(m, name, _)| (m, Setup::Ability(name.to_owned()))));
    if doubles {
        let partner = fake_out
            .filter(|f| setter.as_ref().is_none_or(|(s, _)| s.name != f.name))
            .or_else(|| {
                intimidate.filter(|i| setter.as_ref().is_none_or(|(s, _)| s.name != i.name))
            });
        match (&setter, partner) {
            (Some((s, setup)), Some(p)) => {
                let partner_job = if fake_out.is_some_and(|f| f.name == p.name) {
                    format!(
                        "{}’s Fake Out stops one opponent from moving on turn one",
                        p.name
                    )
                } else {
                    format!(
                        "{}’s Intimidate weakens the opponent’s physical attackers",
                        p.name
                    )
                };
                let why = match setup {
                    Setup::Move(mv) => format!("{partner_job}, so {} can use {mv} safely", s.name),
                    Setup::Ability(effect) => format!(
                        "{} sets {effect} {}, and {partner_job}, giving your side a free turn to attack",
                        s.name,
                        s.ability_timing()
                    ),
                };
                steps.push(format!(
                    "A simple lead is {} and {}: {}.",
                    s.name, p.name, why
                ));
            }
            (Some((s, setup)), None) => {
                let what = match setup {
                    Setup::Move(mv) => format!("it can use {mv} on turn one"),
                    Setup::Ability(effect) => format!("its {effect} is up from turn one"),
                };
                steps.push(format!(
                    "Lead with {} so {what}, alongside whichever attacker suits the matchup.",
                    s.name
                ));
            }
            (None, Some(p)) => {
                steps.push(format!(
                    "{} makes a good lead: {} on turn one buys its partner a free attack.",
                    p.name,
                    if fake_out.is_some_and(|f| f.name == p.name) {
                        "Fake Out"
                    } else {
                        "Intimidate"
                    }
                ));
            }
            (None, None) => {}
        }
    } else if let Some((s, _)) = setter {
        steps.push(format!(
            "Lead with {} so its effect is up from the start.",
            s.name
        ));
    }

    // 3. Mega Evolution.
    let megas: Vec<&str> = team
        .iter()
        .filter(|m| is_mega(&m.name))
        .map(|m| m.name.as_str())
        .collect();
    match megas.as_slice() {
        [] => {}
        [one] => steps.push(format!(
            "Mega Evolve {one} the first time it’s in battle. Mega Evolution happens before anyone moves and doesn’t use up your turn."
        )),
        many => steps.push(format!(
            "The team carries {} Megas ({}), but only one Pokémon can Mega Evolve per battle. Bring the one that matches up best against the opponent.",
            many.len(),
            list_names(many)
        )),
    }

    // 4. Who does the damage.
    let mut hitters: Vec<&PlanMon> = team
        .iter()
        .filter(|m| {
            m.has_move(&["Fake Out", "Follow Me", "Rage Powder", "Trick Room"])
                .is_none()
        })
        .collect();
    hitters.sort_by_key(|m| std::cmp::Reverse(m.stats[1].max(m.stats[3])));
    let hitter_names: Vec<&str> = hitters.iter().take(2).map(|m| m.name.as_str()).collect();
    if !hitter_names.is_empty() {
        steps.push(format!(
            "{} {} most of the damage. Keep {} healthy, and use Protect when the opponent targets {}.",
            list_names(&hitter_names),
            if hitter_names.len() == 1 { "does" } else { "do" },
            if hitter_names.len() == 1 { "it" } else { "them" },
            if hitter_names.len() == 1 { "it" } else { "them" },
        ));
    }

    // 5. Team preview.
    steps.push(if doubles {
        "In VGC you see the opponent’s team first and bring 4 of your 6, so leave out whoever matches up worst.".to_owned()
    } else {
        "In Battle Stadium Singles you see the opponent’s team first and bring 3 of your 6.".to_owned()
    });

    GamePlan {
        style,
        steps,
        roles,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_archetypes() {
        assert_eq!(
            archetypes(&rain_team()),
            vec![Archetype::Rain, Archetype::Tailwind]
        );
        let perish = vec![
            mon(
                "Gengar-Mega",
                "Shadow Tag",
                "Gengarite",
                &["Perish Song", "Protect"],
                [60; 6],
            ),
            mon("Torkoal", "Drought", "Charcoal", &["Eruption"], [70; 6]),
        ];
        assert_eq!(
            archetypes(&perish),
            vec![Archetype::Sun, Archetype::PerishTrap]
        );
        // Perish Song alone, with nothing to trap, isn't Perish Trap.
        let song_only = vec![mon(
            "Politoed",
            "Drizzle",
            "Leftovers",
            &["Perish Song"],
            [90; 6],
        )];
        assert_eq!(archetypes(&song_only), vec![Archetype::Rain]);
        for a in Archetype::ALL {
            assert_eq!(Archetype::parse(a.key()), Some(a));
        }
    }

    fn mon(name: &str, ability: &str, item: &str, moves: &[&str], stats: [u16; 6]) -> PlanMon {
        PlanMon {
            name: name.into(),
            stats,
            item: Some(item.into()),
            ability: Some(ability.into()),
            nature: None,
            moves: moves.iter().map(|m| (*m).to_owned()).collect(),
            ability_on_entry: true,
        }
    }

    fn rain_team() -> Vec<PlanMon> {
        vec![
            mon(
                "Pelipper",
                "Drizzle",
                "Focus Sash",
                &["Hurricane", "Tailwind", "Wide Guard", "Protect"],
                [60, 50, 100, 95, 70, 65],
            ),
            mon(
                "Archaludon",
                "Stamina",
                "Power Herb",
                &["Electro Shot", "Draco Meteor", "Flash Cannon", "Protect"],
                [90, 105, 130, 125, 65, 85],
            ),
            mon(
                "Incineroar",
                "Intimidate",
                "Sitrus Berry",
                &["Fake Out", "Parting Shot", "Flare Blitz", "Throat Chop"],
                [95, 115, 90, 80, 90, 60],
            ),
            mon(
                "Basculegion",
                "Swift Swim",
                "Choice Scarf",
                &["Wave Crash", "Last Respects", "Aqua Jet", "Flip Turn"],
                [120, 112, 65, 80, 75, 78],
            ),
        ]
    }

    #[test]
    fn recognises_a_rain_team_and_its_lead() {
        let p = plan(&rain_team(), true);
        assert_eq!(p.style, Some("Rain"));
        assert!(p.steps[0].starts_with("This is a rain team. Pelipper sets rain"));
        assert!(
            p.steps[0].contains("Archaludon and Basculegion are built to take advantage of it")
        );
        assert!(
            p.steps[1]
                .starts_with("A simple lead is Pelipper and Incineroar: Pelipper sets rain as soon as it comes in, and Incineroar’s Fake Out")
        );
        assert!(p.steps.last().unwrap().contains("bring 4 of your 6"));
    }

    #[test]
    fn explains_each_pokemons_job() {
        let p = plan(&rain_team(), true);
        let labels = |i: usize| p.roles[i].1.iter().map(|r| r.label).collect::<Vec<_>>();
        assert_eq!(labels(0), ["Weather", "Speed control", "Protection"]);
        assert_eq!(labels(2), ["Fake Out", "Intimidate", "Pivot"]);
        assert!(labels(3).contains(&"Weather user"));
    }

    #[test]
    fn singles_skips_doubles_only_advice() {
        let p = plan(&rain_team(), false);
        assert!(!p.roles[2].1.iter().any(|r| r.label == "Fake Out"));
        assert!(p.steps.iter().any(|s| s.starts_with("Lead with Pelipper")));
        assert!(p.steps.last().unwrap().contains("bring 3 of your 6"));
    }

    #[test]
    fn mega_abilities_start_on_mega_evolution() {
        let mut froslass = mon(
            "Froslass-Mega",
            "Snow Warning",
            "Froslassite",
            &["Blizzard"],
            [70, 80, 70, 140, 70, 120],
        );
        froslass.ability_on_entry = false;
        let p = plan(&[froslass], true);
        assert!(p.steps[0].contains("Froslass-Mega sets snow when it Mega Evolves"));
    }

    #[test]
    fn nature_decides_physical_or_special() {
        // Salamence-Mega has higher Attack, but a Timid set is special.
        let mut m = mon(
            "Salamence-Mega",
            "Aerilate",
            "Salamencite",
            &["Hyper Voice"],
            [95, 145, 130, 120, 90, 120],
        );
        assert!(m.physical());
        m.nature = Some("Timid".into());
        assert!(!m.physical());
        m.nature = Some("Jolly".into());
        assert!(m.physical());
    }

    #[test]
    fn move_setters_get_a_safe_turn() {
        let team = vec![
            mon(
                "Farigiraf",
                "Armor Tail",
                "Sitrus Berry",
                &["Trick Room", "Psychic"],
                [120, 90, 70, 110, 70, 60],
            ),
            mon(
                "Kangaskhan",
                "Scrappy",
                "Silk Scarf",
                &["Fake Out", "Double-Edge"],
                [105, 95, 80, 40, 80, 90],
            ),
        ];
        let p = plan(&team, true);
        assert!(p.steps[1].ends_with("so Farigiraf can use Trick Room safely."));
    }

    #[test]
    fn two_megas_are_called_out() {
        let team = vec![
            mon(
                "Salamence-Mega",
                "Aerilate",
                "Salamencite",
                &["Hyper Voice"],
                [95, 145, 130, 120, 90, 120],
            ),
            mon(
                "Tyranitar-Mega",
                "Sand Stream",
                "Tyranitarite",
                &["Rock Slide"],
                [100, 164, 150, 95, 120, 71],
            ),
        ];
        let p = plan(&team, true);
        assert!(
            p.steps
                .iter()
                .any(|s| s.contains("only one Pokémon can Mega Evolve"))
        );
        assert_eq!(list_names(&["A", "B", "C"]), "A, B, and C");
    }
}
