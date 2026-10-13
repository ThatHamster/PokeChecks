//! Finds the newest Pokémon Champions regulation in Smogon's monthly usage stats and
//! downloads its "chaos" files. Parsing lives in `pokechecks-core`, re-exported here.

use anyhow::{Context, Result, bail};

pub use pokechecks_core::smogon::*;

use crate::dex::Dex;

const STATS_ROOT: &str = "https://www.smogon.com/stats/";

pub async fn load_catalog(http: &reqwest::Client) -> Result<Catalog> {
    let months = parse_months(&get_text(http, STATS_ROOT).await?);
    // The newest month directory can exist before every format is uploaded, so look back a little.
    for (i, month) in months.iter().enumerate().rev().take(3) {
        let listing = get_text(http, &format!("{STATS_ROOT}{month}/chaos/")).await?;
        let formats = parse_champions(&listing);
        if !formats.is_empty() {
            tracing::info!(
                month,
                regulation = formats[0].regulation,
                "found Champions stats"
            );
            return Ok(Catalog {
                month: month.clone(),
                prev_month: i.checked_sub(1).map(|p| months[p].clone()),
                formats,
            });
        }
    }
    bail!("Smogon's last three months of stats have no Pokémon Champions regulation formats")
}

pub async fn load_meta(
    http: &reqwest::Client,
    dex: &Dex,
    catalog: &Catalog,
    format: &FormatInfo,
    cutoff: u32,
) -> Result<Meta> {
    let file = format!("{}-{cutoff}.json", format.stem);
    let current_url = format!("{STATS_ROOT}{}/chaos/{file}", catalog.month);
    let prev_url = catalog
        .prev_month
        .as_ref()
        .map(|m| format!("{STATS_ROOT}{m}/chaos/{file}"));

    let (current, prev) = tokio::join!(get_chaos(http, &current_url), async {
        match &prev_url {
            // A regulation that started this month has no file last month; that is expected.
            Some(url) => get_chaos(http, url).await.ok(),
            None => None,
        }
    });
    let current = current?;
    Ok(build_meta(dex, current, prev.as_ref()))
}

async fn get_text(http: &reqwest::Client, url: &str) -> Result<String> {
    http.get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .with_context(|| format!("downloading {url}"))?
        .text()
        .await
        .with_context(|| format!("reading {url}"))
}

async fn get_chaos(http: &reqwest::Client, url: &str) -> Result<Chaos> {
    http.get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .with_context(|| format!("downloading {url}"))?
        .json()
        .await
        .with_context(|| format!("parsing {url}"))
}
