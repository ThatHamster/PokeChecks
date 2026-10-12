//! The damage calculator page. Like the rest of the site it's a GET form, so every
//! calculation has its own shareable URL.

use maud::{Markup, html};
use serde::Deserialize;

use crate::calc::{self, Combatant, Field, Outcome, Terrain, Weather};
use crate::dex::{Category, Dex, MoveData};
use crate::smogon::{Catalog, FormatInfo, Kind, Meta, Mon, Spread};
use crate::team::{
    MAX_POINTS_PER_STAT, MAX_POINTS_TOTAL, NATURES, STAT_LABELS, nature_effect, nature_label,
    points_line,
};
use crate::team_view::option_list;
use crate::view::{Page, Theme, ViewState, mon_icon, shell, site_footer, site_header, type_chips};

/// Calculator fields from the query string. `a_` is the attacker, `d_` the defender.
/// `*_prev` records which Pokémon the fields belong to, so picking a new Pokémon resets
/// its set to the most common one. `sent` tells a submitted form from a first visit, since
/// unticked checkboxes send nothing.
#[derive(Deserialize, Default)]
pub struct CalcParams {
    a: Option<String>,
    a_prev: Option<String>,
    a_item: Option<String>,
    a_ability: Option<String>,
    a_nature: Option<String>,
    a_p0: Option<String>,
    a_p1: Option<String>,
    a_p2: Option<String>,
    a_p3: Option<String>,
    a_p4: Option<String>,
    a_p5: Option<String>,
    a_boost: Option<String>,
    a_burn: Option<String>,
    mv: Option<String>,
    d: Option<String>,
    d_prev: Option<String>,
    d_item: Option<String>,
    d_ability: Option<String>,
    d_nature: Option<String>,
    d_p0: Option<String>,
    d_p1: Option<String>,
    d_p2: Option<String>,
    d_p3: Option<String>,
    d_p4: Option<String>,
    d_p5: Option<String>,
    d_boost: Option<String>,
    /// A common spread picked from the list, applied when its "Use spread" button is pressed.
    a_preset: Option<String>,
    d_preset: Option<String>,
    /// Which submit button was pressed: `a_preset`, `d_preset`, or the main Calculate.
    apply: Option<String>,
    weather: Option<String>,
    terrain: Option<String>,
    crit: Option<String>,
    hh: Option<String>,
    screen: Option<String>,
    both: Option<String>,
    sent: Option<String>,
}

/// One side's set as chosen on the page.
#[derive(Clone)]
pub struct SideInput {
    pub name: String,
    pub item: Option<String>,
    pub ability: Option<String>,
    pub nature: String,
    pub points: [u16; 6],
    pub boost: i8,
    pub burned: bool,
}

impl SideInput {
    /// The Pokémon's most common set.
    fn defaults(mon: &Mon) -> Self {
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

    fn combatant(&self, dex: &Dex) -> Combatant {
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

    fn total_points(&self) -> u16 {
        self.points.iter().sum()
    }
}

pub struct CalcState {
    pub attacker: SideInput,
    pub defender: SideInput,
    pub move_name: String,
    pub weather: Option<Weather>,
    pub terrain: Option<Terrain>,
    pub crit: bool,
    pub helping_hand: bool,
    pub screen: bool,
    pub hits_both: bool,
}

fn non_empty(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn find<'a>(meta: &'a Meta, name: Option<&str>) -> Option<&'a Mon> {
    name.and_then(|n| meta.mons.iter().find(|m| m.name == n))
}

/// Damaging moves the ladder uses on this Pokémon, most used first.
fn damaging_moves<'a>(mon: &'a Mon, dex: &'a Dex) -> Vec<(&'a str, f64, &'a MoveData)> {
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
fn default_move<'a>(moves: &[(&'a str, f64, &MoveData)]) -> Option<&'a str> {
    moves
        .iter()
        .filter(|(_, pct, _)| *pct >= COMMON_MOVE)
        .max_by_key(|(_, _, d)| d.base_power)
        .or(moves.first())
        .map(|(n, _, _)| *n)
}

