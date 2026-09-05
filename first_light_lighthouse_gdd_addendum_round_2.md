# FIRST LIGHT
## GDD addendum 01 — Second prototype round

**Date:** 5 September 2026  
**Applies to:** `first_light_lighthouse_prototype_gdd.md`, version 0.1, and implementations based on it.  
**Purpose:** Change the existing prototypes, not rebuild them. This addendum takes precedence wherever it conflicts with the original GDD. Unmentioned requirements remain in force, except requirements belonging to a suspended or explicitly replaced system.  
**Decision status:** Requirements below are the implementation target. Items labelled **prototype default** resolve remaining questions for this build; they are not settled final design decisions. All numerical tuning is provisional.

---

## 1. Mode selection and scope overrides

Replace the original requirement for three active modes numbered 1–3 with these three menu entries:

| Internal mode | Menu name | This round |
|---|---|---|
| **1** | **Night Watch** | Improve the live, multi-ship navigation game; add a ship-sized, plankton-eating predator. |
| **2** | **Mutable Sea** | **Freeze and hide.** Retain existing implementation where practical, but remove it from the normal menu and stop developing it. |
| **3** | **World Weaver** | Refocus as a maze assembler: copy sectors into World 1, then test any valid shipping-lane-to-harbor route at dawn. |
| **4** | **Spiral Voyage** | Add a live voyage through four worlds, using Mode 1's spotlight, plankton and ship guidance, without a monster. |

Keep existing internal IDs rather than renumbering saved scenarios or debug selectors. Player-facing menu labels need not display numbers or an empty Mode 2 slot. No active mode should inherit Mutable Sea's transformation timers or rules.

### Explicitly superseded parts of the original GDD

- **Sections 1–3 and 12:** Menu, controls and required-mode scope change as described here. Mode 2 is no longer required to be playable from the menu.
- **Sections 4–6:** Update visibility, geography, ship guidance, introduction and predator behavior. Ordinary unseen simulation remains predictable.
- **Section 7:** Suspended, not an instruction to add transformations to another mode.
- **Section 8:** Replace the predetermined voyage, form-based encounter puzzle and separate final-world concept with the Mode 3 specification below.
- **Sections 10–14:** Adjust tuning, system responsibilities, development order and tests to support these changes. Obsolete Mode 2 and fixed-voyage requirements are not acceptance criteria.

The art direction, fixed camera, keyboard-first interface, limited weekend scope and absence of a required runtime AI remain unchanged.

---

## 2. Shared changes for live navigation: Modes 1 and 4

### 2.1 Keep one spotlight and plankton implementation

Modes 1 and 4 must use the **same localized spotlight footprint, physical near/far control, charge accumulation, decay and visibility behavior**. Share the implementation and initial tuning rather than creating subtly different handling for each mode.

| Input | Mode 1 — Night Watch | Mode 3 — World Weaver | Mode 4 — Spiral Voyage |
|---|---|---|---|
| **A / D** | Turn spotlight left / right | Wind backward / forward through candidate worlds | Turn spotlight and wind backward / forward through the spiral |
| **W / S** or **Up / Down** | Move footprint farther / nearer on the water | Unused | Move footprint farther / nearer on the water, exactly as in Mode 1 |
| **Space** | Unused | Copy the previewed sector into World 1 | Unused |
| **Esc** | Pause | Pause | Pause |

Mode 3 retains its full radial-sector preview. **Mode 4 does not:** only a localized patch is illuminated and charged. Its visible beam shaft is not a gameplay-lit radial road.

**Prototype default — beam handling:** Use free, held-input rotation with a capped angular speed and modest acceleration/deceleration. Releasing the controls brings the beam to rest promptly. The intent is deliberate handling, not prolonged overshoot or forced oscillation. Keep constant-speed autorotation/reversal as a developer comparison setting, not the default or another menu variant. Share these settings between Modes 1 and 4.

Longer dwell still stores more energy and buys longer afterglow. There is no additional fuel meter, charge button or free permanent trail. Time spent charging one place remains time not spent inspecting another.

### 2.2 Dusk introduction instead of blind initial searching

**Mode 1 must start visibly at dusk and fade into gameplay darkness over approximately two seconds.** The initial boats and island layout must already be present and readable during this glimpse. Do not fade to darkness first and spawn the first boat afterward.

Fade the ambient illumination, not an opaque black overlay that also hides the lighthouse and controls. The beam takes over as the surroundings darken.

**Prototype default:** Let the player aim during the fade, but start boat movement, charge accumulation and the night timer when it completes. Start with two boats visible; retain the later overlapping arrival schedule. Reuse this introduction in Mode 4, revealing only its starting world and ship—not all four worlds.

