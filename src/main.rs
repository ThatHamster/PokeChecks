//! PokeChecks: Pokémon Champions ranked usage next to base stats, served as plain HTML.

mod api;
mod calc_view;
mod dex;
mod privacy;
mod saved;
mod smogon;
mod speed_view;
mod team_view;
mod tournament_view;
mod tournaments;
mod view;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::Router;
use axum::extract::{Form, FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::sync::{Mutex, OnceCell, RwLock};

use dex::{Dex, Sheets};
pub use pokechecks_core::{calc, gameplan, team};
use smogon::{Catalog, Meta};
use view::{Page, SortKey, Theme, ViewState};

/// How long to trust the list of months and regulations before asking Smogon again.
const CATALOG_TTL: Duration = Duration::from_secs(6 * 60 * 60);
/// How long to keep tournament teams before asking Limitless again.
const TOURNAMENT_TTL: Duration = Duration::from_secs(3 * 60 * 60);
/// How often to reload Showdown's Pokédex, and how soon to retry after a failed reload.
const DEX_REFRESH: Duration = Duration::from_secs(24 * 60 * 60);
const DEX_RETRY: Duration = Duration::from_secs(60 * 60);

struct AppState {
    http: reqwest::Client,
    /// Swapped for a fresh copy once a day by a background task.
    dex: RwLock<Arc<Dex>>,
    /// Reloaded along with the Dex.
    sheets: RwLock<Arc<Sheets>>,
    catalog: RwLock<Option<(Instant, Arc<Catalog>)>>,
    /// Held while the catalog downloads, so visitors arriving meanwhile wait for that
    /// download instead of starting their own.
    catalog_refresh: Mutex<()>,
    /// Parsed usage keyed by `month/stem-cutoff`. Only the current month is kept: older
    /// months are dropped when the catalog moves on. Each key's cell downloads once, however
    /// many visitors ask for it at the same moment.
    metas: Mutex<HashMap<String, Arc<OnceCell<Arc<Meta>>>>>,
    tournaments: RwLock<Option<(Instant, Arc<tournaments::Tournaments>)>>,
    tournaments_refresh: Mutex<()>,
}

impl AppState {
    async fn dex(&self) -> Arc<Dex> {
        self.dex.read().await.clone()
    }

    async fn catalog(&self) -> Result<Arc<Catalog>> {
        if let Some(cat) = self.fresh_catalog().await {
            return Ok(cat);
        }
        let _refreshing = self.catalog_refresh.lock().await;
        // Someone else may have refreshed it while this visitor waited.
        if let Some(cat) = self.fresh_catalog().await {
            return Ok(cat);
        }
        let fresh = Arc::new(smogon::load_catalog(&self.http).await?);
        *self.catalog.write().await = Some((Instant::now(), fresh.clone()));
        // A new month makes earlier months' usage unreachable, so free it.
        let prefix = format!("{}/", fresh.month);
        self.metas
            .lock()
            .await
            .retain(|key, _| key.starts_with(&prefix));
        Ok(fresh)
    }

    async fn fresh_catalog(&self) -> Option<Arc<Catalog>> {
        let cached = self.catalog.read().await;
        let (at, cat) = cached.as_ref()?;
        (at.elapsed() < CATALOG_TTL).then(|| cat.clone())
    }

    async fn meta(
        &self,
        catalog: &Catalog,
        format: &smogon::FormatInfo,
        cutoff: u32,
    ) -> Result<Arc<Meta>> {
        let key = format!("{}/{}-{cutoff}", catalog.month, format.stem);
        let cell = self
            .metas
            .lock()
            .await
            .entry(key.clone())
            .or_default()
            .clone();
        // The first visitor downloads; others wait for it. A failed download leaves the cell
        // empty, so the next visitor tries again.
        cell.get_or_try_init(|| async {
            tracing::info!(%key, "downloading usage stats");
            let dex = self.dex().await;
            let meta = smogon::load_meta(&self.http, &dex, catalog, format, cutoff).await?;
            Ok(Arc::new(meta))
        })
        .await
        .cloned()
    }

    /// Tournament teams for the newest regulation with events. `fallback` is Smogon's
    /// regulation, used if Limitless's format list is unavailable.
    async fn tournaments(&self, fallback: &str) -> Result<Arc<tournaments::Tournaments>> {
        if let Some(t) = self.fresh_tournaments().await {
            return Ok(t);
        }
        let _refreshing = self.tournaments_refresh.lock().await;
        if let Some(t) = self.fresh_tournaments().await {
            return Ok(t);
        }
        let fresh = Arc::new(tournaments::load_current(&self.http, fallback).await?);
        *self.tournaments.write().await = Some((Instant::now(), fresh.clone()));
        Ok(fresh)
    }

    async fn fresh_tournaments(&self) -> Option<Arc<tournaments::Tournaments>> {
        let cached = self.tournaments.read().await;
        let (at, t) = cached.as_ref()?;
        (at.elapsed() < TOURNAMENT_TTL).then(|| t.clone())
    }
}

/// Reloads Showdown's Pokédex once a day so new Pokémon, forms and Megas appear without a
/// restart. A failed reload keeps the current copy and tries again an hour later.
async fn refresh_dex(state: Arc<AppState>) {
    let mut wait = DEX_REFRESH;
    loop {
        tokio::time::sleep(wait).await;
        match dex::load(&state.http).await {
            Ok((dex, sheets)) => {
                *state.dex.write().await = Arc::new(dex);
                *state.sheets.write().await = Arc::new(sheets);
                wait = DEX_REFRESH;
            }
            Err(err) => {
                tracing::warn!("Pokédex reload failed, keeping the current copy: {err:#}");
                wait = DEX_RETRY;
            }
        }
    }
}

/// A query string, like axum's `Query`, but a link PokeChecks can't read (a setting given
/// twice, say `?add=X&add=Y` from a hand-edited link) gets a friendly page instead of a
/// plain-text error.
struct Q<T>(T);

impl<T, S> FromRequestParts<S> for Q<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Response> {
        match Query::<T>::from_request_parts(parts, state).await {
            Ok(Query(value)) => Ok(Q(value)),
            Err(err) => {
                let theme = Theme::from_cookie_header(cookie_header(&parts.headers));
                let repeated = repeated_setting(&err.body_text());
                // `/theme` only redirects, so it starts over at the usage page.
                let path = match parts.uri.path() {
                    "/theme" => "/",
                    p => p,
                };
                let page = view::bad_link_page(repeated.as_deref(), path, theme);
                Err((StatusCode::BAD_REQUEST, Html(page.into_string())).into_response())
            }
        }
    }
}

