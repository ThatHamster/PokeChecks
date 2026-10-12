//! Server-rendered HTML. All interaction (sorting, filtering, picking Pokémon to compare)
//! is plain links and GET forms, so the page needs no JavaScript.

use std::sync::OnceLock;

use maud::{DOCTYPE, Markup, html};

use crate::dex::{Dex, DexEntry};
use crate::smogon::{Catalog, FormatInfo, LADDER_SIZE, Meta, Mon, Share};
use crate::team::Member;

pub const CSS: &str = include_str!("../assets/app.css");

/// FNV-1a hash of the stylesheet, used to bust browser caches when it changes.
static CSS_HASH: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let hash = CSS.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{:08x}", hash >> 32)
});

/// The comparison panel holds at most this many Pokémon.
pub const MAX_PICKS: usize = 4;
const SLOTS: [&str; MAX_PICKS] = ["--s1", "--s2", "--s3", "--s4"];
const STAT_NAMES: [&str; 6] = ["HP", "Atk", "Def", "SpA", "SpD", "Spe"];
/// Types whose chip colour needs dark text.
const LIGHT_TYPES: [&str; 12] = [
    "Electric", "Ice", "Ground", "Normal", "Fairy", "Bug", "Steel", "Flying", "Rock", "Fighting",
    "Grass", "Stellar",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Rank,
    Name,
    Delta,
    Bst,
    Spe,
}

impl SortKey {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "rank" => Self::Rank,
            "name" => Self::Name,
            "change" => Self::Delta,
            "bst" => Self::Bst,
            "spe" => Self::Spe,
            _ => return None,
        })
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Rank => "rank",
            Self::Name => "name",
            Self::Delta => "change",
            Self::Bst => "bst",
            Self::Spe => "spe",
        }
    }

    /// Rank and name read naturally ascending; numbers read biggest first.
    fn default_desc(self) -> bool {
        !matches!(self, Self::Rank | Self::Name)
    }
}

/// The site's pages. They share one URL state, so switching pages keeps the format,
/// comparison and team.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Usage,
    Builder,
    Teams,
    Tournaments,
    Calc,
    Speed,
}

impl Page {
    const ALL: [Page; 6] = [
        Page::Usage,
        Page::Builder,
        Page::Teams,
        Page::Tournaments,
        Page::Calc,
        Page::Speed,
    ];

    pub fn path(self) -> &'static str {
        match self {
            Self::Usage => "/",
            Self::Builder => "/builder",
            Self::Teams => "/teams",
            Self::Tournaments => "/tournaments",
            Self::Calc => "/calc",
            Self::Speed => "/speed",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Usage => "Usage",
            Self::Builder => "Team builder",
            Self::Teams => "Meta teams",
            Self::Tournaments => "Tournament teams",
            Self::Calc => "Damage calc",
            Self::Speed => "Speed tiers",
        }
    }
}

/// Everything the URL carries. Each link on the page is this state with one thing changed.
#[derive(Clone)]
pub struct ViewState {
    pub page: Page,
    pub fmt: String,
    pub cut: u32,
    pub sort: SortKey,
    pub desc: bool,
    pub q: String,
    pub ty: String,
    pub picks: Vec<String>,
    pub team: Vec<Member>,
}

impl ViewState {
    pub(crate) fn href(&self) -> String {
        let mut pairs: Vec<(&str, String)> = vec![
            ("fmt", self.fmt.clone()),
            ("cut", self.cut.to_string()),
            ("sort", self.sort.as_str().to_owned()),
            ("dir", if self.desc { "desc" } else { "asc" }.to_owned()),
        ];
        if !self.q.is_empty() {
            pairs.push(("q", self.q.clone()));
        }
        if !self.ty.is_empty() {
            pairs.push(("type", self.ty.clone()));
        }
        pairs.push(("pick", self.picks.join(",")));
        if !self.team.is_empty() {
            pairs.push(("team", self.team_param()));
        }
        format!(
            "{}?{}",
            self.page.path(),
            serde_urlencoded::to_string(pairs).unwrap_or_default()
        )
    }

    pub(crate) fn on_page(&self, page: Page) -> Self {
        Self {
            page,
            ..self.clone()
        }
    }