impl CalcState {
    pub fn from_params(p: &CalcParams, meta: &Meta, dex: &Dex) -> Self {
        let sent = p.sent.is_some();
        let attacker_mon = find(meta, p.a.as_deref()).or(meta.mons.first());
        let defender_mon = find(meta, p.d.as_deref()).or_else(|| {
            meta.mons
                .iter()
                .find(|m| Some(m.name.as_str()) != attacker_mon.map(|a| a.name.as_str()))
        });
        let (Some(att_mon), Some(def_mon)) = (attacker_mon, defender_mon) else {
            return Self::empty();
        };

        let read_side = |mon: &Mon,
                         prev: &Option<String>,
                         item: &Option<String>,
                         ability: &Option<String>,
                         nature: &Option<String>,
                         points: [&Option<String>; 6],
                         boost: &Option<String>,
                         burn: Option<&Option<String>>|
         -> (SideInput, bool) {
            let fresh = prev.as_deref() != Some(mon.name.as_str());
            if fresh {
                return (SideInput::defaults(mon), true);
            }
            let defaults = SideInput::defaults(mon);
            let mut parsed = [0u16; 6];
            for (i, raw) in points.iter().enumerate() {
                parsed[i] = non_empty(raw)
                    .and_then(|v| v.parse::<u16>().ok())
                    .map_or(defaults.points[i], |v| v.min(MAX_POINTS_PER_STAT));
            }
            (
                SideInput {
                    name: mon.name.clone(),
                    item: non_empty(item),
                    ability: non_empty(ability).or(defaults.ability),
                    nature: non_empty(nature)
                        .filter(|n| NATURES.contains(&n.as_str()))
                        .unwrap_or(defaults.nature),
                    points: parsed,
                    boost: non_empty(boost)
                        .and_then(|b| b.parse::<i8>().ok())
                        .map_or(0, |b| b.clamp(-6, 6)),
                    burned: burn.is_some_and(|b| b.is_some()),
                },
                false,
            )
        };

        let (mut attacker, att_fresh) = read_side(
            att_mon,
            &p.a_prev,
            &p.a_item,
            &p.a_ability,
            &p.a_nature,
            [&p.a_p0, &p.a_p1, &p.a_p2, &p.a_p3, &p.a_p4, &p.a_p5],
            &p.a_boost,
            Some(&p.a_burn),
        );
        let (mut defender, def_fresh) = read_side(
            def_mon,
            &p.d_prev,
            &p.d_item,
            &p.d_ability,
            &p.d_nature,
            [&p.d_p0, &p.d_p1, &p.d_p2, &p.d_p3, &p.d_p4, &p.d_p5],
            &p.d_boost,
            None,
        );

        // "Use spread" swaps in a common spread. It's ignored when the Pokémon itself just
        // changed, since the chosen spread belonged to the previous one.
        for (side, fresh, key, preset) in [
            (&mut attacker, att_fresh, "a_preset", &p.a_preset),
            (&mut defender, def_fresh, "d_preset", &p.d_preset),
        ] {
            if !fresh
                && p.apply.as_deref() == Some(key)
                && let Some((nature, points)) = preset.as_deref().and_then(Spread::parse)
            {
                side.nature = nature;
                side.points = points;
            }
        }

        let moves = damaging_moves(att_mon, dex);
        let move_name = non_empty(&p.mv)
            .filter(|m| {
                !att_fresh
                    && dex
                        .move_data(m)
                        .is_some_and(|d| d.category != Category::Status)
            })
            .or_else(|| default_move(&moves).map(str::to_owned))
            .unwrap_or_default();

        // Weather and terrain follow the Pokémon's abilities on a first visit or when a
        // Pokémon that sets one is picked; otherwise they stay as chosen.
        let auto_weather = || {
            [&attacker, &defender]
                .iter()
                .find_map(|s| s.ability.as_deref().and_then(Weather::from_ability))
        };
        let auto_terrain = || {
            [&attacker, &defender]
                .iter()
                .find_map(|s| s.ability.as_deref().and_then(Terrain::from_ability))
        };
        let changed_sets = |pick: &dyn Fn(&SideInput) -> bool| {
            (att_fresh && pick(&attacker)) || (def_fresh && pick(&defender))
        };
        let weather = if !sent
            || changed_sets(&|s| {
                s.ability
                    .as_deref()
                    .and_then(Weather::from_ability)
                    .is_some()
            }) {
            auto_weather()
        } else {
            p.weather.as_deref().and_then(Weather::parse)
        };
        let terrain = if !sent
            || changed_sets(&|s| {
                s.ability
                    .as_deref()
                    .and_then(Terrain::from_ability)
                    .is_some()
            }) {
            auto_terrain()
        } else {
            p.terrain.as_deref().and_then(Terrain::parse)
        };

        Self {
            attacker,
            defender,
            move_name,
            weather,
            terrain,
            crit: sent && p.crit.is_some(),
            helping_hand: sent && p.hh.is_some(),
            screen: sent && p.screen.is_some(),
            hits_both: !sent || p.both.is_some(),
        }
    }

