//! The team builder and meta teams pages.

use maud::{Markup, html};

use crate::dex::Dex;
use crate::smogon::{Catalog, FormatInfo, Meta, Mon};
use crate::team::{self, Blocked, Set, TEAM_SIZE};
use crate::view::{
    Page, Theme, ViewState, filter_form, filtered, month_name, shell, site_footer, site_header,
    type_chips,
};

pub fn builder_page(
    catalog: &Catalog,
    format: &FormatInfo,
    meta: &Meta,
    dex: &Dex,
    vs: &ViewState,
    theme: Theme,
) -> Markup {
    let members: Vec<&Mon> = vs
        .team
        .iter()
        .filter_map(|n| meta.mons.iter().find(|m| &m.name == n))
        .collect();
    let sets = team::common_sets(&members);
    let dek = html! {
        "Build a team for "
        strong { (format.kind.label()) " Regulation " (format.regulation) }
        ". Each Pokémon starts with its most common set on the ladder, and suggestions come from who players pair together."
    };

    shell(
        "Team builder",
        theme,
        html! {
            (site_header(catalog, format, meta, vs, theme, dek))
            div.bench {
                (picker(meta, dex, vs, &members))
                (team_panel(catalog, format, meta, dex, vs, &sets))
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
                            tr.empty-row { td colspan="4" { "No Pokémon in the top " (meta.mons.len()) " match that search. Clear the filter or try another type." } }
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
) -> Markup {
    let megas = sets.iter().filter(|s| team::is_mega(&s.mon.name)).count();
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
                        @for s in sets { (member_card(s, dex, vs)) }
                        @for _ in sets.len()..TEAM_SIZE {
                            div.slot-empty { "Empty slot" }
                        }
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

fn member_card(s: &Set, dex: &Dex, vs: &ViewState) -> Markup {
    let m = s.mon;
    html! {
        article.card.member {
            div.card-head {
                div.member-top {
                    span.nm { (m.name) }
                    a.remove href=(vs.team_toggled(&m.name).href()) aria-label={ "Remove " (m.name) " from your team" } { "×" }
                }
                (type_chips(dex.entry(&m.name)))
            }
            dl.set {
                @if let Some(item) = &s.item { dt { "Item" } dd { (item) } }
                @if let Some(ability) = &s.ability { dt { "Ability" } dd { (ability) } }
                @if let Some(sp) = s.spread {
                    dt { "Nature" } dd { (sp.nature) }
                    dt { "Points" }
                    dd.num {
                        (team::points_line(&sp.points))
                        " " span.muted.small title="Share of sets with exactly this spread" { "(" (format!("{:.0}", sp.pct)) "% of sets)" }
                    }
                }
                dt { "Speed" } dd.num { (s.speed(dex)) }
            }
            ul.moves {
                @for mv in &s.moves { li { (mv) } }
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
    let names: Vec<String> = t.members.iter().map(|m| m.name.clone()).collect();
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
