//! Speed tiers: where one Pokémon's Speed sits against the rest of the ladder.

use maud::{Markup, html};
use serde::Deserialize;

use crate::dex::Dex;
use crate::smogon::{Catalog, FormatInfo, LADDER_SIZE, Meta, Mon};
use crate::team::{MAX_POINTS_PER_STAT, NATURES, level50_stat, nature_label};
use crate::view::{Page, Theme, ViewState, shell, site_footer, site_header, type_chips};

/// Abilities that double Speed under the right conditions.
const SPEED_ABILITIES: [&str; 6] = [
    "Swift Swim",
    "Chlorophyll",
    "Sand Rush",
    "Slush Rush",
    "Surge Surfer",
    "Unburden",
];
/// Choice Scarf speeds are listed when at least this share of sets carries one.
const SCARF_SHOWN_AT: f64 = 10.0;

#[derive(Deserialize, Default)]
pub struct SpeedParams {
    mon: Option<String>,
    prev: Option<String>,
    nature: Option<String>,
    sp: Option<String>,
    stage: Option<String>,
    scarf: Option<String>,
    tailwind: Option<String>,
    para: Option<String>,
    boost_ability: Option<String>,
    tr: Option<String>,
    opp: Option<String>,
}

/// A change applied to every opposing Pokémon.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Opponents {
    Normal,
    Tailwind,
    /// −1 Speed, as after Icy Wind or Electroweb.
    Slowed,
}

impl Opponents {
    const ALL: [Opponents; 3] = [Opponents::Normal, Opponents::Tailwind, Opponents::Slowed];

    fn key(self) -> &'static str {
        match self {
            Self::Normal => "",
            Self::Tailwind => "tailwind",
            Self::Slowed => "slowed",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Normal => "No change",
            Self::Tailwind => "Opponents have Tailwind",
            Self::Slowed => "Opponents at −1 Speed (Icy Wind, Electroweb)",
        }
    }

    fn apply(self, speed: u32) -> u32 {
        match self {
            Self::Normal => speed,
            Self::Tailwind => speed * 2,
            Self::Slowed => speed * 2 / 3,
        }
    }
}

pub struct SpeedState {
    pub name: String,
    pub nature: String,
    pub points: u16,
    pub stage: i8,
    pub scarf: bool,
    pub tailwind: bool,
    pub paralyzed: bool,
    pub ability_boost: bool,
    pub trick_room: bool,
    pub opponents: Opponents,
}

impl SpeedState {
    pub fn from_params(p: &SpeedParams, meta: &Meta) -> Option<Self> {
        let mon = p
            .mon
            .as_deref()
            .and_then(|n| meta.mons.iter().find(|m| m.name == n))
            .or(meta.mons.first())?;
        let spread = mon.top_spread();
        let fresh = p.prev.as_deref() != Some(mon.name.as_str());
        let flag = |v: &Option<String>| !fresh && v.is_some();
        Some(Self {
            name: mon.name.clone(),
            nature: p
                .nature
                .clone()
                .filter(|n| !fresh && NATURES.contains(&n.as_str()))
                .or_else(|| spread.map(|s| s.nature.clone()))
                .unwrap_or_else(|| "Hardy".into()),
            points: p
                .sp
                .as_deref()
                .filter(|_| !fresh)
                .and_then(|v| v.trim().parse::<u16>().ok())
                .map(|v| v.min(MAX_POINTS_PER_STAT))
                .or_else(|| spread.map(|s| s.points[5]))
                .unwrap_or(0),
            stage: p
                .stage
                .as_deref()
                .filter(|_| !fresh)
                .and_then(|v| v.parse::<i8>().ok())
                .map_or(0, |s| s.clamp(-6, 6)),
            // A new Pokémon starts with a Scarf if that's its most common item.
            scarf: if fresh {
                mon.items.first().is_some_and(|i| i.name == "Choice Scarf")
            } else {
                p.scarf.is_some()
            },
            tailwind: flag(&p.tailwind),
            paralyzed: flag(&p.para),
            ability_boost: flag(&p.boost_ability),
            trick_room: p.tr.is_some(),
            opponents: Opponents::ALL
                .into_iter()
                .find(|o| p.opp.as_deref() == Some(o.key()) && !o.key().is_empty())
                .unwrap_or(Opponents::Normal),
        })
    }

