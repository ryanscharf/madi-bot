## What this bot does

- **Reactions & link conversion** — reacts to messages mentioning "madi"/"activate(d)", and rewrites Instagram links (always) and X/Twitter links (opt-in, see `TWITTER_LINK_CONVERSION_ENABLED`) to embed-friendly domains.
- **Roster change alerts** — listens on a Postgres `LISTEN`/`NOTIFY` channel (`roster_changes`) for JSON payloads describing roster additions/removals and posts them to `ROSTER_CHANNEL_ID`. The trigger that emits these notifications lives outside this repo.
- **Post-match survey watcher** — polls the Tampa Bay Sun FC survey page every 15 minutes; when the survey's match-date dropdown changes, posts a notification to `SURVEY_CHANNEL_ID`.
- **Shop watcher** — polls the Tampa Bay Sun FC Shopify product feed every 30 minutes and posts new merch to `SHOP_CHANNEL_ID`.
- **Game notes watcher** — polls the USL Super League game notes page every 30 minutes for new/updated Tampa Bay Sun FC game notes PDFs, posts an alert to `GAME_NOTES_CHANNEL_ID`, and separately posts a cropped screenshot of the **Player Availability** table for both Tampa Bay and the upcoming opponent (each team's screenshot is its own message so Discord doesn't cram them into small thumbnails). If the opponent hasn't posted their own notes yet, it keeps retrying on later checks (for up to 10 days) and posts their availability as a follow-up once it's out.

---

## Setup

Add a `.env` file in the project root with the variables below.

In the Discord bot page, toggle on **Message Content Intent**.

In the OAuth2 URL generator, check `bot` and in bot permissions check: `Read Message History`, `Add Reactions`, `Send Messages`, `Attach Files` (needed for the game notes availability screenshots).

### Database

None of the tables below are created automatically — run these once against the Postgres database pointed to by `DB_HOST`/`DB_NAME` before starting the bot:

```sql
-- Shop watcher
CREATE TABLE IF NOT EXISTS shop_known_products (
    shopify_id BIGINT PRIMARY KEY,
    title      TEXT NOT NULL,
    handle     TEXT NOT NULL,
    first_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Post-match survey watcher
CREATE TABLE IF NOT EXISTS public.survey_info (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Game notes watcher
CREATE TABLE IF NOT EXISTS game_notes_documents (
    url             TEXT PRIMARY KEY,
    title           TEXT NOT NULL,
    opponent_posted BOOLEAN NOT NULL DEFAULT TRUE,
    first_seen_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

`opponent_posted`/`first_seen_at` track whether the opponent's own availability screenshot still needs to be retried (their notes are often posted later than Tampa's own) — see the Game notes watcher description above.

The roster change listener doesn't own a table here; it expects some other system to run `pg_notify('roster_changes', '<json>')` with a payload matching `RosterChangeEvent` (`event_type`, `number`, `name`, `ao_datetime`, `event_time`).

### Game notes screenshots (libpdfium)

The game notes watcher renders PDF pages to crop out the availability table, via the `pdfium-render` crate binding to a native `libpdfium` library at runtime.

- **Docker**: handled automatically — the `dockerfile` downloads and installs `libpdfium.so` into the image during build.
- **Local development** (running `cargo run`/`cargo test` directly, not via Docker): you need `libpdfium` on your machine yourself. Download a prebuilt binary from [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries/releases) for your OS (`pdfium-win-x64.tgz`, `pdfium-linux-x64.tgz`, `pdfium-mac-*.tgz`, etc.) and place the `pdfium.dll`/`libpdfium.so`/`libpdfium.dylib` from its `bin`/`lib` folder somewhere it'll be found at runtime (next to the built binary, or a directory on your system's library search path).

---

## Environment Variables

### Required

| Variable | Description |
|---|---|
| `DISCORD_TOKEN` | Discord bot token from the Discord developer portal |
| `DB_USERNAME` | PostgreSQL username |
| `DB_PASSWORD` | PostgreSQL password |
| `ROSTER_CHANNEL_ID` | Discord channel ID where roster update messages are posted |
| `SURVEY_CHANNEL_ID` | Discord channel ID where survey update messages are posted |
| `GAME_NOTES_CHANNEL_ID` | Discord channel ID where game notes (and availability screenshots) are posted |
| `SHOP_CHANNEL_ID` | Discord channel ID where new shop product alerts are posted |

### Optional (have defaults)

| Variable | Default | Description |
|---|---|---|
| `DB_HOST` | `192.168.2.66` | PostgreSQL host |
| `DB_PORT` | `5432` | PostgreSQL port |
| `DB_NAME` | `tb_sun` | PostgreSQL database name |
| `TWITTER_LINK_CONVERSION_ENABLED` | `false` | Enable rewriting Twitter/X links to `TWITTER_EMBED_DOMAIN`. Disabled by default since xcancel.com is no longer reliable. Set to `true`/`1`/`on` to enable |
| `TWITTER_EMBED_DOMAIN` | `xcancel.com` | Domain used when converting Twitter/X links for embedding (only when `TWITTER_LINK_CONVERSION_ENABLED` is true) |
| `INSTAGRAM_EMBED_DOMAIN` | `zzinstagram.com` | Domain used when converting Instagram links for embedding |