### 2.3 Afterglow reveals silhouettes, not just empty water

Ships, the monster and rocks must remain readable **against charged plankton even after the direct beam has moved away**. This is normal play information, not a debug reveal.

Render opaque dark hulls, fins and rock shapes against luminous water. Do not hide an entire entity merely because its center is outside the direct spotlight. Visibility should reflect the illuminated water around the visible portion of its silhouette; stronger surrounding glow gives a clearer outline, and fading glow gradually loses it.

For land, evaluate the shoreline rather than only the island's center. A player can trace a shore with light and leave a temporary readable coastline. Land itself does not store plankton charge, and lighting one shore does not reveal the whole island.

This is intentionally a tradeoff: **shoreline painting helps the player remember a hazard but may also attract a ship dangerously close to it.** Do not turn shoreline markings into special non-attracting light.

The desired-heading indicator described below follows the ship's visibility. It must not reveal a hidden ship or its current intentions through darkness or another world.

### 2.4 Replace scattered obstacle dots with small island clusters

Use groups of rocks that form small islands, reefs and recognizable coastlines. A few isolated rocks are fine, but they must not be the whole level structure.

Simple overlapping collision shapes are sufficient. Match the visible silhouettes, prevent apparent water gaps that are actually solid, and give intended channels enough width for the ship and its turning behavior. The harbor approach must be unambiguous.

This replaces the original literal “five fixed rocks” setup. Favor a few identifiable clusters and alternate passages over a dense random obstacle field.

---

## 3. Shared ship-guidance replacement: Modes 1 and 4

### 3.1 Look ahead, not only immediately beside the bow

The ship should scan farther afield to determine the **most promising lit direction**. The player must be able to paint ahead of a vessel and leave it to sail, especially in Mode 4 while scouting another world.

Evaluate several candidate headings across a broad forward arc. For each, inspect a short corridor of charged water extending several ship-lengths ahead. Prefer useful sustained illumination along a direction, rather than simply chasing the single brightest nearby point. A moderately bright continuous trail should often beat an isolated saturated dot.

**Prototype starting points:** Approximately a 150-degree forward arc and a lookahead of six ship-lengths. Expose both. Tune lookahead against speed and turning ability so a ship can notice and execute a turn before passing it.

Retain the original anti-orbit behavior: passed patches must not continuously pull the ship backward, and reaching a bright patch is not an instruction to circle or stop there. Without a sufficiently useful illuminated direction, continue on the last accepted heading.

Existing sparse guidance samples may support this calculation, but they must read current charge and must not substitute an invisible, permanent waypoint route for the visible plankton.

### 3.2 Separate intention, commitment and physical turning

Maintain distinct **desired heading** and **actual hull heading**.

- **Choice:** The light-reading system evaluates directions frequently and can select a new desired heading immediately.
- **Hysteresis:** Retain the current intended direction when alternatives are nearly equal. A competitor must be meaningfully better to displace it. Re-evaluate the incumbent against the current light field; a dead trail must not retain permanent priority.
- **Motion:** The hull turns gradually toward the accepted direction while continuing forward. It does not snap, pivot in place or stop safely whenever the player looks elsewhere.

Hysteresis belongs to the decision; inertia belongs to the ship. Do not simulate both by making every stage sluggish. Once a new choice is accepted, its indicator changes immediately, even though the hull takes time to follow.

**Prototype starting points:** Reconsider guidance about 10–20 times per second, try a switching advantage around 20%, and a maximum hull turn rate around 25–35 degrees per second. Treat these as accessible tuning variables, not fixed balance. No wind, sails or hydrodynamics simulation is required.

### 3.3 Do not give these ships a maze solver

Modes 1 and 4 follow painted light, not a globally calculated harbor route. They must still be capable of grounding if the player paints a bad approach or asks for a turn too late.

A small immediate-obstacle rejection check may be retained as a tunable safeguard, but it must not generate detours or repair the player's route. Local light sampling must not consult remote unseen worlds for a better path.

**Mode 3 uses a separate navigation policy.** Its dawn route finder is not a replacement for this system.

### 3.4 Desired-heading dial line

Add a tasteful, visible-by-default debug indicator to each observable ship: a thin, understated line extending roughly one to two ship-lengths from the ship toward its **currently accepted desired heading**. A tiny endpoint or dial tick is optional; a large gauge is not.

The line changes immediately when the desired heading changes. The hull then visibly turns toward it. Do not point the line toward a remote waypoint if that differs from the steering direction actually being requested.

