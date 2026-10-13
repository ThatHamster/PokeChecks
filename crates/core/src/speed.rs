//! Speed tiers for a live match: your team against the opponent's, sorted by who most
//! likely moves first, with stages, Choice Scarf, weather abilities, Unburden, Tailwind,
//! paralysis and Trick Room applied the way the game does.

use serde::{Deserialize, Serialize};

use crate::calc::Weather;
use crate::dex::Dex;
use crate::smogon::Meta;
use crate::team::{MAX_POINTS_PER_STAT, Set, level50_stat};

/// Weather that doubles a Speed ability.
pub fn speed_weather(ability: &str) -> Option<Weather> {
    match ability {
        "Swift Swim" => Some(Weather::Rain),
        "Chlorophyll" => Some(Weather::Sun),
        "Sand Rush" => Some(Weather::Sand),
        "Slush Rush" => Some(Weather::Snow),
        _ => None,
    }
}

/// Speed with stages, Scarf, a doubling ability, Tailwind and paralysis, applied in the
/// game's order.
#[allow(clippy::too_many_arguments)]
pub fn final_speed(
    base: u16,
    nature: &str,
    points: u16,
    stage: i8,
    scarf: bool,
    doubled: bool,
    tailwind: bool,
    paralyzed: bool,
) -> u32 {
    let mut s = u32::from(level50_stat(base, points, 5, nature));
    let st = u32::from(stage.unsigned_abs().min(6));
    s = if stage >= 0 {
        s * (2 + st) / 2
    } else {
        s * 2 / (2 + st)
    };
    if scarf {
        s = s * 3 / 2;
    }
    if doubled {
        s *= 2;
    }
    if tailwind {
        s *= 2;
    }
    if paralyzed {
        s /= 2;
    }
    s
}

/// One opposing Pokémon and the changes marked on it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Foe {
    pub name: String,
    pub stage: i8,
    pub paralyzed: bool,
    /// Assume it holds a Choice Scarf.
    pub scarf: bool,
    /// Unburden has triggered.
    pub unburden: bool,
}

/// Changes marked on one of your own Pokémon, by team slot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mine {
    pub stage: i8,
    pub paralyzed: bool,
    pub unburden: bool,
}

/// Everything about the match that affects Speed.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MatchState {
    pub foes: Vec<Foe>,
    pub mine: Vec<Mine>,
    pub trick_room: bool,
    pub my_tailwind: bool,
    pub foe_tailwind: bool,
    /// −1 Speed on that whole side, as after Icy Wind or Electroweb.
    pub my_drop: bool,
    pub foe_drop: bool,
    pub weather: Option<Weather>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Mine,
    Foe,
}

/// One Pokémon in the turn order.
#[derive(Debug, Serialize)]
pub struct Runner {
    pub side: Side,
    pub name: String,
    /// Speed used for sorting: yours exactly, theirs at the most common spread.
    pub speed: u32,
    /// The opponent's slowest and fastest possible Speed with the same effects.
    pub range: Option<(u32, u32)>,
    /// The effects applied, e.g. `+1`, `Scarf`, `Swift Swim`, `Tailwind`.
    pub effects: Vec<String>,
    /// A Pokémon on the other side has exactly the same Speed.
    pub tie: bool,
    /// For an opponent: your Pokémon whose Speed falls inside its possible range, so it
    /// could move before or after them depending on its spread.
    pub close: Vec<String>,
}

