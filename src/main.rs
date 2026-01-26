use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::channel::Reaction;
use serenity::model::gateway::Ready;
use serenity::model::id::{EmojiId, ChannelId};
use serenity::model::channel::ReactionType;
use serenity::prelude::*;
use rand::Rng;
use dotenv::dotenv;
use serde::Deserialize;
use std::sync::Arc;
use futures_util::stream::{StreamExt};
use tokio_postgres::{NoTls, Error as PgError, AsyncMessage};


// Emoji constants
const EMOJI_AC: u64 = 1460415544229363944;
const EMOJI_TI: u64 = 1460415567457554483;
const EMOJI_VA: u64 = 1460415586399027200;
const EMOJI_TE: u64 = 1460415609664835807;
const EMOJI_D: u64 = 1460415630074449960;
const EMOJI_MADI_KNIFE: u64 = 1421229718023442563;
const EMOJI_AC_ALT: u64 = 1460433484337385514;

// Helper function to create custom emoji
fn custom_emoji(id: u64, name: &str) -> ReactionType {
    ReactionType::Custom {
        animated: false,
        id: EmojiId::new(id),
        name: Some(name.to_string()),
    }
}

// Helper function to get ACTIVATED sequence
fn activated_sequence() -> Vec<ReactionType> {
    vec![
        custom_emoji(EMOJI_AC, "AC"),
        custom_emoji(EMOJI_TI, "TI"),
        custom_emoji(EMOJI_VA, "VA"),
        custom_emoji(EMOJI_TE, "TE"),
        custom_emoji(EMOJI_D, "D_"),
    ]
}

// Helper function to get ACTIVATE sequence (no D)
fn activate_sequence() -> Vec<ReactionType> {
    vec![
        custom_emoji(EMOJI_AC, "AC"),
        custom_emoji(EMOJI_TI, "TI"),
        custom_emoji(EMOJI_VA, "VA"),
        custom_emoji(EMOJI_TE, "TE"),
    ]
}

// Helper function to add emoji sequence to a message
async fn add_emoji_sequence(msg: &Message, ctx: &Context, emojis: Vec<ReactionType>) {
    for emoji in emojis {
        if let Err(why) = msg.react(&ctx.http, emoji).await {
            println!("Error adding reaction: {:?}", why);
            break;
        }
    }
}

struct Handler;

#[derive(Debug, Deserialize)]
#[allow(dead_code)] 
struct RosterChangeEvent {
    event_type: String,
    number: i32,
    name: String,
    ao_datetime: String,
    event_time: String,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore messages from bots (including itself)
        if msg.author.bot {
            return;
        }
        
        let content_lower = msg.content.to_lowercase();
        println!("Received message: {}", content_lower);
        
        // Check for "activated" (full word)
        if content_lower.contains("activated") {
            println!("Detected 'activated', adding full ACTIVATED sequence");
            add_emoji_sequence(&msg, &ctx, activated_sequence()).await;
            return;
        }
        
        // Check for "activate" (without the d)
        if content_lower.contains("activate") {
            println!("Detected 'activate', adding ACTIVATE sequence (no D)");
            add_emoji_sequence(&msg, &ctx, activate_sequence()).await;
            return;
        }
        
        // Check for "madi" mention
        let has_madi_word = content_lower.split_whitespace()
            .any(|word| word.trim_matches(|c: char| !c.is_alphabetic()) == "madi");
        let has_madi_parsons = content_lower.contains("madi parsons");
        
        if has_madi_word || has_madi_parsons {
            println!("Detected madi mention, adding random reactions");
            handle_madi_reactions(&msg, &ctx).await;
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }

