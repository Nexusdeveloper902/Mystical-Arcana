# Asset Attribution

Every third-party asset integrated into the build is recorded here with: source URL, license, attribution text, and engine-side normalization notes (scale, orientation, LOD generation).

| Asset | Source | License | Attribution | Notes |
|---|---|---|---|---|
| _(empty — assets added via Sketchfab pipeline will be recorded here)_ | | | | |

## License policy

- **Allowed:** CC0, CC-BY, CC-BY-SA, OFL (fonts), MIT, Apache-2.0, GPL family with attribution
- **Disallowed:** "no-derivs" licenses, ND variants, NC-only if commercial distribution is intended, assets with no explicit license
- **Sketchfab:** verify per-asset license in the asset's `license` field; downloading is not the same as redistributing rights

## Validation pipeline

Every asset passes through `Tools/asset_pipeline` before landing in `Assets/`:

1. License verification
2. Geometry validation (manifold check, normal integrity, polygon count budget)
3. Material reference validation (no missing textures)
4. Scale normalization (1 unit = 1 meter, +Y up)
5. LOD generation via `meshopt`
6. Collision mesh generation (where required)
7. Cooking into runtime-friendly format (`.mesh`, `.tex`, `.mat`)
8. Attribution record appended to this file
