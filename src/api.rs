//! The JSON API the mobile app downloads its data from, under `/api/v1`. The app keeps a
//! copy of everything here so it works offline, and checks back with `If-None-Match` to
//! download only what changed.
//!
//! - `GET /api/v1/manifest`: the stats month and the formats with their rating cutoffs.
//! - `GET /api/v1/dex`: Showdown's Pokédex, moves, names and icon positions.
//! - `GET /api/v1/usage/{format}?cutoff=1630`: ranked usage with common sets.
//! - `GET /api/v1/tournaments`: top teams from recent large events.
//! - `GET /api/v1/sprites/pokemon.png` and `/items.png`: the icon sheets.
//!
//! Everything is public and read-only, so any origin may read it.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use serde::{Deserialize, Serialize};
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;

use crate::AppState;
use crate::smogon::{FormatInfo, Kind};

/// Bumped when a response changes shape in a way older apps can't read.
const SCHEMA: u32 = 1;

pub fn router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET])
        .allow_headers([header::IF_NONE_MATCH])
        .expose_headers([header::ETAG]);
    Router::new()
        .route("/api/v1/manifest", get(manifest))
        .route("/api/v1/dex", get(dex))
        .route("/api/v1/usage/{format}", get(usage))
        .route("/api/v1/tournaments", get(tournaments))
        .route("/api/v1/sprites/pokemon.png", get(pokemon_sheet))
        .route("/api/v1/sprites/items.png", get(item_sheet))
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(cors)
}

#[derive(Serialize)]
struct Manifest<'a> {
    schema: u32,
    /// The month the battles were played, e.g. `2026-09`.
    month: &'a str,
    formats: Vec<Format<'a>>,
}

#[derive(Serialize)]
struct Format<'a> {
    key: &'static str,
    label: &'static str,
    /// `Doubles` or `Singles`.
    style: &'static str,
    regulation: &'a str,
    cutoffs: &'a [u32],
    default_cutoff: u32,
}

impl<'a> Format<'a> {
    fn new(f: &'a FormatInfo) -> Self {
        Self {
            key: f.kind.key(),
            label: f.kind.label(),
            style: f.kind.style(),
            regulation: &f.regulation,
            cutoffs: &f.cutoffs,
            default_cutoff: f.default_cutoff(),
        }
    }
}

async fn manifest(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let catalog = match state.catalog().await {
        Ok(c) => c,
        Err(err) => return upstream_error("usage stats", &err),
    };
    json(
        &headers,
        &Manifest {
            schema: SCHEMA,
            month: &catalog.month,
            formats: catalog.formats.iter().map(Format::new).collect(),
        },
    )
}

async fn dex(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    json(&headers, state.dex().await.as_ref())
}

#[derive(Deserialize)]
struct UsageQuery {
    cutoff: Option<u32>,
}

#[derive(Serialize)]
struct Usage<'a> {
    month: &'a str,
    format: &'static str,
    #[serde(flatten)]
    meta: &'a crate::smogon::Meta,
}

async fn usage(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    Query(q): Query<UsageQuery>,
    headers: HeaderMap,
) -> Response {
    let catalog = match state.catalog().await {
        Ok(c) => c,
        Err(err) => return upstream_error("usage stats", &err),
    };
    let Some(format) = catalog.formats.iter().find(|f| f.kind.key() == key) else {
        let known: Vec<&str> = catalog.formats.iter().map(|f| f.kind.key()).collect();
        return error(
            StatusCode::NOT_FOUND,
            &format!("No format “{key}”. Try one of: {}.", known.join(", ")),
        );
    };
    let cutoff = q.cutoff.unwrap_or_else(|| format.default_cutoff());
    if !format.cutoffs.contains(&cutoff) {
        return error(
            StatusCode::NOT_FOUND,
            &format!(
                "{} has no {cutoff} cutoff. Try one of: {:?}.",
                format.kind.label(),
                format.cutoffs
            ),
        );
    }
    match state.meta(&catalog, format, cutoff).await {
        Ok(meta) => json(
            &headers,
            &Usage {
                month: &catalog.month,
                format: format.kind.key(),
                meta: &meta,
            },
        ),
        Err(err) => upstream_error("usage stats", &err),
    }
}

async fn tournaments(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let fallback = match state.catalog().await {
        Ok(c) => c.format(Some(Kind::Vgc.key())).regulation.clone(),
        Err(err) => return upstream_error("usage stats", &err),
    };
    match state.tournaments(&fallback).await {
        Ok(t) => json(&headers, t.as_ref()),
        Err(err) => upstream_error("tournament teams", &err),
    }
}

