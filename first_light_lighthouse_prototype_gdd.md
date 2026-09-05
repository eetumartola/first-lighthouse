# FIRST LIGHT
## Lighthouse prototype — Game Design Document

**Version:** 0.1  
**Format:** Single-player desktop prototype; keyboard controls; one shared scene with three selectable variants.  
**Scope:** Solo weekend game jam, with AI-assisted development but no required runtime AI.  
**Status:** Working specification. Numbers, mode names, and decisions labelled **prototype default** are starting points for testing, not previously settled requirements.

> On a dark, mythological sea, a keeper lights the first lighthouse. What the light does depends on which version of the game you choose.

---

## 1. Project intent

Build one small lighthouse game framework that can demonstrate three different uses of light:

| Variant | What the player does | What happens in darkness | What first light means |
|---|---|---|---|
| **1. Night Watch** | Guides several ships by painting glowing water | The unseen world keeps moving predictably | The night ends and the rescue result is revealed |
| **2. Mutable Sea** | Guides ships while managing which things remain themselves | Entities advance through transformations | All remaining forms become permanent; the rescue attempt ends |
| **3. World Weaver** | Selects slices from alternate versions of the sea | Unchosen possibilities remain available | The assembled world becomes real and the voyage plays out |

These are separate rule sets, not three mechanics stacked into one complicated game. A player chooses a variant from the opening menu. Each must have its own understandable objective, playable loop, and ending.

### Design pillars

**One light, several demands.** The player cannot attend to the entire sea simultaneously. Spending attention in one place means neglecting another.

**Darkness carries information.** In Night Watch and Mutable Sea, the player predicts what is happening outside the beam. Discovering whether that prediction was right is a central moment of play.

**The sea remembers.** Photosensitive plankton stores illumination. A quick scan leaves a brief trace; sustained attention creates a lasting glow. In World Weaver, luminous traces instead identify committed slices.

**Dawn has a mechanical consequence.** It is not merely a timer with a sunrise animation. It ends uncertainty, fixes the world, or begins the simulation the player has prepared.

**Few systems, visible consequences.** Depth should come from light, movement, persistence, and a handful of entities—not inventories, research trees, or extensive content.

### What this prototype is not

It is not a lighthouse-management simulator, an exploration game with a walkable keeper, a combat game, or a realistically simulated ocean. It does not require generated dialogue, procedural mythology, or runtime AI. It does not attempt all the alternative light mechanics discussed during ideation.

---

## 2. Opening menu and session flow

The opening screen shows the unlit lighthouse and three mode choices:

- **Night Watch — Paint safe passage. Remember what moves in the dark.**
- **Mutable Sea — What you abandon to darkness may become something else.**
- **World Weaver — Build the sea by night. Test it at first light.**

Selecting a variant displays its controls and a two-sentence rule summary, then starts a fresh session. Do not require the player to complete another mode first.

The standard flow is:

**Choose variant → brief in-scene introduction → night phase → dawn → result → retry or return to menu.**

Each mode has a fixed initial scenario for the prototype. **Retry** restores that scenario, allowing the player to learn its rules rather than receiving a new random problem after every failure. A pause menu provides restart and return-to-menu actions.

**Prototype default:** The introduction is a short ignition of the beacon, not a separate match-lighting minigame. First-person hands, keeper animation, and a historical montage are optional presentation work.

---

## 3. Shared scene, camera, and controls

### Scene

Use a fixed elevated camera looking over a compact sea surrounding the lighthouse. A near-overhead or shallow isometric view should make direction, distance, routes, and silhouettes readable. The simulation runs on a flat sea plane even if the presentation is 3D.

The lighthouse occupies a small central island. A clearly marked harbor zone beside it is the rescue destination. The playable sea extends around the island, with a few rocks and enough room to steer around them.

No camera movement is required. Keep the lighthouse, horizon or compass reference, and playable boundary spatially consistent so players can remember bearings.

### Default controls

| Input | Night Watch | Mutable Sea | World Weaver |
|---|---|---|---|
| **A / D** | Rotate left / right while held | Rotate left / right while held | Wind backward / forward through the spiral |
| **W / S** or **Up / Down** | Move the footprint farther / nearer | Move the footprint farther / nearer | Unused; preview spans a complete sector |
| **Space** | Unused | Unused | Capture the currently previewed slice |
| **Esc** | Pause | Pause | Pause |

