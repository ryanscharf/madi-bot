use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::id::EmojiId;
use serenity::model::channel::ReactionType;
use serenity::prelude::*;
use rand::Rng;
use dotenv::dotenv;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore messages from bots (including itself)
        if msg.author.bot {
            return;
        }
        
        let content_lower = msg.content.to_lowercase();
        println!("Received message: {}", content_lower);
        
        // Check for :AC: emoji to complete "ACTIVATED"
        if msg.content.contains("<:AC:1460415544229363944>") {
            println!("Detected AC emoji, completing ACTIVATED sequence");
            
            let activated_emojis = vec![
                ReactionType::Custom {
                    animated: false,
                    id: EmojiId::new(1460415567457554483),
                    name: Some("TI".to_string()),
                },
                ReactionType::Custom {
                    animated: false,
                    id: EmojiId::new(1460415586399027200),
                    name: Some("VA".to_string()),
                },
                ReactionType::Custom {
                    animated: false,
                    id: EmojiId::new(1460415609664835807),
                    name: Some("TE".to_string()),
                },
                ReactionType::Custom {
                    animated: false,
                    id: EmojiId::new(1460415630074449960),
                    name: Some("D_".to_string()),
                },
            ];
            
            for emoji in activated_emojis {
                if let Err(why) = msg.react(&ctx.http, emoji).await {
                    println!("Error adding ACTIVATED reaction: {:?}", why);
                    break;
                }
            }
            return; // Don't check for madi if we already handled AC
        }
        
        // Check if the message contains "madi" as a complete word or "madi parsons"
        let has_madi_word = content_lower.split_whitespace()
            .any(|word| word.trim_matches(|c: char| !c.is_alphabetic()) == "madi");
        let has_madi_parsons = content_lower.contains("madi parsons");
        
        println!("Has madi word: {}, Has madi parsons: {}", has_madi_word, has_madi_parsons);
        
        if has_madi_word || has_madi_parsons {
            // Define all possible reaction options
            // Custom madi_knife emoji
            let custom_madi_knife = ReactionType::Custom {
                animated: false,
                id: EmojiId::new(1421229718023442563),
                name: Some("madi_knife".to_string()),
            };
            
            // Unicode emoji reactions (simple)
            let reactions_unicode: Vec<Vec<char>> = vec![
                vec!['🥵'],
                vec!['😍'],
                vec!['💖'],
                vec!['🥹'],
                vec!['🤤'],
                vec!['😋'],
                vec!['🤠'],
                vec!['💪'],
                vec!['🇲', '🇦', '🇩', '🇮'],  // MADI
                vec!['🇾', '🇪', '🇸'],        // YES
            ];
            
            // Generate random selection in a scope that ends before await
            let (selected_unicode, use_custom): (Vec<Vec<char>>, bool) = {
                let mut rng = rand::thread_rng();
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
                
                // 20% chance to also add the custom emoji
                let add_custom = rng.gen_range(0..100) < 20;
                (selected, add_custom)
            }; // rng is dropped here
            
            // Add unicode reactions
            for reaction_set in selected_unicode {
                for emoji in reaction_set {
                    if let Err(why) = msg.react(&ctx.http, emoji).await {
                        println!("Error adding reaction: {:?}", why);
                        break;
                    }
                }
            }
            
            // Optionally add custom emoji
            if use_custom {
                if let Err(why) = msg.react(&ctx.http, custom_madi_knife).await {
                    println!("Error adding custom reaction: {:?}", why);
                }
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    dotenv().ok();
    
    // Configure the client with your bot token
    let token = std::env::var("DISCORD_TOKEN")
        .expect("Expected a token in the environment variable DISCORD_TOKEN");

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES 
        | GatewayIntents::DIRECT_MESSAGES;

    // Create a new instance of the Client, logging in as a bot
    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Error creating client");

    // Finally, start a single shard, and start listening to events
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}