    /// The `team` query value: members separated by commas.
    pub(crate) fn team_param(&self) -> String {
        self.team
            .iter()
            .map(Member::encode)
            .collect::<Vec<_>>()
            .join(",")
    }

    pub(crate) fn with_team(&self, team: Vec<Member>) -> Self {
        Self {
            team,
            ..self.clone()
        }
    }

    pub(crate) fn on_team(&self, name: &str) -> bool {
        self.team.iter().any(|m| m.name == name)
    }

    /// Adds `name` to the team, or removes it if it's already there.
    pub(crate) fn team_toggled(&self, name: &str) -> Self {
        let mut team = self.team.clone();
        if let Some(i) = team.iter().position(|m| m.name == name) {
            team.remove(i);
        } else if team.len() < crate::team::TEAM_SIZE {
            team.push(Member::new(name));
        }
        self.with_team(team)
    }

    /// Puts member `slot` back on its most common set.
    pub(crate) fn with_default_set(&self, slot: usize) -> Self {
        let mut team = self.team.clone();
        if let Some(m) = team.get_mut(slot) {
            m.custom = None;
        }
        self.with_team(team)
    }

    pub(crate) fn without_filters(&self) -> Self {
        Self {
            q: String::new(),
            ty: String::new(),
            ..self.clone()
        }
    }

    fn toggled(&self, name: &str) -> Self {
        let mut v = self.clone();
        if let Some(i) = v.picks.iter().position(|p| p == name) {
            v.picks.remove(i);
        } else if v.picks.len() < MAX_PICKS {
            v.picks.push(name.to_owned());
        }
        v
    }

    fn sorted_by(&self, key: SortKey) -> Self {
        let mut v = self.clone();
        v.desc = if v.sort == key {
            !v.desc
        } else {
            key.default_desc()
        };
        v.sort = key;
        v
    }

    fn with_format(&self, f: &FormatInfo) -> Self {
        Self {
            fmt: f.kind.key().to_owned(),
            cut: f.default_cutoff(),
            ..self.clone()
        }
    }

    fn with_cutoff(&self, cut: u32) -> Self {
        Self {
            cut,
            ..self.clone()
        }
    }

    fn slot(&self, name: &str) -> Option<&'static str> {
        self.picks.iter().position(|p| p == name).map(|i| SLOTS[i])
    }
}

/// The visitor's colour theme, kept in a cookie. `System` follows the device setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "system" => Self::System,
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    /// Reads the `theme` cookie out of a raw `Cookie` header.
    pub fn from_cookie_header(header: &str) -> Self {
        header
            .split(';')
            .filter_map(|pair| pair.trim().split_once('='))
            .find(|(name, _)| *name == "theme")
            .and_then(|(_, value)| Self::parse(value))
            .unwrap_or_default()
    }
}

/// Links to `/theme`, which stores the choice and sends the visitor back to `back`.
fn theme_switch(theme: Theme, back: &str) -> Markup {
    html! {
        div {
            span.label { "Theme" }
            nav.cutoffs aria-label="Colour theme" {
                @for t in Theme::ALL {
                    a.cut href={ "/theme?" (serde_urlencoded::to_string([("mode", t.as_str()), ("back", back)]).unwrap_or_default()) }
                        aria-current=[(t == theme).then_some("true")] {
                        (t.label())
                    }
                }
            }
        }
    }
}

pub fn page(
    catalog: &Catalog,
    format: &FormatInfo,
    meta: &Meta,
    dex: &Dex,
    vs: &ViewState,
    theme: Theme,
) -> Markup {
    let regulation = &format.regulation;
    let dek = html! {
        "Pokémon Champions ranked usage for "
        strong { "Regulation " (regulation) }
        ", side by side with base stats. Pick up to four Pokémon from the usage list to compare their stats and most common sets."
    };
    shell(
        &format!("Regulation {regulation}"),
        theme,
        html! {
            (site_header(catalog, format, meta, vs, theme, dek))
            div.bench {
                (ladder(catalog, meta, dex, vs))
                (scout(catalog, format, meta, dex, vs))
            }
            (site_footer(catalog))
        },
    )
}