    /// Speed with every chosen modifier, the way the game rounds it.
    fn effective(&self, base: u16) -> u32 {
        let mut s = u32::from(level50_stat(base, self.points, 5, &self.nature));
        let stage = u32::from(self.stage.unsigned_abs());
        s = if self.stage >= 0 {
            s * (2 + stage) / 2
        } else {
            s * 2 / (2 + stage)
        };
        if self.scarf {
            s = s * 3 / 2;
        }
        if self.ability_boost {
            s *= 2;
        }
        if self.tailwind {
            s *= 2;
        }
        if self.paralyzed {
            s /= 2;
        }
        s
    }

    /// Tiers for another Pokémon on its own most common set, keeping Trick Room and the
    /// opponents' modifier.
    fn href_for(&self, vs: &ViewState, mon: &str) -> String {
        let mut q: Vec<(&'static str, String)> = vec![
            ("fmt", vs.fmt.clone()),
            ("cut", vs.cut.to_string()),
            ("mon", mon.to_owned()),
        ];
        if self.trick_room {
            q.push(("tr", "1".into()));
        }
        if self.opponents != Opponents::Normal {
            q.push(("opp", self.opponents.key().into()));
        }
        format!(
            "{}?{}",
            Page::Speed.path(),
            serde_urlencoded::to_string(q).unwrap_or_default()
        )
    }
}

/// How the visitor's Pokémon compares with one opponent.
#[derive(PartialEq, Eq)]
enum Verdict {
    AlwaysFirst,
    UsuallyFirst,
    Tie,
    Second,
}

impl Verdict {
    fn label(&self) -> &'static str {
        match self {
            Self::AlwaysFirst => "You always move first",
            Self::UsuallyFirst => "You move first vs. the common spread",
            Self::Tie => "Speed tie",
            Self::Second => "They move first",
        }
    }

    fn class(&self) -> &'static str {
        match self {
            Self::AlwaysFirst => "v-first",
            Self::UsuallyFirst => "v-usually",
            Self::Tie => "v-tie",
            Self::Second => "v-second",
        }
    }
}

/// Normally the faster Pokémon moves first, so "always" means beating their maximum
/// Speed. Under Trick Room the slower one moves first, so "always" means being slower than
/// their minimum Speed.
fn verdict(you: u32, common: u32, max: u32, min: u32, trick_room: bool) -> Verdict {
    let (beats_all, beats_common) = if trick_room {
        (you < min, you < common)
    } else {
        (you > max, you > common)
    };
    if beats_all {
        Verdict::AlwaysFirst
    } else if beats_common {
        Verdict::UsuallyFirst
    } else if you == common {
        Verdict::Tie
    } else {
        Verdict::Second
    }
}

struct Row<'a> {
    mon: &'a Mon,
    common: u32,
    nature: String,
    points: u16,
    max: u32,
    /// No Speed points and a Speed-lowering nature, for Trick Room.
    min: u32,
    scarf: Option<(f64, u32)>,
}