**Prototype default:** Releasing A/D stops rotation. There is no beam shutter, boost button, fuel supply, or separate charge action. Lingering is the charge action.

For the first two modes, aiming controls a spotlight-like patch on the water. Its range can move independently of its bearing. The visible shaft connects it to the lighthouse, but **only the gameplay footprint charges the water**. The shaft must not misleadingly reveal everything between the tower and the patch. Use a stylized footprint rather than making rock occlusion and secondary shadowcasting additional gameplay systems.

World Weaver uses the same beam presentation with a different footprint: a complete radial slice, with no near/far adjustment. This prevents ambiguity about whether the player is choosing an entire slice or only part of one.

### Optional control experiment: constant-speed rotation

Retain this as a developer setting, not a required additional game mode:

**A selects counterclockwise rotation; D selects clockwise rotation. Rotation continues after release.**

In Night Watch and Mutable Sea, range control remains available. Repeated reversals let the player work a small area, but the beam cannot simply be parked. World Weaver keeps Space to capture a slice.

Compare this against direct rotation after the default controls work. Keep it only if reversing feels purposeful rather than like repetitive input needed to perform a basic task. Do not implement every possible combination of automatic rotation, shuttering, and focus control.

---

## 4. Shared light, plankton, and visibility

### Charge and decay

Water inside the footprint accumulates stored light. After illumination stops, that charge drains gradually. A patch has a charge cap, so camping in one place cannot create a permanent safe region.

Longer illumination produces a brighter, longer-lived glow. Initial tuning should distinguish three useful gestures:

| Gesture | Intended result |
|---|---|
| Quick sweep | A momentary view and a faint trace |
| Brief dwell | A marker or short route useful while attending elsewhere |
| Sustained painting | A durable route, stabilizing patch, or deliberate lure |

The resource is **time spent illuminating**, not a stock of energy in the lighthouse. There is no global plankton meter. Charge should be legible through brightness, saturation, and the progression from a luminous patch to scattered motes and then darkness.

A strong patch should not become an opaque bloom effect. The player still needs to see the ship or rock inside it.

### Visibility rules

Direct illumination reveals current positions, forms, and nearby hazards clearly. Strong afterglow can reveal rough silhouettes locally. Weak afterglow shows the water trail but not reliable current information about moving entities.

Outside these regions, hide entities rather than drawing continuously updated icons. Do not add a minimap, offscreen target arrows, or ghost positions that secretly follow unseen ships.

The central island and a minimal bearing reference remain legible. The player should be uncertain about the sea, not about which way the camera faces.

### Information from sound

Sound can provide occasional approximate information from darkness: a ship's bell, a collision, an approaching creature, or a transformation cue. These should attract attention without reporting exact coordinates or a complete hidden state.

For the first build, a few directional cues are sufficient. Distinct audio identities for every vessel are optional.

### One authoritative light state

Gameplay and rendering must agree about where illumination exists. Boats, monsters, transformations, and the visible plankton all consult the same charge field and beam footprint.

In Mutable Sea, use the same strong-afterglow threshold for revealing an entity's current form and preserving that form. This avoids visibly watching a supposedly light-protected entity transform.

---

## 5. Shared entities and movement

### Ships

Ships move forward at a modest speed and turn gradually. In Night Watch and Mutable Sea, they seek usable glowing route markers ahead of them. Without a usable marker, they maintain their last heading rather than stopping safely or finding their own route home.

The lighthouse does **not** magically pull every ship directly to its center. The player guides ships by painting water ahead of them.

Avoid implementing guidance as “move toward the brightest nearby point” alone. That can strand ships at local brightness peaks or make them circle a patch. Use sparse route samples created as the player paints:

- Prefer nearby, charged samples ahead of the vessel; brightness influences the choice.
- Once a sample is reached or passed, do not select it again immediately.
- If no suitable next sample exists, continue forward.
- Never automatically route around an unseen rock or repair the player's route.

A stationary bright patch is a beacon to pass toward, not an instruction to orbit forever. At intersections, a brighter competing route can still draw a ship away; that is a readable consequence of the player's painting.