/// Title, page links, intro, ladder facts, theme switch and format tabs.
pub(crate) fn site_header(
    catalog: &Catalog,
    format: &FormatInfo,
    meta: &Meta,
    vs: &ViewState,
    theme: Theme,
    dek: Markup,
) -> Markup {
    html! {
        header {
            div {
                h1 { "Poke" span { "Checks" } }
                nav.pages aria-label="Pages" {
                    @for p in Page::ALL {
                        a href=(vs.on_page(p).href()) aria-current=[(p == vs.page).then_some("page")] { (p.label()) }
                    }
                }
                p.dek { (dek) }
            }
            div.ladder-meta {
                div { span.label { "Month" } strong { (month_name(&catalog.month)) } }
                div { span.label { "Battles" } strong.num { (thousands(meta.battles)) } }
                div {
                    span.label { "Rating cutoff" }
                    nav.cutoffs aria-label="Rating cutoff" {
                        @for &c in &format.cutoffs {
                            a.cut href=(vs.with_cutoff(c).href()) aria-current=[(c == meta.cutoff).then_some("true")] {
                                span.num { (c) "+" }
                            }
                        }
                    }
                }
                (theme_switch(theme, &vs.href()))
            }
        }

        nav.tabs aria-label="Format" {
            @for f in &catalog.formats {
                a.tab href=(vs.with_format(f).href()) aria-current=[(f.kind == format.kind).then_some("page")] {
                    (f.kind.label()) " " small { (f.kind.style()) }
                }
            }
        }
    }
}

pub(crate) fn site_footer(catalog: &Catalog) -> Markup {
    html! {
        footer {
            "Usage comes from the "
            a href={ "https://www.smogon.com/stats/" (catalog.month) "/chaos/" } target="_blank" rel="noopener" {
                "Smogon usage stats for " (month_name(&catalog.month))
            }
            ", weighted by player rating at the cutoff you pick. PokeChecks always shows the newest Pokémon Champions regulation Smogon has published and checks for new stats every few hours. "
            @if let Some(prev) = &catalog.prev_month {
                "“vs " (short_month(prev)) "” is the change in percentage points from " (month_name(prev))
                "; a format that did not exist then shows “new”. "
            }
            "Move figures are the share of sets that carry the move. Base stats, types and sprites come from Pokémon Showdown."
            p.fine {
                "PokeChecks is a free fan project, not affiliated with Nintendo, Game Freak, Creatures or The Pokémon Company. Pokémon and its names and images belong to them."
                @if let Some(url) = support_url() {
                    " "
                    a.support href=(url) target="_blank" rel="noopener" { "Support PokeChecks" }
                    " to help cover hosting."
                }
            }
        }
    }
}

/// Where the footer's support link points, from the `SUPPORT_URL` environment variable
/// (a Ko-fi or GitHub Sponsors page, say). No link without one; only https links count.
fn support_url() -> Option<&'static str> {
    static URL: OnceLock<Option<String>> = OnceLock::new();
    URL.get_or_init(|| {
        std::env::var("SUPPORT_URL")
            .ok()
            .map(|u| u.trim().to_owned())
            .filter(|u| u.starts_with("https://"))
    })
    .as_deref()
}

/// Search box and type filter. Hidden fields carry the rest of the state through the GET form.
pub(crate) fn filter_form(meta: &Meta, dex: &Dex, vs: &ViewState) -> Markup {
    let mut types: Vec<&str> = meta
        .mons
        .iter()
        .filter_map(|m| dex.entry(&m.name))
        .flat_map(|e| e.types.iter().map(String::as_str))
        .collect();
    types.sort_unstable();
    types.dedup();

    html! {
        form.filters method="get" action=(vs.page.path()) {
            input type="hidden" name="fmt" value=(vs.fmt);
            input type="hidden" name="cut" value=(vs.cut);
            input type="hidden" name="sort" value=(vs.sort.as_str());
            input type="hidden" name="dir" value=(if vs.desc { "desc" } else { "asc" });
            input type="hidden" name="pick" value=(vs.picks.join(","));
            @if !vs.team.is_empty() {
                input type="hidden" name="team" value=(vs.team_param());
            }
            label.sr-only for="q" { "Search Pokémon" }
            input #q type="search" name="q" value=(vs.q) placeholder="Search Pokémon" autocomplete="off";
            label.sr-only for="type" { "Type" }
            select #type name="type" {
                option value="" selected[vs.ty.is_empty()] { "All types" }
                @for t in &types {
                    option value=(t) selected[vs.ty == *t] { (t) }
                }
            }
            button.go type="submit" { "Filter" }
            @if !vs.q.is_empty() || !vs.ty.is_empty() {
                a.clear href=(vs.without_filters().href()) { "Clear" }
            }
        }
    }
}

