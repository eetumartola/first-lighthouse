// Sea surface: PBR water with animated ripples, plankton glow read from the authoritative charge
// grid, and the stylized beam footprint. Nothing here decides gameplay; it only draws the sim.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

struct SeaParams {
    time: f32,
    sea_radius: f32,
    charge_cap: f32,
    // 0 = none, 1 = spotlight patch, 2 = full sector
    fp_kind: u32,
    fp_bearing: f32,
    fp_half_angle: f32,
    fp_r_min: f32,
    fp_r_max: f32,
    // 0 night .. 1 full daylight
    dawn: f32,
    strong_threshold: f32,
    // 1 = the beam's whole length dimly lights the water (spotlight modes only)
    beam_lane: f32,
    _pad1: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> sea: SeaParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var charge_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var charge_sampler: sampler;

const PI: f32 = 3.14159265;
const TAU: f32 = 6.28318531;

fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn angle_delta(a: f32, b: f32) -> f32 {
    var d = (b - a) % TAU;
    if d < 0.0 { d = d + TAU; }
    if d > PI { d = d - TAU; }
    return d;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    let wp = in.world_position.xyz;
    // Simulation plane: x east, y north (= -z).
    let sim = vec2<f32>(wp.x, -wp.z);
    let r = length(sim);
    let t = sea.time;

    // Ripples: a few travelling waves perturb the shading normal.
    let w1 = sin(dot(sim, vec2<f32>(0.35, 0.20)) + t * 1.1);
    let w2 = sin(dot(sim, vec2<f32>(-0.22, 0.41)) * 1.3 + t * 0.8);
    let w3 = sin(dot(sim, vec2<f32>(0.13, -0.30)) * 2.1 + t * 1.7);
    let w4 = value_noise(sim * 0.6 + vec2<f32>(t * 0.2, -t * 0.13)) * 2.0 - 1.0;
    let tilt = vec3<f32>(0.045 * (w1 + w3) + 0.03 * w4, 0.0, 0.045 * (w2 - w3) - 0.03 * w4);
    pbr_input.N = normalize(pbr_input.N + tilt);

    // Plankton: charge in seconds of remaining afterglow.
    let uv = (sim + vec2<f32>(sea.sea_radius)) / (2.0 * sea.sea_radius);
    let charge = textureSample(charge_tex, charge_sampler, uv).r * sea.charge_cap;
    let faint = smoothstep(0.0, sea.strong_threshold, charge);
    let strong = smoothstep(sea.strong_threshold, sea.charge_cap, charge);
    // Weak afterglow breaks into scattered motes; strong glow is a continuous luminous patch.
    let mote_field = value_noise(sim * 2.2 + vec2<f32>(t * 0.12, t * 0.07));
    let motes = smoothstep(0.55, 0.9, mote_field) * 1.6 + 0.08;
    let coverage = mix(motes, 1.0, faint * faint);
    let shimmer = 0.88 + 0.12 * sin(t * 2.4 + value_noise(sim * 0.9) * TAU);
    let glow = faint * coverage * (0.6 + 2.6 * strong) * shimmer;
    let plankton_color = mix(vec3<f32>(0.10, 0.55, 0.75), vec3<f32>(0.35, 0.95, 0.95), strong);
    var emissive = plankton_color * glow;

    // Beam footprint: the stylized patch that actually charges the water. It brightens the water
    // itself (so the spotlight's ripples show) and adds a warm glow with a wave-broken edge.
    var fp = 0.0;
    var lane = 0.0;
    if sea.fp_kind != 0u {
        let b = atan2(sim.x, sim.y);
        let d = abs(angle_delta(sea.fp_bearing, b));
        if sea.fp_kind == 1u {
            // Spot: an oval in polar space, matching the simulation's footprint exactly at its
            // rim (elliptical distance 1) with a wave-broken soft edge inside it.
            let half_len = (sea.fp_r_max - sea.fp_r_min) * 0.5;
            let u = d / sea.fp_half_angle;
            let v = (r - (sea.fp_r_min + sea.fp_r_max) * 0.5) / half_len;
            let e = sqrt(u * u + v * v);
            fp = 1.0 - smoothstep(0.7, 1.0, e);
        } else {
            let in_ang = 1.0 - smoothstep(sea.fp_half_angle * 0.8, sea.fp_half_angle, d);
            let in_r = smoothstep(sea.fp_r_min, sea.fp_r_min + 1.5, r) * (1.0 - smoothstep(sea.fp_r_max - 1.5, sea.fp_r_max, r));
            fp = in_ang * in_r;
        }
        // The beam itself, from the tower to the sea's edge: a faint lane the spotlight sits in.
        if sea.fp_kind == 1u {
            let lane_ang = 1.0 - smoothstep(sea.fp_half_angle * 0.5, sea.fp_half_angle, d);
            let lane_r = smoothstep(8.0, 14.0, r) * (1.0 - smoothstep(sea.sea_radius - 4.0, sea.sea_radius, r));
            lane = sea.beam_lane * lane_ang * lane_r * (1.0 - fp);
        }
    }
    // Light on water: waves catch it unevenly.
    let wave_catch = 0.75 + 0.25 * (w1 * 0.5 + w3 * 0.5);
    let fp_strength = select(1.1, 0.06, sea.fp_kind == 2u) * wave_catch;
    emissive = emissive + vec3<f32>(1.0, 0.86, 0.6) * (fp * fp_strength + lane * 0.05 * wave_catch);
    let lit_water = vec4<f32>(0.32, 0.42, 0.5, 1.0);
    pbr_input.material.base_color = mix(pbr_input.material.base_color, lit_water, fp * select(0.85, 0.2, sea.fp_kind == 2u) + lane * 0.12);

    // Dawn: the sea lightens and the plankton fades against daylight.
    emissive = emissive * (1.0 - 0.85 * sea.dawn);
    pbr_input.material.base_color = mix(
        pbr_input.material.base_color,
        vec4<f32>(0.05, 0.13, 0.17, 1.0),
        sea.dawn,
    );
    pbr_input.material.emissive = pbr_input.material.emissive + vec4<f32>(emissive, 0.0);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