    fn empty() -> Self {
        let blank = SideInput {
            name: String::new(),
            item: None,
            ability: None,
            nature: "Hardy".into(),
            points: [0; 6],
            boost: 0,
            burned: false,
        };
        Self {
            attacker: blank.clone(),
            defender: blank,
            move_name: String::new(),
            weather: None,
            terrain: None,
            crit: false,
            helping_hand: false,
            screen: false,
            hits_both: true,
        }
    }

    fn field(&self, doubles: bool) -> Field {
        Field {
            doubles,
            hits_both: self.hits_both,
            weather: self.weather,
            terrain: self.terrain,
            crit: self.crit,
            helping_hand: self.helping_hand,
            screen: self.screen,
        }
    }

    /// A link to this exact calculation, optionally with another move selected.
    fn href(&self, vs: &ViewState, move_name: Option<&str>) -> String {
        let mut q: Vec<(String, String)> = vec![
            ("fmt".into(), vs.fmt.clone()),
            ("cut".into(), vs.cut.to_string()),
            ("sent".into(), "1".into()),
        ];
        for (prefix, side) in [("a", &self.attacker), ("d", &self.defender)] {
            q.push((prefix.into(), side.name.clone()));
            q.push((format!("{prefix}_prev"), side.name.clone()));
            q.push((
                format!("{prefix}_item"),
                side.item.clone().unwrap_or_default(),
            ));
            q.push((
                format!("{prefix}_ability"),
                side.ability.clone().unwrap_or_default(),
            ));
            q.push((format!("{prefix}_nature"), side.nature.clone()));
            for (i, p) in side.points.iter().enumerate() {
                q.push((format!("{prefix}_p{i}"), p.to_string()));
            }
            q.push((format!("{prefix}_boost"), side.boost.to_string()));
        }
        if self.attacker.burned {
            q.push(("a_burn".into(), "1".into()));
        }
        q.push(("mv".into(), move_name.unwrap_or(&self.move_name).to_owned()));
        if let Some(w) = self.weather {
            q.push(("weather".into(), w.key().into()));
        }
        if let Some(t) = self.terrain {
            q.push(("terrain".into(), t.key().into()));
        }
        for (key, on) in [
            ("crit", self.crit),
            ("hh", self.helping_hand),
            ("screen", self.screen),
            ("both", self.hits_both),
        ] {
            if on {
                q.push((key.into(), "1".into()));
            }
        }
        format!(
            "{}?{}",
            Page::Calc.path(),
            serde_urlencoded::to_string(q).unwrap_or_default()
        )
    }

