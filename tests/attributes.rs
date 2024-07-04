use bevy_bundlication::prelude::*;

use std::io::{Read, Write};

use bevy::{prelude::*, reflect::TypePath};
use bevy_replicon::core::{
    replication_registry::{test_fns::TestFnsEntityExt, ReplicationRegistry},
    replication_rules::GroupReplication,
    replicon_tick::RepliconTick,
};
use bincode::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Position(u8, u8, u8);

impl NetworkedWrapper<Transform> for Position {
    fn write_data(from: &Transform, w: impl Write, _: &SerializeCtx) -> Result<()> {
        serialize(
            w,
            &Self(
                from.translation.x as u8,
                from.translation.y as u8,
                from.translation.z as u8,
            ),
        )
        .unwrap();
        Ok(())
    }
    fn read_new(r: impl Read, _: &mut DeserializeCtx) -> Result<Transform> {
        let pos: Self = deserialize(r)?;
        Ok(Transform {
            translation: Vec3::new(pos.0 as f32, pos.1 as f32, pos.2 as f32),
            ..default()
        })
    }
}

#[derive(Component, PartialEq, Eq, Default, Debug)]
pub struct NotSent(u8);

#[derive(NetworkedBundle, Bundle, TypePath, Default)]
#[bundlication(priority = 17)]
struct BundleWithAttributes {
    #[bundlication(as = Position)]
    trans: Transform,
    #[bundlication(no_send)]
    not_sent: NotSent,
}

#[test]
fn test_attributes() {
    let mut app = App::new();
    app.add_plugins(bevy_replicon::RepliconPlugins);

    let mut replication_fns = ReplicationRegistry::default();
    let rule = BundleWithAttributes::register(app.world_mut(), &mut replication_fns);
    app.insert_resource(replication_fns);

    assert_eq!(17, rule.priority);
    let components = rule.components;

    let tick = RepliconTick::default();
    let mut entity = app.world_mut().spawn_empty();

    // Test the functions for Transform (as Position)

    // Test if the Transform write function behaves correctly
    entity.apply_write(&[1, 2, 3], components[0], tick);
    assert_eq!(
        entity.get::<Transform>(),
        Some(&Transform::from_xyz(1., 2., 3.))
    );

    // Test Transform's serialize output
    let mut transform = entity.get_mut::<Transform>().unwrap();
    transform.translation += Vec3::ONE;
    transform.rotation = Quat::from_rotation_z(1.5);
    assert_eq!(
        entity.serialize(components[0], RepliconTick::new(0)),
        vec![2, 3, 4]
    );

    // Test the function for NotSent

    // Test if the NotSent write function spawns from no data
    entity.apply_write(&[], components[1], tick);
    assert_eq!(entity.get::<NotSent>(), Some(&NotSent::default()));

    // Test NotSent's serialize output
    *entity.get_mut::<NotSent>().unwrap() = NotSent(12);
    assert_eq!(
        entity.serialize(components[1], RepliconTick::new(0)),
        vec![]
    );
}