/// Ladder rows matching the search box and type filter, in ladder order. Without a filter
/// this is the top of the ladder; a search reaches every Pokémon in the stats.
pub(crate) fn filtered<'a>(meta: &'a Meta, dex: &Dex, vs: &ViewState) -> Vec<&'a Mon> {
    let query = vs.q.trim().to_lowercase();
    let limit = if query.is_empty() && vs.ty.is_empty() {
        LADDER_SIZE
    } else {
        usize::MAX
    };
    meta.mons
        .iter()
        .filter(|m| query.is_empty() || m.name.to_lowercase().contains(&query))
        .filter(|m| {
            vs.ty.is_empty() || dex.entry(&m.name).is_some_and(|e| e.types.contains(&vs.ty))
        })
        .take(limit)
        .collect()
}

pub fn error_page(message: &str, theme: Theme) -> Markup {
    shell(
        "Stats unavailable",
        theme,
        html! {
            header {
                div { h1 { "Poke" span { "Checks" } } }
                div.ladder-meta { (theme_switch(theme, "/")) }
            }
            section.pane.notice {
                h2 { "Usage stats are unavailable right now" }
                p { (message) }
                p { "Smogon may be down or still uploading this month’s files. Reload the page in a few minutes." }
            }
        },
    )
}

pub(crate) fn shell(subtitle: &str, theme: Theme, body: Markup) -> Markup {
    let data_theme = match theme {
        Theme::System => None,
        Theme::Light | Theme::Dark => Some(theme.as_str()),
    };
    html! {
        (DOCTYPE)
        html lang="en" data-theme=[data_theme] {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "PokeChecks · " (subtitle) }
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin;
                link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Fredoka:wght@600;700&family=Nunito:wght@400;600;700;800&family=JetBrains+Mono:wght@400;600&display=swap";
                // The hash in the URL makes browsers fetch the stylesheet again whenever it changes.
                link rel="stylesheet" href={ "/app.css?v=" (*CSS_HASH) };
            }
            body { div.wrap { (body) } }
        }
    }
}

// ---------- usage ladder ----------

fn ladder(catalog: &Catalog, meta: &Meta, dex: &Dex, vs: &ViewState) -> Markup {
    let mut rows = filtered(meta, dex, vs);
    rows.sort_by(|a, b| {
        let ord = match vs.sort {
            SortKey::Rank => a.rank.cmp(&b.rank),
            SortKey::Name => a.name.cmp(&b.name),
            SortKey::Delta => a
                .delta()
                .unwrap_or(f64::NEG_INFINITY)
                .total_cmp(&b.delta().unwrap_or(f64::NEG_INFINITY)),
            SortKey::Bst => bst(dex, &a.name).cmp(&bst(dex, &b.name)),
            SortKey::Spe => stat(dex, &a.name, 5).cmp(&stat(dex, &b.name, 5)),
        };
        if vs.desc { ord.reverse() } else { ord }
    });

    let max_usage = meta.mons.first().map_or(1.0, |m| m.usage);
    let full = vs.picks.len() >= MAX_PICKS;
    let change_label = catalog
        .prev_month
        .as_deref()
        .map_or("Change".to_owned(), |p| format!("vs {}", short_month(p)));

    html! {
        section.pane aria-labelledby="ladder-h" {
            div.pane-head {
                h2 #ladder-h { "Usage ladder" }
                (filter_form(meta, dex, vs))
            }
            div.table-scroll {
                table {
                    thead {
                        tr {
                            (sort_header(vs, SortKey::Rank, "#", "r"))
                            (sort_header(vs, SortKey::Name, "Pokémon", ""))
                            th { span.th-label { "Usage" } }
                            (sort_header(vs, SortKey::Delta, &change_label, "r"))
                            (sort_header(vs, SortKey::Bst, "BST", "r hide-sm"))
                            (sort_header(vs, SortKey::Spe, "Spe", "r hide-sm"))
                        }
                    }
                    tbody {
                        @for m in &rows {
                            @let picked = vs.slot(&m.name);
                            tr.on[picked.is_some()] {
                                td.r.rank.num { (m.rank) }
                                td {
                                    div.mon-cell {
                                        @if picked.is_some() || !full {
                                            a.pick href=(vs.toggled(&m.name).href())
                                                aria-pressed=(picked.is_some())
                                                style=[picked.map(|s| format!("--slot:var({s})"))]
                                                title=(if picked.is_some() { format!("Remove {} from the comparison", m.name) } else { format!("Add {} to the comparison", m.name) }) {
                                                span.box aria-hidden="true" {}
                                                (mon_icon(dex, &m.name))
                                                (m.name)
                                            }
                                        } @else {
                                            span.pick.disabled title="The comparison is full. Remove a Pokémon first." {
                                                span.box aria-hidden="true" {}
                                                (mon_icon(dex, &m.name))
                                                (m.name)
                                            }
                                        }
                                        (type_chips(dex.entry(&m.name)))
                                    }
                                }
                                td.usage-cell {
                                    div.ubar {
                                        span.track { i style=(format!("width:{:.1}%", m.usage / max_usage * 100.0)) {} }
                                        span.num { (format!("{:.1}%", m.usage)) }
                                    }
                                }
                                td.r { (delta(m)) }
                                td.r.num.hide-sm { (bst(dex, &m.name)) }
                                td.r.num.hide-sm { (stat(dex, &m.name, 5)) }
                            }
                        }
                        @if rows.is_empty() {
                            tr.empty-row { td colspan="6" { "No Pokémon in this month’s stats match that search. Clear the filter or try another type." } }
                        }
                    }
                }
            }
            p.hint {
                @if full {
                    "The comparison holds " (MAX_PICKS) " Pokémon. Remove one before adding another."
                } @else {
                    "Showing the top " (LADDER_SIZE.min(meta.mons.len())) " by usage; search to find any of the " (meta.mons.len()) " in this month’s stats. Tick a box to add that Pokémon to the comparison."
                }
            }
        }
    }
}

