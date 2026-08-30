# Runes — Visual Language, System, Combinations, Tablets, Iconography

> Subsystem spec — derived from `mystical-arcana-design.md`.

> Sections covered: 6, 15, 16, 17, 33.


---


# Runes are also a visual language

The project uses **Greek-inspired runic symbols** as the visual basis for the rune system.

That means runes should not look like generic fantasy glyphs randomly generated for decoration.

They should look like a coherent **written magical language**.

The player should eventually recognize:

> “That symbol means movement.”

or

> “That is the stabilization rune.”

before even reading a tooltip.

This language should appear throughout the game:

* UI
* spell effects
* tablets
* environmental ruins
* sanctuary structures
* research
* magical objects
* casting animations
* world markings

The rune language is therefore simultaneously:

**game mechanic + UI language + worldbuilding + visual identity.**

---


---

# Rune tablets

This is one of the important extensions we've made to the original system.

The player can obtain a **rune tablet**.

They can:

1. acquire a tablet
2. inscribe a rune onto it
3. carry the resulting magical object
4. place that rune into the world

That changes the meaning of runes dramatically.

A rune isn't necessarily something that lives exclusively in the player's inventory or spell wheel.

It can become **an object that exists in physical space.**

This lays the foundation for environmental magic.

---


---

# Rune system

Runes are data-driven.

The planned architecture uses ScriptableObjects containing information such as:

* ID
* category
* mana modifier
* effect type
* icon
* description



The player can equip runes into quick slots and have their effects modify the player's capabilities. 

But the broader vision is that runes eventually become the **building blocks of the entire magic system**.

---


---

# Rune combinations

Individual runes are words.

Combinations are sentences.

A combination can create a **Schematic**.

For example, the development prototype describes deterministic rune combinations producing spells such as a Fire + Pierce combination becoming a Fire Bolt. 

The important part is that magic is **compositional**.

Instead of designing hundreds of completely independent abilities, the system can derive magical behavior from combinations.

That creates room for experimentation.

---


---

# Rune iconography

The rune icons should be immediately recognizable.

They should use the same symbolic language as:

* world inscriptions
* tablets
* spell effects
* sanctuary markings

This creates visual continuity.

A rune discovered in the world should look exactly like the rune shown in the inventory.

A rune on a tablet should look like the rune in the UI.

A rune appearing during casting should be recognizably the same symbol.

---


---
