//! The team builder and meta teams pages.

use maud::{Markup, html};

use crate::dex::Dex;
use crate::smogon::{Catalog, FormatInfo, Meta, Mon, Share};
use crate::team::{
    self, Blocked, Custom, MAX_POINTS_PER_STAT, MAX_POINTS_TOTAL, MOVE_SLOTS, Member, NATURES,
    STAT_LABELS, Set, TEAM_SIZE,
};
use crate::view::{
    Page, Theme, ViewState, filter_form, filtered, month_name, shell, site_footer, site_header,
    type_chips,
};

/// A set edit that broke a rule. The page shows the message and keeps what was typed.
pub struct EditError {
    pub slot: usize,
    pub message: String,
    pub draft: Custom,
}

pub fn builder_page(
    catalog: &Catalog,
    format: &FormatInfo,
    meta: &Meta,
    dex: &Dex,
    vs: &ViewState,
    theme: Theme,
    error: Option<&EditError>,
) -> Markup {
    let chosen: Vec<(&Mon, Option<&Custom>)> = vs
        .team
        .iter()
        .filter_map(|member| {
            meta.mons
                .iter()
                .find(|m| m.name == member.name)
                .map(|m| (m, member.custom.as_ref()))
        })
        .collect();
    let members: Vec<&Mon> = chosen.iter().map(|(m, _)| *m).collect();
    let sets = team::resolve_sets(&chosen);
    let dek = html! {
        "Build a team for "
        strong { (format.kind.label()) " Regulation " (format.regulation) }
        ". Each Pokémon starts with its most common set on the ladder. Open “Edit set” to change anything; every option shows how often the ladder uses it."
    };

    shell(
        "Team builder",
        theme,
        html! {
            (site_header(catalog, format, meta, vs, theme, dek))
            div.bench {
                (picker(meta, dex, vs, &members))
                (team_panel(catalog, format, meta, dex, vs, &sets, error))
            }
            (site_footer(catalog))
        },
    )
}

fn picker(meta: &Meta, dex: &Dex, vs: &ViewState, members: &[&Mon]) -> Markup {
    let suggested = team::suggestions(members, meta, dex, 6);
    let rows = filtered(meta, dex, vs);
    let max_usage = meta.mons.first().map_or(1.0, |m| m.usage);

    html! {
        section.pane aria-labelledby="picker-h" {
            div.pane-head {
                h2 #picker-h { "Add Pokémon" }
                (filter_form(meta, dex, vs))
            }
            @if members.len() < TEAM_SIZE && !suggested.is_empty() {
                div.section {
                    div.section-title {
                        h3 { "Suggested next" }
                        p {
                            @if members.is_empty() { "The most used Pokémon right now" }
                            @else { "How often each one appears on teams with yours" }
                        }
                    }
                    ul.suggest {
                        @for (m, score) in &suggested {
                            li {
                                div.mon-cell { span.strong { (m.name) } (type_chips(dex.entry(&m.name))) }
                                span.num.muted {
                                    @if members.is_empty() { (format!("{:.1}% usage", m.usage)) }
                                    @else { (format!("{:.0}%", score * 100.0)) }
                                }
                                a.mini href=(vs.team_toggled(&m.name).href()) aria-label={ "Add " (m.name) " to your team" } { "Add" }
                            }
                        }
                    }
                }
            }
            div.table-scroll {
                table {
                    thead {
                        tr {
                            th.r { span.th-label { "#" } }
                            th { span.th-label { "Pokémon" } }
                            th { span.th-label { "Usage" } }
                            th.r { span.th-label { "Team" } }
                        }
                    }
                    tbody {
                        @for m in &rows {
                            @let state = team::blocked(members, &m.name, dex);
                            tr.on[state == Some(Blocked::OnTeam)] {
                                td.r.rank.num { (m.rank) }
                                td { div.mon-cell { span.strong { (m.name) } (type_chips(dex.entry(&m.name))) } }
                                td.usage-cell {
                                    div.ubar {
                                        span.track { i style=(format!("width:{:.1}%", m.usage / max_usage * 100.0)) {} }
                                        span.num { (format!("{:.1}%", m.usage)) }
                                    }
                                }
                                td.r {
                                    @match &state {
                                        None => a.mini href=(vs.team_toggled(&m.name).href()) aria-label={ "Add " (m.name) " to your team" } { "Add" },
                                        Some(Blocked::OnTeam) => a.mini.remove-btn href=(vs.team_toggled(&m.name).href()) aria-label={ "Remove " (m.name) " from your team" } { "Remove" },
                                        Some(Blocked::SameSpecies(other)) => span.muted.small title={ "Only one " (dex.species(&m.name)) " per team" } { "Same species as " (other) },
                                        Some(Blocked::Full) => span.muted.small { "Team full" },
                                    }
                                }
                            }
                        }
                        @if rows.is_empty() {
                            tr.empty-row { td colspan="4" { "No Pokémon in this month’s stats match that search. Clear the filter or try another type." } }
                        }
                    }
                }
            }
        }
    }
}