/// The setting named in serde's "duplicate field `add`" error, if that's what went wrong.
fn repeated_setting(error: &str) -> Option<String> {
    let rest = error.split("duplicate field `").nth(1)?;
    rest.split('`').next().map(str::to_owned)
}

/// Query string. Everything is a string so a blank or odd value falls back to a default
/// instead of failing the whole request.
#[derive(Deserialize, Default)]
struct Params {
    fmt: Option<String>,
    cut: Option<String>,
    sort: Option<String>,
    dir: Option<String>,
    q: Option<String>,
    #[serde(rename = "type")]
    ty: Option<String>,
    pick: Option<String>,
    team: Option<String>,
    /// Confirmation after saving or deleting a team: `saved`, `deleted`, `full`, `empty`.
    msg: Option<String>,
    // Set edit form (team builder): which slot, the button pressed, and the fields.
    edit: Option<String>,
    apply: Option<String>,
    item: Option<String>,
    ability: Option<String>,
    nature: Option<String>,
    preset: Option<String>,
    p0: Option<String>,
    p1: Option<String>,
    p2: Option<String>,
    p3: Option<String>,
    p4: Option<String>,
    p5: Option<String>,
    m1: Option<String>,
    m2: Option<String>,
    m3: Option<String>,
    m4: Option<String>,
}

/// The outcome of a set edit: the new set, or why it was refused plus what was entered.
type EditOutcome = Result<team::Custom, (String, team::Custom)>;

