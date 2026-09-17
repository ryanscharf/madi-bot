use crate::pdf_availability;
use reqwest::Client;
use serenity::builder::{CreateAttachment, CreateMessage};
use serenity::http::Http;
use serenity::model::id::ChannelId;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

const GAME_NOTES_URL: &str =
    "https://www.uslchampionship.com/page/show/8562056-usl-super-league-game-notes";
const CHECK_INTERVAL_SECS: u64 = 1800; // 30 minutes
const TAMPA_BAY_HEADING: &str = "Tampa Bay Sun FC";
const TAMPA_BAY_TAG: &str = "h2";

/// Other USL Super League teams' `<h2>`/`<h3>` section headings on the game
/// notes page, plus the abbreviation(s) Tampa's own doc titles use for them
/// (e.g. "9.18 vs DC Power", "8.15 TB vs DAL") so the opponent can be
/// resolved from `GameNotesDoc.title` without any schedule parsing.
const OPPONENT_TEAMS: &[(&str, &str, &[&str])] = &[
    ("Brooklyn FC", "h2", &["BKN", "BROOKLYN"]),
    ("Carolina Ascent FC", "h2", &["CAR", "CAROLINA"]),
    ("Dallas Trinity FC", "h2", &["DAL", "DALLAS"]),
    ("DC Power FC", "h2", &["DC POWER", "DC", "POWER"]),
    ("Fort Lauderdale United FC", "h2", &["FTL", "FORT LAUDERDALE"]),
    ("Lexington SC", "h2", &["LEX", "LEXINGTON"]),
    ("Sporting JAX", "h3", &["JAX", "SPORTING JAX"]),
];

#[derive(Debug, Clone)]
struct GameNotesDoc {
    title: String,
    url: String,
}

enum DocChange<'a> {
    New(&'a GameNotesDoc),
    Updated { doc: &'a GameNotesDoc, old_title: String },
}

async fn fetch_page_html(client: &Client) -> anyhow::Result<String> {
    let html = client
        .get(GAME_NOTES_URL)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .send()
        .await?
        .text()
        .await?;
    Ok(html)
}

/// Extracts every sportngin.com game notes link listed under a given team's
/// section heading on the USL game notes page.
fn extract_team_docs(html: &str, team_heading: &str, tag: &str) -> anyhow::Result<Vec<GameNotesDoc>> {
    let marker = format!("<{tag}>{team_heading}</{tag}>");
    let start = html
        .find(&marker)
        .ok_or_else(|| anyhow::anyhow!("Could not find {} section on page", team_heading))?;

    let section = &html[start..];
    // Bound the section to just this team — stop at the next team's h2/h3,
    // whichever comes first (the page mixes heading levels between teams).
    let next = ["<h2>", "<h3>"]
        .iter()
        .filter_map(|m| section[marker.len()..].find(m).map(|i| i + marker.len()))
        .min()
        .unwrap_or(section.len());
    let section = &section[..next];

    let fragment = scraper::Html::parse_fragment(section);
    let a_selector = scraper::Selector::parse("a[href]").unwrap();

    let docs = fragment
        .select(&a_selector)
        .filter_map(|el| {
            let href = el.value().attr("href")?;
            let title = el.text().collect::<String>().trim().to_string();
            if href.contains("sportngin.com") && !title.is_empty() {
                Some(GameNotesDoc {
                    title,
                    url: href.to_string(),
                })
            } else {
                None
            }
        })
        .collect();

    Ok(docs)
}

/// Resolves the upcoming opponent's section heading/tag from a Tampa Bay Sun
/// FC doc title such as "9.18 vs DC Power" or "8.15 TB vs DAL".
fn parse_opponent_team(title: &str) -> Option<(&'static str, &'static str)> {
    let upper = title.to_ascii_uppercase();
    let idx = upper.find("VS ")?;
    let abbrev = upper[idx + 3..].trim();
    if abbrev.is_empty() {
        return None;
    }
    OPPONENT_TEAMS
        .iter()
        .find(|(_, _, aliases)| aliases.iter().any(|a| *a == abbrev || abbrev.starts_with(a)))
        .map(|(team, tag, _)| (*team, *tag))
}

/// Insert new docs or detect title changes on existing ones.
async fn find_and_store_changes<'a>(
    pool: &PgPool,
    docs: &'a [GameNotesDoc],
) -> anyhow::Result<Vec<DocChange<'a>>> {
    let mut changes = Vec::new();
    for doc in docs {
        let inserted = sqlx::query(
            "INSERT INTO game_notes_documents (url, title) VALUES ($1, $2)
             ON CONFLICT (url) DO NOTHING",
        )
        .bind(&doc.url)
        .bind(&doc.title)
        .execute(pool)
        .await?;

        if inserted.rows_affected() > 0 {
            changes.push(DocChange::New(doc));
        } else {
            // URL already known — check if title changed
            let row = sqlx::query_as::<_, (String,)>(
                "SELECT title FROM game_notes_documents WHERE url = $1",
            )
            .bind(&doc.url)
            .fetch_optional(pool)
            .await?;

            if let Some((old_title,)) = row {
                if old_title != doc.title {
                    sqlx::query(
                        "UPDATE game_notes_documents SET title = $2 WHERE url = $1",
                    )
                    .bind(&doc.url)
                    .bind(&doc.title)
                    .execute(pool)
                    .await?;

                    changes.push(DocChange::Updated { doc, old_title });
                }
            }
        }
    }
    Ok(changes)
}