fn team_panel(
    catalog: &Catalog,
    format: &FormatInfo,
    meta: &Meta,
    dex: &Dex,
    vs: &ViewState,
    sets: &[Set],
    error: Option<&EditError>,
) -> Markup {
    let megas = sets.iter().filter(|s| team::is_mega(&s.mon.name)).count();
    let repeated = team::repeated_items(sets);
    html! {
        section.pane.scout aria-labelledby="team-h" {
            div.pane-head {
                h2 #team-h { "Your team" }
                div.team-count {
                    span.num { (sets.len()) " / " (TEAM_SIZE) }
                    @if !sets.is_empty() {
                        a.clear href=(vs.with_team(vec![]).href()) { "Clear team" }
                    }
                }
            }
            @if sets.is_empty() {
                div.empty {
                    strong { "Your team is empty" }
                    "Add Pokémon from the list, or "
                    a href=(vs.on_page(Page::Teams).href()) { "start from a meta team" }
                    "."
                }
            } @else {
                div.section {
                    div.team-grid {
                        @for (slot, s) in sets.iter().enumerate() {
                            (member_card(slot, s, dex, vs, error.filter(|e| e.slot == slot)))
                        }
                        @for _ in sets.len()..TEAM_SIZE {
                            div.slot-empty { "Empty slot" }
                        }
                    }
                    @if !repeated.is_empty() {
                        p.note.warn-text { "Item clause: " (repeated.join(", ")) " is held by more than one Pokémon. Each item can only be used once per team." }
                    }
                    @if megas > 1 {
                        p.note { "Only one Pokémon can Mega Evolve per battle, and this team has " (megas) " Megas." }
                    }
                }
                (matchup_table(sets, dex))
                (speed_list(sets, dex))
                div.section {
                    div.section-title {
                        h3 { "Showdown export" }
                        p { (month_name(&catalog.month)) ", " (format.kind.label()) " at " (meta.cutoff) "+" }
                    }
                    label.sr-only for="paste" { "Team in Showdown format" }
                    textarea #paste.paste readonly rows=(sets.len() * 9) { (team::showdown_paste(sets, dex)) }
                    p.note {
                        "Select the text and paste it into Pokémon Showdown’s Import or Poképaste. Stat points go on the EVs line, which Champions formats read as 0–32 points. Mega Pokémon are listed as their base form holding the Mega Stone."
                    }
                }
            }
        }
    }
}

fn member_card(
    slot: usize,
    s: &Set,
    dex: &Dex,
    vs: &ViewState,
    error: Option<&EditError>,
) -> Markup {
    let m = s.mon;
    let stats = s.stats(dex);
    html! {
        article.card.member {
            div.card-head {
                div.member-top {
                    span.nm { (m.name) }
                    a.remove href=(vs.team_toggled(&m.name).href()) aria-label={ "Remove " (m.name) " from your team" } { "×" }
                }
                div.member-sub {
                    (type_chips(dex.entry(&m.name)))
                    @if s.custom { span.badge { "Your set" } } @else { span.badge.plain { "Most common set" } }
                }
            }
            dl.set {
                dt { "Item" }
                dd { (s.item.as_deref().unwrap_or("No item")) (usage_note(&m.items, s.item.as_deref())) }
                dt { "Ability" }
                dd { (s.ability.as_deref().unwrap_or("—")) (usage_note(&m.abilities, s.ability.as_deref())) }
                @if let Some(nature) = &s.nature {
                    dt { "Nature" } dd { (team::nature_label(nature)) }
                }
                @if let Some(points) = &s.points {
                    dt { "Points" }
                    dd.num {
                        @let line = team::points_line(points);
                        (if line.is_empty() { "None".to_owned() } else { line })
                        @match s.spread_pct() {
                            Some(p) => span.muted.small title="Share of ladder sets with exactly this nature and spread" { (format!("{p:.1}% of sets")) },
                            None => span.muted.small { "Not a common spread" },
                        }
                    }
                }
            }
            div.stat-line {
                @for (label, value) in STAT_LABELS.iter().zip(stats) {
                    span { span.label { (label) } span.num.strong { (value) } }
                }
            }
            ul.moves {
                @for mv in &s.moves { li { (mv) } }
                @for _ in s.moves.len()..MOVE_SLOTS { li.empty-move { "No move" } }
            }
            details.edit open[error.is_some()] {
                summary { "Edit set" }
                @if let Some(e) = error {
                    p.form-error role="alert" { (e.message) }
                }
                (edit_form(slot, s, dex, vs, error.map(|e| &e.draft)))
            }
            @if s.custom {
                a.reset href=(vs.with_default_set(slot).href()) { "Reset to most common set" }
            }
        }
    }
}

