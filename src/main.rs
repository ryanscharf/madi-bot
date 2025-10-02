use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use rand::Rng;
use dotenv::dotenv;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        let content_lower = msg.content.to_lowercase();
        
        // Check if the message contains "madi" as a complete word or "madi parsons"
        let has_madi_word = content_lower.split_whitespace()
            .any(|word| word.trim_matches(|c: char| !c.is_alphabetic()) == "madi");
        let has_madi_parsons = content_lower.contains("madi parsons");
        
        if has_madi_word || has_madi_parsons {
            // Define all possible reaction options as an array
            let reactions: [&[char]; 10] = [
                &['🥵'],
                &['😍'],
                &['💖'],
                &['🥹'],
                &['🤤'],
                &['😋'],
                &['🤠'],
                &['💪'],
                &['🇲', '🇦', '🇩', '🇮'],  // MADI
                &['🇾', '🇪', '🇸'],        // YES
            ];
            
            // Generate random numbers in a scope that ends before await
            let selected_reactions: Vec<&[char]> = {
                let mut rng = rand::thread_rng();
                let num_reactions = rng.gen_range(1..=3);
                let mut selected = Vec::new();
                let mut used_indices = Vec::new();
                
                while selected.len() < num_reactions {
                    let idx = rng.gen_range(0..reactions.len());
                    if !used_indices.contains(&idx) {
                        used_indices.push(idx);
                        selected.push(reactions[idx]);
                    }
                }
                selected
            }; // rng is dropped here, before any await
            
            // Now do the async operations
            for reaction_set in selected_reactions {
                for emoji in reaction_set {
                    if let Err(why) = msg.react(&ctx.http, *emoji).await {
                        println!("Error adding reaction: {:?}", why);
                        break;
                    }
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
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

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