/// Applies the set edit form to its slot. `None` when the request isn't an edit.
fn edit_from_params(p: &Params, team: &[team::Member]) -> Option<(usize, EditOutcome)> {
    let slot: usize = p.edit.as_deref()?.parse().ok()?;
    team.get(slot)?;
    let non_empty = |v: &Option<String>| {
        v.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };

    let mut moves: Vec<String> = Vec::new();
    for m in [&p.m1, &p.m2, &p.m3, &p.m4]
        .into_iter()
        .filter_map(non_empty)
    {
        if !moves.contains(&m) {
            moves.push(m);
        }
    }
    let raw_points = [&p.p0, &p.p1, &p.p2, &p.p3, &p.p4, &p.p5];
    let mut points = [0u16; 6];
    let mut bad_number = None;
    for (i, raw) in raw_points.iter().enumerate() {
        match non_empty(raw).map(|v| v.parse::<u16>()) {
            None => {}
            Some(Ok(n)) => points[i] = n,
            Some(Err(_)) => bad_number = Some(team::STAT_LABELS[i]),
        }
    }
    let mut custom = team::Custom {
        item: non_empty(&p.item),
        ability: non_empty(&p.ability),
        nature: non_empty(&p.nature).unwrap_or_else(|| "Hardy".to_owned()),
        points,
        moves,
    };

    if p.apply.as_deref() == Some("preset") {
        match p.preset.as_deref().and_then(smogon::Spread::parse) {
            Some((nature, preset_points)) => {
                custom.nature = nature;
                custom.points = preset_points;
            }
            None => {
                return Some((
                    slot,
                    Err(("Pick a spread from the list first.".to_owned(), custom)),
                ));
            }
        }
    } else if let Some(stat) = bad_number {
        let msg = format!(
            "{stat} needs a whole number from 0 to {}.",
            team::MAX_POINTS_PER_STAT
        );
        return Some((slot, Err((msg, custom))));
    }
    Some((
        slot,
        custom
            .validate()
            .map(|()| custom.clone())
            .map_err(|msg| (msg, custom)),
    ))
}

/// Page-specific query fields, on top of the shared ones in `Params`.
enum Extra {
    None,
    Calc(Box<calc_view::CalcParams>),
    Speed(Box<speed_view::SpeedParams>),
    Tournaments(Box<tournament_view::FilterParams>),
}

async fn usage_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Q(p): Q<Params>,
) -> Response {
    respond(&state, Page::Usage, &headers, p, Extra::None).await
}

async fn builder_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Q(p): Q<Params>,
) -> Response {
    respond(&state, Page::Builder, &headers, p, Extra::None).await
}

/// The meta teams page was replaced by filterable tournament teams; old links go there.
async fn teams_moved(uri: axum::http::Uri) -> Redirect {
    match uri.query() {
        Some(q) => Redirect::permanent(&format!("/tournaments?{q}")),
        None => Redirect::permanent("/tournaments"),
    }
}

async fn tournaments_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Q(p): Q<Params>,
    Q(f): Q<tournament_view::FilterParams>,
) -> Response {
    let extra = Extra::Tournaments(Box::new(f));
    respond(&state, Page::Tournaments, &headers, p, extra).await
}

async fn calc_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Q(p): Q<Params>,
    Q(c): Q<calc_view::CalcParams>,
) -> Response {
    respond(&state, Page::Calc, &headers, p, Extra::Calc(Box::new(c))).await
}

async fn speed_page(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Q(p): Q<Params>,
    Q(s): Q<speed_view::SpeedParams>,
) -> Response {
    respond(&state, Page::Speed, &headers, p, Extra::Speed(Box::new(s))).await
}

async fn respond(
    state: &AppState,
    page: Page,
    headers: &HeaderMap,
    p: Params,
    extra: Extra,
) -> Response {
    let cookies = cookie_header(headers);
    let theme = Theme::from_cookie_header(cookies);
    let saved = saved::from_cookie_header(cookies);
    match render(state, page, p, extra, theme, &saved).await {
        Ok(response) => response,
        Err(err) => {
            tracing::error!("{err:#}");
            let page = view::error_page(&format!("{err:#}"), theme);
            (StatusCode::BAD_GATEWAY, Html(page.into_string())).into_response()
        }
    }
}

#[derive(Deserialize)]
struct ThemeParams {
    mode: Option<String>,
    back: Option<String>,
}