/// ` · 63.7%` when the ladder uses this option, nothing otherwise.
fn usage_note(options: &[Share], chosen: Option<&str>) -> Markup {
    let pct = chosen
        .and_then(|c| options.iter().find(|o| o.name == c))
        .map(|o| o.pct);
    html! {
        @if let Some(p) = pct { span.muted.small.inline { (format!(" · {p:.1}%")) } }
    }
}

/// Every editable field of one member's set, sent as a GET form back to the builder.
fn edit_form(slot: usize, s: &Set, dex: &Dex, vs: &ViewState, draft: Option<&Custom>) -> Markup {
    let m = s.mon;
    let current = draft.cloned().unwrap_or_else(|| s.to_custom());
    let id = |field: &str| format!("s{slot}-{field}");
    let total: u16 = current.points.iter().sum();

    // Abilities the form can have that the ladder never used, listed after the used ones.
    let legal_abilities: Vec<String> = dex
        .entry(&m.name)
        .map(|e| e.abilities.clone())
        .unwrap_or_default();
    let unused_abilities: Vec<&String> = legal_abilities
        .iter()
        .filter(|a| !m.abilities.iter().any(|u| &u.name == *a))
        .collect();

    // Natures in order of use, then the rest alphabetically.
    let mut natures: Vec<(&str, Option<f64>)> = m
        .natures
        .iter()
        .filter(|n| NATURES.contains(&n.name.as_str()))
        .map(|n| (n.name.as_str(), Some(n.pct)))
        .collect();
    for n in NATURES {
        if !natures.iter().any(|(x, _)| *x == n) {
            natures.push((n, None));
        }
    }

    html! {
        form.set-form method="get" action=(Page::Builder.path()) {
            input type="hidden" name="fmt" value=(vs.fmt);
            input type="hidden" name="cut" value=(vs.cut);
            input type="hidden" name="pick" value=(vs.picks.join(","));
            input type="hidden" name="team" value=(vs.team_param());
            @if !vs.q.is_empty() { input type="hidden" name="q" value=(vs.q); }
            @if !vs.ty.is_empty() { input type="hidden" name="type" value=(vs.ty); }
            input type="hidden" name="edit" value=(slot);

            div.field {
                label for=(id("item")) { "Item" }
                select #(id("item")) name="item" {
                    option value="" selected[current.item.is_none()] { "No item" }
                    (option_list(&m.items, current.item.as_deref()))
                }
            }
            div.field {
                label for=(id("ability")) { "Ability" }
                select #(id("ability")) name="ability" {
                    (option_list(&m.abilities, current.ability.as_deref()))
                    @for a in &unused_abilities {
                        option value=(a) selected[current.ability.as_deref() == Some(a.as_str())] { (a) " · not used on ladder" }
                    }
                }
            }
            fieldset.field {
                legend { "Moves" }
                div.move-grid {
                    @for i in 0..MOVE_SLOTS {
                        @let chosen = current.moves.get(i).map(String::as_str);
                        label.sr-only for=(id(&format!("m{}", i + 1))) { "Move " (i + 1) }
                        select #(id(&format!("m{}", i + 1))) name={ "m" (i + 1) } {
                            option value="" selected[chosen.is_none()] { "No move" }
                            (option_list(&m.moves, chosen))
                        }
                    }
                }
            }
            div.field {
                label for=(id("nature")) { "Nature" }
                select #(id("nature")) name="nature" {
                    @for (n, pct) in &natures {
                        option value=(n) selected[current.nature == *n] {
                            (team::nature_label(n))
                            @if let Some(p) = pct { (format!(" · {p:.1}%")) }
                        }
                    }
                }
            }
            fieldset.field {
                legend { "Stat points" }
                div.points-grid {
                    @for (i, label) in STAT_LABELS.iter().enumerate() {
                        div {
                            label for=(id(&format!("p{i}"))) { (label) }
                            input #(id(&format!("p{i}"))) type="number" name={ "p" (i) } min="0" max=(MAX_POINTS_PER_STAT) step="1" value=(current.points[i]) inputmode="numeric";
                        }
                    }
                }
                p.note { "0–" (MAX_POINTS_PER_STAT) " per stat, " (MAX_POINTS_TOTAL) " in total. This set uses " (total) "." }
            }
            button.go type="submit" name="apply" value="set" { "Save set" }

            @if !m.spreads.is_empty() {
                div.field.preset {
                    label for=(id("preset")) { "Or use a common spread" }
                    div.preset-row {
                        select #(id("preset")) name="preset" {
                            @for sp in &m.spreads {
                                @let value = format!("{}:{}", sp.nature, sp.points.map(|p| p.to_string()).join("/"));
                                @let line = team::points_line(&sp.points);
                                option value=(value) selected[sp.nature == current.nature && sp.points == current.points] {
                                    (sp.nature) " · " (if line.is_empty() { "no points".to_owned() } else { line }) (format!(" · {:.1}%", sp.pct))
                                }
                            }
                        }
                        button.mini type="submit" name="apply" value="preset" { "Use spread" }
                    }
                }
            }
        }
    }
}

