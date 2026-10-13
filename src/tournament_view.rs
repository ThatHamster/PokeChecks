//! The tournament teams page: top finishers' open team lists from Limitless events, in one
//! list with filters like Pikalytics' (Pokémon every team must have, archetypes, finish).

use maud::{Markup, html};
use serde::Deserialize;

use crate::dex::Dex;
use crate::gameplan::{self, Archetype, PlanMon};
use crate::smogon::{Catalog, FormatInfo, Kind, Meta};
use crate::team::TEAM_SIZE;
use crate::team_view::{newcomer_note, plan_section};
use crate::tournaments::{self, EntryTeam, Event, Tournaments};
use crate::view::{Page, Theme, ViewState, mon_icon, shell, site_footer, site_header, type_chips};

/// Teams per screenful; "Show more" adds another batch.
const PAGE_SIZE: usize = 24;
/// Finishes the "Finish" filter offers: winners, top 4, top 8.
const FINISHES: [(u32, &str); 3] = [(1, "Winners"), (4, "Top 4"), (8, "Top 8")];

/// What the page could load: events, a reason there are none for this format, or an error.
pub enum Source<'a> {
    /// The events, with the filters from the link.
    Events(&'a Tournaments, &'a FilterParams),
    /// Limitless only runs VGC (doubles) events.
    SinglesFormat,
    Unavailable(String),
}

/// The filters in the query string. Lists are comma-separated so a setting never repeats.
#[derive(Deserialize, Default)]
pub struct FilterParams {
    /// Pokémon every team must have.
    mons: Option<String>,
    /// A Pokémon typed into the "Require Pokémon" box, added to `mons`.
    add: Option<String>,
    /// Archetype keys, e.g. `rain,tailwind`.
    arch: Option<String>,
    /// Best finish: 1, 4 or 8.
    finish: Option<String>,
    /// How many teams to show.
    show: Option<String>,
}

/// One team, with what the filters and the card need worked out once.
struct Row<'a> {
    event: &'a Event,
    team: &'a EntryTeam,
    plan_mons: Vec<PlanMon>,
    archetypes: Vec<Archetype>,
}

impl Row<'_> {
    /// Whether the team has `name`. A base species (`Charizard`) matches any of its forms,
    /// Megas included; a form (`Charizard-Mega-Y`) only matches itself.
    fn has(&self, name: &str, dex: &Dex) -> bool {
        let is_species = dex.species(name) == name;
        self.plan_mons
            .iter()
            .any(|m| m.name == name || (is_species && dex.species(&m.name) == name))
    }

    fn win_rate(&self) -> f64 {
        match self.team.record {
            Some((w, l, t)) if w + l + t > 0 => f64::from(w) / f64::from(w + l + t),
            _ => 0.0,
        }
    }
}

/// The filters in effect.
#[derive(Clone, Default, PartialEq)]
struct Filter {
    mons: Vec<String>,
    archetypes: Vec<Archetype>,
    finish: Option<u32>,
    show: usize,
}

