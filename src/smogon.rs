//! Finds the newest Pokémon Champions regulation in Smogon's monthly usage stats
//! and turns its "chaos" JSON into ranked usage with common sets.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::Deserialize;

use crate::dex::Dex;

const STATS_ROOT: &str = "https://www.smogon.com/stats/";
/// How many Pokémon the usage ladder lists per format.
const LADDER_SIZE: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Vgc,
    VgcBo3,
    Bss,
}

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Kind::Vgc => "vgc",
            Kind::VgcBo3 => "vgc-bo3",
            Kind::Bss => "bss",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Vgc => "VGC",
            Kind::VgcBo3 => "VGC Best of 3",
            Kind::Bss => "Battle Stadium Singles",
        }
    }

    pub fn style(self) -> &'static str {
        match self {
            Kind::Vgc | Kind::VgcBo3 => "Doubles",
            Kind::Bss => "Singles",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FormatInfo {
    pub kind: Kind,
    /// File name without the rating suffix, e.g. `gen9championsvgc2026regmc`.
    pub stem: String,
    /// Display form of the regulation, e.g. `M-C`.
    pub regulation: String,
    pub cutoffs: Vec<u32>,
}

impl FormatInfo {
    /// 1630 is the middle ladder band Smogon publishes for these formats.
    pub fn default_cutoff(&self) -> u32 {
        if self.cutoffs.contains(&1630) {
            1630
        } else {
            self.cutoffs
                .iter()
                .rev()
                .nth(1)
                .or(self.cutoffs.last())
                .copied()
                .unwrap_or(0)
        }
    }
}

pub struct Catalog {
    pub month: String,
    pub prev_month: Option<String>,
    pub formats: Vec<FormatInfo>,
}

impl Catalog {
    pub fn format(&self, key: Option<&str>) -> &FormatInfo {
        key.and_then(|k| self.formats.iter().find(|f| f.kind.key() == k))
            .unwrap_or(&self.formats[0])
    }
}

pub struct Share {
    pub name: String,
    pub pct: f64,
}

pub struct Mon {
    pub name: String,
    pub rank: usize,
    /// Weighted usage, in percent.
    pub usage: f64,
    /// Usage in the previous month, when the same format existed then.
    pub prev: Option<f64>,
    pub abilities: Vec<Share>,
    pub items: Vec<Share>,
    /// Share of sets that carry the move.
    pub moves: Vec<Share>,
    pub tera: Vec<Share>,
    pub teammates: Vec<String>,
    /// The most common nature and stat point spread.
    pub spread: Option<Spread>,
    /// For each other Pokémon on the ladder, the share of this Pokémon's teams that include it.
    pub pair_rates: HashMap<String, f64>,
}

/// A nature plus stat points (0–32 each) in HP/Atk/Def/SpA/SpD/Spe order.
pub struct Spread {
    pub nature: String,
    pub points: [u16; 6],
    /// Share of sets using exactly this spread.
    pub pct: f64,
}

impl Spread {
    /// Parses Smogon's `Adamant:32/32/0/0/2/0`.
    fn parse(s: &str) -> Option<(String, [u16; 6])> {
        let (nature, rest) = s.split_once(':')?;
        let mut points = [0u16; 6];
        let mut parts = rest.split('/');
        for p in &mut points {
            *p = parts.next()?.parse().ok()?;
        }
        Some((nature.to_owned(), points))
    }
}

impl Mon {
    /// Change from last month in percentage points.
    pub fn delta(&self) -> Option<f64> {
        self.prev.map(|p| self.usage - p)
    }
}

pub struct Meta {
    pub battles: u64,
    pub cutoff: u32,
    pub mons: Vec<Mon>,
}

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

// ---------- Smogon file formats ----------

#[derive(Deserialize)]
struct Chaos {
    info: Info,
    data: HashMap<String, RawMon>,
}

#[derive(Deserialize)]
struct Info {
    #[serde(rename = "number of battles")]
    battles: u64,
    cutoff: f64,
}

#[derive(Deserialize)]
struct RawMon {
    usage: f64,
    #[serde(rename = "Abilities", default)]
    abilities: HashMap<String, f64>,
    #[serde(rename = "Items", default)]
    items: HashMap<String, f64>,
    #[serde(rename = "Moves", default)]
    moves: HashMap<String, f64>,
    #[serde(rename = "Teammates", default)]
    teammates: HashMap<String, f64>,
    #[serde(rename = "Tera Types", default)]
    tera: HashMap<String, f64>,
    #[serde(rename = "Spreads", default)]
    spreads: HashMap<String, f64>,
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

/// Month directories (`2026-09`) in ascending order.
fn parse_months(index_html: &str) -> Vec<String> {
    let re = Regex::new(r#"href="(\d{4}-\d{2})/""#).expect("valid regex");
    let set: BTreeSet<String> = re
        .captures_iter(index_html)
        .map(|c| c[1].to_owned())
        .collect();
    set.into_iter().collect()
}

/// Picks the newest regulation for each Champions format in a chaos directory listing.
fn parse_champions(listing: &str) -> Vec<FormatInfo> {
    let re = Regex::new(r"\b(gen9champions(?:vgc(\d{4})|bss)reg([a-z]+?)(bo3)?)-(\d+)\.json")
        .expect("valid regex");

    struct Newest {
        /// (year, code length, code) so 2027 beats 2026 and `mc` beats `mb`.
        version: (u32, usize, String),
        stem: String,
        reg: String,
    }
    let mut newest: BTreeMap<Kind, Newest> = BTreeMap::new();
    let mut cutoffs: HashMap<String, BTreeSet<u32>> = HashMap::new();

    for c in re.captures_iter(listing) {
        let stem = c[1].to_owned();
        let year: u32 = c.get(2).map_or(0, |y| y.as_str().parse().unwrap_or(0));
        let reg = c[3].to_owned();
        let kind = match (c.get(2).is_some(), c.get(4).is_some()) {
            (true, false) => Kind::Vgc,
            (true, true) => Kind::VgcBo3,
            (false, false) => Kind::Bss,
            (false, true) => continue,
        };
        let cutoff: u32 = c[5].parse().unwrap_or(0);
        cutoffs.entry(stem.clone()).or_default().insert(cutoff);

        let version = (year, reg.len(), reg.clone());
        if newest.get(&kind).is_none_or(|n| version > n.version) {
            newest.insert(kind, Newest { version, stem, reg });
        }
    }

    newest
        .into_iter()
        .map(|(kind, Newest { stem, reg, .. })| FormatInfo {
            kind,
            regulation: regulation_label(&reg),
            cutoffs: cutoffs
                .remove(&stem)
                .unwrap_or_default()
                .into_iter()
                .collect(),
            stem,
        })
        .collect()
}

/// `mc` -> `M-C`
fn regulation_label(code: &str) -> String {
    code.chars()
        .map(|c| c.to_ascii_uppercase().to_string())
        .collect::<Vec<_>>()
        .join("-")
}

fn build_meta(dex: &Dex, current: Chaos, prev: Option<&Chaos>) -> Meta {
    let mut ranked: Vec<(String, RawMon)> = current.data.into_iter().collect();
    ranked.sort_by(|a, b| b.1.usage.total_cmp(&a.1.usage).then_with(|| a.0.cmp(&b.0)));
    ranked.truncate(LADDER_SIZE);
    let ladder: Vec<String> = ranked.iter().map(|(n, _)| n.clone()).collect();

    let named = |id: &str| dex.display_name(id);
    let tera_name = |id: &str| {
        let mut c = id.chars();
        c.next()
            .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
    };

    let mons = ranked
        .into_iter()
        .enumerate()
        .map(|(i, (name, raw))| {
            // Every set has exactly one ability, so the ability total is the number of sets.
            let sets: f64 = raw.abilities.values().sum::<f64>().max(f64::MIN_POSITIVE);
            let pair_rates = ladder
                .iter()
                .filter(|other| **other != name)
                .filter_map(|other| raw.teammates.get(other).map(|c| (other.clone(), c / sets)))
                .collect();
            let spread = raw
                .spreads
                .iter()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .and_then(|(s, count)| {
                    Spread::parse(s).map(|(nature, points)| Spread {
                        nature,
                        points,
                        pct: count / sets * 100.0,
                    })
                });
            let mut teammates: Vec<(String, f64)> = raw.teammates.into_iter().collect();
            teammates.sort_by(|a, b| b.1.total_cmp(&a.1));
            Mon {
                spread,
                pair_rates,
                prev: prev
                    .and_then(|p| p.data.get(&name))
                    .map(|p| p.usage * 100.0),
                rank: i + 1,
                usage: raw.usage * 100.0,
                abilities: top_shares(&raw.abilities, 3, None, named),
                items: top_shares(&raw.items, 5, None, named),
                moves: top_shares(&raw.moves, 8, Some(sets), named),
                tera: top_shares(&raw.tera, 4, None, tera_name),
                teammates: teammates.into_iter().take(6).map(|(n, _)| n).collect(),
                name,
            }
        })
        .collect();

    Meta {
        battles: current.info.battles,
        cutoff: current.info.cutoff as u32,
        mons,
    }
}

fn top_shares(
    counts: &HashMap<String, f64>,
    n: usize,
    total: Option<f64>,
    name: impl Fn(&str) -> String,
) -> Vec<Share> {
    let total = total
        .unwrap_or_else(|| counts.values().sum())
        .max(f64::MIN_POSITIVE);
    let mut v: Vec<(&String, &f64)> = counts
        .iter()
        .filter(|(k, _)| !k.is_empty() && k.as_str() != "nothing")
        .collect();
    v.sort_by(|a, b| b.1.total_cmp(a.1));
    v.into_iter()
        .take(n)
        .map(|(k, c)| Share {
            name: name(k),
            pct: c / total * 100.0,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = r#"
        <a href="gen9championsbssregmb-1630.json">x</a>
        <a href="gen9championsbssregmc-0.json">x</a>
        <a href="gen9championsbssregmc-1630.json">x</a>
        <a href="gen9championsbssregmc-1760.json">x</a>
        <a href="gen9championsou-1630.json">x</a>
        <a href="gen9championsvgc2026regmb-1630.json">x</a>
        <a href="gen9championsvgc2026regmbbo3-1630.json">x</a>
        <a href="gen9championsvgc2026regmc-1500.json">x</a>
        <a href="gen9championsvgc2026regmc-1630.json">x</a>
        <a href="gen9championsvgc2026regmcbo3-1630.json">x</a>
        <a href="championsvgc2026regmc-1630.json">x</a>
        <a href="gen9ou-1695.json">x</a>
    "#;

    #[test]
    fn picks_newest_regulation_per_format() {
        let f = parse_champions(LISTING);
        let got: Vec<_> = f
            .iter()
            .map(|f| (f.kind, f.stem.as_str(), f.regulation.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                (Kind::Vgc, "gen9championsvgc2026regmc", "M-C"),
                (Kind::VgcBo3, "gen9championsvgc2026regmcbo3", "M-C"),
                (Kind::Bss, "gen9championsbssregmc", "M-C"),
            ]
        );
        assert_eq!(f[0].cutoffs, vec![1500, 1630]);
        assert_eq!(f[2].cutoffs, vec![0, 1630, 1760]);
        assert_eq!(f[2].default_cutoff(), 1630);
    }

    #[test]
    fn newer_year_beats_later_letter() {
        let f = parse_champions(
            "gen9championsvgc2026regmz-1630.json gen9championsvgc2027rega-1630.json",
        );
        assert_eq!(f[0].stem, "gen9championsvgc2027rega");
    }

    #[test]
    fn parses_stat_point_spreads() {
        let (nature, points) = Spread::parse("Adamant:32/32/0/0/2/0").unwrap();
        assert_eq!(nature, "Adamant");
        assert_eq!(points, [32, 32, 0, 0, 2, 0]);
        assert!(Spread::parse("Adamant:32/32/0").is_none());
        assert!(Spread::parse("nonsense").is_none());
    }

    #[test]
    fn months_are_sorted() {
        let html =
            r#"<a href="2026-09/">2026-09/</a><a href="2025-12/">x</a><a href="2026-10/">x</a>"#;
        assert_eq!(parse_months(html), vec!["2025-12", "2026-09", "2026-10"]);
    }
}
