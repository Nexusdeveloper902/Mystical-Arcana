# Technical Identity & Performance Philosophy

> Subsystem spec — derived from `mystical-arcana-design.md`.

> Sections covered: 40, 41.


---


# Technical identity

Underneath the visual experience, the project is deliberately being structured to remain extensible.

The project uses:

* Unity
* URP
* Input System
* Cinemachine
* ScriptableObjects
* data-driven spells/runes
* procedural generation
* chunk streaming
* LOD
* VFX pooling
* Unity Version Control / Plastic SCM

The development conventions also establish `MysticalArcana.*` namespaces, `[SerializeField] private` Inspector exposure, and controlled singleton use for core managers. 

The architecture should make adding:

> new rune → new spell → new enemy → new resource → new biome

possible without rewriting the entire game.

The release checklist explicitly calls for a content pipeline where new runes, spells, and enemies can be added without code changes. 

---


---

# Performance philosophy

The goal isn't photorealism at any cost.

It's **stylized visual quality with systemic complexity**.

That means prioritizing:

* stable framerate
* efficient VFX
* LOD
* optimized assets
* chunk streaming
* controlled draw calls
* avoiding unnecessary allocations
* scalable world generation

The planned target is stable 60 FPS on target hardware. 

---


---