/// The icon sheets, with ETags like the JSON so the app downloads them only when they
/// change. If a sheet couldn't be downloaded at startup, this points at Showdown's copy.
async fn pokemon_sheet(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let png = state.sheets.read().await.icons.clone();
    sheet(&headers, png, crate::dex::SHOWDOWN_ICON_SHEET)
}

async fn item_sheet(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let png = state.sheets.read().await.items.clone();
    sheet(&headers, png, crate::dex::SHOWDOWN_ITEM_SHEET)
}

fn sheet(request: &HeaderMap, png: Option<Bytes>, fallback: &'static str) -> Response {
    match png {
        Some(png) => tagged(request, png, "image/png"),
        None => Redirect::temporary(fallback).into_response(),
    }
}

fn json(request: &HeaderMap, value: &impl Serialize) -> Response {
    match serde_json::to_vec(value) {
        Ok(body) => tagged(request, body, "application/json"),
        Err(err) => {
            tracing::error!("couldn't serialize API response: {err}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong.")
        }
    }
}

/// Sends `body` with an ETag, or answers 304 Not Modified when the app already has this
/// exact copy. The app keeps its own copy, so `no-cache` makes every request check in.
fn tagged<B: AsRef<[u8]> + IntoResponse>(
    request: &HeaderMap,
    body: B,
    content_type: &'static str,
) -> Response {
    let etag = etag(body.as_ref());
    let headers = [
        (header::ETAG, etag.clone()),
        (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
    ];
    let unchanged = request
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|t| t.trim() == etag));
    if unchanged {
        return (StatusCode::NOT_MODIFIED, headers).into_response();
    }
    (
        headers,
        [(header::CONTENT_TYPE, HeaderValue::from_static(content_type))],
        body,
    )
        .into_response()
}

/// A weak ETag from the body's hash. It only has to tell copies apart within one running
/// server; a redeploy may change it, which costs the app one extra download.
fn etag(body: &[u8]) -> HeaderValue {
    let mut h = std::hash::DefaultHasher::new();
    body.hash(&mut h);
    HeaderValue::from_str(&format!("W/\"{:016x}\"", h.finish())).expect("valid header")
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, axum::Json(serde_json::json!({ "error": message }))).into_response()
}

fn upstream_error(what: &str, err: &anyhow::Error) -> Response {
    tracing::warn!("API couldn't load {what}: {err:#}");
    error(
        StatusCode::BAD_GATEWAY,
        &format!("Couldn't load the {what} right now. Try again in a minute."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_copies_get_304() {
        let value = serde_json::json!({ "a": 1 });
        let first = json(&HeaderMap::new(), &value);
        assert_eq!(first.status(), StatusCode::OK);
        let tag = first.headers()[header::ETAG].clone();

        let mut again = HeaderMap::new();
        again.insert(header::IF_NONE_MATCH, tag.clone());
        let second = json(&again, &value);
        assert_eq!(second.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(second.headers()[header::ETAG], tag);

        let mut stale = HeaderMap::new();
        stale.insert(header::IF_NONE_MATCH, HeaderValue::from_static("W/\"0\""));
        assert_eq!(json(&stale, &value).status(), StatusCode::OK);
        let changed = json(&again, &serde_json::json!({ "a": 2 }));
        assert_eq!(changed.status(), StatusCode::OK);
    }

    #[test]
    fn sheets_get_304_too() {
        let png = Bytes::from_static(b"not really a PNG");
        let first = sheet(
            &HeaderMap::new(),
            Some(png.clone()),
            "https://example.com/x.png",
        );
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(first.headers()[header::CONTENT_TYPE], "image/png");
        let mut again = HeaderMap::new();
        again.insert(header::IF_NONE_MATCH, first.headers()[header::ETAG].clone());
        assert_eq!(
            sheet(&again, Some(png), "https://example.com/x.png").status(),
            StatusCode::NOT_MODIFIED
        );
        let missing = sheet(&HeaderMap::new(), None, "https://example.com/x.png");
        assert_eq!(missing.status(), StatusCode::TEMPORARY_REDIRECT);
    }

    #[test]
    fn format_keys_match_the_website() {
        for kind in [Kind::Vgc, Kind::VgcBo3, Kind::Bss] {
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::Value::from(kind.key())
            );
        }
    }
}