/// Stores the visitor's theme in a cookie, then returns them to the page they were on.
async fn set_theme(Q(p): Q<ThemeParams>) -> Response {
    let theme = p.mode.as_deref().and_then(Theme::parse).unwrap_or_default();
    let cookie = match theme {
        // "System" is the default, so clear the cookie rather than storing it.
        Theme::System => "theme=; Path=/; Max-Age=0; SameSite=Lax".to_owned(),
        _ => format!(
            "theme={}; Path=/; Max-Age=31536000; SameSite=Lax",
            theme.as_str()
        ),
    };
    let back = safe_back(p.back.as_deref());
    ([(header::SET_COOKIE, cookie)], Redirect::to(back)).into_response()
}

fn cookie_header(headers: &HeaderMap) -> &str {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

#[derive(Deserialize)]
struct SaveForm {
    name: Option<String>,
    fmt: Option<String>,
    team: Option<String>,
    back: Option<String>,
}

/// Longest team name kept; longer names are cut.
const TEAM_NAME_LIMIT: usize = 40;
/// Longest encoded team accepted. Six fully edited sets come to well under 1,000
/// characters, and browsers drop cookies over about 4 KB anyway.
const TEAM_LIMIT: usize = 2_000;

/// Saves the builder's team in a cookie, then returns to the page with a message.
async fn save_team(headers: HeaderMap, Form(f): Form<SaveForm>) -> Response {
    if !same_site(&headers) {
        return cross_site_refused();
    }
    let back = safe_back(f.back.as_deref());
    let team = f.team.unwrap_or_default();
    if team.trim().is_empty() {
        return Redirect::to(&with_message(back, "empty")).into_response();
    }
    if team.len() > TEAM_LIMIT {
        return Redirect::to(&with_message(back, "too-big")).into_response();
    }
    let fmt = f.fmt.unwrap_or_default();
    let name: String = f
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or("My team")
        .chars()
        .take(TEAM_NAME_LIMIT)
        .collect();
    let existing = saved::from_cookie_header(cookie_header(&headers));
    let Some(slot) = saved::slot_for(&existing, &name, &fmt) else {
        return Redirect::to(&with_message(back, "full")).into_response();
    };
    let team = saved::SavedTeam {
        slot,
        name,
        fmt,
        team,
    };
    (
        [(header::SET_COOKIE, team.set_cookie())],
        Redirect::to(&with_message(back, "saved")),
    )
        .into_response()
}

#[derive(Deserialize)]
struct DeleteForm {
    slot: Option<usize>,
    back: Option<String>,
}

async fn delete_team(headers: HeaderMap, Form(f): Form<DeleteForm>) -> Response {
    if !same_site(&headers) {
        return cross_site_refused();
    }
    let back = safe_back(f.back.as_deref());
    match f.slot.filter(|s| *s < saved::MAX_SAVED) {
        Some(slot) => (
            [(header::SET_COOKIE, saved::delete_cookie(slot))],
            Redirect::to(&with_message(back, "deleted")),
        )
            .into_response(),
        None => Redirect::to(back).into_response(),
    }
}

/// Whether a form post came from PokeChecks itself rather than another site's page, so other
/// sites can't add or delete a visitor's saved teams. Browsers send `Sec-Fetch-Site`; older
/// ones send `Origin`, compared with `Host`. Requests with neither (scripts, very old
/// browsers) can't come from another site's page, so they're allowed.
fn same_site(headers: &HeaderMap) -> bool {
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    if let Some(site) = get("sec-fetch-site") {
        return matches!(site, "same-origin" | "none");
    }
    match (get("origin"), get("host")) {
        (Some(origin), Some(host)) => origin
            .split_once("://")
            .is_some_and(|(_, origin_host)| origin_host.eq_ignore_ascii_case(host)),
        (Some(_), None) => false,
        (None, _) => true,
    }
}

fn cross_site_refused() -> Response {
    (
        StatusCode::FORBIDDEN,
        "Saved teams can only be changed from PokeChecks itself.",
    )
        .into_response()
}

/// Content Security Policy. The pages run no scripts, so none are allowed at all; styles,
/// images and fonts come from PokeChecks (style attributes position sprites and colour
/// bars); the only frame is Ko-fi's donation panel; and no other site may frame PokeChecks.
/// Showdown is allowed for images because `/sprites.png` falls back to its copy.
const CSP: &str = "default-src 'none'; style-src 'self' 'unsafe-inline'; \
    img-src 'self' https://play.pokemonshowdown.com; font-src 'self'; \
    frame-src https://ko-fi.com; form-action 'self'; base-uri 'none'; frame-ancestors 'none'";

/// Security headers on every response.
async fn security_headers(mut res: Response) -> Response {
    let headers = res.headers_mut();
    let set = [
        (header::CONTENT_SECURITY_POLICY, CSP),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::REFERRER_POLICY, "strict-origin-when-cross-origin"),
        // Browsers only honour this over HTTPS, so plain-HTTP local runs are unaffected.
        (header::STRICT_TRANSPORT_SECURITY, "max-age=31536000"),
    ];
    for (name, value) in set {
        headers.insert(name, HeaderValue::from_static(value));
    }
    res
}

