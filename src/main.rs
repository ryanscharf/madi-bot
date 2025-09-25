use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use dotenv::dotenv;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        // Check if the message contains "madi parsons" (case insensitive)
        let content_lower = msg.content.to_lowercase();

        // Check if the message contains "madi" as a complete word or "madi parsons"
        let has_madi_word = content_lower.split_whitespace()
            .any(|word| word.trim_matches(|c: char| !c.is_alphabetic()) == "madi");
        let has_madi_parsons = content_lower.contains("madi parsons");

        if has_madi_word || has_madi_parsons {
            // React with the 🥵 emoji
            if let Err(why) = msg.react(&ctx.http, '🥵').await {
                println!("Error adding reaction: {:?}", why);
            }
        }
    }
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() {
    dotenv().ok(); // This loads the .env file

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