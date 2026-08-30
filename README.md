# Mystical Arcana

Repo consolidating two parallel tracks of work that drifted apart:

1. **Project scaffold** (on `main`): environment, ignore rules, download scratch area.
2. **Creative / product direction** (now merged from `feature/mystical-arcana-design`):
   the complete design identity document for the game.

## Layout

| Path | Purpose |
|------|---------|
| `docs/mystical-arcana-design.md` | The full creative & product direction document. |
| `download/` | Scratch area for generated artifacts. |
| `.env` | Local environment (e.g. `DATABASE_URL`). |
| `.gitignore` | Ignores `skills/`, `node_modules/`, and `upload/` (drop zone for pasted content). |

## Working with the design doc

Open `docs/mystical-arcana-design.md` directly. It is plain Markdown with embedded
images hosted on OpenAI's CDN, so no binary assets need to live in the repo.
