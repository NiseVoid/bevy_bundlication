use bevy_bundlication::prelude::*;

use std::io::{Read, Write};

use bevy::{prelude::*, reflect::TypePath};
use serde::{Deserialize, Serialize};

#[derive(Component, Default)]
pub struct Marker;

#[derive(Serialize, Deserialize)]
pub struct Position(u8, u8, u8);

impl NetworkedWrapper<Transform> for Position {
    fn write_data(
        from: &Transform,
        w: impl Write,
        _: Tick,
        _: &IdentifierMap,
    ) -> IdentifierResult<()> {
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
    fn read_new(r: impl Read, _: Tick, _: &mut IdentifierMap) -> IdentifierResult<Transform> {
        let pos: Self = deserialize(r).unwrap();
        Ok(Transform {
            translation: Vec3::new(pos.0 as f32, pos.1 as f32, pos.2 as f32),
            ..default()
        })
    }
}

#[derive(Component, Default)]
pub struct CanBeMissing;

#[derive(NetworkedBundle, Bundle, TypePath, Default)]
struct BundleWithAttributes {
    #[networked(as = Position)]
    trans: Transform,
    #[networked(no_send)]
    marker: Marker,
    #[networked(no_send, optional)]
    can_be_missing: CanBeMissing,
}

#[test]
fn test_attributes() {
    let mut app = App::new();
    app.add_plugins(ServerNetworkingPlugin::new(0));
    app.init_resource::<ServerMessages>();
    app.register_bundle::<ServerToAll, BundleWithAttributes, 0>();

    // This entity has the complete bundle
    app.world.spawn_client(
        1,
        (BundleWithAttributes {
            trans: Transform::from_translation(Vec3::new(1., 2., 3.)),
            ..default()
        },),
    );

    // This entity is missing CanBeMissing, but it's optional so it still gets sent
    app.world.spawn_client(
        2,
        (Transform::from_translation(Vec3::new(6., 5., 4.)), Marker),
    );

    // This entity is missing the required marker
    app.world.spawn_client(
        3,
        (
            Transform::from_translation(Vec3::new(18., 28., 38.)),
            CanBeMissing,
        ),
    );
    app.world.send_event(Connected(Identity::Client(1)));
    app.world.send_event(StartReplication(Identity::Client(1)));

    app.update();

    let mut msgs = app.world.resource_mut::<ServerMessages>();
    assert_eq!(msgs.output.len(), 1);
    assert!(msgs.output.contains(&(
        0,
        Identity::Client(1),
        vec![
            0, 0, 0, 0, // Tick
            1, 0, 1, 0, 0, 0, 1, 1, 2, 3, 0, // 1
            1, 0, 2, 0, 0, 0, 1, 6, 5, 4, 0, // 2
        ]
    )));
    msgs.output.clear();
}