impl Filter {
    /// Reads the query string. Pokémon are matched to names in `known` (any case), so links
    /// stay tidy; a typed name that matches nothing is returned separately.
    fn from_params(p: &FilterParams, known: &[String]) -> (Self, Option<String>) {
        let resolve = |typed: &str| -> Option<String> {
            let t = typed.trim().to_lowercase();
            if t.is_empty() {
                return None;
            }
            known
                .iter()
                .find(|k| k.to_lowercase() == t)
                .or_else(|| known.iter().find(|k| k.to_lowercase().starts_with(&t)))
                .or_else(|| known.iter().find(|k| k.to_lowercase().contains(&t)))
                .cloned()
        };
        let mut mons: Vec<String> = Vec::new();
        for name in p.mons.as_deref().unwrap_or("").split(',') {
            if let Some(n) = resolve(name)
                && !mons.contains(&n)
            {
                mons.push(n);
            }
        }
        let mut unknown = None;
        if let Some(typed) = p.add.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            match resolve(typed) {
                Some(n) if !mons.contains(&n) => mons.push(n),
                Some(_) => {}
                None => unknown = Some(typed.chars().take(40).collect()),
            }
        }
        mons.truncate(TEAM_SIZE);
        let mut archetypes: Vec<Archetype> = Vec::new();
        for a in p
            .arch
            .as_deref()
            .unwrap_or("")
            .split(',')
            .filter_map(Archetype::parse)
        {
            if !archetypes.contains(&a) {
                archetypes.push(a);
            }
        }
        let finish = p
            .finish
            .as_deref()
            .and_then(|f| f.parse().ok())
            .filter(|f| FINISHES.iter().any(|(n, _)| n == f));
        let show = p
            .show
            .as_deref()
            .and_then(|s| s.parse::<usize>().ok())
            .map_or(PAGE_SIZE, |s| s.clamp(PAGE_SIZE, 1_000));
        (
            Self {
                mons,
                archetypes,
                finish,
                show,
            },
            unknown,
        )
    }

    fn is_active(&self) -> bool {
        !self.mons.is_empty() || !self.archetypes.is_empty() || self.finish.is_some()
    }

    fn matches(&self, row: &Row, dex: &Dex) -> bool {
        self.finish.is_none_or(|f| row.team.placing <= f)
            && self.mons.iter().all(|m| row.has(m, dex))
            && self.archetypes.iter().all(|a| row.archetypes.contains(a))
    }

    /// This page with these filters, keeping the rest of the URL state.
    fn href(&self, vs: &ViewState) -> String {
        let mut url = vs.on_page(Page::Tournaments).href();
        let mut pairs: Vec<(&str, String)> = Vec::new();
        if !self.mons.is_empty() {
            pairs.push(("mons", self.mons.join(",")));
        }
        if !self.archetypes.is_empty() {
            let keys: Vec<&str> = self.archetypes.iter().map(|a| a.key()).collect();
            pairs.push(("arch", keys.join(",")));
        }
        if let Some(f) = self.finish {
            pairs.push(("finish", f.to_string()));
        }
        if self.show > PAGE_SIZE {
            pairs.push(("show", self.show.to_string()));
        }
        if !pairs.is_empty() {
            url.push('&');
            url.push_str(&serde_urlencoded::to_string(pairs).unwrap_or_default());
        }
        url
    }

    /// A changed copy, back on the first screenful of results.
    fn with(&self, change: impl FnOnce(&mut Self)) -> Self {
        let mut f = self.clone();
        change(&mut f);
        f.show = PAGE_SIZE;
        f
    }

    fn toggled_archetype(&self, a: Archetype) -> Self {
        self.with(|f| match f.archetypes.iter().position(|x| *x == a) {
            Some(i) => {
                f.archetypes.remove(i);
            }
            None => f.archetypes.push(a),
        })
    }
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
        Source::Events(t, _) => t.regulation.as_str(),
        _ => format.regulation.as_str(),
    };
    let dek = html! {
        "Top 8 finishers from the largest recent "
        strong { "Regulation " (regulation) }
        " events on "
        a href="https://play.limitlesstcg.com/tournaments?game=VGC" target="_blank" rel="noopener" { "Limitless" }
        ", the online tournament platform: the open team lists players submitted. Filter by Pokémon, archetype or finish."
    };

    shell(
        "Tournament teams",
        theme,
        html! {
            (site_header(catalog, format, meta, vs, theme, dek))
            @match source {
                Source::Events(t, params) if !t.events.is_empty() => {
                    @if t.regulation != format.regulation {
                        p.note.page-note.warn-text {
                            "These events are Regulation " (t.regulation) ", but usage stats are still Regulation " (format.regulation)
                            " until Smogon publishes a month of " (t.regulation) " data. Pokémon new to " (t.regulation)
                            " can’t be opened in the team builder until then."
                        }
                    }
                    (team_list(t, meta, dex, vs, params))
                }
                Source::Events(t, _) => {
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

/// The filters and the matching teams, best finishes first.
fn team_list(
    t: &Tournaments,
    meta: &Meta,
    dex: &Dex,
    vs: &ViewState,
    params: &FilterParams,
) -> Markup {
    let mut rows: Vec<Row> = t
        .events
        .iter()
        .flat_map(|e| e.teams.iter().map(move |team| (e, team)))
        .map(|(event, team)| {
            let plan_mons: Vec<PlanMon> = team
                .members
                .iter()
                .map(|m| PlanMon::from_list(m, dex))
                .collect();
            let archetypes = gameplan::archetypes(&plan_mons);
            Row {
                event,
                team,
                plan_mons,
                archetypes,
            }
        })
        .collect();
    // Best finish, then best record, then the bigger and newer event.
    rows.sort_by(|a, b| {
        a.team
            .placing
            .cmp(&b.team.placing)
            .then(b.win_rate().total_cmp(&a.win_rate()))
            .then(b.event.players.cmp(&a.event.players))
            .then(b.event.date.cmp(&a.event.date))
    });

    // Every Pokémon on these teams, most common first, plus the base species of forms so
    // "Charizard" finds Mega Charizard teams too.
    let mut counts: Vec<(String, usize)> = Vec::new();
    for m in rows.iter().flat_map(|r| &r.plan_mons) {
        match counts.iter_mut().find(|(n, _)| *n == m.name) {
            Some((_, c)) => *c += 1,
            None => counts.push((m.name.clone(), 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut known: Vec<String> = counts.iter().map(|(n, _)| n.clone()).collect();
    for (n, _) in &counts {
        let species = dex.species(n).to_owned();
        if !known.contains(&species) {
            known.push(species);
        }
    }

    let (filter, unknown) = Filter::from_params(params, &known);
    let matching: Vec<&Row> = rows.iter().filter(|r| filter.matches(r, dex)).collect();
    let shown = &matching[..matching.len().min(filter.show)];
    // How many teams each archetype chip would leave, with the other filters as they are.
    let chip_count = |a: Archetype| {
        let f = filter.with(|f| {
            if !f.archetypes.contains(&a) {
                f.archetypes.push(a);
            }
        });
        rows.iter().filter(|r| f.matches(r, dex)).count()
    };

    html! {
        section.pane.tt-filters aria-labelledby="tt-filters-h" {
            div.pane-head {
                h2 #tt-filters-h { "Filter teams" }
                span.muted.small { strong.num { (matching.len()) } " of " (rows.len()) " teams" }
            }
            div.tt-filter-body {
                form.tt-add method="get" action="/tournaments" {
                    @for (k, v) in vs.on_page(Page::Tournaments).query_pairs() {
                        input type="hidden" name=(k) value=(v);
                    }
                    @if !filter.mons.is_empty() { input type="hidden" name="mons" value=(filter.mons.join(",")); }
                    @if !filter.archetypes.is_empty() {
                        input type="hidden" name="arch" value=(filter.archetypes.iter().map(|a| a.key()).collect::<Vec<_>>().join(","));
                    }
                    @if let Some(f) = filter.finish { input type="hidden" name="finish" value=(f); }
                    label for="tt-add" { "Require Pokémon" }
                    div.preset-row {
                        input #tt-add type="text" name="add" list="tt-mons" autocomplete="off"
                            placeholder="Type a Pokémon, press Enter"
                            disabled[filter.mons.len() >= TEAM_SIZE];
                        button.go type="submit" disabled[filter.mons.len() >= TEAM_SIZE] { "Add" }
                    }
                    datalist #tt-mons { @for (n, _) in &counts { option value=(n) {} } }
                }
                @if let Some(typed) = &unknown {
                    p.warn-text.small { "None of these teams has a Pokémon called “" (typed) "”." }
                }
                @if !filter.mons.is_empty() {
                    div.chips-sel aria-label="Required Pokémon" {
                        @for m in &filter.mons {
                            span.chip-sel {
                                (mon_icon(dex, m))
                                (m)
                                a.remove href=(filter.with(|f| f.mons.retain(|x| x != m)).href(vs)) aria-label={ "Stop requiring " (m) } { "×" }
                            }
                        }
                    }
                }
                div.tt-row {
                    span.label { "Archetype" }
                    nav.tt-chips aria-label="Archetype" {
                        @for a in Archetype::ALL {
                            @let on = filter.archetypes.contains(&a);
                            @let n = chip_count(a);
                            @if on || n > 0 {
                                a.toggle href=(filter.toggled_archetype(a).href(vs)) aria-pressed=(on) {
                                    (a.label()) " " span.count { (n) }
                                }
                            } @else {
                                span.toggle.off title="No teams match with your other filters" { (a.label()) " " span.count { "0" } }
                            }
                        }
                    }
                }
                div.tt-row {
                    span.label { "Finish" }
                    nav.tt-chips aria-label="Finish" {
                        a.toggle href=(filter.with(|f| f.finish = None).href(vs)) aria-pressed=(filter.finish.is_none()) { "Any" }
                        @for (n, label) in FINISHES {
                            a.toggle href=(filter.with(|f| f.finish = Some(n)).href(vs)) aria-pressed=(filter.finish == Some(n)) { (label) }
                        }
                    }
                }
                @if filter.is_active() {
                    a.clear-filters href=(Filter::default().with(|_| {}).href(vs)) { "Clear filters" }
                }
            }
        }
        p.note.page-note {
            "Team lists don’t include stat points. “Open in team builder” fills them in with the most common spread for each Pokémon and nature, so check them before you play."
        }
        (newcomer_note())
        @if matching.is_empty() {
            section.pane.empty {
                strong { "No teams match" }
                "Try removing a filter. "
                a href=(Filter::default().with(|_| {}).href(vs)) { "Clear filters" }
            }
        } @else {
            div.tt-grid {
                @for row in shown { (entry_card(row, meta, dex, vs)) }
            }
            @if matching.len() > shown.len() {
                div.show-more {
                    @let next = Filter { show: filter.show + PAGE_SIZE, ..filter.clone() };
                    a.go href=(next.href(vs)) { "Show more teams" }
                    span.muted.small { "Showing " (shown.len()) " of " (matching.len()) }
                }
            }
        }
    }
}

fn entry_card(row: &Row, meta: &Meta, dex: &Dex, vs: &ViewState) -> Markup {
    let (t, e) = (row.team, row.event);
    let (members, missing) = tournaments::to_members(t, meta, dex);
    // Limitless only runs VGC, so these are always doubles teams.
    let plan = gameplan::plan(&row.plan_mons, true);
    html! {
        article.pane.team-card {
            div.team-card-head {
                div.placing-row {
                    span class={ "placing" @if t.placing == 1 { " first" } } { (ordinal(t.placing)) }
                    span.muted.small { "of " span.num { (e.players) } }
                    @if let Some((w, l, ties)) = t.record {
                        span.num.muted.record { (w) "–" (l) @if ties > 0 { "–" (ties) } }
                    }
                }
                h3 { (t.player) }
                p.event-line.small {
                    a href=(e.url()) target="_blank" rel="noopener" { (e.name) }
                    span.muted { " · " (format_date(&e.date)) }
                }
                @if !row.archetypes.is_empty() {
                    div.tags { @for a in &row.archetypes { span.tag { (a.label()) } } }
                }
            }
            ul.roster {
                @for (m, pm) in t.members.iter().zip(&row.plan_mons) {
                    li {
                        div.mon-cell {
                            span.mon-name { (mon_icon(dex, &pm.name)) span.strong { (pm.name) } }
                            (type_chips(dex.entry(&pm.name)))
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

    fn params(mons: &str, add: &str, arch: &str, finish: &str, show: &str) -> FilterParams {
        let opt = |s: &str| (!s.is_empty()).then(|| s.to_owned());
        FilterParams {
            mons: opt(mons),
            add: opt(add),
            arch: opt(arch),
            finish: opt(finish),
            show: opt(show),
        }
    }

    #[test]
    fn reads_filters_from_the_link() {
        let known: Vec<String> = ["Incineroar", "Charizard-Mega-Y", "Charizard", "Rillaboom"]
            .map(str::to_owned)
            .to_vec();
        let (f, unknown) = Filter::from_params(
            &params(
                "incineroar,Nope,Incineroar",
                "rilla",
                "rain,bogus,rain,trick-room",
                "4",
                "48",
            ),
            &known,
        );
        assert_eq!(f.mons, ["Incineroar", "Rillaboom"]);
        assert_eq!(f.archetypes, [Archetype::Rain, Archetype::TrickRoom]);
        assert_eq!((f.finish, f.show, unknown), (Some(4), 48, None));

        // "char" finds the exact base species name before forms that merely start with it.
        let (f, _) = Filter::from_params(&params("", "charizard", "", "3", "5"), &known);
        assert_eq!(f.mons, ["Charizard"]);
        assert_eq!(
            (f.finish, f.show),
            (None, PAGE_SIZE),
            "3 isn't a finish we offer"
        );

        let (_, unknown) = Filter::from_params(&params("", "Missingno", "", "", ""), &known);
        assert_eq!(unknown.as_deref(), Some("Missingno"));
    }
}
