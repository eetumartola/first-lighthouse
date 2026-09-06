# Known issues (prototype)

Minor visual glitches noted and deferred; gameplay is unaffected.

- **Screenshot writer**: `FIRST_LIGHT_SHOTS` occasionally emits 72-byte (1x1) PNGs when two captures
  overlap; the surrounding frames are fine.
- **Sector footprint leaks past the disc**: in World Weaver the lit sector's water shows a thin
  band continuing beyond the sea's edge along the sector's leading edge (`sea.wgsl` sector branch
  lacks an outer-radius clamp against `sea_radius`).