Use one ship handling model initially. Multiple ships, different initial headings, and spatial separation should establish the tracking challenge before adding special vessel classes.

### Rocks, islands, and wrecks

Rocks and islands obstruct navigation. Keep their gameplay collision shapes simple and consistent with their silhouettes. The central harbor has an obvious safe entrance.

Wrecks reuse the same visual family but have mode-specific consequences: ordinary hazards where needed, recoverable forms in Mutable Sea, and temporary delays in World Weaver.

### Sea creature

Use one creature behavior model. It moves toward the strongest nearby detectable light, with a preference for retaining its current target rather than flickering between almost equal targets. Charged plankton and small ship lanterns can attract it. If nothing is detectable, it continues toward its last target or drifts slowly.

The creature is not omniscient and does not automatically know where every ship is. It threatens a ship through physical proximity, not an invisible attack from elsewhere on the map.

Land blocks it. Elaborate pathfinding is unnecessary: stopping or sliding along an obstruction is acceptable if it is consistent and visually readable.

This gives the beam two uses without another control: create a route for a ship, or create a brighter lure away from it.

### Rescue

A ship entering the harbor is secured and removed from active danger. Each vessel counts as one rescue; there is no passenger simulation. Secured ships can remain visibly moored as a cumulative record of success.

---

## 6. Variant 1 — NIGHT WATCH

### Pitch

**Guide several ships home through a sea you can only inspect a little at a time. The trails you leave help sailors—and attract something else.**

### Objective and session

Rescue as many incoming ships as possible before dawn. A sunk vessel is a lost rescue, not an immediate game over.

**Prototype scenario:** Five ships enter over the night, with up to three active simultaneously. Five fixed rocks shape the approaches. One creature becomes active after the player has had an opportunity to learn basic guidance.

Use a fixed spawn schedule that deliberately creates overlapping responsibilities. A long sequence of isolated ships will not test the intended mechanic. The final arrival must leave enough time for an ordinary successful approach before dawn.

### Core loop

Find a vessel, assess its heading and nearby hazards, then paint a route ahead of it. Invest enough light for that route to last while attending elsewhere. Return later to find out whether the vessel, route, and creature are where expected.

Movement in darkness follows the same rules as visible movement. No ships teleport, no rocks reroll, and no surprise danger is spawned solely because the player looked away.

The player's recurring decision is:

> Do I spend another moment making this route reliable, or inspect the ship I have not seen for a while?

### Example decision

A northern ship needs a turn around a reef. A southern ship is following an older trail. The creature was last seen moving toward the eastern glow.

The player can strengthen the northern turn, but doing so leaves the southern vessel unobserved longer and creates a more attractive northern lure. Alternatively, a brief eastern decoy may buy time—but its old glow will remain after the player leaves.

The desired tension comes from understandable delayed consequences, not constant emergency prompts.

### Dawn and outcome

At dawn, reveal the remaining sea and conclude the run. Secured vessels count as rescued; vessels still offshore are reported separately. The creature does not need a defeat animation or combat resolution.

A suggested initial success target is three of five vessels rescued, while the result screen always reports the actual total. Retry uses the same scenario.

### Prototype success test

This mode works when players can explain both a successful prediction and a mistaken one: “I knew that ship would reach the turn” or “I forgot the trail was fading.” If success is simply holding the beam on one vessel, or failure feels like random disappearance, the design is not yet working.

---

## 7. Variant 2 — MUTABLE SEA

### Pitch

**The first beacon preserves more than visibility. It keeps things themselves. Beyond its light, a ship can become a wreck, a creature, or land.**

This is not Night Watch with random surprises. It is attention management over a small, learnable transformation system.

### Concrete prototype rule

Each of three persistent entities carries one rescueable identity. Its form follows a fixed cycle:

**Ship → Wreck → Creature → Island → Ship**

The entity retains its identity and position when changing form. Use a recurring visual motif—a prow curve, crest, or small emblem—to suggest that these are interpretations of the same thing.

**Prototype default:** All three use the same cycle, with different starting progress. Variation in cycle order is unnecessary until the basic rule is understandable.

### What light preserves

Direct lighthouse illumination pauses an entity's transformation timer. Strong charged plankton beneath it does the same. Darkness advances the timer toward the next form.