/// `<option>`s for a usage list, with the chosen value first if the ladder never used it.
fn option_list(options: &[Share], chosen: Option<&str>) -> Markup {
    let missing = chosen.filter(|c| !options.iter().any(|o| o.name == *c));
    html! {
        @if let Some(c) = missing {
            option value=(c) selected { (c) " · not in usage data" }
        }
        @for o in options {
            option value=(o.name) selected[chosen == Some(o.name.as_str())] {
                (o.name) (format!(" · {:.1}%", o.pct))
            }
        }
    }
}

fn matchup_table(sets: &[Set], dex: &Dex) -> Markup {
    let rows = team::matchups(sets, dex);
    let problems: Vec<&str> = rows
        .iter()
        .filter(|r| r.is_problem())
        .map(|r| r.attack)
        .collect();
    html! {
        div.section {
            div.section-title {
                h3 { "Type matchups" }
                p {
                    @if problems.is_empty() { "No type hits three or more of your team super effectively." }
                    @else { "Watch out for " (problems.join(", ")) "." }
                }
            }
            div.table-scroll {
                table.matchups {
                    thead {
                        tr {
                            th { span.th-label { "Attack" } }
                            @for s in sets { th.c { span.th-label title=(s.mon.name) { (short_name(&s.mon.name)) } } }
                            th.c { span.th-label { "Weak" } }
                            th.c { span.th-label { "Resist" } }
                        }
                    }
                    tbody {
                        @for r in &rows {
                            tr.warn[r.is_problem()] {
                                td { span class={ "ty ink-" (if crate::view::is_light_type(r.attack) { "dark" } else { "light" }) } style=(format!("background:var(--ty-{})", r.attack.to_lowercase())) { (r.attack) } }
                                @for m in &r.multipliers {
                                    td class={ "c mult " (mult_class(*m)) } { (mult_label(*m)) }
                                }
                                td.c.num.strong.weak-count[r.weak > 0] { (r.weak) }
                                td.c.num { (r.resist) }
                            }
                        }
                    }
                }
            }
            p.note { "Uses each Pokémon’s most common ability, so Levitate, Flash Fire and similar abilities count." }
        }
    }
}

fn speed_list(sets: &[Set], dex: &Dex) -> Markup {
    let mut by_speed: Vec<(&Set, u16)> = sets.iter().map(|s| (s, s.speed(dex))).collect();
    by_speed.sort_by_key(|a| std::cmp::Reverse(a.1));
    let fastest = by_speed.first().map_or(1, |x| x.1).max(1);
    html! {
        div.section {
            div.section-title {
                h3 { "Speed at level 50" }
                p { "With each Pokémon’s most common nature and stat points" }
            }
            ul.speeds {
                @for (s, spe) in &by_speed {
                    li {
                        span { (s.mon.name) }
                        span.track { i style=(format!("width:{:.1}%", f64::from(*spe) / f64::from(fastest) * 100.0)) {} }
                        span.num.strong { (spe) }
                    }
                }
            }
        }
    }
}