/// The self-hosted fonts and their licence, built into the binary.
const FONTS: [(&str, &[u8]); 7] = [
    (
        "fredoka-latin.woff2",
        include_bytes!("../assets/fonts/fredoka-latin.woff2"),
    ),
    (
        "fredoka-latin-ext.woff2",
        include_bytes!("../assets/fonts/fredoka-latin-ext.woff2"),
    ),
    (
        "nunito-latin.woff2",
        include_bytes!("../assets/fonts/nunito-latin.woff2"),
    ),
    (
        "nunito-latin-ext.woff2",
        include_bytes!("../assets/fonts/nunito-latin-ext.woff2"),
    ),
    (
        "jetbrains-mono-latin.woff2",
        include_bytes!("../assets/fonts/jetbrains-mono-latin.woff2"),
    ),
    (
        "jetbrains-mono-latin-ext.woff2",
        include_bytes!("../assets/fonts/jetbrains-mono-latin-ext.woff2"),
    ),
    ("OFL.txt", include_bytes!("../assets/fonts/OFL.txt")),
];

async fn font(Path(file): Path<String>) -> Response {
    let Some((_, bytes)) = FONTS.iter().find(|(name, _)| *name == file) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let kind = if file.ends_with(".txt") {
        "text/plain; charset=utf-8"
    } else {
        "font/woff2"
    };
    (
        [
            (header::CONTENT_TYPE, kind),
            (header::CACHE_CONTROL, "public, max-age=2592000"),
        ],
        *bytes,
    )
        .into_response()
}

/// Adds `msg=…` to an on-site path so the page can confirm what happened.
fn with_message(back: &str, msg: &str) -> String {
    let sep = if back.contains('?') { '&' } else { '?' };
    format!("{back}{sep}msg={msg}")
}

/// Only redirect to a path on this site, never to another host.
fn safe_back(back: Option<&str>) -> &str {
    match back {
        Some(b)
            if b.starts_with('/')
                && !b.starts_with("//")
                && !b.contains('\\')
                && !b.chars().any(char::is_control) =>
        {
            b
        }
        _ => "/",
    }
}