fn sort_header(vs: &ViewState, key: SortKey, label: &str, class: &str) -> Markup {
    let active = vs.sort == key;
    let arrow = if active {
        if vs.desc { " ↓" } else { " ↑" }
    } else {
        ""
    };
    let sort_attr = active.then_some(if vs.desc { "descending" } else { "ascending" });
    html! {
        th class=(class) aria-sort=[sort_attr] {
            a href=(vs.sorted_by(key).href()) { (label) (arrow) }
        }
    }
}

// ---------- comparison panel ----------

fn scout(catalog: &Catalog, format: &FormatInfo, meta: &Meta, dex: &Dex, vs: &ViewState) -> Markup {
    let picked: Vec<&Mon> = vs
        .picks
        .iter()
        .filter_map(|n| meta.mons.iter().find(|m| &m.name == n))
        .collect();

    html! {
        section.pane.scout aria-labelledby="scout-h" {
            div.pane-head {
                h2 #scout-h { "Side by side" }
                div.chips-sel {
                    @for m in &picked {
                        span.chip-sel style=(slot_style(vs, &m.name)) {
                            span.dot {}
                            (mon_icon(dex, &m.name))
                            (m.name)
                            a.remove href=(vs.toggled(&m.name).href()) aria-label={ "Remove " (m.name) } { "×" }
                        }
                    }
                }
            }
            @if picked.is_empty() {
                div.empty {
                    strong { "Nothing to compare yet" }
                    "Tick up to four Pokémon in the usage ladder and their stats and sets appear here."
                }
            } @else {
                div.section {
                    div.section-title {
                        h3 { "Base stats" }
                        p { "Shared scale. The highest value in each row is in bold." }
                    }
                    (stat_chart(&picked, dex, vs))
                }
                div.section {
                    div.section-title {
                        h3 { "Usage and common sets" }
                        p { (month_name(&catalog.month)) ", " (format.kind.label()) " Reg " (format.regulation) " at " (meta.cutoff) "+" }
                    }
                    div.cards {
                        @for m in &picked {
                            (card(m, format, meta, dex, vs))
                        }
                    }
                }
            }
        }
    }
}