fn format_new(doc: &GameNotesDoc) -> String {
    format!("📋 **New Game Notes Posted!**\n**{}**\n{}", doc.title, doc.url)
}

fn format_updated(doc: &GameNotesDoc, old_title: &str) -> String {
    format!(
        "📋 **Game Notes Updated!**\n~~{}~~ → **{}**\n{}",
        old_title, doc.title, doc.url
    )
}

/// Fetches an availability screenshot and logs (without failing the whole
/// alert) if it can't be produced.
async fn screenshot_or_log(client: &Client, label: &str, url: &str) -> Option<Vec<u8>> {
    match pdf_availability::availability_screenshot(client, url).await {
        Ok(Some(png)) => Some(png),
        Ok(None) => {
            println!("[game_notes_watcher] No availability crop found for {label}");
            None
        }
        Err(e) => {
            eprintln!("[game_notes_watcher] {label} screenshot error: {e}");
            None
        }
    }
}

/// Extracts a leading date token like "9.18" or "8/13" from the start of a
/// doc title (e.g. "9.18 vs DC Power", "8.15 TB vs DAL").
fn extract_date_token(title: &str) -> Option<&str> {
    let end = title
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '/'))
        .unwrap_or(title.len());
    let token = &title[..end];
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

/// Picks which of the opponent's own listed docs is for this specific
/// matchup against Tampa Bay. Unlike Tampa's own section (where the current
/// week's doc happens to be listed first), other teams' pages list docs in
/// no reliable order — e.g. DC Power FC's page listed "DCvCAR" before
/// "DC vs TB (9.18)" — so picking the first one silently posted the wrong
/// team's data. Match instead on: the opponent's title mentioning Tampa Bay
/// ("TB"/"TAMPA"), or sharing the same leading date token as Tampa's own
/// title for this doc. If neither signal narrows it to exactly one doc,
/// don't guess — a wrong screenshot is worse than a missing one.
fn select_opponent_doc<'a>(opp_docs: &'a [GameNotesDoc], tampa_title: &str) -> Option<&'a GameNotesDoc> {
    let tb_matches: Vec<&GameNotesDoc> = opp_docs
        .iter()
        .filter(|d| {
            let upper = d.title.to_ascii_uppercase();
            upper.contains("TB") || upper.contains("TAMPA")
        })
        .collect();
    if tb_matches.len() == 1 {
        return Some(tb_matches[0]);
    }

    if let Some(date_token) = extract_date_token(tampa_title) {
        let candidates = if tb_matches.is_empty() { opp_docs.iter().collect::<Vec<_>>() } else { tb_matches };
        let date_matches: Vec<&GameNotesDoc> =
            candidates.into_iter().filter(|d| d.title.contains(date_token)).collect();
        if date_matches.len() == 1 {
            return Some(date_matches[0]);
        }
    }

    None
}

/// Resolves and fetches the opponent's own availability screenshot, if the
/// opponent can be identified and their doc found on the page.
async fn opponent_screenshot(client: &Client, html: &str, title: &str) -> Option<(String, Vec<u8>)> {
    let (team, tag) = parse_opponent_team(title)?;

    let opp_docs = match extract_team_docs(html, team, tag) {
        Ok(docs) => docs,
        Err(e) => {
            eprintln!("[game_notes_watcher] Opponent section lookup error for {team}: {e}");
            return None;
        }
    };

    let Some(opp_doc) = select_opponent_doc(&opp_docs, title) else {
        let titles: Vec<&str> = opp_docs.iter().map(|d| d.title.as_str()).collect();
        println!(
            "[game_notes_watcher] Couldn't uniquely match {team}'s doc for \"{title}\" among: {:?}",
            titles
        );
        return None;
    };

    let png = screenshot_or_log(client, team, &opp_doc.url).await?;
    Some((team.to_string(), png))
}