    async fn reaction_add(&self, ctx: Context, reaction: Reaction) {
        // Ignore reactions from bots
        if let Some(user_id) = reaction.user_id {
            if let Ok(user) = user_id.to_user(&ctx.http).await {
                if user.bot {
                    return;
                }
            }
        }
        
        println!("Reaction detected: {:?}", reaction.emoji);
        
        // Check if the reaction is :AC: emoji
        if let ReactionType::Custom { id, .. } = &reaction.emoji {
            if id.get() == EMOJI_AC || id.get() == EMOJI_AC_ALT {
                println!("Detected :AC: reaction, completing ACTIVATED sequence");
                
                if let Ok(msg) = reaction.message(&ctx.http).await {
                    // Add remaining emojis (skip AC since it's already there)
                    let remaining = vec![
                        custom_emoji(EMOJI_TI, "TI"),
                        custom_emoji(EMOJI_VA, "VA"),
                        custom_emoji(EMOJI_TE, "TE"),
                        custom_emoji(EMOJI_D, "D_"),
                    ];
                    add_emoji_sequence(&msg, &ctx, remaining).await;
                } else {
                    println!("Error fetching message");
                }
            }
        }
    }
}

// Handle madi-specific reactions
async fn handle_madi_reactions(msg: &Message, ctx: &Context) {
// Change this line to:
let (selected_unicode, use_custom, use_activated) = {
            // All RNG work happens in this temporary block
        let mut rng = rand::thread_rng();
        
        let reactions_unicode: Vec<Vec<char>> = vec![
            vec!['🥵'], vec!['😍'], vec!['💖'], vec!['🥹'],
            vec!['🤤'], vec!['😋'], vec!['🤠'], vec!['💪'],
            vec!['🇲', '🇦', '🇩', '🇮'], vec!['🇾', '🇪', '🇸'],
        ];

        let num_reactions = rng.gen_range(1..=3);
        let mut selected = Vec::new();
        let mut used_indices = Vec::new();
        
        while selected.len() < num_reactions {
            let idx = rng.gen_range(0..reactions_unicode.len());
            if !used_indices.contains(&idx) {
                used_indices.push(idx);
                selected.push(reactions_unicode[idx].clone());
            }
        }
        
        let custom = rng.gen_range(0..100) < 20;
        let activated = rng.gen_range(0..100) < 10;
        
        (selected, custom, activated)
    }; // rng is dropped here, so it doesn't break the 'Send' requirement for the .await below

    // Now it is safe to use .await
for reaction_set in selected_unicode {
        for emoji in reaction_set {
            let _ = msg.react(&ctx.http, emoji).await;
        }
    }


// Use the custom flag
    if use_custom && !use_activated {
        let _ = msg.react(&ctx.http, custom_emoji(EMOJI_MADI_KNIFE, "madi_knife")).await;
    }
    
    // Use the activated flag
    if use_activated {
        add_emoji_sequence(msg, ctx, activated_sequence()).await;
    }
}