pub fn speed_page(
    catalog: &Catalog,
    format: &FormatInfo,
    meta: &Meta,
    dex: &Dex,
    vs: &ViewState,
    theme: Theme,
    state: &SpeedState,
) -> Markup {
    let your_mon = meta.mons.iter().find(|m| m.name == state.name);
    let your_base = dex.entry(&state.name).map_or(0, |e| e.stats[5]);
    let you = state.effective(your_base);

    let mut rows: Vec<Row> = meta
        .mons
        .iter()
        .take(LADDER_SIZE)
        .map(|m| {
            let base = dex.entry(&m.name).map_or(0, |e| e.stats[5]);
            let (nature, points) = m
                .top_spread()
                .map_or(("Hardy".to_owned(), 0), |s| (s.nature.clone(), s.points[5]));
            let common = state
                .opponents
                .apply(u32::from(level50_stat(base, points, 5, &nature)));
            let max = state.opponents.apply(u32::from(level50_stat(
                base,
                MAX_POINTS_PER_STAT,
                5,
                "Timid",
            )));
            let min = state
                .opponents
                .apply(u32::from(level50_stat(base, 0, 5, "Brave")));
            let scarf = m
                .items
                .iter()
                .find(|i| i.name == "Choice Scarf" && i.pct >= SCARF_SHOWN_AT)
                .map(|i| (i.pct, common * 3 / 2));
            Row {
                mon: m,
                common,
                nature,
                points,
                max,
                min,
                scarf,
            }
        })
        .collect();
    rows.sort_by(|a, b| b.common.cmp(&a.common).then(a.mon.rank.cmp(&b.mon.rank)));

    let verdicts: Vec<Verdict> = rows
        .iter()
        .map(|r| verdict(you, r.common, r.max, r.min, state.trick_room))
        .collect();
    let first = verdicts
        .iter()
        .filter(|v| matches!(v, Verdict::AlwaysFirst | Verdict::UsuallyFirst))
        .count();
    let ties = verdicts.iter().filter(|v| **v == Verdict::Tie).count();
    let second = verdicts.len() - first - ties;
    let your_slot = rows
        .iter()
        .position(|r| r.common < you)
        .unwrap_or(rows.len());

    let can_boost = your_mon.is_some_and(|m| {
        m.abilities
            .iter()
            .any(|a| SPEED_ABILITIES.contains(&a.name.as_str()))
    });

    let dek = html! {
        "See where a Pokémon’s Speed sits against the top " (LADDER_SIZE) " in "
        strong { (format.kind.label()) " Regulation " (format.regulation) }
        ". Everyone else is shown at their most common nature and Speed points, and at the fastest they could be."
    };

    shell(
        "Speed tiers",
        theme,
        html! {
            (site_header(catalog, format, meta, vs, theme, dek))
            div.bench {
                section.pane aria-labelledby="speed-h" {
                    div.pane-head { h2 #speed-h { "Your Pokémon" } }
                    form.section.set-form method="get" action=(Page::Speed.path()) {
                        input type="hidden" name="fmt" value=(vs.fmt);
                        input type="hidden" name="cut" value=(vs.cut);
                        input type="hidden" name="prev" value=(state.name);
                        div.field {
                            label for="mon" { "Pokémon" }
                            select #mon name="mon" {
                                @for m in &meta.mons {
                                    option value=(m.name) selected[m.name == state.name] { (m.name) (format!(" · {:.1}%", m.usage)) }
                                }
                            }
                        }
                        div.field {
                            label for="nature" { "Nature" }
                            select #nature name="nature" {
                                @for n in NATURES {
                                    option value=(n) selected[state.nature == n] { (nature_label(n)) }
                                }
                            }
                        }
                        div.points-grid {
                            div {
                                label for="sp" { "Speed points" }
                                input #sp type="number" name="sp" min="0" max=(MAX_POINTS_PER_STAT) value=(state.points) inputmode="numeric";
                            }
                            div {
                                label for="stage" { "Speed stage" }
                                select #stage name="stage" {
                                    @for s in -6i8..=6 {
                                        option value=(s) selected[state.stage == s] { @if s > 0 { "+" } (s) }
                                    }
                                }
                            }
                        }
                        label.check { input type="checkbox" name="scarf" value="1" checked[state.scarf]; " Choice Scarf (×1.5)" }
                        label.check { input type="checkbox" name="tailwind" value="1" checked[state.tailwind]; " Tailwind on your side (×2)" }
                        label.check { input type="checkbox" name="para" value="1" checked[state.paralyzed]; " Paralyzed (×0.5)" }
                        @if can_boost {
                            label.check { input type="checkbox" name="boost_ability" value="1" checked[state.ability_boost]; " Speed-doubling ability active (Swift Swim, Chlorophyll, Unburden…)" }
                        }
                        div.field {
                            label for="opp" { "Opponents" }
                            select #opp name="opp" {
                                @for o in Opponents::ALL {
                                    option value=(o.key()) selected[state.opponents == o] { (o.label()) }
                                }
                            }
                        }
                        label.check { input type="checkbox" name="tr" value="1" checked[state.trick_room]; " Trick Room (slower Pokémon move first)" }
                        button.go type="submit" { "Compare" }
                    }
                }
                section.pane.scout aria-labelledby="tiers-h" {
                    div.pane-head {
                        h2 #tiers-h { "Speed tiers" }
                        @if state.trick_room { span.badge { "Trick Room" } }
                    }
                    div.section {
                        div.speed-summary {
                            div {
                                span.label { (state.name) }
                                strong.num { (you) }
                                span.muted.small { " Speed · " (state.nature) ", " (state.points) " points" }
                            }
                            p {
                                "Against the common spreads it moves first against "
                                strong { (first) } ", ties " strong { (ties) } ", and moves after "
                                strong { (second) } " of the top " (rows.len()) "."
                                @if state.trick_room { " Under Trick Room the slower Pokémon moves first." }
                            }
                        }
                    }
                    div.table-scroll {
                        table.speed-table {
                            thead {
                                tr {
                                    th { span.th-label { "Pokémon" } }
                                    th.r { span.th-label { "Common" } }
                                    th.r.hide-sm { span.th-label { "Max" } }
                                    th { span.th-label { "Against you" } }
                                }
                            }
                            tbody {
                                @for (i, (r, v)) in rows.iter().zip(&verdicts).enumerate() {
                                    @if i == your_slot { (you_row(state, you)) }
                                    tr {
                                        td {
                                            div.mon-cell {
                                                a.strong href=(state.href_for(vs, &r.mon.name)) { (r.mon.name) }
                                                (type_chips(dex.entry(&r.mon.name)))
                                            }
                                        }
                                        td.r {
                                            span.num.strong { (r.common) }
                                            span.muted.small.block { (r.nature) ", " (r.points) " pts" }
                                            @if let Some((pct, s)) = r.scarf {
                                                span.muted.small.block title=(format!("{pct:.0}% of sets carry Choice Scarf")) { "Scarf: " (s) }
                                            }
                                        }
                                        td.r.num.hide-sm { (r.max) }
                                        td { span class={ "verdict " (v.class()) } { (v.label()) } }
                                    }
                                }
                                @if your_slot == rows.len() { (you_row(state, you)) }
                            }
                        }
                    }
                    p.hint { "“Max” is 32 Speed points with a Speed-raising nature. Click a Pokémon to see its own tiers." }
                }
            }
            (site_footer(catalog))
        },
    )
}

