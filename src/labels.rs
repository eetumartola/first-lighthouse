//! World-space labels: anything the light reveals is named, and Mutable Sea says why it holds.
//! Labels follow the same visibility rule as the meshes, so they never leak hidden positions.

use crate::app::{to_world_h, Session};
use crate::scene::MainCamera;
use crate::sim::{self, mutable_sea, Form, Phase, Rules, Visibility as SimVis};

use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum LabelKey {
    Entity(sim::EntityId),
}

#[derive(Component)]
struct WorldLabel;

#[derive(Resource, Default)]
struct LabelMap(HashMap<LabelKey, Entity>);

pub struct LabelsPlugin;

impl Plugin for LabelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LabelMap>().add_systems(Update, update_labels);
    }
}

fn label_text(world: &sim::World, e: &sim::Entity) -> String {
    let mut text = format!("{}  {}", e.name, e.form.name());
    if let Rules::MutableSea(_) = &world.rules {
        if e.is_active() && world.phase == Phase::Night {
            if world.sea.is_preserved(e.pos) {
                text.push_str("  held by light");
            } else if mutable_sea::instability(e, &world.sea) > 0.7 {
                text.push_str("  changing");
            }
        }
    }
    text
}

fn update_labels(
    mut commands: Commands,
    session: Res<Session>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut labels: ResMut<LabelMap>,
    mut nodes: Query<(&mut Node, &mut Text), With<WorldLabel>>,
) {
    let (camera, cam_tf) = *camera;
    let mut wanted: Vec<(LabelKey, Vec3, String)> = Vec::new();
    if let Some(world) = session.world() {
        for e in &world.sea.entities {
            let show = match world.entity_visibility(e) {
                SimVis::Lit => e.is_active() || e.status == sim::Status::Secured,
                // Surfaced creatures are named too; afterglow silhouettes stay anonymous.
                SimVis::Silhouette(_) => e.form == Form::Creature && world.sea.time < e.surfaced_until,
                SimVis::Hidden => false,
            };
            if show && world.phase != Phase::Finished {
                wanted.push((LabelKey::Entity(e.id), to_world_h(e.pos, 4.5), label_text(world, e)));
            }
        }
    }

    let mut keep: Vec<LabelKey> = Vec::with_capacity(wanted.len());
    for (key, pos, text) in wanted {
        let Ok(screen) = camera.world_to_viewport(cam_tf, pos) else { continue };
        keep.push(key);
        let node = Node {
            position_type: PositionType::Absolute,
            left: px(screen.x - 40.0),
            top: px(screen.y - 24.0),
            ..default()
        };
        match labels.0.get(&key).copied() {
            Some(entity) => {
                if let Ok((mut n, mut t)) = nodes.get_mut(entity) {
                    *n = node;
                    if t.0 != text {
                        t.0 = text;
                    }
                }
            }
            None => {
                let entity = commands
                    .spawn((
                        WorldLabel,
                        Text::new(text),
                        TextFont::from_font_size(14.0),
                        TextColor(Color::srgba(0.9, 0.88, 0.8, 0.9)),
                        node,
                    ))
                    .id();
                labels.0.insert(key, entity);
            }
        }
    }
    labels.0.retain(|key, entity| {
        if keep.contains(key) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });
}