async fn listen_for_roster_changes(http: Arc<serenity::http::Http>) -> Result<(), PgError> {
    // 1. Fetch environment variables at the start to ensure they are in scope
    let db_host = std::env::var("DB_HOST").unwrap_or_else(|_| "192.168.2.66".to_string());
    let db_port = std::env::var("DB_PORT").unwrap_or_else(|_| "5432".to_string());
    let db_name = std::env::var("DB_NAME").unwrap_or_else(|_| "tb_sun".to_string());
    let db_user = std::env::var("DB_USERNAME").expect("DB_USERNAME must be set");
    let db_password = std::env::var("DB_PASSWORD").expect("DB_PASSWORD must be set");
    
    let roster_channel_id: u64 = std::env::var("ROSTER_CHANNEL_ID")
        .expect("ROSTER_CHANNEL_ID must be set")
        .parse()
        .expect("ROSTER_CHANNEL_ID must be a valid u64");
    
    let connection_string = format!(
        "host={} port={} dbname={} user={} password={}",
        db_host, db_port, db_name, db_user, db_password
    );
    
    // 2. Establish Connection
    let (client, connection) = tokio_postgres::connect(&connection_string, NoTls).await?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<tokio_postgres::Notification>();

    // 3. Background Task to handle the connection
    tokio::spawn(async move {
        // Fix: Shadow 'connection' as mutable so poll_fn can use it
        let mut connection = connection; 
        let mut stream = futures_util::stream::poll_fn(move |cx| connection.poll_message(cx));
        
        while let Some(msg_result) = stream.next().await {
            match msg_result {
                Ok(AsyncMessage::Notification(notification)) => {
                    let _ = tx.send(notification);
                }
                Ok(_) => {}, 
                Err(e) => {
                    eprintln!("Postgres connection error: {}", e);
                    break;
                }
            }
        }
    });

    // 4. Start Listening
    client.execute("LISTEN roster_changes", &[]).await?;    
    println!(">>> System active: Waiting for roster_changes NOTIFY from database...");
    
    // 5. Event-Driven Loop
    while let Some(_notification) = rx.recv().await {
        // LOG: Detected the database signal
        println!(">>> [DATABASE EVENT] Change notification received!");

        let rows = client.query(
            "SELECT id, event_type, number, name, ao_datetime, event_time 
             FROM roster_events 
             ORDER BY event_time DESC 
             LIMIT 1", 
            &[]
        ).await?;
        
        for row in rows {
            let event = RosterChangeEvent {
                event_type: row.get(1),
                number: row.get(2),
                name: row.get(3),
                ao_datetime: row.get::<_, chrono::DateTime<chrono::Utc>>(4).to_string(),
                event_time: row.get::<_, chrono::DateTime<chrono::Utc>>(5).to_string(),
            };
            
            // LOG: Print details of what was found in the database
            println!(">>> [ROSTER CHANGE] Type: {}, Name: {}, Number: {}", 
                     event.event_type, event.name, event.number);
            
            let message = format_roster_change_message(&event);
            
            // LOG: Show exactly what will be posted to Discord
            println!(">>> [DISCORD PREVIEW]\n---\n{}\n---", message);

            let channel = ChannelId::new(roster_channel_id);
            if let Err(why) = channel.say(&http, &message).await {
                eprintln!(">>> [ERROR] Failed to send to Discord: {:?}", why);
            } else {
                println!(">>> [SUCCESS] Message posted to Discord channel: {}", roster_channel_id);
            }
        }
    }

    Ok(())
}

fn format_roster_change_message(event: &RosterChangeEvent) -> String {
    let emoji = match event.event_type.as_str() {
        "added" => "✅",
        "removed" => "❌",
        _ => "ℹ️",
    };
    
    let action = match event.event_type.as_str() {
        "added" => "**ADDED TO ROSTER**",
        "removed" => "**REMOVED FROM ROSTER**",
        _ => "ROSTER CHANGE",
    };
    
    format!(
        "{} {} {}\n**#{}** - {}",
        emoji, action, emoji,
        event.number, event.name
    )
}

#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    dotenv().ok();
    println!("Loaded env vars:");
    println!("DB_USERNAME: {}", std::env::var("DB_USERNAME").unwrap_or_else(|_| "NOT SET".to_string()));
    println!("ROSTER_CHANNEL_ID: {}", std::env::var("ROSTER_CHANNEL_ID").unwrap_or_else(|_| "NOT SET".to_string()));

    // Configure the client with your bot token
    let token = std::env::var("DISCORD_TOKEN")
        .expect("Expected a token in the environment variable DISCORD_TOKEN");

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES 
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_MESSAGE_REACTIONS;

    // Create a new instance of the Client, logging in as a bot
    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Error creating client");

    // Get HTTP client before moving client into start()
    let http = client.http.clone();

    // Spawn roster change listener in background
    tokio::spawn(async move {
        loop {
            if let Err(e) = listen_for_roster_changes(http.clone()).await {
                eprintln!("Roster change listener error: {:?}", e);
                eprintln!("Reconnecting in 5 seconds...");
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    });

    // Start the Discord bot
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}