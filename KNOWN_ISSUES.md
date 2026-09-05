# Known issues (prototype)

Minor visual glitches noted and deferred; gameplay is unaffected.

- **Spiral widget backdrop**: the helicoid's viewport (lower-left) draws over an opaque black box
  instead of blending with the scene. `ClearColorConfig::None` on a second camera does not clear the
  intermediate texture's alpha; the fix is `ClearColorConfig::Custom(Color::NONE)` plus
  `CameraOutputMode::Write { blend_state: Some(BlendState::ALPHA_BLENDING), .. }`.
- **Spiral widget bottom turn**: World 1's turn (the lowest) is not clearly readable in the
  capture; the three upper turns and the highlight of the inspected world are correct.
- **Screenshot writer**: `FIRST_LIGHT_SHOTS` occasionally emits 72-byte (1x1) PNGs when two captures
  overlap; the surrounding frames are fine.
- **Autoplay keepers are geometry-sensitive**: the scripted Night Watch keeper only barely reaches
  3/5 rescues, so guidance tuning (`guidance_switch_advantage`, `guidance_dwell`) cannot be raised
  without re-authoring its routes.
- **Sector footprint leaks past the disc**: in World Weaver the lit sector's water shows a thin
  band continuing beyond the sea's edge along the sector's leading edge (`sea.wgsl` sector branch
  lacks an outer-radius clamp against `sea_radius`).
- **Spiral seams have zero width**: a ship turning shallowly across the north seam can flip
  world twice within a second (HUD world label and widget bead jump). Harmless for the voyage;
  a small crossing hysteresis (seam band of a few units) would remove the flicker.
