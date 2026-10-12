//! Teams saved from the team builder. They live in the visitor's own browser as cookies,
//! one per team, so there's no account and nothing stored on the server. A full team with
//! edited sets is well under the 4 KB cookie limit.

/// How many teams a browser can keep.
pub const MAX_SAVED: usize = 12;
const PREFIX: &str = "pc_t";
const ONE_YEAR: u32 = 365 * 24 * 60 * 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedTeam {
    pub slot: usize,
    pub name: String,
    /// Format key, e.g. `vgc`.
    pub fmt: String,
    /// The team builder's `team` value: members separated by commas.
    pub team: String,
}

impl SavedTeam {
    /// Member names, for showing the team at a glance.
    pub fn names(&self) -> Vec<&str> {
        self.team
            .split(',')
            .filter_map(|m| m.split('~').next())
            .filter(|n| !n.is_empty())
            .collect()
    }

    fn cookie_value(&self) -> String {
        serde_urlencoded::to_string([
            ("n", self.name.as_str()),
            ("f", self.fmt.as_str()),
            ("t", self.team.as_str()),
        ])
        .unwrap_or_default()
    }

    /// `Set-Cookie` value that stores this team.
    pub fn set_cookie(&self) -> String {
        format!(
            "{PREFIX}{}={}; Path=/; Max-Age={ONE_YEAR}; SameSite=Lax; HttpOnly",
            self.slot,
            self.cookie_value()
        )
    }
}

/// `Set-Cookie` value that deletes a slot.
pub fn delete_cookie(slot: usize) -> String {
    format!("{PREFIX}{slot}=; Path=/; Max-Age=0; SameSite=Lax; HttpOnly")
}

/// Saved teams from a raw `Cookie` header, by slot.
pub fn from_cookie_header(header: &str) -> Vec<SavedTeam> {
    let mut teams: Vec<SavedTeam> = header
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .filter_map(|(name, value)| {
            let slot: usize = name.strip_prefix(PREFIX)?.parse().ok()?;
            if slot >= MAX_SAVED {
                return None;
            }
            let fields: Vec<(String, String)> = serde_urlencoded::from_str(value).ok()?;
            let get = |k: &str| {
                fields
                    .iter()
                    .find(|(key, _)| key == k)
                    .map(|(_, v)| v.clone())
            };
            let team = get("t").filter(|t| !t.is_empty())?;
            Some(SavedTeam {
                slot,
                name: get("n").filter(|n| !n.trim().is_empty())?,
                fmt: get("f").unwrap_or_default(),
                team,
            })
        })
        .collect();
    teams.sort_by_key(|t| t.slot);
    teams
}

/// Where a team named `name` should go: its existing slot (saving again overwrites it),
/// else the first free one. `None` when every slot is taken.
pub fn slot_for(saved: &[SavedTeam], name: &str, fmt: &str) -> Option<usize> {
    if let Some(t) = saved
        .iter()
        .find(|t| t.fmt == fmt && t.name.eq_ignore_ascii_case(name))
    {
        return Some(t.slot);
    }
    (0..MAX_SAVED).find(|s| !saved.iter().any(|t| t.slot == *s))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team(slot: usize, name: &str) -> SavedTeam {
        SavedTeam {
            slot,
            name: name.into(),
            fmt: "vgc".into(),
            team: "Rillaboom~Miracle Seed~Grassy Surge~Adamant~32/32/0/0/2/0~Fake Out|U-turn,Incineroar".into(),
        }
    }

    #[test]
    fn round_trips_through_a_cookie() {
        let t = team(3, "Rain & Tailwind; v2");
        let set = t.set_cookie();
        let pair = set.split(';').next().unwrap();
        let header = format!("theme=dark; {pair}; other=1");
        assert_eq!(from_cookie_header(&header), vec![t.clone()]);
        assert_eq!(t.names(), vec!["Rillaboom", "Incineroar"]);
        assert!(set.len() < 4000);
    }

    #[test]
    fn ignores_foreign_and_broken_cookies() {
        assert!(from_cookie_header("pc_t99=n=x&t=A; pc_tx=1; pc_t1=; theme=dark").is_empty());
    }

    #[test]
    fn saving_reuses_a_name_or_takes_the_first_free_slot() {
        let saved = vec![team(0, "Rain"), team(2, "Sun")];
        assert_eq!(slot_for(&saved, "sun", "vgc"), Some(2));
        assert_eq!(slot_for(&saved, "Sand", "vgc"), Some(1));
        let full: Vec<SavedTeam> = (0..MAX_SAVED).map(|i| team(i, &format!("T{i}"))).collect();
        assert_eq!(slot_for(&full, "New", "vgc"), None);
    }
}
