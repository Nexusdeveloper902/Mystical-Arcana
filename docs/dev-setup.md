# Dev Setup

This file is the engineering onboarding for Mystical Arcana. It assumes a
single contributor on a workstation; team-mode variations live at the bottom.

## 1. Prerequisites

| Tool | Version | Why |
|------|---------|-----|
| Unity Editor (LTS) | 2022.3 LTS or newer | URP target per [`systems/20-technical.md`](./systems/20-technical.md) |
| Universal RP package | matches Unity version | stylized rendering + performance |
| Git | ≥ 2.40 | branch hygiene |
| Git LFS | ≥ 3.4 | binary assets (textures, audio) |
| Python | ≥ 3.11 | design-doc tooling (see `scripts/`) |
| Node.js | ≥ 20 LTS | optional: doc tooling, asset pipelines |

## 2. Clone and initial setup

```bash
git clone https://github.com/Nexusdeveloper902/Mystical-Arcana.git
cd Mystical-Arcana
git lfs install   # one-time per machine

# design doc tooling
pip install -r scripts/requirements.txt   # (only if/when scripts gain deps)
```

## 3. Repo layout

```
.
├── docs/
│   ├── mystical-arcana-design.md     # index → systems/
│   ├── systems/                       # per-subsystem specs (the source of truth)
│   ├── _archive/                      # legacy / deprecated / pre-split files
│   └── roadmap.md                     # active development trajectory
├── download/                          # scratch dir for generated artifacts (gitignored mostly)
├── scripts/                           # dev utilities (design-doc splitter, etc.)
├── skills/                            # (gitignored — local-only)
├── upload/                            # (gitignored — drop zone for pasted content)
├── .env                               # local environment (DATABASE_URL, etc.)
├── .gitignore
└── README.md
```

## 4. Reading the design docs

Always start at [`docs/mystical-arcana-design.md`](./docs/mystical-arcana-design.md)
— it is now an index into [`docs/systems/`](./docs/systems/). Each subsystem
file carries a one-line description and a "Sections covered" footer pointing
back to the original monolithic draft (preserved in
[`docs/_archive/mystical-arcana-design.md.bak`](./docs/_archive/mystical-arcana-design.md.bak)).

When a subsystem file references another subsystem, it links to it. Treat the
systems directory as a small dependency graph; the canonical reading order is:

1. [`00-overview.md`](./docs/systems/00-overview.md)
2. [`01-philosophy.md`](./docs/systems/01-philosophy.md)
3. Whatever subsystem your current task touches
4. [`25-principles.md`](./docs/systems/25-principles.md) before any design call

## 5. Branch hygiene

- `main` is always shippable to a vertical-slice state.
- Feature work happens on `feature/<area>-<topic>` branches.
- Design changes happen on `docs/<area>-<topic>` branches.
- Hotfixes go on `hotfix/<area>-<topic>` and are merged fast-forward into `main`.
- A merge into `main` always uses `--no-ff` so the merge commit acts as a
  chapter marker. (The initial scaffold/design merge is the template for this
  convention.)

## 6. Commit message convention

```
<type>(<area>): <subject>

<body — wrap at 72 chars, explain why not what>

<footer — refer to subsystem file or roadmap row>
```

`type` ∈ `feat`, `fix`, `docs`, `chore`, `refactor`, `perf`, `test`, `build`.
`area` is the subsystem slug, e.g. `runes`, `mana`, `world`, `ui`, `vfx`,
`sound`, `persistence`, `accessibility`, `technical`.

Example:

```
feat(mana): stabilize field at sanctuary tile boundary

When a player places a sanctuary tile on a non-flat surface, the mana field
regen radius was using the tile origin rather than the surface centroid,
causing regen to extend into adjacent corrupted zones.

Closes M4.2 on docs/roadmap.md.
Refs: docs/systems/06-mana.md, docs/systems/12-building.md
```

## 7. Updating the design docs

The subsystem specs are the source of truth. If you discover an ambiguity
during implementation:

1. Update the relevant subsystem file in the same PR as your fix.
2. If the change moves roadmap rows, update `docs/roadmap.md` too.
3. If the change is large enough to invalidate the original monolithic draft,
   add a note to the subsystem file pointing to a "design delta" entry in
   `docs/_archive/`. Do **not** delete the archive copy.

## 8. Regenerating per-subsystem files from the original draft

If the canonical monolithic draft is amended (rare), re-run the splitter:

```bash
python scripts/split_design_doc.py
```

The splitter is idempotent: it will overwrite `docs/systems/*.md` and refresh
`docs/mystical-arcana-design.md` as an index. It will also refresh the archive
copy at `docs/_archive/mystical-arcana-design.md.bak`.

## 9. Team-mode variations

- Add a `CONTRIBUTING.md` mirroring sections 5–7 above.
- Protect `main` with required reviews and status checks.
- For binary assets, ensure Git LFS patterns are agreed project-wide; recommended
  tracked extensions: `.png`, `.psd`, `.tif`, `.wav`, `.mp3`, `.fbx`, `.unity`,
  `.asset`, `.mat`, `.prefab`, `.controller`, `.anim`.

## 10. Engine-side project layout (when the Unity project is added)

When the Unity project is committed (currently not in repo), it should live at
`unity/MysticalArcana/` so the repo root stays design-first. The Unity project
must respect the same subsystem decomposition: each subsystem spec under
`docs/systems/` should map to a folder namespace under `Assets/Scripts/`, e.g.

```
Assets/Scripts/Runes/         -> docs/systems/03-runes.md
Assets/Scripts/Mana/         -> docs/systems/06-mana.md
Assets/Scripts/World/        -> docs/systems/05-world.md
Assets/Scripts/Combat/       -> docs/systems/11-combat.md
Assets/Scripts/UI/           -> docs/systems/15-ui.md
...
```