async fn render(
    state: &AppState,
    page: Page,
    p: Params,
    extra: Extra,
    theme: Theme,
    saved: &[saved::SavedTeam],
) -> Result<Response> {
    let catalog = state.catalog().await?;
    let format = catalog.format(p.fmt.as_deref());
    let cut = p
        .cut
        .as_deref()
        .and_then(|c| c.parse().ok())
        .filter(|c| format.cutoffs.contains(c))
        .unwrap_or_else(|| format.default_cutoff());
    let meta = state.meta(&catalog, format, cut).await?;

    let sort = p
        .sort
        .as_deref()
        .and_then(SortKey::parse)
        .unwrap_or(SortKey::Rank);
    let desc = match p.dir.as_deref() {
        Some("desc") => true,
        Some("asc") => false,
        _ => sort != SortKey::Rank && sort != SortKey::Name,
    };

    // No `pick` at all means a fresh visit: start with the top three. An explicit empty
    // `pick` means the visitor cleared the comparison, so respect that.
    let in_ladder = |n: &str| meta.mons.iter().any(|m| m.name == n);
    let picks: Vec<String> = match &p.pick {
        None => meta.mons.iter().take(3).map(|m| m.name.clone()).collect(),
        Some(list) => {
            let mut seen = Vec::new();
            for n in list
                .split(',')
                .map(str::trim)
                .filter(|n| !n.is_empty() && in_ladder(n))
            {
                if !seen.iter().any(|s| s == n) && seen.len() < view::MAX_PICKS {
                    seen.push(n.to_owned());
                }
            }
            // Picks from another format that aren't in this ladder: fall back to the top three.
            if seen.is_empty() && !list.trim().is_empty() {
                meta.mons.iter().take(3).map(|m| m.name.clone()).collect()
            } else {
                seen
            }
        }
    };

    let dex_handle = state.dex().await;
    let dex = dex_handle.as_ref();
    let mut team = parse_team(p.team.as_deref(), &meta, dex);
    let edit = if page == Page::Builder {
        edit_from_params(&p, &team)
    } else {
        None
    };

    let mut vs = ViewState {
        page,
        fmt: format.kind.key().to_owned(),
        cut: meta.cutoff,
        sort,
        desc,
        q: p.q.clone().unwrap_or_default(),
        ty: p.ty.clone().unwrap_or_default(),
        picks,
        team: Vec::new(),
    };

    let mut edit_error = None;
    match edit {
        // A good edit goes back to the builder's clean URL so reloading doesn't resubmit it.
        Some((slot, Ok(custom))) => {
            team[slot].custom = Some(custom);
            vs.team = team;
            return Ok(Redirect::to(&vs.href()).into_response());
        }
        Some((slot, Err((message, draft)))) => {
            edit_error = Some(team_view::EditError {
                slot,
                message,
                draft,
            });
        }
        None => {}
    }
    vs.team = team;

    let html = match page {
        Page::Usage => view::page(&catalog, format, &meta, dex, &vs, theme),
        Page::Builder => team_view::builder_page(
            &catalog,
            format,
            &meta,
            dex,
            &vs,
            theme,
            edit_error.as_ref(),
            saved,
            p.msg.as_deref(),
        ),
        Page::Tournaments => {
            let filters = match extra {
                Extra::Tournaments(f) => *f,
                _ => tournament_view::FilterParams::default(),
            };
            let loaded;
            let source = if format.kind == smogon::Kind::Bss {
                tournament_view::Source::SinglesFormat
            } else {
                match state.tournaments(&format.regulation).await {
                    Ok(t) => {
                        loaded = t;
                        tournament_view::Source::Events(&loaded, &filters)
                    }
                    Err(err) => {
                        tracing::warn!("tournament teams unavailable: {err:#}");
                        tournament_view::Source::Unavailable(format!("{err:#}"))
                    }
                }
            };
            tournament_view::tournaments_page(&catalog, format, &meta, dex, &vs, theme, source)
        }
        Page::Calc => {
            let params = match extra {
                Extra::Calc(c) => *c,
                _ => calc_view::CalcParams::default(),
            };
            let calc = calc_view::CalcState::from_params(&params, &meta, dex);
            calc_view::calc_page(&catalog, format, &meta, dex, &vs, theme, &calc)
        }
        Page::Speed => {
            let params = match extra {
                Extra::Speed(s) => *s,
                _ => speed_view::SpeedParams::default(),
            };
            if params.is_meta_mode() {
                match speed_view::SpeedState::from_params(&params, &meta) {
                    Some(speed) => {
                        speed_view::speed_page(&catalog, format, &meta, dex, &vs, theme, &speed)
                    }
                    None => view::error_page("This format has no usage data yet.", theme),
                }
            } else {
                let state = speed_view::match_from_params(&params, &meta, vs.team.len());
                speed_view::match_page(&catalog, format, &meta, dex, &vs, theme, &state, saved)
            }
        }
    };
    Ok(Html(html.into_string()).into_response())
}

/// Keeps team members that are in this format's stats, dropping repeats, a second form of
/// the same species, and anything past six.
fn parse_team(list: Option<&str>, meta: &Meta, dex: &Dex) -> Vec<team::Member> {
    let mut mons: Vec<&smogon::Mon> = Vec::new();
    let mut members = Vec::new();
    for token in list
        .unwrap_or("")
        .split(',')
        .filter(|t| !t.trim().is_empty())
    {
        let member = team::Member::decode(token);
        if let Some(m) = meta.mons.iter().find(|m| m.name == member.name)
            && team::blocked(&mons, &member.name, dex).is_none()
        {
            mons.push(m);
            members.push(member);
        }
    }
    members
}