Light **pauses**, rather than resets, that progress. A momentary scan cannot erase all accumulated danger. After a transformation, the timer starts anew for the next form.

Preservation affects spontaneous changes of form, not ordinary motion. A lit ship still sails. A lit creature still moves. Physical collisions can still damage a ship.

Only the lighthouse's light and its charged plankton preserve forms. Small vessel lanterns do not preserve their own ships; they are navigation and attraction cues, not sources of this mythological power.

### Form behavior

| Form | Behavior | Why the player might watch or abandon it |
|---|---|---|
| **Ship** | Follows charged routes and can be rescued | Preserve it long enough to reach harbor |
| **Wreck** | Stationary obstruction | Let darkness advance it; preserving it may keep an obstruction from becoming a creature |
| **Creature** | Follows light and threatens ships | Lure it aside, or leave it dark to progress toward a less mobile form |
| **Island** | Stationary barrier | Preserve it as a barrier, or allow it to return to a ship |

Keep transformation footprints similar enough that a change does not unexpectedly create a huge collision area. Prevent a new solid form from appearing through another entity; defer the transition until placement is clear.

A visible pre-transition cue should show that a form is becoming unstable when inspected. Do not display precise offscreen countdowns for every entity.

### Objective and damage

Bring at least two of the three identities into harbor **while in ship form** before dawn. Once secured, an identity cannot transform again.

For this variant, a ship hit by a creature or grounded on a rock becomes a wreck rather than disappearing permanently. Its rescue remains possible through the cycle, but costs time. Physical damage can cause this change even in light; the beam is not an invulnerability field.

### Core loop

Locate the identities, infer how far their unseen timers have advanced, and decide which forms are worth preserving. Paint sustained light along a ship's route to give it time away from direct supervision. Deliberately abandon unwanted forms when allowing them to evolve is more useful than stabilizing them.

The important choice is not always “light the endangered ship.” Keeping a convenient island intact may stop a creature, while leaving that island dark may return a rescueable ship to the world.

### Dawn and outcome

Dawn permanently fixes all remaining forms. Count the secured identities and reveal what the others became. The run ends here; it does not enter World Weaver's separate voyage phase.

### Prototype success test

Players should intentionally use darkness, not merely avoid it. They should learn to say, “Leave that creature alone long enough and it will become land,” while remembering that their useful ship is also changing offscreen.

If transformations feel arbitrary, simplify the cycle or improve cues. Do not add more entity types to solve a readability problem.

---

## 8. Variant 3 — WORLD WEAVER

### Pitch

**Each turn of the lighthouse reveals another possible sea. Choose pieces of those possibilities during the night. At first light, your assembled world becomes real—and a ship attempts the journey through it.**

### Night and dawn are different phases

During the night, entity movement is paused. The player inspects and captures alternatives. At dawn, editing stops, the selected world is instantiated, and movement begins.

This mode tests composition and prediction, not real-time ship guidance.

### The spiral of worlds

Keep track of the beam's total winding, not just its compass bearing. Continuing clockwise into another revolution reaches another world layer. Reversing retraces the earlier layer.

**Prototype default:** Four authored layers repeat cyclically. There is no endless generation. Revisiting a layer always shows exactly the same candidate state.

Use a fixed, legible seam at south where the winding advances to the next layer. Put this seam on a sector boundary so the active preview never straddles it. Do not swap the whole visible ocean when crossing it; only the active preview slice changes. A subtle sound and a small layer glyph communicate the transition.

Reversing direction is navigation through possibilities. It does not, by itself, edit the assembled world.

### Shared underlying entities

All layers use the same underlying entity anchors. Each anchor has a different form in each layer: ship, wreck, creature, or island. Different anchors can have different phase offsets, so one revolution is not simply “the good ship world” and another “the bad monster world.”

Forms should share a recognizable silhouette or motif. The visual suggestion is that a ship, its wreck, the thing beneath it, and the island it becomes belong to the same myth.

Keep preview anchors and their collision bounds wholly within their sectors. Do not create half an island in one slice and its other half in another.

### Selecting a slice

Divide the sea into twelve angular sectors. The beam highlights one active sector at a time, across its full radial extent.

**Space captures the active sector's current layer into the final world.** It can be overwritten by capturing another layer later. There is no limited currency for captures.