fn stat_chart(picked: &[&Mon], dex: &Dex, vs: &ViewState) -> Markup {
    // One scale for every bar, rounded up to the next 50 so gridlines land on round numbers.
    let top = picked
        .iter()
        .flat_map(|m| (0..6).map(|i| stat(dex, &m.name, i)))
        .max()
        .unwrap_or(0);
    let scale = ((top as u32).div_ceil(50) * 50).max(150);
    let pct = |v: u32| format!("{:.2}%", v as f64 / scale as f64 * 100.0);
    let ticks: Vec<u32> = (0..=scale).step_by(50).collect();

    html! {
        div.stat-chart {
            div.axis.num aria-hidden="true" {
                @for t in &ticks { span style=(format!("left:{}", pct(*t))) { (t) } }
            }
            @for (si, name) in STAT_NAMES.iter().enumerate() {
                @let values: Vec<u16> = picked.iter().map(|m| stat(dex, &m.name, si)).collect();
                @let best = values.iter().copied().max().unwrap_or(0);
                div.stat-name { (name) }
                div.stat-bars {
                    @for t in &ticks { span.grid-line style=(format!("left:{}", pct(*t))) {} }
                    @for (m, v) in picked.iter().zip(&values) {
                        div.sbar.best[*v == best && picked.len() > 1] style=(slot_style(vs, &m.name)) title={ (m.name) " " (name) " " (v) } {
                            i style=(format!("width:{}", pct(*v as u32))) {}
                            b { (v) }
                        }
                    }
                }
            }
            div.bst-row {
                span.stat-name { "BST" }
                @for m in picked {
                    span.v style=(slot_style(vs, &m.name)) { span.dot {} span.num.strong { (bst(dex, &m.name)) } }
                }
            }
        }
    }
}

