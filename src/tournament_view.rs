//! The tournament teams page: top finishers' open team lists from Limitless events.

use maud::{Markup, html};

use crate::dex::Dex;
use crate::gameplan::{self, PlanMon};
use crate::smogon::{Catalog, FormatInfo, Kind, Meta};
use crate::team_view::{newcomer_note, plan_section};
use crate::tournaments::{self, EntryTeam, Event, Tournaments};
use crate::view::{Page, Theme, ViewState, mon_icon, shell, site_footer, site_header, type_chips};

/// What the page could load: events, a reason there are none for this format, or an error.
pub enum Source<'a> {
    Events(&'a Tournaments),
    /// Limitless only runs VGC (doubles) events.
    SinglesFormat,
    Unavailable(String),
}

pub fn tournaments_page(
    catalog: &Catalog,
    format: &FormatInfo,
    meta: &Meta,
    dex: &Dex,
    vs: &ViewState,
    theme: Theme,
    source: Source,
) -> Markup {
    // Limitless picks up a new regulation as soon as events run under it; Smogon's usage
    // stats only follow a month later.
    let regulation = match &source {
        Source::Events(t) => t.regulation.as_str(),
        _ => format.regulation.as_str(),
    };
    let dek = html! {
        "Top finishers from the largest recent "
        strong { "Regulation " (regulation) }
        " events on "
        a href="https://play.limitlesstcg.com/tournaments?game=VGC" target="_blank" rel="noopener" { "Limitless" }
        ", the online tournament platform. These are the open team lists players submitted."
    };

    shell(
        "Tournament teams",
        theme,
        html! {
            (site_header(catalog, format, meta, vs, theme, dek))
            @match source {
                Source::Events(t) if !t.events.is_empty() => {
                    @if t.regulation != format.regulation {
                        p.note.page-note.warn-text {
                            "These events are Regulation " (t.regulation) ", but usage stats are still Regulation " (format.regulation)
                            " until Smogon publishes a month of " (t.regulation) " data. Pokémon new to " (t.regulation)
                            " can’t be opened in the team builder until then."
                        }
                    }
                    p.note.page-note {
                        "Team lists don’t include stat points. “Open in team builder” fills them in with the most common spread for each Pokémon and nature, so check them before you play."
                    }
                    (newcomer_note())
                    @for e in &t.events { (event_section(e, meta, dex, vs)) }
                }
                Source::Events(t) => {
                    section.pane.empty {
                        strong { "No large events yet" }
                        "Limitless has no recent Regulation " (t.regulation) " events with 32 or more players and published team lists. Check back after this weekend’s tournaments."
                    }
                }
                Source::SinglesFormat => {
                    section.pane.empty {
                        strong { "Tournament teams are VGC only" }
                        "Limitless runs doubles events, so there are no Battle Stadium Singles team lists. "
                        @if let Some(vgc) = catalog.formats.iter().find(|f| f.kind == Kind::Vgc) {
                            a href=(ViewState { fmt: vgc.kind.key().to_owned(), cut: vgc.default_cutoff(), ..vs.clone() }.href()) { "See VGC tournament teams" }
                            "."
                        }
                    }
                }
                Source::Unavailable(message) => {
                    section.pane.notice {
                        h2 { "Tournament teams are unavailable right now" }
                        p { (message) }
                        p { "Limitless may be down or rate limiting requests. Reload the page in a few minutes; the rest of PokeChecks still works." }
                    }
                }
            }
            (site_footer(catalog))
        },
    )
}

fn event_section(e: &Event, meta: &Meta, dex: &Dex, vs: &ViewState) -> Markup {
    html! {
        section.event {
            div.event-head {
                h2 { (e.name) }
                p.muted {
                    span.num { (e.players) } " players · " (format_date(&e.date)) " · "
                    a href=(e.url()) target="_blank" rel="noopener" { "Standings on Limitless" }
                }
            }
            div.meta-teams {
                @for t in &e.teams { (entry_card(t, meta, dex, vs)) }
            }
        }
    }
}

fn entry_card(t: &EntryTeam, meta: &Meta, dex: &Dex, vs: &ViewState) -> Markup {
    let (members, missing) = tournaments::to_members(t, meta, dex);
    // Limitless only runs VGC, so these are always doubles teams.
    let plan_mons: Vec<PlanMon> = t
        .members
        .iter()
        .map(|m| PlanMon::from_list(m, dex))
        .collect();
    let plan = gameplan::plan(&plan_mons, true);
    html! {
        article.pane.team-card {
            div.team-card-head {
                div.placing-row {
                    span class={ "placing" @if t.placing == 1 { " first" } } { (ordinal(t.placing)) }
                    @if let Some((w, l, ties)) = t.record {
                        span.num.muted { (w) "–" (l) @if ties > 0 { "–" (ties) } }
                    }
                }
                h3 { (t.player) }
            }
            ul.roster {
                @for m in &t.members {
                    @let form = dex.battle_form(&m.id, m.item.as_deref());
                    li {
                        div.mon-cell {
                            span.mon-name { (mon_icon(dex, &form)) span.strong { (form) } }
                            (type_chips(dex.entry(&form)))
                        }
                        span.item.muted { (m.item.as_deref().unwrap_or("")) }
                    }
                }
            }
            details.team-list {
                summary { "Full team list" }
                div.list-sets {
                    @for m in &t.members {
                        div.list-set {
                            strong { (m.name) @if let Some(i) = &m.item { " @ " (i) } }
                            span.muted.small {
                                (m.ability.as_deref().unwrap_or("—"))
                                @if let Some(n) = &m.nature { " · " (n) }
                            }
                            span.small { (m.moves.join(" / ")) }
                        }
                    }
                }
            }
            (plan_section(&plan, dex))
            div.team-actions {
                @if members.is_empty() {
                    span.muted.small { "None of these Pokémon are in this month’s usage data." }
                } @else {
                    a.go href=(vs.with_team(members).on_page(Page::Builder).href()) { "Open in team builder" }
                }
                @if !missing.is_empty() {
                    span.muted.small { "Not in this month’s usage data, so left out of the builder: " (missing.join(", ")) "." }
                }
                details {
                    summary { "Showdown export" }
                    textarea.paste readonly rows="14" aria-label={ (t.player) "’s team in Showdown format" } { (tournaments::team_list_paste(t)) }
                }
            }
        }
    }
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// `2026-10-11` -> `Oct 11, 2026`
fn format_date(ymd: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let parts: Vec<&str> = ymd.split('-').collect();
    match parts.as_slice() {
        [y, m, d] => match (m.parse::<usize>(), d.parse::<u32>()) {
            (Ok(m @ 1..=12), Ok(d)) => format!("{} {d}, {y}", MONTHS[m - 1]),
            _ => ymd.to_owned(),
        },
        _ => ymd.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinals_and_dates() {
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(3), "3rd");
        assert_eq!(ordinal(4), "4th");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(format_date("2026-10-11"), "Oct 11, 2026");
        assert_eq!(format_date("soon"), "soon");
    }
}