Keep **preview state** and **committed state** separate. Passing over a sector must never silently replace its saved choice. A small segmented ring around the lighthouse shows which sectors have been captured and their chosen layer glyphs, but does not expose a full object map.

Uncaptured sectors use a stated default layer at dawn. They are not randomly resolved. The opening instruction makes this clear.

For this mode, glowing water is a visual record of captured slices, not an additional ship-guidance system. Do not require the player to maintain plankton charge while also browsing worlds. Capture traces persist until dawn. They then fade and do not supply light-attraction targets during playback; the voyage uses the authored route and vessel lights.

### The voyage must cross the composition

The expedition ship follows a known, winding sea lane through several sectors before reaching the harbor. Do not use a straight radial journey that only tests one chosen slice.

The route is authored and can be previewed in the illuminated sector as a subtle line or buoy sequence. At dawn, the ship follows these fixed waypoints; it does not find a perfect route through whatever the player built.

The starting expedition ship and harbor are fixed scenario elements, not replaceable world candidates.

### What the chosen forms do at dawn

| Form | Consequence during the voyage |
|---|---|
| **Ship** | Can join the expedition when it passes nearby, adding a bonus rescue |
| **Wreck** | Causes a short salvage/navigation delay when it lies on the route, then allows passage |
| **Creature** | Follows nearby vessel lights and can sink ships; land can obstruct it |
| **Island** | Blocks a route if placed across it, but can also form a barrier between a creature and the voyage |

Do not make every sector a trivial search for its one harmless form. Some slices should contain two related anchors, creating a tradeoff: an extra rescue together with a threat, a useful barrier with a route obstruction, or a delay that changes the timing of an encounter.

**Author one small solvable puzzle.** Cross-sector interactions are desirable, but procedural puzzle generation is outside the prototype. Validate at least one successful combination and one clearly understandable failure before adding variants.

### First-light playback

When the night timer expires, freeze the commitments and build the complete world from them. Reveal the sea with a short sunrise transition. Remove editing controls and start the expedition.

The voyage should resolve quickly. The player watches their choices interact: a wreck delays departure from a sector, a creature approaches, an island blocks it, or a chosen ship joins the journey.

Success is the expedition reaching harbor. Bonus rescues improve the result. Sinking, grounding on an impassable island, or failing to finish within the short playback limit ends the attempt.

Give a concrete explanation of failure: “The western island blocked the route,” rather than only “Failed.” Retry returns to the same candidates and route. Preserving the previous selections for revision is optional, not required.

### Prototype success test

The player must understand what was saved, recognize that reversing revisits earlier candidates, and connect the dawn outcome to their choices. The whole-world reveal is a reward, but the voyage must test something meaningful rather than simply decorate the reveal.

---

## 9. Presentation and atmosphere

### Visual direction

Use the Dredge reference from the concept discussion as a direction for mood and stylization, not a requirement to reproduce its systems: simplified angular forms, a dark sea, restrained fog, small vulnerable vessels, and a strong separation between warm beacon light and cold luminous water.

The mythological lighthouse can be a brazier and bronze reflector on a rough stone platform. It does not require a modern tower, realistic optics, or historically accurate machinery.

The sea should be visually quiet enough that a mast, wake, changing silhouette, or fading trail matters. A simple animated surface is preferable to an elaborate ocean that hides gameplay cues.

### Reading transformations

A hull can rhyme with a creature's back and a ridge of rock. Reuse proportions and anchor points to make forms feel connected. Changes occur under darkness; elaborate visible morphing is not required.

Night Watch should remain physically trustworthy. Save impossible transformations for the two variants whose rules explicitly promise them.

### Sound and text

Prioritize the rotating mechanism, sea ambience, a ship cue, a creature cue, and clear capture/rescue feedback. Sound should sometimes draw attention away from a task, but should not repeatedly demand an immediate reaction.

Keep narrative text sparse. A possible shared opening is:

> Before the first beacon, no one trusted the sea after dark.

Mode-specific instructions provide the actual rule. The ending can use one sentence and the concrete result. Voice acting and generated narration are unnecessary.

### Readability requirements

