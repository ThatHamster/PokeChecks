//! The privacy policy (`/privacy`), which the app stores require, and `/app-ads.txt`,
//! which AdMob reads to confirm the apps' ads come from this developer.
//!
//! Both read their details from environment variables set in the Render dashboard, so
//! nothing in the code changes when they're filled in:
//! - `CONTACT_EMAIL`: the address the policy gives for questions and requests.
//! - `APP_ADS_TXT`: the line AdMob gives, e.g. `google.com, pub-…, DIRECT, f08c47fec0942fa0`.

use std::sync::OnceLock;

use maud::{Markup, html};

use crate::view::{Theme, shell, theme_switch};

/// When this policy last changed in a way that matters to readers.
const LAST_UPDATED: &str = "13 October 2026";

/// The contact address from `CONTACT_EMAIL`, if it looks like one.
fn contact_email() -> Option<&'static str> {
    static EMAIL: OnceLock<Option<String>> = OnceLock::new();
    EMAIL
        .get_or_init(|| {
            std::env::var("CONTACT_EMAIL")
                .ok()
                .map(|e| e.trim().to_owned())
                .filter(|e| e.contains('@') && !e.contains(char::is_whitespace))
        })
        .as_deref()
}

/// The text of `/app-ads.txt` from `APP_ADS_TXT`, or `None` until it's set.
pub fn app_ads_txt() -> Option<&'static str> {
    static TXT: OnceLock<Option<String>> = OnceLock::new();
    TXT.get_or_init(|| {
        std::env::var("APP_ADS_TXT")
            .ok()
            .map(|t| t.trim().to_owned() + "\n")
            .filter(|t| t.trim().len() > 1)
    })
    .as_deref()
}

pub fn privacy_page(theme: Theme) -> Markup {
    shell(
        "Privacy policy",
        theme,
        html! {
            header {
                div { a.home-link href="/" { h1 { "Poke" span { "Checks" } } } }
                div.ladder-meta { (theme_switch(theme, "/privacy")) }
            }
            article.pane.policy {
                div.pane-head {
                    h2 { "Privacy policy" }
                    span.muted.small { "Last updated " (LAST_UPDATED) }
                }
                div.policy-body {
                    p.lead {
                        strong { "In short: " }
                        "PokeChecks has no accounts and collects nothing about you. The website has no ads, analytics or tracking. "
                        "Your teams and settings stay on your own device. The Android and iOS apps show one small, non-personalized ad on their Usage screen "
                        "and an optional video ad you can choose to watch (see " a href="#ads" { "Ads in the apps" } "). We never sell your data."
                    }
                    p {
                        "This policy covers the PokeChecks website (pokechecks.onrender.com) and the PokeChecks apps for Android and iOS. "
                        "PokeChecks is an unofficial fan project and isn’t affiliated with Nintendo, Creatures Inc., GAME FREAK or The Pokémon Company."
                    }

                    h3 { "What stays on your device" }
                    ul {
                        li {
                            strong { "On the website: " }
                            "your theme choice and saved teams are kept in cookies in your browser. They’re sent back to the site so it can show them to you, "
                            "and we don’t keep a copy or use them for anything else. There are no accounts."
                        }
                        li {
                            strong { "In the apps: " }
                            "your teams, saved teams, calcs and settings, and a copy of the usage stats, are stored only on your phone. "
                            "Deleting the app deletes them."
                        }
                    }

                    h3 { "What the server sees" }
                    p {
                        "When you open a page, or the app checks for new stats, your device connects to our server, which is hosted by "
                        a href="https://render.com/privacy" target="_blank" rel="noopener" { "Render" }
                        ". Like any website, the hosting keeps standard logs of each request, such as the IP address, time and page asked for, "
                        "to keep the service secure and working. We don’t combine them with anything else or use them to identify you. "
                        "The app’s requests carry no account, name or device identifier."
                    }
                    p {
                        "The server itself downloads public data (usage stats from Smogon, Pokémon data and sprites from Pokémon Showdown, "
                        "and tournament team lists from Limitless). Your device never contacts those sites through PokeChecks."
                    }

                    h3 #ads { "Ads in the apps" }
                    p {
                        "The Android and iOS apps (not the website) are kept free by ads from "
                        a href="https://admob.google.com" target="_blank" rel="noopener" { "Google AdMob" } ":"
                    }
                    ul {
                        li { "a small banner on the Usage screen only, never on the builder, calc, speed or tournament screens; and" }
                        li { "an optional video ad under “Support PokeChecks”, shown only if you choose to watch it." }
                    }
                    p {
                        "All ads are " strong { "non-personalized" } ": they aren’t chosen based on your interests or your activity in other apps, "
                        "and the apps don’t ask to track you. To show ads, limit how often you see them, measure them and prevent fraud, "
                        "Google may collect information such as your device’s advertising identifier, IP address, approximate location and how you "
                        "interact with the ads, as described in "
                        a href="https://policies.google.com/technologies/partner-sites" target="_blank" rel="noopener" {
                            "how Google uses information from sites or apps that use its services"
                        }
                        ". We don’t share your teams or anything else from the app with advertisers."
                    }
                    p {
                        "Where the law requires it (for example in the EU, the UK and Switzerland) the app asks for your consent before any ads are requested, "
                        "and “Ad privacy choices” on the About screen lets you change your choice at any time. "
                        "You can also reset or delete your advertising identifier in your device’s settings."
                    }

                    h3 { "Supporting PokeChecks" }
                    p {
                        "The website’s “Support PokeChecks” button opens our "
                        a href="https://ko-fi.com" target="_blank" rel="noopener" { "Ko-fi" }
                        " tip form inside the page. Nothing is loaded from Ko-fi until you open it, and tipping is optional and changes nothing on the site. "
                        "Payments are handled by Ko-fi and its payment providers under "
                        a href="https://more.ko-fi.com/privacy" target="_blank" rel="noopener" { "Ko-fi’s privacy policy" }
                        "; we never see your card details. The apps don’t include this link."
                    }

                    h3 { "Children" }
                    p {
                        "PokeChecks is a tool for competitive players and isn’t directed at children under 13. "
                        "We don’t knowingly collect personal information from anyone, of any age."
                    }

                    h3 { "Changes" }
                    p {
                        "If this policy changes, the new version will appear on this page with a new “Last updated” date."
                    }

                    h3 { "Contact" }
                    p {
                        @if let Some(email) = contact_email() {
                            "Questions or requests about your privacy: "
                            a href={ "mailto:" (email) } { (email) } "."
                        } @else {
                            "Contact details are coming soon."
                        }
                    }
                }
            }
            footer {
                a href="/" { "Back to PokeChecks" }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_page_covers_what_the_stores_ask_for() {
        let page = privacy_page(Theme::System).into_string();
        for must in [
            "Privacy policy",
            "non-personalized",
            "Google AdMob",
            "Ad privacy choices",
            "advertising identifier",
            "Render",
            "Contact",
        ] {
            assert!(page.contains(must), "missing {must}");
        }
        assert!(
            !page.contains("<script"),
            "the site's CSP allows no scripts"
        );
    }
}
