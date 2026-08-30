# Mystical Arcana

A first-person systemic fantasy survival, exploration, crafting, and magic
game where the player cannot rely on conventional tools — they must learn the
language of magic and use it to interact with the world itself.

This repo currently holds the **design documentation** for the project plus
the **engineering onboarding** for when the Unity implementation lands.

## Quick links

| Where | What |
|-------|------|
| [`docs/mystical-arcana-design.md`](./docs/mystical-arcana-design.md) | Index into per-subsystem specs |
| [`docs/systems/`](./docs/systems/) | 26 subsystem specs (source of truth) |
| [`docs/systems/prototypes/`](./docs/systems/prototypes/) | Implementation-ready expansions of specific subsystems |
| [`docs/roadmap.md`](./docs/roadmap.md) | Milestone M0–M6 development trajectory |
| [`docs/dev-setup.md`](./docs/dev-setup.md) | Engineering onboarding, branch hygiene, commit conventions |
| [`docs/_archive/`](./docs/_archive/) | Pre-split monolithic draft, preserved |

## Layout

```
.
├── docs/
│   ├── mystical-arcana-design.md     # index -> systems/
│   ├── systems/                       # 26 per-subsystem specs (source of truth)
│   ├── systems/prototypes/            # implementation-ready expansions
│   ├── _archive/                      # legacy / pre-split files
│   ├── roadmap.md                     # active development trajectory
│   └── dev-setup.md                   # engineering onboarding
├── download/                          # scratch dir for generated artifacts
├── scripts/                           # dev utilities (design-doc splitter, etc.)
├── skills/                            # (gitignored — local-only)
├── upload/                            # (gitignored — drop zone for pasted content)
├── .env                               # local environment
└── .gitignore
```

## Working with the design docs

1. Start at [`docs/systems/00-overview.md`](./docs/systems/00-overview.md) for
   the premise.
2. Read [`docs/systems/01-philosophy.md`](./docs/systems/01-philosophy.md) for
   the four pillars.
3. Jump to whatever subsystem your current task touches — each spec links to
   related ones where they overlap.
4. Before any design call, re-read
   [`docs/systems/25-principles.md`](./docs/systems/25-principles.md).
5. Before any engineering work, consult
   [`docs/roadmap.md`](./docs/roadmap.md) to know which milestone your task
   belongs to and [`docs/dev-setup.md`](./docs/dev-setup.md) for branch /
   commit conventions.

## Repo history (where the two directions merged)

The repo's first real merge brought together two parallel tracks that had
drifted apart:

- **Direction A (scaffold)** — `.env`, `.gitignore`, `download/README.md`
  committed in the initial commit and tightened in a follow-up.
- **Direction B (design)** — the full Mystical Arcana creative & product
  direction, committed on a feature branch.

See `git log --graph` for the merge commit that unified them.