async fn stylesheet() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        view::CSS,
    )
}

/// The Pokémon icon sheet. Falls back to Showdown's copy if the download failed at startup.
async fn sprites(State(state): State<Arc<AppState>>) -> Response {
    sheet(
        state.sheets.read().await.icons.clone(),
        dex::SHOWDOWN_ICON_SHEET,
    )
}

/// The item icon sheet, with the same fallback.
async fn item_sprites(State(state): State<Arc<AppState>>) -> Response {
    sheet(
        state.sheets.read().await.items.clone(),
        dex::SHOWDOWN_ITEM_SHEET,
    )
}

fn sheet(png: Option<axum::body::Bytes>, fallback: &'static str) -> Response {
    match png {
        Some(png) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            png,
        )
            .into_response(),
        None => Redirect::temporary(fallback).into_response(),
    }
}

/// The privacy policy, which the app stores link to.
async fn privacy_page(headers: HeaderMap) -> Html<String> {
    let theme = Theme::from_cookie_header(cookie_header(&headers));
    Html(privacy::privacy_page(theme).into_string())
}

/// AdMob's authorized sellers file, once `APP_ADS_TXT` is set in the host's settings.
async fn app_ads_txt() -> Response {
    match privacy::app_ads_txt() {
        Some(txt) => ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], txt).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// For the host's health checks. Doesn't touch Smogon, so a slow download can't fail it.