/// Who moves first: your Pokémon on their sets (from the team builder) and the opponent's
/// at their most common spreads. Fastest first, or slowest first under Trick Room; equal
/// speeds list yours first and are marked as ties.
pub fn turn_order(state: &MatchState, sets: &[Set], meta: &Meta, dex: &Dex) -> Vec<Runner> {
    let mut out = Vec::new();
    let weather_doubles = |ability: Option<&str>| {
        ability
            .and_then(speed_weather)
            .is_some_and(|w| state.weather == Some(w))
    };
    let effects = |stage: i8, scarf: bool, doubled: Option<&str>, tailwind: bool, para: bool| {
        let mut e = Vec::new();
        if stage != 0 {
            e.push(format!("{stage:+}"));
        }
        if scarf {
            e.push("Scarf".into());
        }
        if let Some(a) = doubled {
            e.push(a.to_owned());
        }
        if tailwind {
            e.push("Tailwind".into());
        }
        if para {
            e.push("Paralyzed".into());
        }
        e
    };

    let default_mods = Mine::default();
    for (i, set) in sets.iter().enumerate() {
        let mods = state.mine.get(i).unwrap_or(&default_mods);
        let base = dex.entry(&set.mon.name).map_or(0, |e| e.stats[5]);
        let nature = set.nature.clone().unwrap_or_else(|| "Hardy".into());
        let points = set.points.map_or(0, |p| p[5]);
        let stage = (mods.stage - i8::from(state.my_drop)).clamp(-6, 6);
        let scarf = set.item.as_deref() == Some("Choice Scarf");
        let ability = set.ability.as_deref();
        let doubled = weather_doubles(ability) || (mods.unburden && ability == Some("Unburden"));
        out.push(Runner {
            side: Side::Mine,
            name: set.mon.name.clone(),
            speed: final_speed(
                base,
                &nature,
                points,
                stage,
                scarf,
                doubled,
                state.my_tailwind,
                mods.paralyzed,
            ),
            range: None,
            effects: effects(
                stage,
                scarf,
                ability.filter(|_| doubled),
                state.my_tailwind,
                mods.paralyzed,
            ),
            tie: false,
            close: Vec::new(),
        });
    }

    for foe in &state.foes {
        let Some(mon) = meta.mons.iter().find(|m| m.name == foe.name) else {
            continue;
        };
        let base = dex.entry(&mon.name).map_or(0, |e| e.stats[5]);
        let (nature, points) = mon
            .top_spread()
            .map_or(("Hardy".to_owned(), 0), |s| (s.nature.clone(), s.points[5]));
        let ability = mon.abilities.first().map(|a| a.name.as_str());
        let stage = (foe.stage - i8::from(state.foe_drop)).clamp(-6, 6);
        let doubled = weather_doubles(ability) || (foe.unburden && ability == Some("Unburden"));
        let calc = |nature: &str, points: u16| {
            final_speed(
                base,
                nature,
                points,
                stage,
                foe.scarf,
                doubled,
                state.foe_tailwind,
                foe.paralyzed,
            )
        };
        out.push(Runner {
            side: Side::Foe,
            name: mon.name.clone(),
            speed: calc(&nature, points),
            range: Some((calc("Brave", 0), calc("Timid", MAX_POINTS_PER_STAT))),
            effects: effects(
                stage,
                foe.scarf,
                ability.filter(|_| doubled),
                state.foe_tailwind,
                foe.paralyzed,
            ),
            tie: false,
            close: Vec::new(),
        });
    }

    out.sort_by(|a, b| {
        let order = if state.trick_room {
            a.speed.cmp(&b.speed)
        } else {
            b.speed.cmp(&a.speed)
        };
        order.then((a.side == Side::Foe).cmp(&(b.side == Side::Foe)))
    });

    let speeds: Vec<(Side, u32, String)> = out
        .iter()
        .map(|r| (r.side, r.speed, r.name.clone()))
        .collect();
    for r in &mut out {
        r.tie = speeds
            .iter()
            .any(|(side, speed, _)| *side != r.side && *speed == r.speed);
        if let Some((lo, hi)) = r.range {
            r.close = speeds
                .iter()
                .filter(|(side, s, _)| *side == Side::Mine && *s >= lo && *s <= hi && *s != r.speed)
                .map(|(_, _, n)| n.clone())
                .collect();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn final_speed_applies_effects_in_order() {
        // Jolly 32: (100 + 20 + 32) × 1.1 = 167; Scarf 250; Tailwind 500; paralysis 250.
        assert_eq!(
            final_speed(100, "Jolly", 32, 0, true, false, true, true),
            250
        );
        // −1 stage: 167 × 2 / 3 = 111.
        assert_eq!(
            final_speed(100, "Jolly", 32, -1, false, false, false, false),
            111
        );
        assert_eq!(speed_weather("Swift Swim"), Some(Weather::Rain));
    }
}
