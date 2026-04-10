use reqwest::Client;
use serenity::http::Http;
use serenity::model::id::ChannelId;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

const GAME_NOTES_URL: &str =
    "https://www.uslchampionship.com/page/show/8562056-usl-super-league-game-notes";
const CHECK_INTERVAL_SECS: u64 = 1800; // 30 minutes

#[derive(Debug)]
struct GameNotesDoc {
    title: String,
    url: String,
}

enum DocChange<'a> {
    New(&'a GameNotesDoc),
    Updated { doc: &'a GameNotesDoc, old_title: String },
}

async fn fetch_sun_fc_docs(client: &Client) -> anyhow::Result<Vec<GameNotesDoc>> {
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

    extract_sun_fc_docs(&html)
}

fn extract_sun_fc_docs(html: &str) -> anyhow::Result<Vec<GameNotesDoc>> {
    let marker = "<h2>Tampa Bay Sun FC</h2>";
    let sun_start = html
        .find(marker)
        .ok_or_else(|| anyhow::anyhow!("Could not find Tampa Bay Sun FC section on page"))?;

    let section = &html[sun_start..];
    // Bound the section to just Tampa Bay Sun FC — stop at the next team's h2
    let next_h2 = section[marker.len()..]
        .find("<h2>")
        .map(|i| i + marker.len())
        .unwrap_or(section.len());
    let section = &section[..next_h2];

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

pub async fn run(pool: PgPool, http: Arc<Http>, channel_id: u64) {
    let client = Client::new();

    // Seed existing documents without alerting
    println!("[game_notes_watcher] Seeding existing documents...");
    match fetch_sun_fc_docs(&client).await {
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
        match fetch_sun_fc_docs(&client).await {
            Err(e) => eprintln!("[game_notes_watcher] Fetch error: {}", e),
            Ok(docs) => match find_and_store_changes(&pool, &docs).await {
                Err(e) => eprintln!("[game_notes_watcher] DB error: {}", e),
                Ok(changes) => {
                    if changes.is_empty() {
                        println!("[game_notes_watcher] No changes found.");
                    } else {
                        println!("[game_notes_watcher] {} change(s) found", changes.len());
                        let channel = ChannelId::new(channel_id);
                        for change in &changes {
                            let msg = match change {
                                DocChange::New(doc) => format_new(doc),
                                DocChange::Updated { doc, old_title } => {
                                    format_updated(doc, old_title)
                                }
                            };
                            println!("[game_notes_watcher] Alerting:\n{}", msg);
                            if let Err(e) = channel.say(&http, &msg).await {
                                eprintln!("[game_notes_watcher] Discord error: {:?}", e);
                            }
                        }
                    }
                }
            },
        }
    }
}