fn card(m: &Mon, format: &FormatInfo, meta: &Meta, dex: &Dex, vs: &ViewState) -> Markup {
    let full = vs.picks.len() >= MAX_PICKS;
    html! {
        article.card style=(slot_style(vs, &m.name)) {
            div.card-head {
                div.nm { span.dot {} (mon_icon(dex, &m.name)) (m.name) }
                (type_chips(dex.entry(&m.name)))
                div.big-usage {
                    strong.num { (format!("{:.1}%", m.usage)) }
                    (delta(m))
                    span.sub { "rank " (m.rank) " in " (format.kind.label()) }
                }
                @if vs.on_team(&m.name) {
                    a.to-team href=(vs.on_page(Page::Builder).href()) { "On your team →" }
                } @else if vs.team.len() < crate::team::TEAM_SIZE {
                    a.to-team href=(vs.team_toggled(&m.name).on_page(Page::Builder).href()) { "Add to team →" }
                }
            }
            (share_list("Ability", &m.abilities, 2))
            (share_list("Items", &m.items, 4))
            (share_list("Moves (share of sets)", &m.moves, 6))
            @if !m.tera.is_empty() { (share_list("Tera types", &m.tera, 3)) }
            div.kv {
                span.label { "Common teammates" }
                div.mates {
                    @for t in &m.teammates {
                        @let in_ladder = meta.mons.iter().any(|x| &x.name == t);
                        @if in_ladder && !full && vs.slot(t).is_none() {
                            a.mate href=(vs.toggled(t).href()) title={ "Add " (t) " to the comparison" } { (mon_icon(dex, t)) (t) }
                        } @else {
                            span.mate.static { (mon_icon(dex, t)) (t) }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn share_list(label: &str, rows: &[Share], n: usize) -> Markup {
    html! {
        div.kv {
            span.label { (label) }
            ul {
                @for s in rows.iter().take(n) {
                    li {
                        span.fill style=(format!("width:{:.1}%", s.pct.min(100.0))) {}
                        span { (s.name) }
                        span.num { (format!("{:.1}%", s.pct)) }
                    }
                }
            }
        }
    }
}

// ---------- small pieces ----------

/// A Pokémon's mini sprite from Showdown's icon sheet. Decorative: the name always sits
/// beside it.
pub(crate) fn mon_icon(dex: &Dex, name: &str) -> Markup {
    let (x, y) = dex.icon_offset(name);
    html! {
        span.mon-icon aria-hidden="true" style=(format!("background-position:-{x}px -{y}px")) {}
    }
}

pub(crate) fn type_chips(entry: Option<&DexEntry>) -> Markup {
    html! {
        span.types {
            @for t in entry.map(|e| e.types.as_slice()).unwrap_or_default() {
                @let ink = if is_light_type(t) { "dark" } else { "light" };
                span class={ "ty ink-" (ink) } style=(format!("background:var(--ty-{})", t.to_lowercase())) { (t) }
            }
        }
    }
}

/// Whether a type's chip colour is light enough to need dark text.
pub(crate) fn is_light_type(t: &str) -> bool {
    LIGHT_TYPES.contains(&t)
}

pub(crate) fn delta(m: &Mon) -> Markup {
    match m.delta() {
        None => html! { span.delta.flat { "new" } },
        Some(d) if d.abs() < 0.05 => html! { span.delta.flat.num { "±0.0" } },
        Some(d) if d > 0.0 => html! { span.delta.up.num { "▲ " (format!("{:.1}", d)) } },
        Some(d) => html! { span.delta.down.num { "▼ " (format!("{:.1}", -d)) } },
    }
}

fn slot_style(vs: &ViewState, name: &str) -> String {
    vs.slot(name)
        .map(|s| format!("--slot:var({s})"))
        .unwrap_or_default()
}

pub(crate) fn stat(dex: &Dex, name: &str, i: usize) -> u16 {
    dex.entry(name).map_or(0, |e| e.stats[i])
}

pub(crate) fn bst(dex: &Dex, name: &str) -> u16 {
    dex.entry(name).map_or(0, DexEntry::bst)
}

pub(crate) fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// `2026-09` -> `September 2026`
pub(crate) fn month_name(ym: &str) -> String {
    match ym.split_once('-') {
        Some((y, m)) => match m.parse::<usize>() {
            Ok(m @ 1..=12) => format!("{} {y}", MONTHS[m - 1]),
            _ => ym.to_owned(),
        },
        None => ym.to_owned(),
    }
}

/// `2026-08` -> `Aug`
fn short_month(ym: &str) -> String {
    month_name(ym).chars().take(3).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_theme_cookie() {
        assert_eq!(Theme::from_cookie_header("theme=dark"), Theme::Dark);
        assert_eq!(
            Theme::from_cookie_header("a=1; theme=light; b=2"),
            Theme::Light
        );
        assert_eq!(Theme::from_cookie_header("theme=purple"), Theme::System);
        assert_eq!(Theme::from_cookie_header("mytheme=dark"), Theme::System);
        assert_eq!(Theme::from_cookie_header(""), Theme::System);
    }

    fn state() -> ViewState {
        ViewState {
            page: Page::Usage,
            fmt: "vgc".into(),
            cut: 1630,
            sort: SortKey::Rank,
            desc: false,
            q: String::new(),
            ty: String::new(),
            picks: vec!["Rillaboom".into()],
            team: vec![],
        }
    }

    #[test]
    fn team_links_keep_the_rest_of_the_state() {
        let v = state().team_toggled("Incineroar").on_page(Page::Builder);
        assert_eq!(
            v.href(),
            "/builder?fmt=vgc&cut=1630&sort=rank&dir=asc&pick=Rillaboom&team=Incineroar"
        );
        assert!(v.team_toggled("Incineroar").team.is_empty());
    }

    #[test]
    fn links_encode_names() {
        let v = state().toggled("Raichu-Mega-Y").toggled("Mr. Mime");
        assert_eq!(
            v.href(),
            "/?fmt=vgc&cut=1630&sort=rank&dir=asc&pick=Rillaboom%2CRaichu-Mega-Y%2CMr.+Mime"
        );
    }

    #[test]
    fn toggling_respects_the_limit() {
        let mut v = state();
        for n in ["A", "B", "C", "D"] {
            v = v.toggled(n);
        }
        assert_eq!(v.picks, ["Rillaboom", "A", "B", "C"]);
        assert_eq!(v.toggled("A").picks, ["Rillaboom", "B", "C"]);
    }

    #[test]
    fn sorting_flips_on_second_click() {
        let v = state().sorted_by(SortKey::Bst);
        assert!(v.desc);
        assert!(!v.sorted_by(SortKey::Bst).desc);
    }

    #[test]
    fn formats_numbers_and_months() {
        assert_eq!(thousands(1_631_943), "1,631,943");
        assert_eq!(thousands(980), "980");
        assert_eq!(month_name("2026-09"), "September 2026");
        assert_eq!(short_month("2026-08"), "Aug");
    }
}