This is intended to distinguish:

> The ship has misunderstood my painted route.

from:

> The ship understands, but cannot turn quickly enough.

Provide a developer toggle, but keep it enabled in ordinary prototype play. Its survival as a final visual element is an open art-direction question, not a reason to hide it now.

---

## 4. Mode 1 — Night Watch changes

Keep the original rescue-before-dawn structure, multiple simultaneous ships, finite afterglow and continuous simulation in darkness. A lost ship remains a lost rescue rather than an immediate end to the whole run.

The next test is whether **multiple responsibilities plus one predator** make route painting interesting. Do not add extra ship classes, wind, currents or more monsters to compensate before testing this combination.

### Ship-sized plankton eater

The monster should be approximately ship-sized, not an enormous leviathan. Give it a distinct silhouette that can still be confused with ordinary activity at a glance, without implementing a disguise or false-ship mechanic.

Use this behavior for the next prototype:

| Situation | Behavior |
|---|---|
| Detectable charged water nearby | Move toward the strongest useful local glow, with target persistence to prevent twitching. |
| Passing through plankton | Consume or sharply dim charge locally along its track. The visible trail and ship guidance both lose that charge. |
| Contact with a ship | Sink that ship. No player attack action is added. |
| No detectable glow | Continue its last course or cruise slowly and predictably. |
| Land obstruction | Stop, slide or turn along it consistently; never travel through islands. |

**Prototype default:** Attract it using stored plankton charge only. Do not also add omniscient ship hunting, direct-beam attraction or repulsion in this round. Freshness is already partly expressed by remaining charge; a separate trail-age scoring system is unnecessary initially.

Use a finite detection radius. The creature does not choose the globally brightest patch across the entire map. Consumption is local and continuous, not instantaneous erasure of a whole connected road. Painting can replenish a patch, but camping there must not effortlessly neutralize the creature.

The intended interaction is:

> A durable route buys time for the sailor, but makes a tempting meal for the creature. A brighter decoy elsewhere can redirect it.

The creature both threatens ships and damages their temporary navigation infrastructure. A partially extinguished road is meaningful evidence even when the predator itself is no longer visible.

**Prototype default:** Introduce the single monster after the player has had an initial opportunity to guide boats. Start its speed near ship speed and tune detection and consumption so decoys work. Retain a monster-off developer setting to compare the same scenario, not as a separate mode.

---

## 5. Mode 3 — World Weaver becomes the maze assembler

### 5.1 World 1 is the assembled world

Use four worlds and the existing reversible spiral selector. **World 1 is now the editable destination and the only world evaluated at dawn.** Worlds 2–4 contain stable alternative sectors.

**Prototype default — copy, not swap:** Pressing Space while previewing a sector in Worlds 2–4 copies that sector's maze geometry into the matching sector of World 1. The source remains unchanged. Re-copying overwrites that destination sector. This is not a move, swap, blend or automatic edit caused by browsing.

World 1 begins with a fixed baseline maze. Unedited sectors retain that baseline. Inspecting World 1 shows the **current assembled result**, not an immutable original candidate. Space there can simply do nothing with brief “Assembled world” feedback.

Keep an immutable baseline for restart, separate from the mutable assembled World 1. Harbor and shipping-lane markers are fixed scenario elements and are never overwritten by a sector copy.

### 5.2 Selection visibility and memory

**Prototype default:** After a copy, mark the sector at the **outer edge of the selector**. A small persistent tick or dot is sufficient; a compact source-world glyph is optional.

While viewing another world, do not overlay the selected World 1 geometry, illuminate the entire saved sector or display a full assembled-map thumbnail. The player can wind back to World 1 and inspect it through the sector beam to verify or refresh their mental model.

The marker answers “Have I edited this sector?” It does not reveal its complete saved contents. Viewing a different candidate never silently changes the marker's meaning or the saved geometry.

This deliberately selects one baseline from the options discussed. Always-visible selected sectors and swapping are **not additional features to implement now**.

### 5.3 Known endpoints, freely chosen route

Replace the original prescribed winding voyage and fixed waypoint line.

The player knows the **shipping-lane entrance and harbor location before construction**. Their objective is to assemble any ship-navigable connection between them. Do not show a mandatory intermediate route or demand a particular sequence of selected sectors.

**Prototype default:** Use one ship and a deterministic, marked starting position within the shipping-lane entrance. Do not introduce a hidden random starting offset after the player finishes building. Several known entry positions can be a later puzzle extension.

