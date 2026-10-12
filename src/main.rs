//! PokeChecks: Pokémon Champions ranked usage next to base stats, served as plain HTML.

mod dex;
mod smogon;
mod view;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::get;
use serde::Deserialize;
use tokio::sync::{Mutex, RwLock};

use dex::Dex;
use smogon::{Catalog, Meta};
use view::{SortKey, Theme, ViewState};

/// How long to trust the list of months and regulations before asking Smogon again.
const CATALOG_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const CSS: &str = include_str!("../assets/app.css");

struct AppState {
    http: reqwest::Client,
    dex: Dex,
    catalog: RwLock<Option<(Instant, Arc<Catalog>)>>,
    /// Parsed usage keyed by `month/stem-cutoff`. Published months never change, so entries never expire.
    metas: Mutex<HashMap<String, Arc<Meta>>>,
}

impl AppState {
    async fn catalog(&self) -> Result<Arc<Catalog>> {
        if let Some((at, cat)) = self.catalog.read().await.as_ref()
            && at.elapsed() < CATALOG_TTL
        {
            return Ok(cat.clone());
        }
        let fresh = Arc::new(smogon::load_catalog(&self.http).await?);
        *self.catalog.write().await = Some((Instant::now(), fresh.clone()));
        Ok(fresh)
    }

    async fn meta(
        &self,
        catalog: &Catalog,
        format: &smogon::FormatInfo,
        cutoff: u32,
    ) -> Result<Arc<Meta>> {
        let key = format!("{}/{}-{cutoff}", catalog.month, format.stem);
        if let Some(m) = self.metas.lock().await.get(&key) {
            return Ok(m.clone());
        }
        tracing::info!(%key, "downloading usage stats");
        let meta =
            Arc::new(smogon::load_meta(&self.http, &self.dex, catalog, format, cutoff).await?);
        self.metas.lock().await.insert(key, meta.clone());
        Ok(meta)
    }
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
}

async fn index(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(p): Query<Params>,
) -> Response {
    let theme = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(Theme::from_cookie_header)
        .unwrap_or_default();
    match render_index(&state, p, theme).await {
        Ok(html) => Html(html).into_response(),
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
async fn set_theme(Query(p): Query<ThemeParams>) -> Response {
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

async fn render_index(state: &AppState, p: Params, theme: Theme) -> Result<String> {
    let catalog = state.catalog().await?;
    let format = catalog.format(p.fmt.as_deref());
    let cut = p
        .cut
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

    let vs = ViewState {
        fmt: format.kind.key().to_owned(),
        cut: meta.cutoff,
        sort,
        desc,
        q: p.q.unwrap_or_default(),
        ty: p.ty.unwrap_or_default(),
        picks,
    };
    Ok(view::page(&catalog, format, &meta, &state.dex, &vs, theme).into_string())
}

async fn stylesheet() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        CSS,
    )
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let http = reqwest::Client::builder()
        .user_agent(concat!("PokeChecks/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(180))
        .build()?;
    let dex = Dex::load(&http)
        .await
        .context("loading Pokémon Showdown dex data")?;
    let state = Arc::new(AppState {
        http,
        dex,
        catalog: RwLock::new(None),
        metas: Mutex::new(HashMap::new()),
    });

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
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/app.css", get(stylesheet))
        .route("/theme", get(set_theme))
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
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
    use super::safe_back;

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
