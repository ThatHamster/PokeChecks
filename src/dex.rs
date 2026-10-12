//! Base stats, types and display names from Pokémon Showdown's public data files.

use std::collections::HashMap;

use anyhow::{Context, Result};
use regex::Regex;
use serde_json::Value;

const SHOWDOWN_DATA: &str = "https://play.pokemonshowdown.com/data/";

/// Showdown's ID format: lowercase ASCII letters and digits only.
pub fn to_id(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

pub struct DexEntry {
    pub types: Vec<String>,
    /// HP, Atk, Def, SpA, SpD, Spe.
    pub stats: [u16; 6],
    /// The species a form belongs to, e.g. `Salamence` for `Salamence-Mega`. Two forms of
    /// one species can't share a team.
    pub base_species: String,
}

impl DexEntry {
    pub fn bst(&self) -> u16 {
        self.stats.iter().sum()
    }
}

pub struct Dex {
    entries: HashMap<String, DexEntry>,
    /// Move, item and ability IDs mapped to display names.
    names: HashMap<String, String>,
}

impl Dex {
    pub async fn load(http: &reqwest::Client) -> Result<Self> {
        let (pokedex, moves, items, abilities) = tokio::try_join!(
            fetch_json(http, "pokedex.json"),
            fetch_json(http, "moves.json"),
            fetch_text(http, "items.js"),
            fetch_text(http, "abilities.js"),
        )?;

        let entries = parse_pokedex(&pokedex);
        let mut names = HashMap::new();
        if let Value::Object(map) = &moves {
            for (id, m) in map {
                if let Some(name) = m.get("name").and_then(Value::as_str) {
                    names.insert(id.clone(), name.to_owned());
                }
            }
        }
        parse_js_names(&items, &mut names);
        parse_js_names(&abilities, &mut names);

        tracing::info!(
            pokemon = entries.len(),
            names = names.len(),
            "loaded Showdown dex"
        );
        Ok(Self { entries, names })
    }

    pub fn entry(&self, name: &str) -> Option<&DexEntry> {
        self.entries.get(&to_id(name))
    }

    /// Base species for the duplicate-species rule; falls back to the name itself.
    pub fn species<'a>(&'a self, name: &'a str) -> &'a str {
        self.entry(name).map_or(name, |e| e.base_species.as_str())
    }

    /// Turns a stats-file ID like `heavydutyboots` into `Heavy-Duty Boots`.
    pub fn display_name(&self, id: &str) -> String {
        self.names.get(id).cloned().unwrap_or_else(|| id.to_owned())
    }
}

async fn fetch_text(http: &reqwest::Client, file: &str) -> Result<String> {
    let url = format!("{SHOWDOWN_DATA}{file}");
    http.get(&url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .with_context(|| format!("downloading {url}"))?
        .text()
        .await
        .with_context(|| format!("reading {url}"))
}

async fn fetch_json(http: &reqwest::Client, file: &str) -> Result<Value> {
    let text = fetch_text(http, file).await?;
    serde_json::from_str(&text).with_context(|| format!("parsing {file}"))
}

fn parse_pokedex(v: &Value) -> HashMap<String, DexEntry> {
    let Value::Object(map) = v else {
        return HashMap::new();
    };
    map.iter()
        .filter_map(|(id, e)| {
            let types = e
                .get("types")?
                .as_array()?
                .iter()
                .filter_map(|t| t.as_str().map(str::to_owned))
                .collect();
            let bs = e.get("baseStats")?;
            let stat = |k: &str| bs.get(k).and_then(Value::as_u64).map(|n| n as u16);
            let stats = [
                stat("hp")?,
                stat("atk")?,
                stat("def")?,
                stat("spa")?,
                stat("spd")?,
                stat("spe")?,
            ];
            let base_species = e
                .get("baseSpecies")
                .or_else(|| e.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_owned();
            Some((
                id.clone(),
                DexEntry {
                    types,
                    stats,
                    base_species,
                },
            ))
        })
        .collect()
}

/// `items.js` and `abilities.js` are JS object literals rather than JSON, so pull out
/// each top-level `id:{ ... name:"Display Name"` pair.
fn parse_js_names(src: &str, out: &mut HashMap<String, String>) {
    let key_re = Regex::new(r"[,{](\w+):\{").expect("valid regex");
    let name_re = Regex::new(r#"name:"([^"]+)""#).expect("valid regex");
    let names: Vec<(usize, &str)> = name_re
        .captures_iter(src)
        .map(|c| (c.get(0).unwrap().start(), c.get(1).unwrap().as_str()))
        .collect();

    for key in key_re.captures_iter(src) {
        let end = key.get(0).unwrap().end();
        let next = names.partition_point(|(pos, _)| *pos < end);
        if let Some((pos, name)) = names.get(next)
            && pos - end <= 1500
        {
            out.entry(key[1].to_owned())
                .or_insert_with(|| (*name).to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_match_showdown() {
        assert_eq!(to_id("Ogerpon-Wellspring"), "ogerponwellspring");
        assert_eq!(to_id("Mr. Mime"), "mrmime");
    }

    #[test]
    fn reads_names_from_js_literals() {
        let src = r#"exports.BattleAbilities={intimidate:{flags:{},name:"Intimidate",rating:3.5},heavydutyboots:{num:1120,fling:{basePower:80},spritenum:715,name:"Heavy-Duty Boots"}}"#;
        let mut names = HashMap::new();
        parse_js_names(src, &mut names);
        assert_eq!(names["intimidate"], "Intimidate");
        assert_eq!(names["heavydutyboots"], "Heavy-Duty Boots");
    }
}