Place obstacles and endpoints so navigation meaningfully involves several sectors, not one trivially clear radial strip. Author at least two successful sector combinations. The alternatives should relocate land and change how passages connect, rather than offer an obviously empty “best sector” at every angle.

This is the intended middle ground: **many valid solutions to a concrete journey**, rather than matching a predetermined route or constructing a maze with no purpose.

### 5.4 Simplify the piece vocabulary

For this round, the puzzle is about **water, clustered land/rocks and connected passages**. Remove the requirement for selected ships to join a convoy, wrecks to impose salvage delays, or monsters to hunt during playback. The original ship/wreck/creature/island transformation vocabulary is not necessary to this mode's current test.

Copy geometry and collision data together. Keep pieces whole at sector boundaries, and ensure adjacent selections can form real navigable channels. Do not add instability penalties, special seam hazards or “similar realities must be neighbors” rules.

### 5.5 FIRST LIGHT and route-finding playback

When the editing night ends:

1. Disable editing and freeze the exact current World 1 composition.
2. Show a large, tasteful **FIRST LIGHT** title as dawn reveals all of World 1.
3. Find a route from the known ship start to the harbor through that assembled world's navigable water.
4. Sail the ship automatically and resolve the result. There is no player steering during playback.

A simple grid-based route finder is sufficient. Account for hull clearance, not just a point fitting through a gap. Use a path-following controller suited to the resulting route; do not make it follow plankton or the old fixed voyage waypoints.

**Outcomes:** Harbor reached means success. Grounding produces a visible sinking. If no route exists, give a clear “No passage to harbor” failure and a short failure/sinking beat rather than indefinite wandering. Never invent a route through solid land.

A valid route must not routinely fail because smoothing cuts a corner or the autopilot cannot execute its own path. Those are implementation problems, not extra puzzle difficulty. All routes accepted by the navigation model should be executable by its ship controller.

Keep playback brief, approximately the original 30–45-second target. Use sufficiently short level distances or consistent simulation speedup; do not turn this target into an arbitrary failure for an otherwise valid longer solution. The reveal must show the exact saved composition, not regenerated or repaired geometry.

---

## 6. Mode 4 — Spiral Voyage

### 6.1 Core requirement

**One ship must sail safely through World 1 → World 2 → World 3 → World 4, whose harbor is the destination.** This is live seafaring through the spiral, not copying maze pieces or preparing a later autoplay journey.

Reuse Mode 1's localized spotlight, charge field behavior, silhouettes, clustered islands, lookahead guidance, hysteresis, ship inertia and desired-heading line. **There is no monster.** There are no spontaneous entity transformations either.

The attention tradeoff is between tending the current voyage and inspecting/preparing what comes next. The ship continues sailing while the player looks into another world; its previously charged water continues fading there.

### 6.2 Prototype default for crossing between worlds

The exact crossing mechanism was not fully settled. Use this explicit default to preserve the spiral concept without inventing a second focus-control scheme:

- Reuse accumulated angular winding. A complete clockwise circuit advances to the next world at a clearly marked seam; reversing retraces the preceding world.
- Track the **ship's winding separately from the beam's winding**. The ship changes world by sailing across the seam, not because the player rotates the light elsewhere.
- In Mode 4 the voyage is finite, from World 1 to World 4. Do not cyclically wrap World 4 back to World 1 as the Mode 3 candidate browser may do.
- W/S remains physical near/far aiming within the inspected sea. It does **not** become a world-selection slider.

Thus the player can wind the spotlight ahead into World 2 while the ship still sails in World 1, then return to tend it. The player's beam and the vessel need not occupy the same world.

Treat seam crossings as continuous travel: preserve the ship's heading, speed and physical radius. Author clear approaches on both sides of the seam, and prevent sailing outside the finite beginning/end of the voyage. No arbitrary teleport buttons, scene-reset transitions or costly portals are required.

This crossing rule is a prototype choice to test, not a commitment that every later version must use full-circuit progression.

### 6.3 World-local light and visibility

Each world has its own geometry and stored plankton charge. Light painted at a particular bearing and radius in World 2 must not also charge that position in Worlds 1, 3 and 4.

Only the inspected world is visible through the spotlight and its local afterglow. The other worlds keep their state and continue simulating/decaying. No off-world ship marker or heading line should reveal the vessel's exact current position.

At the winding seam, light sampling, guidance and collision must recognize the adjacent world's water correctly. Handle the local neighborhood across the seam rather than duplicating actors, jumping between colliders or leaking charge between unrelated layers.

Display the currently inspected world clearly and keep the harbor-in-World-4 objective legible. Any “last seen” ship information must remain genuinely last-seen information, not silently track it through darkness.