pub fn teams_page(
    catalog: &Catalog,
    format: &FormatInfo,
    meta: &Meta,
    dex: &Dex,
    vs: &ViewState,
    theme: Theme,
) -> Markup {
    let teams = team::meta_teams(meta, dex);
    let dek = html! {
        "Teams assembled from "
        strong { (month_name(&catalog.month)) " " (format.kind.label()) " Regulation " (format.regulation) }
        " ladder data. Each starts from a popular Pokémon and adds whichever Pokémon most often appear alongside the ones already picked. They show common cores, not specific players’ teams."
    };

    shell(
        "Meta teams",
        theme,
        html! {
            (site_header(catalog, format, meta, vs, theme, dek))
            @if teams.is_empty() {
                section.pane.empty {
                    strong { "Not enough data for suggested teams" }
                    "This format doesn’t have enough teammate data at the " (meta.cutoff) "+ cutoff. Try a lower rating cutoff."
                }
            } @else {
                div.meta-teams {
                    @for t in &teams { (meta_team_card(t, dex, vs)) }
                }
            }
            (site_footer(catalog))
        },
    )
}

fn meta_team_card(t: &team::MetaTeam, dex: &Dex, vs: &ViewState) -> Markup {
    let sets = team::common_sets(&t.members);
    let names: Vec<Member> = t.members.iter().map(|m| Member::new(&m.name)).collect();
    // Named after the Pokémon the team was built from and the first partner it picked.
    let title = format!("{} + {}", t.members[0].name, t.members[1].name);
    let megas = t.members.iter().filter(|m| team::is_mega(&m.name)).count();
    let avg_usage = t.members.iter().map(|m| m.usage).sum::<f64>() / t.members.len() as f64;

    html! {
        article.pane.team-card {
            div.team-card-head {
                h3 { (title) }
                @if !t.tags.is_empty() {
                    div.tags { @for tag in &t.tags { span.tag { (tag) } } }
                }
            }
            ul.roster {
                @for s in &sets {
                    li {
                        div.mon-cell {
                            span.strong { (s.mon.name) }
                            (type_chips(dex.entry(&s.mon.name)))
                        }
                        span.item.muted { (s.item.as_deref().unwrap_or("")) }
                        span.num.muted { (format!("{:.1}%", s.mon.usage)) }
                    }
                }
            }
            @if megas > 1 {
                p.note { "Carries " (megas) " Megas. Only one can Mega Evolve per battle, so pick which one to bring." }
            }
            div.team-stats {
                div title="On average, the share of each member’s teams that include each other member" {
                    span.label { "Played together" }
                    strong.num { (format!("{:.0}%", t.cohesion * 100.0)) }
                }
                div {
                    span.label { "Average usage" }
                    strong.num { (format!("{:.1}%", avg_usage)) }
                }
            }
            div.team-actions {
                a.go href=(vs.with_team(names).on_page(Page::Builder).href()) { "Open in team builder" }
                details {
                    summary { "Showdown export" }
                    textarea.paste readonly rows="14" aria-label="Team in Showdown format" { (team::showdown_paste(&sets, dex)) }
                }
            }
        }
    }
}

/// Keeps matchup table headers narrow: `Ogerpon-Wellspring` -> `Ogerpon-W.`
fn short_name(name: &str) -> String {
    if name.chars().count() <= 10 {
        return name.to_owned();
    }
    match name.split_once('-') {
        Some((base, form)) => format!("{base}-{}.", form.chars().next().unwrap_or(' ')),
        None => name.chars().take(9).collect::<String>() + ".",
    }
}

fn mult_label(m: f32) -> &'static str {
    match m {
        0.0 => "0",
        x if x >= 4.0 => "4×",
        x if x >= 2.0 => "2×",
        x if x <= 0.25 => "¼",
        x if x < 1.0 => "½",
        _ => "",
    }
}

fn mult_class(m: f32) -> &'static str {
    match m {
        0.0 => "immune",
        x if x >= 4.0 => "x4",
        x if x >= 2.0 => "x2",
        x if x < 1.0 => "resist",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_long_form_names() {
        assert_eq!(short_name("Incineroar"), "Incineroar");
        assert_eq!(short_name("Ogerpon-Wellspring"), "Ogerpon-W.");
        assert_eq!(short_name("Salamence-Mega"), "Salamence-M.");
        assert_eq!(short_name("Fezandipiti"), "Fezandipi.");
    }

    #[test]
    fn labels_multipliers() {
        assert_eq!(mult_label(4.0), "4×");
        assert_eq!(mult_label(0.5), "½");
        assert_eq!(mult_label(0.25), "¼");
        assert_eq!(mult_label(0.0), "0");
        assert_eq!(mult_label(1.0), "");
    }
}