fn you_row(state: &SpeedState, you: u32) -> Markup {
    html! {
        tr.you-row {
            td { strong { "You: " (state.name) } }
            td.r.num.strong { (you) }
            td.hide-sm {}
            td.small.muted { "Your Speed with the options on the left" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdicts_flip_under_trick_room() {
        assert!(matches!(
            verdict(150, 120, 140, 90, false),
            Verdict::AlwaysFirst
        ));
        assert!(matches!(
            verdict(130, 120, 140, 90, false),
            Verdict::UsuallyFirst
        ));
        assert!(matches!(verdict(120, 120, 140, 90, false), Verdict::Tie));
        assert!(matches!(verdict(100, 120, 140, 90, false), Verdict::Second));
        assert!(matches!(
            verdict(50, 120, 140, 90, true),
            Verdict::AlwaysFirst
        ));
        assert!(matches!(
            verdict(100, 120, 140, 90, true),
            Verdict::UsuallyFirst
        ));
        assert!(matches!(verdict(150, 120, 140, 90, true), Verdict::Second));
    }

    #[test]
    fn modifiers_stack_like_the_game() {
        let s = SpeedState {
            name: "X".into(),
            nature: "Jolly".into(),
            points: 32,
            stage: 1,
            scarf: true,
            tailwind: false,
            paralyzed: false,
            ability_boost: false,
            trick_room: false,
            opponents: Opponents::Normal,
        };
        // (100 + 20 + 32) × 1.1 = 167 → +1 = 250 → Scarf = 375.
        assert_eq!(s.effective(100), 375);
        assert_eq!(Opponents::Slowed.apply(150), 100);
    }
}