Do not make the game unplayable on a dim monitor. Provide a brightness adjustment and distinguish critical states by shape or movement as well as color. Avoid rapid flashing as a necessary interaction. Pausing should always be available.

---

## 10. Starting tuning values

These are deliberately provisional. Expose them together so playtests do not require changes scattered through the implementation.

| Setting | Initial value or target |
|---|---|
| Night Watch night | About 3 minutes |
| Mutable Sea night | About 3 minutes |
| World Weaver editing phase | About 3 minutes |
| World Weaver playback | About 30–45 seconds maximum |
| Direct beam rotation | A full held-input turn in about 8 seconds |
| Optional automatic rotation | A revolution in about 12 seconds |
| Spotlight angular width | Roughly 12–18 degrees |
| Spotlight radial length | Roughly one eighth of the playable sea radius |
| Plankton charge return | About 5 seconds of afterglow per second of illumination |
| Maximum stored afterglow | About 30 seconds |
| Strong-glow threshold | Enough charge for about 5 seconds of remaining glow |
| Unobstructed outer-sea-to-harbor travel | Roughly 35–50 seconds |
| Night Watch vessels | Five total; up to three active at once |
| Night Watch creature | One |
| Mutable Sea identities | Three |
| Mutable ship / wreck / creature / island dark durations | Initially 16 / 10 / 12 / 8 seconds |
| World Weaver sectors / layers | Twelve / four |
| Wreck delay in World Weaver | About 4 seconds |

The useful ratios matter more than any particular number. A ship should survive an ordinary scan elsewhere, but not indefinite neglect. A charged route should buy attention, not solve the rest of the run permanently. Dark transformations must be slow enough to predict and fast enough to matter.

---

## 11. Implementation shape

### Shared foundation, separate rules

Keep the implementation engine-agnostic at the design level. Use the engine and asset workflow that permit the fastest iteration. The game needs a flat logical sea, not physically simulated light transport.

| System | Shared responsibility |
|---|---|
| **Session controller** | Menu selection, restart, night timer, dawn, results |
| **Beam controller** | Bearing, range, footprint, direction, optional automatic rotation, total winding |
| **Charge field** | Store and decay plankton charge; answer local illumination queries |
| **Visibility layer** | Render current entities only where the mode permits |
| **Entity model** | Persistent identity, form, position, heading, movement and collision state |
| **Guidance samples** | Sparse painted targets referencing the charge field |
| **Scenario data** | Initial positions, rocks, spawn schedule, transformation timings, alternate layers and voyage route |
| **Mode rules** | Guidance versus editing, transformations, damage consequences, rescue and dawn behavior |

A coarse 2D charge grid is an appropriate prototype representation. Render the plankton from that same data; do not derive authoritative gameplay from decorative bloom or fog. Guidance samples can reference grid cells rather than maintaining a separate incompatible brightness model.

Night Watch and Mutable Sea run live movement during the night. World Weaver previews paused candidates and only starts movement after the assembled snapshot is committed at dawn.

### World Weaver data separation

Store the fixed anchors, four candidate layers, and selected layer per sector separately. During playback, instantiate each entity once from its sector's selected candidate. Do not simulate all alternate worlds and try to hide the unwanted actors.

World choices should be deterministic and stable across preview, capture, reverse rotation, and retry.

### Debug tools worth having

A developer overlay should be able to reveal all entities, show the beam footprint and charge grid, display current guidance targets, expose transformation timers, and show World Weaver's winding, preview layer, and committed selections.

These are debugging tools, not the normal player interface. A visible failure is much easier to diagnose when the simulation can be inspected without changing its behavior.

---

## 12. Required scope and optional work

### Minimum complete submission

The prototype includes the start menu and **all three playable variants**. The scope reduction is within each mode, not removing two of the requested modes.

Required shared content is one lighthouse scene, one ship family, rocks/island and wreck forms, one creature, the beam, glowing plankton, basic sound cues, a sunrise, and a result screen.

Night Watch needs multiple simultaneous vessels, usable painted guidance, fading charge, one light-seeking creature, and rescue/failure outcomes.

Mutable Sea adds persistent identities, the fixed four-form cycle, light-based preservation, recovery from wrecks, and a clear rescue objective.

World Weaver adds stable alternate layers, reversible winding, explicit slice capture, one authored voyage, and a dawn playback that can succeed or fail.