### 6.4 More than radial Flappy Bird

Do not automatically push the ship outward through rings with one timed gap each. It sails according to the shared light-reading rules and can use different radii and passages within each world.

Use small island clusters with alternate approaches. For example, an inner and outer route through one world can reach the next seam at different distances from the lighthouse; the next world's geography makes one approach easier to continue. The player can scout ahead before committing the vessel.

The difficulty should come from painted-route preparation, limited observation and the vessel's turning constraints—not compulsory new mechanics in every world. Use the same light rules throughout all four worlds for now.

### 6.5 Ending

Reaching the World 4 harbor succeeds; grounding and sinking ends the attempt. Unlike Mode 1, there is only one ship to lose.

**Prototype default:** Let safe arrival trigger the first-light reveal over World 4 and the result. Do not inherit Mode 1's three-minute deadline automatically. Establish a manageable full-voyage length before deciding whether a separate dawn deadline improves it. This remains a live voyage all the way to its ending, with no Mode 3 construction/playback split.

---

## 7. Implementation boundaries and next-round acceptance

### Shared implementation, distinct navigation policies

Reuse existing architecture and assets. The important separation is:

| Shared component | Mode-specific responsibility |
|---|---|
| Spotlight, charge/decay, silhouette rendering | Mode 3 previews sectors; Modes 1 and 4 paint localized water. |
| Ship body, movement and desired-heading presentation | Modes 1 and 4 derive intent from light; Mode 3 derives it from an actual computed maze route. |
| Island geometry and collision | Mode 3 copies sector geometry; Mode 4 stores persistent per-world geography. |
| Winding-aware selector | Mode 3 browses candidates cyclically; Mode 4 supports a finite live spiral voyage and independent ship winding. |
| Session and results | Mode 1 ends its rescue night; Mode 3 begins playback at dawn; Mode 4 resolves the live voyage. |

Do not fork ship-guidance code between Modes 1 and 4. Do not accidentally expose Mode 3's pathfinder to them. Clear charge, monster, selection and winding state correctly when switching modes or restarting.

### Recommended implementation order

1. **Shared handling/readability:** Dusk glimpse, silhouette visibility, island clusters, farther-ahead steering, hysteresis and the heading line.
2. **Mode 1:** Add the ship-sized charge-consuming predator and verify a useful decoy interaction.
3. **Mode 3:** Make World 1 authoritative, implement copying/markers/inspection, and replace the fixed voyage with route finding.
4. **Mode 4:** Reuse the improved live navigation across four persistent worlds; validate winding and seam behavior before adding level complexity.
5. **Integration and tuning:** Test complete runs, mode switching and outcomes. Do not spend this round rescuing Mode 2.

### Required demonstrations

| Area | A passing prototype demonstrates |
|---|---|
| Menu | Modes 1, 3 and 4 are selectable; Mode 2 is hidden and its transformation rules do not run in them. |
| Introduction | Mode 1's first boats can be seen during the two-second dusk fade rather than requiring an initial blind search. |
| Silhouettes | A boat, predator and traced island shore remain readable against afterglow outside the direct beam, then disappear as it fades. |
| Guidance | A ship responds to a trail several lengths ahead, resists nearly equal competing trails, and does not orbit a passed bright spot. |
| Intention versus motion | An accepted new heading appears immediately on the line while the hull turns gradually; an unseen ship's line is hidden. |
| Predator | A brighter local decoy can divert it, consumed charge actually disappears from both rendering and navigation, and land blocks it. |
| World 1 assembly | Copying changes only the selected destination sector; the source is unchanged; returning to World 1 shows the accumulated edits. |
| Maze gameplay | At least two different assembled maps connect the known endpoints successfully. An unconnected map fails clearly. No old fixed-route requirement remains. |
| Dawn | FIRST LIGHT reveals precisely the saved World 1, then the ship follows a clearance-valid computed route without corner-cutting failures. |
| Spiral independence | Beam and ship can be in different worlds. Looking ahead neither teleports nor pauses the ship, and charge stays world-local. |
| Spiral crossings | Ship, light and local guidance cross the seam consistently in both directions; World 4 does not wrap to World 1. |
| Complete voyage | One demonstrated playable route reaches the World 4 harbor using shared light guidance, without a monster or hidden maze autopilot. |

The next round should answer three separate questions: **Does the predator make painting routes strategic? Does assembling World 1 make a satisfying open-solution puzzle? Does guiding a live ship through the spiral create worthwhile scouting and preparation?** No additional systems are required before those tests.