    fn swapped(&self) -> Self {
        Self {
            attacker: SideInput {
                boost: 0,
                burned: false,
                ..self.defender.clone()
            },
            defender: SideInput {
                boost: 0,
                burned: false,
                ..self.attacker.clone()
            },
            move_name: String::new(),
            weather: self.weather,
            terrain: self.terrain,
            crit: self.crit,
            helping_hand: false,
            screen: self.screen,
            hits_both: self.hits_both,
        }
    }
}

pub fn calc_page(
    catalog: &Catalog,
    format: &FormatInfo,
    meta: &Meta,
    dex: &Dex,
    vs: &ViewState,
    theme: Theme,
    state: &CalcState,
) -> Markup {
    let doubles = format.kind != Kind::Bss;
    let dek = html! {
        "Work out how much damage one Pokémon does to another in "
        strong { (format.kind.label()) " Regulation " (format.regulation) }
        " at level 50. Both sides start on their most common set; change anything and press Calculate."
    };
    let att_mon = find(meta, Some(&state.attacker.name));
    let def_mon = find(meta, Some(&state.defender.name));

    shell(
        "Damage calculator",
        theme,
        html! {
            (site_header(catalog, format, meta, vs, theme, dek))
            @if let (Some(att_mon), Some(def_mon)) = (att_mon, def_mon) {
                (results(state, att_mon, dex, vs, doubles))
                (calc_form(state, att_mon, def_mon, meta, dex, vs, doubles))
            } @else {
                section.pane.empty { strong { "No usage data for this format yet" } }
            }
            (site_footer(catalog))
        },
    )
}