### Optional after all three loops function

Special ship handling, additional scenarios, advanced control presets, more elaborate creature presentation, keeper animation, a match-strike opening, visual history of prior sweeps, and retaining a previous World Weaver composition for revision are secondary.

Do not add upgrade shops, inventories, fuel management, elaborate wind or currents, several monster species, procedural campaign generation, online accounts, or a runtime LLM dependency.

### Development order

1. **Shared readability:** Menu, fixed scene, beam controls, charge and visibility.
2. **Night Watch foundation:** One ship follows painted guidance correctly; then test several ships, obstacles, rescue and creature attraction.
3. **Mutable Sea rules:** Add identity-preserving transformations to the existing movement and light systems.
4. **World Weaver rules:** Add the candidate/commit distinction, reversible layers and a deliberately simple playable voyage.
5. **Integration:** Test switching and restarting modes, tune the authored scenarios, then spend remaining effort on the reveal, audio and visual polish.

The first technical gate is not the water shader. It is whether a ship follows a short painted route without circling a bright patch. The first design gate is whether leaving that ship to attend another creates interesting uncertainty.

---

## 13. Playtest questions and failure checks

| Risk | What to look for | First adjustment |
|---|---|---|
| Beam control dominates everything | Players fight the inputs instead of making decisions | Adjust rotation/range speed and footprint before adding mechanics |
| Ships require constant babysitting | Looking away is almost always fatal | Slow ships, extend useful glow, simplify the first approach |
| Darkness feels arbitrary | Players cannot explain where something went | Remove randomness, expose heading cues, improve occasional sound information |
| Bright trails create passive wins | A single charged route solves the scenario | Lower charge cap or change the authored arrival geometry |
| Guidance creates loops | Ships stall or circle the brightest patch | Fix forward target selection and reached-sample handling |
| Monsters merely punish useful play | Any good ship route is immediately fatal | Reduce detection range/speed and make decoys effective |
| Mutable Sea becomes frantic scanning | Brief touches preserve everything indefinitely | Verify that light pauses rather than resets transformation progress |
| Mutable Sea is unreadable | Players cannot predict the next form | Use the same cycle for every identity and improve inspected-state cues |
| World Weaver has accidental edits | Returning to a sector overwrites it | Enforce explicit Space capture and separate preview from commitment |
| World Weaver is trivial | Every sector has an obviously best independent option | Add a small number of paired anchors and cross-sector consequences |
| Dawn failures are opaque | Players cannot connect the result to their choices | Shorten playback and identify the concrete obstruction or threat |

### Integration acceptance checks

- Each mode can be selected, played, completed, retried, and exited without carrying state into another mode.
- Darkness affects visibility without accidentally pausing Night Watch's simulation.
- Strong afterglow is finite, charges only under the footprint, and matches what actors respond to.
- Rescued entities cannot be lost later or counted twice.
- Mutable Sea preserves identity, pauses timers correctly, and never spontaneously transforms an entity under preserving light.
- World Weaver returns to the same candidates when reversed, captures only the intended sector, and builds exactly the stored composition at dawn.
- There is a demonstrated successful run for each authored scenario, not merely a theoretically possible one.

---

## 14. Decisions to revisit after the first playable build

The main open control question is **direct aiming versus constant-speed rotation with reversal**. The specification defaults to direct aiming so charge investment can be tested without compulsory oscillation.

The main Night Watch question is whether the glow should primarily function as a route or as a set of attractors. Start with route samples to avoid trapping ships, then tune how strongly competing brightness affects decisions.

The main Mutable Sea question is whether the four-form cycle produces deliberate use of darkness. If the island form contributes little, a three-form cycle is a valid simplification; preserve the distinction between movement and transformation.

The main World Weaver question is whether selecting and remembering slices is satisfying before adding complicated interactions. Keep its first puzzle small and legible. Do not solve a weak composition loop with more layers.

These questions can remain open in the design while the implementation uses explicit defaults.

---

## 15. Core identities to preserve

**Night Watch:** You remember a world that continues without your attention.

**Mutable Sea:** You decide which things can be left to become something else.

**World Weaver:** You choose what the world will be before it has to work.

The same lighthouse should make those three different promises clear. Everything else is in service of testing them.
