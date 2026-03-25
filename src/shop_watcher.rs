use reqwest::Client;
use serde::Deserialize;
use serenity::http::Http;
use serenity::model::id::ChannelId;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

const SHOP_URL: &str = "https://tampabaysports.com/collections/sun/products.json?limit=250";
const CHECK_INTERVAL_SECS: u64 = 1800; // 30 minutes

#[derive(Debug, Deserialize)]
struct ShopifyResponse {
    products: Vec<ShopifyProduct>,
}

#[derive(Debug, Deserialize)]
struct ShopifyProduct {
    id: u64,
    title: String,
    handle: String,
    variants: Vec<ShopifyVariant>,
}

#[derive(Debug, Deserialize)]
struct ShopifyVariant {
    price: String,
}

impl ShopifyProduct {
    fn url(&self) -> String {
        format!("https://tampabaysports.com/products/{}", self.handle)
    }

    fn lowest_price(&self) -> Option<f64> {
        self.variants
            .iter()
            .filter_map(|v| v.price.parse::<f64>().ok())
            .reduce(f64::min)
    }
}

async fn fetch_products(client: &Client) -> anyhow::Result<Vec<ShopifyProduct>> {
    let resp: ShopifyResponse = client
        .get(SHOP_URL)
        .header("User-Agent", "MadiBot/1.0")
        .send()
        .await?
        .json()
        .await?;
    Ok(resp.products)
}

async fn ensure_table(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS shop_known_products (
            shopify_id BIGINT PRIMARY KEY,
            title      TEXT NOT NULL,
            handle     TEXT NOT NULL,
            first_seen TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert new products; returns only the ones that were actually new.
async fn find_and_store_new<'a>(
    pool: &PgPool,
    products: &'a [ShopifyProduct],
) -> anyhow::Result<Vec<&'a ShopifyProduct>> {
    let mut new_items = Vec::new();
    for product in products {
        let result = sqlx::query(
            "INSERT INTO shop_known_products (shopify_id, title, handle)
             VALUES ($1, $2, $3)
             ON CONFLICT (shopify_id) DO NOTHING",
        )
        .bind(product.id as i64)
        .bind(&product.title)
        .bind(&product.handle)
        .execute(pool)
        .await?;

        if result.rows_affected() > 0 {
            new_items.push(product);
        }
    }
    Ok(new_items)
}

fn format_alert(product: &ShopifyProduct) -> String {
    let price_str = product
        .lowest_price()
        .map(|p| format!("Starting at **${:.2}**\n", p))
        .unwrap_or_default();
    format!(
        "🛍️ **NEW SUN FC MERCH** 🛍️\n**{}**\n{}{}",
        product.title,
        price_str,
        product.url()
    )
}

pub async fn run(pool: PgPool, http: Arc<Http>, channel_id: u64) {
    let client = Client::new();

    if let Err(e) = ensure_table(&pool).await {
        eprintln!("[shop_watcher] Failed to create table: {}", e);
        return;
    }

    // Seed without alerting
    println!("[shop_watcher] Seeding existing products...");
    match fetch_products(&client).await {
        Ok(products) => {
            let new = find_and_store_new(&pool, &products).await.unwrap_or_default();
            println!(
                "[shop_watcher] Seeded {} products ({} were new to DB)",
                products.len(),
                new.len()
            );
        }
        Err(e) => eprintln!("[shop_watcher] Seed error: {}", e),
    }

    loop {
        sleep(Duration::from_secs(CHECK_INTERVAL_SECS)).await;

        println!("[shop_watcher] Checking for new Sun FC merch...");
        match fetch_products(&client).await {
            Err(e) => eprintln!("[shop_watcher] Fetch error: {}", e),
            Ok(products) => match find_and_store_new(&pool, &products).await {
                Err(e) => eprintln!("[shop_watcher] DB error: {}", e),
                Ok(new_items) => {
                    println!("[shop_watcher] {} new item(s) found", new_items.len());
                    let channel = ChannelId::new(channel_id);
                    for product in new_items {
                        let msg = format_alert(product);
                        println!("[shop_watcher] Alerting:\n{}", msg);
                        if let Err(e) = channel.say(&http, &msg).await {
                            eprintln!("[shop_watcher] Discord error: {:?}", e);
                        }
                    }
                }
            },
        }
    }
}