pub async fn run(pool: PgPool, http: Arc<Http>, channel_id: u64) {
    let client = Client::new();

    // Seed existing documents without alerting
    println!("[game_notes_watcher] Seeding existing documents...");
    match fetch_page_html(&client).await.and_then(|html| extract_team_docs(&html, TAMPA_BAY_HEADING, TAMPA_BAY_TAG)) {
        Ok(docs) => {
            let changes = find_and_store_changes(&pool, &docs).await.unwrap_or_default();
            println!(
                "[game_notes_watcher] Seeded {} docs ({} were new to DB)",
                docs.len(),
                changes.len()
            );
        }
        Err(e) => eprintln!("[game_notes_watcher] Seed error: {}", e),
    }

    loop {
        sleep(Duration::from_secs(CHECK_INTERVAL_SECS)).await;

        println!("[game_notes_watcher] Checking for new Tampa Bay Sun FC game notes...");
        match fetch_page_html(&client).await {
            Err(e) => eprintln!("[game_notes_watcher] Fetch error: {}", e),
            Ok(html) => match extract_team_docs(&html, TAMPA_BAY_HEADING, TAMPA_BAY_TAG) {
                Err(e) => eprintln!("[game_notes_watcher] Parse error: {}", e),
                Ok(docs) => match find_and_store_changes(&pool, &docs).await {
                    Err(e) => eprintln!("[game_notes_watcher] DB error: {}", e),
                    Ok(changes) => {
                        if changes.is_empty() {
                            println!("[game_notes_watcher] No changes found.");
                        } else {
                            println!("[game_notes_watcher] {} change(s) found", changes.len());
                            let channel = ChannelId::new(channel_id);
                            for change in &changes {
                                let (doc, msg) = match change {
                                    DocChange::New(doc) => (*doc, format_new(doc)),
                                    DocChange::Updated { doc, old_title } => {
                                        (*doc, format_updated(doc, old_title))
                                    }
                                };

                                let mut attachments = Vec::new();

                                if let Some(png) =
                                    screenshot_or_log(&client, "Tampa Bay Sun FC", &doc.url).await
                                {
                                    attachments
                                        .push(CreateAttachment::bytes(png, "tampa_bay_availability.png"));
                                }

                                if let Some((team, png)) =
                                    opponent_screenshot(&client, &html, &doc.title).await
                                {
                                    let filename =
                                        format!("{}_availability.png", team.replace(' ', "_").to_lowercase());
                                    attachments.push(CreateAttachment::bytes(png, filename));
                                } else {
                                    println!(
                                        "[game_notes_watcher] No opponent screenshot for title: {}",
                                        doc.title
                                    );
                                }

                                println!("[game_notes_watcher] Alerting:\n{}", msg);
                                let builder = CreateMessage::new().content(&msg).add_files(attachments);
                                if let Err(e) = channel.send_message(&http, builder).await {
                                    eprintln!("[game_notes_watcher] Discord error: {:?}", e);
                                }
                            }
                        }
                    }
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_opponent_from_real_titles() {
        assert_eq!(
            parse_opponent_team("9.18 vs DC Power"),
            Some(("DC Power FC", "h2"))
        );
        assert_eq!(parse_opponent_team("8.22 vs JAX"), Some(("Sporting JAX", "h3")));
        assert_eq!(
            parse_opponent_team("9.12 vs FTL"),
            Some(("Fort Lauderdale United FC", "h2"))
        );
        assert_eq!(
            parse_opponent_team("8.15 TB vs DAL"),
            Some(("Dallas Trinity FC", "h2"))
        );
        assert_eq!(
            parse_opponent_team("8.29 TB vs CAR"),
            Some(("Carolina Ascent FC", "h2"))
        );
    }

    #[test]
    fn parses_opponent_not_yet_seen_this_season() {
        assert_eq!(parse_opponent_team("10.17 vs LEX"), Some(("Lexington SC", "h2")));
        assert_eq!(parse_opponent_team("10.24 vs BKN"), Some(("Brooklyn FC", "h2")));
        assert_eq!(
            parse_opponent_team("11.21 vs Brooklyn"),
            Some(("Brooklyn FC", "h2"))
        );
    }

    #[test]
    fn unrecognized_or_missing_opponent_returns_none() {
        assert_eq!(parse_opponent_team("Week 3 Notes"), None);
        assert_eq!(parse_opponent_team("9.18 vs Some Unknown Team"), None);
    }

    fn doc(title: &str) -> GameNotesDoc {
        GameNotesDoc {
            title: title.to_string(),
            url: format!("https://example.com/{}.pdf", title),
        }
    }

    #[test]
    fn selects_the_right_doc_among_unordered_opponent_docs() {
        // Real scenario: DC Power FC's page listed these in this order, and
        // picking the first one (DCvCAR) silently posted the wrong team's
        // availability table instead of the actual Tampa Bay matchup doc.
        let docs = vec![
            doc("DCvCAR"),
            doc("DCvsDAL 8/13"),
            doc("DC vs TB (9.18)"),
            doc("DCvJAX"),
            doc("BKNvDC 8.21"),
            doc("DCxLEX 8.29"),
        ];
        let selected = select_opponent_doc(&docs, "9.18 vs DC Power").unwrap();
        assert_eq!(selected.title, "DC vs TB (9.18)");
    }

    #[test]
    fn falls_back_to_date_when_no_tb_mention() {
        let docs = vec![doc("DCvCAR 8.29"), doc("DCvsDAL 8.13"), doc("DCvJAX 9.4")];
        let selected = select_opponent_doc(&docs, "8.29 TB vs DC Power").unwrap();
        assert_eq!(selected.title, "DCvCAR 8.29");
    }

    #[test]
    fn refuses_to_guess_when_ambiguous() {
        let docs = vec![doc("DCvCAR"), doc("DCvJAX")];
        assert!(select_opponent_doc(&docs, "9.18 vs DC Power").is_none());
    }
}
