## Setup

Add a `.env` file in the project root with the variables below.

In the Discord bot page, toggle on **Message Content Intent**.

In the OAuth2 URL generator, check `bot` and in bot permissions check: `Read Message History`, `Add Reactions`, `Send Messages`.

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
| `GAME_NOTES_CHANNEL_ID` | Discord channel ID where game notes are posted |
| `SHOP_CHANNEL_ID` | Discord channel ID where new shop product alerts are posted |

### Optional (have defaults)

| Variable | Default | Description |
|---|---|---|
| `DB_HOST` | `192.168.2.66` | PostgreSQL host |
| `DB_PORT` | `5432` | PostgreSQL port |
| `DB_NAME` | `tb_sun` | PostgreSQL database name |
| `TWITTER_EMBED_DOMAIN` | `xcancel.com` | Domain used when converting Twitter/X links for embedding |
| `INSTAGRAM_EMBED_DOMAIN` | `zzinstagram.com` | Domain used when converting Instagram links for embedding |