async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let http = reqwest::Client::builder()
        .user_agent(concat!("PokeChecks/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(180))
        .build()?;
    let (dex, sheets) = dex::load(&http)
        .await
        .context("loading Pokémon Showdown dex data")?;
    let state = Arc::new(AppState {
        http,
        dex: RwLock::new(Arc::new(dex)),
        sheets: RwLock::new(Arc::new(sheets)),
        catalog: RwLock::new(None),
        catalog_refresh: Mutex::new(()),
        metas: Mutex::new(HashMap::new()),
        tournaments: RwLock::new(None),
        tournaments_refresh: Mutex::new(()),
    });
    tokio::spawn(refresh_dex(state.clone()));

    // Warm the default view so the first visitor doesn't wait on the download.
    let warm = state.clone();
    tokio::spawn(async move {
        let result = async {
            let catalog = warm.catalog().await?;
            let format = catalog.format(None);
            warm.meta(&catalog, format, format.default_cutoff()).await
        }
        .await;
        if let Err(err) = result {
            tracing::warn!("could not preload usage stats: {err:#}");
        }
        // Tournament teams take a few seconds to gather from Limitless.
        if let Ok(catalog) = warm.catalog().await
            && let Err(err) = warm.tournaments(&catalog.format(None).regulation).await
        {
            tracing::warn!("could not preload tournament teams: {err:#}");
        }
    });

    let app = Router::new()
        .route("/", get(usage_page))
        .route("/builder", get(builder_page))
        .route("/teams", get(teams_moved))
        .route("/tournaments", get(tournaments_page))
        .route("/calc", get(calc_page))
        .route("/speed", get(speed_page))
        .route("/app.css", get(stylesheet))
        .route("/sprites.png", get(sprites))
        .route("/item-sprites.png", get(item_sprites))
        .route("/fonts/{file}", get(font))
        .route("/healthz", get(health))
        .route("/privacy", get(privacy_page))
        .route("/app-ads.txt", get(app_ads_txt))
        .route("/theme", get(set_theme))
        .route("/saved/save", post(save_team))
        .route("/saved/delete", post(delete_team))
        .with_state(state.clone())
        .merge(api::router(state))
        .layer(middleware::map_response(security_headers));

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    // Local runs only accept this computer; hosts set HOST=0.0.0.0 to accept the internet.
    let host: std::net::IpAddr = std::env::var("HOST")
        .ok()
        .and_then(|h| h.parse().ok())
        .unwrap_or([127, 0, 0, 1].into());
    let addr = std::net::SocketAddr::new(host, port);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!("PokeChecks is running at http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, header};
    use axum::response::{IntoResponse, Response};

    use super::{FONTS, Q, repeated_setting, safe_back, same_site, security_headers};

    #[derive(serde::Deserialize)]
    struct TestParams {
        #[allow(dead_code)]
        add: Option<String>,
    }

    /// Runs a URL through the query extractor; on failure, returns the status and page.
    async fn extract(uri: &str) -> Result<(), (axum::http::StatusCode, String)> {
        use axum::extract::FromRequestParts;
        let (mut parts, ()) = axum::http::Request::builder()
            .uri(uri)
            .body(())
            .unwrap()
            .into_parts();
        match Q::<TestParams>::from_request_parts(&mut parts, &()).await {
            Ok(_) => Ok(()),
            Err(res) => {
                let status = res.status();
                let body = axum::body::to_bytes(res.into_body(), 1 << 20)
                    .await
                    .unwrap();
                Err((status, String::from_utf8(body.to_vec()).unwrap()))
            }
        }
    }

    #[tokio::test]
    async fn repeated_settings_get_a_friendly_page() {
        assert!(extract("/speed?add=Kingambit").await.is_ok());
        let (status, page) = extract("/speed?fmt=vgc&add=Belli&add=Medic")
            .await
            .unwrap_err();
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
        assert!(page.contains("This link doesn’t look quite right"));
        assert!(page.contains("It sets “add” more than once"));
        assert!(
            page.contains(r#"href="/speed""#),
            "start over on the same page"
        );
        let (_, theme_page) = extract("/theme?add=a&add=b").await.unwrap_err();
        assert!(theme_page.contains(r#"class="go" href="/""#));
    }

    #[test]
    fn names_the_repeated_setting() {
        let msg = "Failed to deserialize query string: duplicate field `add`";
        assert_eq!(repeated_setting(msg).as_deref(), Some("add"));
        assert_eq!(
            repeated_setting("Failed to deserialize query string: invalid type"),
            None
        );
    }

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (name, value) in pairs {
            h.insert(*name, HeaderValue::from_static(value));
        }
        h
    }

    #[test]
    fn saved_teams_only_change_from_this_site() {
        assert!(same_site(&headers(&[("sec-fetch-site", "same-origin")])));
        assert!(same_site(&headers(&[("sec-fetch-site", "none")])));
        assert!(!same_site(&headers(&[("sec-fetch-site", "cross-site")])));
        assert!(!same_site(&headers(&[("sec-fetch-site", "same-site")])));
        let ours = [
            ("origin", "https://pokechecks.onrender.com"),
            ("host", "pokechecks.onrender.com"),
        ];
        assert!(same_site(&headers(&ours)));
        let theirs = [
            ("origin", "https://evil.example"),
            ("host", "pokechecks.onrender.com"),
        ];
        assert!(!same_site(&headers(&theirs)));
        assert!(!same_site(&headers(&[
            ("origin", "null"),
            ("host", "pokechecks.onrender.com")
        ])));
        assert!(same_site(&headers(&[("host", "127.0.0.1:3000")])));
    }

    #[tokio::test]
    async fn every_response_gets_security_headers() {
        let res: Response = security_headers("ok".into_response()).await;
        let csp = res.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap();
        assert!(csp.contains("default-src 'none'"));
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(!csp.contains("script-src"), "pages run no scripts");
        assert_eq!(res.headers()[header::X_FRAME_OPTIONS], "DENY");
        assert_eq!(res.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert!(
            res.headers()
                .contains_key(header::STRICT_TRANSPORT_SECURITY)
        );
    }

    #[test]
    fn every_font_in_the_stylesheet_is_served() {
        let css = crate::view::CSS;
        let used: Vec<&str> = css
            .split("url(\"/fonts/")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .collect();
        assert_eq!(used.len(), 6);
        for file in used {
            assert!(
                FONTS.iter().any(|(name, _)| *name == file),
                "{file} isn't served"
            );
        }
    }

    #[test]
    fn theme_redirect_stays_on_site() {
        assert_eq!(
            safe_back(Some("/?fmt=bss&pick=Garchomp")),
            "/?fmt=bss&pick=Garchomp"
        );
        assert_eq!(safe_back(Some("//evil.example")), "/");
        assert_eq!(safe_back(Some("/\\evil.example")), "/");
        assert_eq!(safe_back(Some("https://evil.example")), "/");
        assert_eq!(safe_back(Some("/\r\nSet-Cookie: x")), "/");
        assert_eq!(safe_back(None), "/");
    }
}
