//! Speed tiers. The default view is for live matches: your team against the opponent's,
//! sorted by who most likely moves first. A second view compares one Pokémon with the
//! top of the ladder.

use maud::{Markup, html};
use serde::Deserialize;

use crate::calc::Weather;
use crate::dex::Dex;
use crate::saved::SavedTeam;
use crate::smogon::{Catalog, FormatInfo, LADDER_SIZE, Meta, Mon};
use crate::team::{
    self, Custom, MAX_POINTS_PER_STAT, Member, NATURES, TEAM_SIZE, level50_stat, nature_label,
};
use crate::view::{Page, Theme, ViewState, mon_icon, shell, site_footer, site_header, type_chips};
use pokechecks_core::speed::{Foe, MatchState, Mine, Side, turn_order};

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
    /// `meta` for the one-Pokémon comparison; anything else is the match view.
    mode: Option<String>,
    // Match view: opponents, an opponent to add, per-slot changes to your team, and field.
    foes: Option<String>,
    add: Option<String>,
    mods: Option<String>,
    my_tw: Option<String>,
    foe_tw: Option<String>,
    my_drop: Option<String>,
    foe_drop: Option<String>,
    weather: Option<String>,
}

impl SpeedParams {
    pub fn is_meta_mode(&self) -> bool {
        self.mode.as_deref() == Some("meta")
    }
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
            ("mode", "meta".to_owned()),
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
            (mode_tabs(vs, true))
            div.bench {
                section.pane aria-labelledby="speed-h" {
                    div.pane-head { h2 #speed-h { "Your Pokémon" } }
                    form.section.set-form method="get" action=(Page::Speed.path()) {
                        input type="hidden" name="fmt" value=(vs.fmt);
                        input type="hidden" name="cut" value=(vs.cut);
                        input type="hidden" name="prev" value=(state.name);
                        input type="hidden" name="mode" value="meta";
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
                                span.label.mon-name { (mon_icon(dex, &state.name)) (state.name) }
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
                                    @if i == your_slot { (you_row(state, you, dex)) }
                                    tr {
                                        td {
                                            div.mon-cell {
                                                a.strong.mon-name href=(state.href_for(vs, &r.mon.name)) { (mon_icon(dex, &r.mon.name)) (r.mon.name) }
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
                                @if your_slot == rows.len() { (you_row(state, you, dex)) }
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

fn you_row(state: &SpeedState, you: u32, dex: &Dex) -> Markup {
    html! {
        tr.you-row {
            td { span.mon-name { (mon_icon(dex, &state.name)) strong { "You: " (state.name) } } }
            td.r.num.strong { (you) }
            td.hide-sm {}
            td.small.muted { "Your Speed with the options on the left" }
        }
    }
}

// ---------- match view ----------

/// Tabs between the match view and the one-Pokémon comparison.
fn mode_tabs(vs: &ViewState, meta_mode: bool) -> Markup {
    let base = |mode: Option<&str>| {
        let mut q: Vec<(&str, String)> = vec![("fmt", vs.fmt.clone()), ("cut", vs.cut.to_string())];
        if !vs.team.is_empty() {
            q.push(("team", vs.team_param()));
        }
        if let Some(m) = mode {
            q.push(("mode", m.to_owned()));
        }
        format!(
            "{}?{}",
            Page::Speed.path(),
            serde_urlencoded::to_string(q).unwrap_or_default()
        )
    };
    html! {
        nav.subtabs aria-label="Speed tools" {
            a href=(base(None)) aria-current=[(!meta_mode).then_some("page")] { "Your match" }
            a href=(base(Some("meta"))) aria-current=[meta_mode.then_some("page")] { "One Pokémon vs. the top " (LADDER_SIZE) }
        }
    }
}

fn flag(s: Option<&str>) -> bool {
    s == Some("1")
}

fn bit(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

/// Finds a Pokémon from typed text: an exact name, then a name that starts with it, then
/// one that contains it. Ladder order breaks ties, so "Inc" finds Incineroar.
fn find_typed<'a>(meta: &'a Meta, typed: &str) -> Option<&'a Mon> {
    let t = typed.trim().to_lowercase();
    if t.is_empty() {
        return None;
    }
    meta.mons
        .iter()
        .find(|m| m.name.to_lowercase() == t)
        .or_else(|| {
            meta.mons
                .iter()
                .find(|m| m.name.to_lowercase().starts_with(&t))
        })
        .or_else(|| {
            meta.mons
                .iter()
                .find(|m| m.name.to_lowercase().contains(&t))
        })
}

/// Reads the match from the link. `pokechecks-core` owns `MatchState`, so these are free
/// functions rather than methods.
pub fn match_from_params(p: &SpeedParams, meta: &Meta, team_len: usize) -> MatchState {
    let mut foes: Vec<Foe> = Vec::new();
    for token in p.foes.as_deref().unwrap_or("").split(',') {
        let f: Vec<&str> = token.split('~').collect();
        let Some(mon) = meta.mons.iter().find(|m| m.name == f[0]) else {
            continue;
        };
        if foes.len() < TEAM_SIZE && !foes.iter().any(|x| x.name == mon.name) {
            foes.push(Foe {
                name: mon.name.clone(),
                stage: f
                    .get(1)
                    .and_then(|s| s.parse().ok())
                    .map_or(0, |s: i8| s.clamp(-6, 6)),
                paralyzed: flag(f.get(2).copied()),
                scarf: flag(f.get(3).copied()),
                unburden: flag(f.get(4).copied()),
            });
        }
    }
    if let Some(mon) = p.add.as_deref().and_then(|t| find_typed(meta, t))
        && foes.len() < TEAM_SIZE
        && !foes.iter().any(|x| x.name == mon.name)
    {
        foes.push(Foe {
            name: mon.name.clone(),
            stage: 0,
            paralyzed: false,
            // Assume a Scarf when that's what this Pokémon usually holds.
            scarf: mon.items.first().is_some_and(|i| i.name == "Choice Scarf"),
            unburden: false,
        });
    }

    let mut mine: Vec<Mine> = p
        .mods
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|token| {
            let f: Vec<&str> = token.split('~').collect();
            Mine {
                stage: f
                    .first()
                    .and_then(|s| s.parse().ok())
                    .map_or(0, |s: i8| s.clamp(-6, 6)),
                paralyzed: flag(f.get(1).copied()),
                unburden: flag(f.get(2).copied()),
            }
        })
        .collect();
    mine.resize(team_len, Mine::default());

    MatchState {
        foes,
        mine,
        trick_room: flag(p.tr.as_deref()),
        my_tailwind: flag(p.my_tw.as_deref()),
        foe_tailwind: flag(p.foe_tw.as_deref()),
        my_drop: flag(p.my_drop.as_deref()),
        foe_drop: flag(p.foe_drop.as_deref()),
        weather: p.weather.as_deref().and_then(Weather::parse),
    }
}

fn match_query(state: &MatchState, vs: &ViewState) -> Vec<(&'static str, String)> {
    let mut q: Vec<(&'static str, String)> =
        vec![("fmt", vs.fmt.clone()), ("cut", vs.cut.to_string())];
    if !vs.team.is_empty() {
        q.push(("team", vs.team_param()));
        q.push((
            "mods",
            state
                .mine
                .iter()
                .map(|m| format!("{}~{}~{}", m.stage, bit(m.paralyzed), bit(m.unburden)))
                .collect::<Vec<_>>()
                .join(","),
        ));
    }
    if !state.foes.is_empty() {
        q.push((
            "foes",
            state
                .foes
                .iter()
                .map(|f| {
                    format!(
                        "{}~{}~{}~{}~{}",
                        f.name,
                        f.stage,
                        bit(f.paralyzed),
                        bit(f.scarf),
                        bit(f.unburden)
                    )
                })
                .collect::<Vec<_>>()
                .join(","),
        ));
    }
    for (key, on) in [
        ("tr", state.trick_room),
        ("my_tw", state.my_tailwind),
        ("foe_tw", state.foe_tailwind),
        ("my_drop", state.my_drop),
        ("foe_drop", state.foe_drop),
    ] {
        if on {
            q.push((key, "1".into()));
        }
    }
    if let Some(w) = state.weather {
        q.push(("weather", w.key().into()));
    }
    q
}

/// This page with one change applied: each toggle on the page is a plain link.
fn match_href(state: &MatchState, vs: &ViewState, change: impl FnOnce(&mut MatchState)) -> String {
    let mut next = state.clone();
    change(&mut next);
    // Land on the turn order so the result of each tap is in view, mid-match.
    format!(
        "{}?{}#order",
        Page::Speed.path(),
        serde_urlencoded::to_string(match_query(&next, vs)).unwrap_or_default()
    )
}

/// Your team's sets from the team builder, for the turn order.
fn my_sets<'a>(vs: &ViewState, meta: &'a Meta) -> Vec<team::Set<'a>> {
    let chosen: Vec<(&Mon, Option<&Custom>)> = vs
        .team
        .iter()
        .filter_map(|m| {
            meta.mons
                .iter()
                .find(|x| x.name == m.name)
                .map(|x| (x, m.custom.as_ref()))
        })
        .collect();
    team::resolve_sets(&chosen)
}

#[allow(clippy::too_many_arguments)]
pub fn match_page(
    catalog: &Catalog,
    format: &FormatInfo,
    meta: &Meta,
    dex: &Dex,
    vs: &ViewState,
    theme: Theme,
    state: &MatchState,
    saved: &[SavedTeam],
) -> Markup {
    let order = turn_order(state, &my_sets(vs, meta), meta, dex);
    let dek = html! {
        "Check turn order during a match in "
        strong { (format.kind.label()) " Regulation " (format.regulation) }
        ". Your team comes from the team builder; add the opponent’s Pokémon at team preview, then tap effects as they happen."
    };

    shell(
        "Speed tiers",
        theme,
        html! {
            (site_header(catalog, format, meta, vs, theme, dek))
            (mode_tabs(vs, false))
            div.bench {
                section.pane.match-setup aria-labelledby="setup-h" {
                    div.pane-head { h2 #setup-h { "Match setup" } }
                    (field_controls(state, vs))
                    (my_team_controls(state, vs, meta, dex, saved, format))
                    (foe_controls(state, vs, meta, dex))
                }
                section #order .pane.scout aria-labelledby="order-h" {
                    div.pane-head {
                        h2 #order-h { "Turn order" }
                        @if state.trick_room { span.badge { "Trick Room: slowest first" } }
                    }
                    @if order.is_empty() {
                        div.empty {
                            strong { "Nothing to compare yet" }
                            "Pick your team and add the opponent’s Pokémon to see who moves first."
                        }
                    } @else {
                        ol.turn-order {
                            @for (i, r) in order.iter().enumerate() {
                                li class={ "runner " (if r.side == Side::Mine { "mine" } else { "foe" }) } {
                                    span.place.num { (i + 1) }
                                    div.runner-main {
                                        div.runner-name {
                                            span class={ "side-chip " (if r.side == Side::Mine { "mine" } else { "foe" }) } { (if r.side == Side::Mine { "You" } else { "Opp" }) }
                                            (mon_icon(dex, &r.name))
                                            strong { (r.name) }
                                            (type_chips(dex.entry(&r.name)))
                                        }
                                        div.runner-notes {
                                            @for e in &r.effects { span.tag.plain-tag { (e) } }
                                            @if r.tie { span.tag.tie-tag { "Speed tie" } }
                                            @if let Some((lo, hi)) = r.range {
                                                span.muted.small { "Could be " (lo) "–" (hi) }
                                            }
                                            @if !r.close.is_empty() {
                                                span.small.warn-text { "Could move before or after " (r.close.join(", ")) ", depending on its spread" }
                                            }
                                        }
                                    }
                                    span.runner-speed.num { (r.speed) }
                                }
                            }
                        }
                        p.hint {
                            "Opponents are shown at their most common nature and Speed points; “Could be” is their slowest to fastest possible Speed with the same effects. "
                            "Priority moves like Fake Out and Sucker Punch go before all of this."
                        }
                    }
                }
            }
            (site_footer(catalog))
        },
    )
}

fn toggle(label: &str, on: bool, href: String) -> Markup {
    html! {
        a.toggle href=(href) aria-pressed=(on) { (label) }
    }
}

fn field_controls(state: &MatchState, vs: &ViewState) -> Markup {
    html! {
        div.section {
            h3.small-h { "Field" }
            div.toggles {
                (toggle("Trick Room", state.trick_room, match_href(state, vs, |s| s.trick_room = !s.trick_room)))
            }
            div.toggle-grid {
                span.label { "Tailwind" }
                div.toggles {
                    (toggle("Your side", state.my_tailwind, match_href(state, vs, |s| s.my_tailwind = !s.my_tailwind)))
                    (toggle("Their side", state.foe_tailwind, match_href(state, vs, |s| s.foe_tailwind = !s.foe_tailwind)))
                }
                span.label { "−1 Speed (Icy Wind)" }
                div.toggles {
                    (toggle("Your side", state.my_drop, match_href(state, vs, |s| s.my_drop = !s.my_drop)))
                    (toggle("Their side", state.foe_drop, match_href(state, vs, |s| s.foe_drop = !s.foe_drop)))
                }
                span.label { "Weather" }
                div.toggles {
                    (toggle("None", state.weather.is_none(), match_href(state, vs, |s| s.weather = None)))
                    @for w in Weather::ALL {
                        (toggle(w.label(), state.weather == Some(w), match_href(state, vs, |s| s.weather = Some(w))))
                    }
                }
            }
        }
    }
}

/// Stage −/+ and status toggles for one Pokémon.
fn stage_controls(stage: i8, minus: String, plus: String) -> Markup {
    html! {
        span.stage {
            a.step href=(minus) aria-label="Lower Speed stage" { "−" }
            span.num { (format!("{stage:+}")) }
            a.step href=(plus) aria-label="Raise Speed stage" { "+" }
        }
    }
}

fn my_team_controls(
    state: &MatchState,
    vs: &ViewState,
    meta: &Meta,
    dex: &Dex,
    saved: &[SavedTeam],
    format: &FormatInfo,
) -> Markup {
    let usable: Vec<&SavedTeam> = saved
        .iter()
        .filter(|t| t.fmt == format.kind.key())
        .collect();
    let chosen: Vec<(&Mon, Option<&Custom>)> = vs
        .team
        .iter()
        .filter_map(|m| {
            meta.mons
                .iter()
                .find(|x| x.name == m.name)
                .map(|x| (x, m.custom.as_ref()))
        })
        .collect();
    let sets = team::resolve_sets(&chosen);
    let load = |t: &SavedTeam| {
        let team: Vec<Member> = t.team.split(',').map(Member::decode).collect();
        let next = MatchState {
            mine: vec![Mine::default(); team.len()],
            ..state.clone()
        };
        format!(
            "{}?{}#order",
            Page::Speed.path(),
            serde_urlencoded::to_string(match_query(&next, &vs.with_team(team)))
                .unwrap_or_default()
        )
    };
    html! {
        div.section {
            h3.small-h { "Your team" }
            @if sets.is_empty() {
                p.note {
                    "No team yet. "
                    a href=(vs.on_page(Page::Builder).href()) { "Build one in the team builder" }
                    @if !usable.is_empty() { " or pick a saved team:" } @else { " and save it to use it here." }
                }
            }
            @if !usable.is_empty() {
                div.toggles.saved-picks {
                    @for t in &usable {
                        a.toggle href=(load(t)) aria-pressed=(vs.team_param() == t.team) { (t.name) }
                    }
                }
            }
            @if !sets.is_empty() {
                ul.side-list {
                    @for (i, (set, mods)) in sets.iter().zip(&state.mine).enumerate() {
                        li {
                            div.side-name {
                                span.mon-name { (mon_icon(dex, &set.mon.name)) strong { (set.mon.name) } }
                                span.muted.small.block {
                                    (set.nature.as_deref().unwrap_or("Hardy")) ", " (set.points.map_or(0, |p| p[5])) " Spe pts"
                                    @if set.item.as_deref() == Some("Choice Scarf") { ", Scarf" }
                                }
                            }
                            div.side-actions {
                                (stage_controls(mods.stage,
                                    match_href(state, vs, |s| s.mine[i].stage = (s.mine[i].stage - 1).max(-6)),
                                    match_href(state, vs, |s| s.mine[i].stage = (s.mine[i].stage + 1).min(6))))
                                (toggle("Par", mods.paralyzed, match_href(state, vs, |s| s.mine[i].paralyzed = !s.mine[i].paralyzed)))
                                @if set.ability.as_deref() == Some("Unburden") {
                                    (toggle("Unburden", mods.unburden, match_href(state, vs, |s| s.mine[i].unburden = !s.mine[i].unburden)))
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn foe_controls(state: &MatchState, vs: &ViewState, meta: &Meta, dex: &Dex) -> Markup {
    html! {
        div.section {
            h3.small-h { "Opponent’s team" }
            @if state.foes.len() < TEAM_SIZE {
                form.add-foe method="get" action={ (Page::Speed.path()) "#order" } {
                    @for (k, v) in match_query(state, vs) {
                        input type="hidden" name=(k) value=(v);
                    }
                    label.sr-only for="add" { "Add an opposing Pokémon" }
                    div.preset-row {
                        input #add type="search" name="add" list="mon-names" placeholder="Type a Pokémon, press Enter" autocomplete="off";
                        button.go type="submit" { "Add" }
                    }
                    datalist #mon-names {
                        @for m in &meta.mons { option value=(m.name) {} }
                    }
                }
            }
            @if state.foes.is_empty() {
                p.note { "Add up to six at team preview. Partial names work: “kinga” finds Kingambit." }
            } @else {
                ul.side-list {
                    @for (i, foe) in state.foes.iter().enumerate() {
                        @let ability = meta.mons.iter().find(|m| m.name == foe.name).and_then(|m| m.abilities.first()).map(|a| a.name.as_str());
                        li {
                            div.side-name {
                                span.mon-name { (mon_icon(dex, &foe.name)) strong { (foe.name) } }
                                @if let Some(a) = ability { span.muted.small.block { (a) } }
                            }
                            div.side-actions {
                                (stage_controls(foe.stage,
                                    match_href(state, vs, |s| s.foes[i].stage = (s.foes[i].stage - 1).max(-6)),
                                    match_href(state, vs, |s| s.foes[i].stage = (s.foes[i].stage + 1).min(6))))
                                (toggle("Scarf", foe.scarf, match_href(state, vs, |s| s.foes[i].scarf = !s.foes[i].scarf)))
                                (toggle("Par", foe.paralyzed, match_href(state, vs, |s| s.foes[i].paralyzed = !s.foes[i].paralyzed)))
                                @if ability == Some("Unburden") {
                                    (toggle("Unburden", foe.unburden, match_href(state, vs, |s| s.foes[i].unburden = !s.foes[i].unburden)))
                                }
                                a.remove href=(match_href(state, vs, |s| { s.foes.remove(i); })) aria-label={ "Remove " (foe.name) } { "×" }
                            }
                        }
                    }
                }
                a.clear href=(match_href(state, vs, |s| s.foes.clear())) { "Clear opponents" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smogon::Share;

    fn meta(names: &[(&str, &str)]) -> Meta {
        Meta {
            battles: 0,
            cutoff: 1630,
            mons: names
                .iter()
                .enumerate()
                .map(|(i, (name, item))| Mon {
                    name: (*name).into(),
                    rank: i + 1,
                    usage: 10.0,
                    prev: None,
                    abilities: vec![],
                    items: vec![Share {
                        name: (*item).into(),
                        pct: 60.0,
                    }],
                    moves: vec![],
                    tera: vec![],
                    teammates: vec![],
                    spreads: vec![],
                    natures: vec![],
                    pair_rates: Default::default(),
                })
                .collect(),
        }
    }

    fn params(foes: &str, add: Option<&str>) -> SpeedParams {
        SpeedParams {
            foes: Some(foes.into()),
            add: add.map(Into::into),
            tr: Some("1".into()),
            weather: Some("rain".into()),
            ..SpeedParams::default()
        }
    }

    #[test]
    fn typed_names_find_the_closest_pokemon() {
        let m = meta(&[
            ("Incineroar", ""),
            ("Kingambit", ""),
            ("Indeedee-F", ""),
            ("Sneasler", ""),
        ]);
        assert_eq!(find_typed(&m, "kinga").unwrap().name, "Kingambit");
        assert_eq!(find_typed(&m, "IN").unwrap().name, "Incineroar");
        assert_eq!(find_typed(&m, "dee-f").unwrap().name, "Indeedee-F");
        assert!(find_typed(&m, "zzz").is_none());
        assert!(find_typed(&m, "  ").is_none());
    }

    #[test]
    fn adding_foes_keeps_six_and_skips_repeats() {
        let m = meta(&[
            ("Incineroar", "Sitrus Berry"),
            ("Kingambit", "Choice Scarf"),
        ]);
        let s = match_from_params(&params("Incineroar~1~1~0~0", Some("king")), &m, 0);
        assert_eq!(s.foes.len(), 2);
        assert_eq!((s.foes[0].stage, s.foes[0].paralyzed), (1, true));
        // A Pokémon that usually holds a Scarf is assumed to have one.
        assert!(s.foes[1].scarf);
        let again = match_from_params(&params("Incineroar,Incineroar", Some("Incin")), &m, 0);
        assert_eq!(again.foes.len(), 1);
        assert!(s.trick_room && s.weather == Some(Weather::Rain));
    }

    #[test]
    fn state_survives_a_round_trip_through_links() {
        let m = meta(&[("Incineroar", ""), ("Kingambit", "")]);
        let s = match_from_params(&params("Kingambit~-1~0~1~0,Incineroar", None), &m, 2);
        let vs = ViewState {
            page: Page::Speed,
            fmt: "vgc".into(),
            cut: 1630,
            sort: crate::view::SortKey::Rank,
            desc: false,
            q: String::new(),
            ty: String::new(),
            picks: vec![],
            team: vec![],
        };
        let q: Vec<(String, String)> = match_query(&s, &vs)
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        let back: SpeedParams =
            serde_urlencoded::from_str(&serde_urlencoded::to_string(q).unwrap()).unwrap();
        let again = match_from_params(&back, &m, 2);
        assert_eq!(again.foes, s.foes);
        assert_eq!(again.trick_room, s.trick_room);
    }

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