fn results(state: &CalcState, att_mon: &Mon, dex: &Dex, vs: &ViewState, doubles: bool) -> Markup {
    let att = state.attacker.combatant(dex);
    let def = state.defender.combatant(dex);
    let field = state.field(doubles);
    let selected = dex.move_data(&state.move_name);

    // The selected move plus the attacker's other common damaging moves.
    let mut rows: Vec<(&MoveData, Option<f64>)> = Vec::new();
    if let Some(m) = selected {
        rows.push((
            m,
            att_mon
                .moves
                .iter()
                .find(|x| x.name == m.name)
                .map(|x| x.pct),
        ));
    }
    for (_, pct, data) in damaging_moves(att_mon, dex).into_iter().take(6) {
        if !rows.iter().any(|(m, _)| m.name == data.name) {
            rows.push((data, Some(pct)));
        }
    }

    html! {
        section.pane.calc-result aria-live="polite" {
            @match selected {
                Some(mv) => {
                    @let o = calc::calculate(&att, &def, mv, &field);
                    (headline(state, &o, mv, &field))
                }
                None => p.muted { "Pick a damaging move." }
            }
            @if !rows.is_empty() {
                div.table-scroll {
                    table.calc-moves {
                        thead {
                            tr {
                                th { span.th-label.th-mon-inline { (mon_icon(dex, &state.attacker.name)) (state.attacker.name) "’s moves" } }
                                th { span.th-label { "Type" } }
                                th.r { span.th-label { "Power" } }
                                th.r { span.th-label { "Damage" } }
                                th { span.th-label { "Result" } }
                            }
                        }
                        tbody {
                            @for (m, pct) in &rows {
                                @let o = calc::calculate(&att, &def, m, &field);
                                tr.on[m.name == state.move_name] {
                                    td {
                                        a href=(state.href(vs, Some(&m.name))) { (m.name) }
                                        @if let Some(p) = pct { span.muted.small.inline { (format!(" · {p:.0}% of sets")) } }
                                    }
                                    td {
                                        (type_chip(&o.move_type))
                                        @if o.effectiveness != 1.0 && o.max() > 0 {
                                            span.muted.small.inline { (format!(" {}×", o.effectiveness)) }
                                        }
                                        @if o.spread { span.muted.small.block { "hits both" } }
                                    }
                                    td.r.num { (o.base_power) }
                                    td.r.num {
                                        @if o.max() == 0 { "—" }
                                        @else { (format!("{:.1}–{:.1}%", o.percent(o.min()), o.percent(o.max()))) }
                                    }
                                    td.small { (calc::ko_chance(&o.rolls, o.defender_hp)) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Showdown-style summary line, the damage range, an HP bar and the KO chance.
fn headline(state: &CalcState, o: &Outcome, mv: &MoveData, field: &Field) -> Markup {
    let physical = mv.category == Category::Physical;
    let attack_index = if mv.uses_defense {
        2
    } else if physical {
        1
    } else {
        3
    };
    let defense_index = if physical || mv.targets_defense { 2 } else { 4 };
    let a = &state.attacker;
    let d = &state.defender;

    let stat_part = |side: &SideInput, i: usize| {
        let sign = match nature_effect(&side.nature) {
            Some((up, _)) if up == i => "+",
            Some((_, down)) if down == i => "−",
            _ => "",
        };
        format!("{}{sign} {}", side.points[i], STAT_LABELS[i])
    };
    let boost = |b: i8| match b {
        0 => String::new(),
        b if b > 0 => format!("+{b} "),
        b => format!("−{} ", -b),
    };
    let mut line = format!(
        "{}{} {}{} {}",
        boost(a.boost),
        stat_part(a, attack_index),
        a.item
            .as_deref()
            .map(|i| format!("{i} "))
            .unwrap_or_default(),
        a.name,
        mv.name
    );
    line.push_str(&format!(
        " vs. {}{} HP / {} {}{}",
        boost(d.boost),
        d.points[0],
        stat_part(d, defense_index),
        d.item
            .as_deref()
            .map(|i| format!("{i} "))
            .unwrap_or_default(),
        d.name
    ));
    let mut extras = Vec::new();
    if let Some(w) = field.weather {
        extras.push(format!("in {}", w.label()));
    }
    if let Some(t) = field.terrain {
        extras.push(format!("in {}", t.label()));
    }
    if field.helping_hand {
        extras.push("with Helping Hand".into());
    }
    if field.screen && !o.crit {
        extras.push("through a screen".into());
    }
    if o.crit {
        extras.push("on a critical hit".into());
    }
    if !extras.is_empty() {
        line.push(' ');
        line.push_str(&extras.join(" "));
    }

    // The HP bar can't show more than 100%; the text shows the real numbers.
    let (lo, hi) = (o.percent(o.min()).min(100.0), o.percent(o.max()).min(100.0));
    html! {
        div.headline {
            p.calc-line { (line) }
            @if o.max() == 0 {
                p.big-result { "No damage" }
                @if !o.notes.is_empty() { p.muted { (o.notes.join(" · ")) } }
            } @else {
                div.result-row {
                    p.big-result {
                        span.num { (o.min()) "–" (o.max()) }
                        span.muted.num { " / " (o.defender_hp) " HP" }
                    }
                    p.pct.num { (format!("{:.1}–{:.1}%", o.percent(o.min()), o.percent(o.max()))) }
                    p.ko { (calc::ko_chance(&o.rolls, o.defender_hp)) }
                }
                div.hpbar role="img" aria-label={ (format!("Leaves {:.0}–{:.0}% HP", 100.0 - hi, 100.0 - lo)) } {
                    span.hp-left style=(format!("width:{:.1}%", 100.0 - hi)) {}
                    span.hp-maybe style=(format!("width:{:.1}%", hi - lo)) {}
                }
                p.small.muted {
                    "Rolls: " (o.rolls.iter().map(u32::to_string).collect::<Vec<_>>().join(", "))
                    @if o.hits > 1 { " (" (o.hits) " hits)" }
                }
                @if !o.notes.is_empty() {
                    div.tags { @for n in &o.notes { span.tag.plain-tag { (n) } } }
                }
            }
        }
    }
}

fn type_chip(t: &str) -> Markup {
    let ink = if crate::view::is_light_type(t) {
        "dark"
    } else {
        "light"
    };
    html! {
        span class={ "ty ink-" (ink) } style=(format!("background:var(--ty-{})", t.to_lowercase())) { (t) }
    }
}

#[allow(clippy::too_many_arguments)]
fn calc_form(
    state: &CalcState,
    att_mon: &Mon,
    def_mon: &Mon,
    meta: &Meta,
    dex: &Dex,
    vs: &ViewState,
    doubles: bool,
) -> Markup {
    let moves = damaging_moves(att_mon, dex);
    html! {
        form.calc-form method="get" action=(Page::Calc.path()) {
            input type="hidden" name="fmt" value=(vs.fmt);
            input type="hidden" name="cut" value=(vs.cut);
            input type="hidden" name="sent" value="1";
            // Pressing Enter uses the form's first submit button, so this makes it Calculate
            // rather than a "Use spread" button further down.
            button.sr-only type="submit" name="apply" value="calc" tabindex="-1" aria-hidden="true" { "Calculate" }
            div.calc-grid {
                section.pane.calc-side {
                    div.pane-head { h2 { "Attacker" } }
                    div.section {
                        (side_fields("a", &state.attacker, att_mon, meta, dex, "Attacking stat stage"))
                        div.field {
                            label for="mv" { "Move" }
                            select #mv name="mv" {
                                @if !moves.iter().any(|(n, _, _)| *n == state.move_name) && !state.move_name.is_empty() {
                                    option value=(state.move_name) selected { (state.move_name) " · not in usage data" }
                                }
                                @for (name, pct, data) in &moves {
                                    option value=(name) selected[*name == state.move_name] {
                                        (name) " (" (data.move_type) ", " (data.base_power) ")" (format!(" · {pct:.1}%"))
                                    }
                                }
                            }
                        }
                        label.check {
                            input type="checkbox" name="a_burn" value="1" checked[state.attacker.burned];
                            " Burned"
                        }
                    }
                }
                section.pane.calc-side.calc-field {
                    div.pane-head { h2 { "Field" } }
                    div.section {
                        div.field {
                            label for="weather" { "Weather" }
                            select #weather name="weather" {
                                option value="" selected[state.weather.is_none()] { "None" }
                                @for w in Weather::ALL {
                                    option value=(w.key()) selected[state.weather == Some(w)] { (w.label()) }
                                }
                            }
                        }
                        div.field {
                            label for="terrain" { "Terrain" }
                            select #terrain name="terrain" {
                                option value="" selected[state.terrain.is_none()] { "None" }
                                @for t in Terrain::ALL {
                                    option value=(t.key()) selected[state.terrain == Some(t)] { (t.label()) }
                                }
                            }
                        }
                        label.check { input type="checkbox" name="crit" value="1" checked[state.crit]; " Critical hit" }
                        label.check { input type="checkbox" name="screen" value="1" checked[state.screen]; " Reflect / Light Screen on the defender’s side" }
                        @if doubles {
                            label.check { input type="checkbox" name="hh" value="1" checked[state.helping_hand]; " Helping Hand" }
                            label.check { input type="checkbox" name="both" value="1" checked[state.hits_both]; " Spread moves hit two targets (75% damage)" }
                        }
                        div.calc-actions {
                            button.go type="submit" { "Calculate" }
                            a.mini href=(state.swapped().href(vs, None)) { "Swap sides" }
                        }
                    }
                }
                section.pane.calc-side {
                    div.pane-head { h2 { "Defender" } }
                    div.section {
                        (side_fields("d", &state.defender, def_mon, meta, dex, "Defending stat stage"))
                    }
                }
            }
        }
    }
}

fn side_fields(
    prefix: &str,
    side: &SideInput,
    mon: &Mon,
    meta: &Meta,
    dex: &Dex,
    stage_label: &str,
) -> Markup {
    let id = |f: &str| format!("{prefix}_{f}");
    let legal: Vec<String> = dex
        .entry(&mon.name)
        .map(|e| e.abilities.clone())
        .unwrap_or_default();
    let total = side.total_points();
    html! {
        input type="hidden" name=(id("prev")) value=(side.name);
        div.field {
            label for=(prefix) { "Pokémon" }
            select #(prefix) name=(prefix) {
                @for m in &meta.mons {
                    option value=(m.name) selected[m.name == side.name] { (m.name) (format!(" · {:.1}%", m.usage)) }
                }
            }
        }
        div.calc-mon {
            (mon_icon(dex, &mon.name))
            (type_chips(dex.entry(&mon.name)))
        }
        div.field {
            label for=(id("item")) { "Item" }
            select #(id("item")) name=(id("item")) {
                option value="" selected[side.item.is_none()] { "No item" }
                (option_list(&mon.items, side.item.as_deref()))
            }
        }
        div.field {
            label for=(id("ability")) { "Ability" }
            select #(id("ability")) name=(id("ability")) {
                (option_list(&mon.abilities, side.ability.as_deref()))
                @for a in legal.iter().filter(|a| !mon.abilities.iter().any(|u| &u.name == *a)) {
                    option value=(a) selected[side.ability.as_deref() == Some(a.as_str())] { (a) " · not used on ladder" }
                }
            }
        }
        div.field {
            label for=(id("nature")) { "Nature" }
            select #(id("nature")) name=(id("nature")) {
                @for n in NATURES {
                    option value=(n) selected[side.nature == n] { (nature_label(n)) }
                }
            }
        }
        fieldset.field {
            legend { "Stat points" }
            div.points-grid {
                @for (i, label) in STAT_LABELS.iter().enumerate() {
                    div {
                        label for=(id(&format!("p{i}"))) { (label) }
                        input #(id(&format!("p{i}"))) type="number" name=(id(&format!("p{i}"))) min="0" max=(MAX_POINTS_PER_STAT) value=(side.points[i]) inputmode="numeric";
                    }
                }
            }
            p class={ "note" @if total > MAX_POINTS_TOTAL { " warn-text" } } {
                (total) " of " (MAX_POINTS_TOTAL) " points used"
                @if total > MAX_POINTS_TOTAL { ". That’s over the Champions limit, so this set can’t exist in game." }
                @else {
                    @match mon.spreads.iter().find(|s| s.nature == side.nature && s.points == side.points) {
                        Some(s) => { (format!(" · {:.1}% of {} sets use this exact spread", s.pct, mon.name)) }
                        None => { " · not one of " (mon.name) "’s common spreads" }
                    }
                }
            }
        }
        @if !mon.spreads.is_empty() {
            div.field.preset {
                label for=(id("preset")) { "Common spreads" }
                div.preset-row {
                    select #(id("preset")) name=(id("preset")) {
                        @for s in &mon.spreads {
                            @let value = format!("{}:{}", s.nature, s.points.map(|p| p.to_string()).join("/"));
                            @let line = points_line(&s.points);
                            option value=(value) selected[s.nature == side.nature && s.points == side.points] {
                                (s.nature) " · " (if line.is_empty() { "no points".to_owned() } else { line }) (format!(" · {:.1}%", s.pct))
                            }
                        }
                    }
                    button.mini type="submit" name="apply" value=(id("preset")) { "Use spread" }
                }
            }
        }
        div.field {
            label for=(id("boost")) { (stage_label) }
            select #(id("boost")) name=(id("boost")) {
                @for b in -6i8..=6 {
                    option value=(b) selected[side.boost == b] {
                        @if b > 0 { "+" } (b)
                    }
                }
            }
        }
    }
}
