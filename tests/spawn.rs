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
    fn read_new(r: impl Read, _: Tick, _: &mut IdentifierManager) -> NetworkReadResult<Transform> {
        let pos: Position = deserialize(r)?;
        Ok(Transform {
            translation: Vec3::new(pos.0 as f32, pos.1 as f32, pos.2 as f32),
            ..default()
        })
    }
}

#[derive(NetworkedBundle, Bundle, TypePath, Default)]
struct SpawnTestBundle {
    #[networked(as = Position)]
    trans: Transform,
    #[networked(no_send)]
    marker: Marker,
}

#[test]
fn test_spawn() {
    let mut app = App::new();
    app.add_plugins(ServerNetworkingPlugin::new(0));
    app.insert_resource(Tick(1));
    app.init_resource::<ServerMessages>();
    app.register_bundle::<ServerToAll, SpawnTestBundle, 0>();
    app.world.send_event(Connected(Identity::Client(1)));
    app.world.send_event(StartReplication(Identity::Client(1)));

    // This entity has the complete bundle
    app.world.spawn_client(
        1,
        SpawnTestBundle {
            trans: Transform::from_translation(Vec3::new(1., 2., 3.)),
            ..default()
        },
    );

    app.world.spawn_with_id(
        1,
        1,
        SpawnTestBundle {
            trans: Transform::from_translation(Vec3::new(4., 0., 4.)),
            ..default()
        },
    );

    app.world.spawn_with_id(1, 2, SpawnTestBundle::default());

    app.update();

    let mut msgs = ClientMessages::default();
    for msg in app.world.resource::<ServerMessages>().output.iter() {
        msgs.input.push(msg.2.clone());
    }

    let mut app = App::new();
    app.add_plugins(ClientNetworkingPlugin::new(0));
    app.insert_resource(msgs);
    app.register_bundle::<ServerToAll, SpawnTestBundle, 0>();

    let entity = app.world.spawn_client(1, Transform::default()).id();

    app.update();

    assert_eq!(app.world.entities().len(), 3);
    let map = app.world.resource::<IdentifierMap>();
    let entity1 = map.get_client(1).unwrap();
    assert_eq!(entity, entity1);
    let entity2 = map.get_id(1, 1).unwrap();
    let entity3 = map.get_id(1, 2).unwrap();

    let mut trans_query = app.world.query::<&Transform>();
    let mut marker_query = app.world.query::<&Marker>();

    assert_eq!(
        *trans_query.get(&app.world, entity1).unwrap(),
        Transform::from_translation(Vec3::new(1., 2., 3.))
    );
    assert!(marker_query.get(&app.world, entity1).is_ok());
    assert_eq!(
        *trans_query.get(&app.world, entity2).unwrap(),
        Transform::from_translation(Vec3::new(4., 0., 4.))
    );
    assert!(marker_query.get(&app.world, entity2).is_ok());
    assert_eq!(
        *trans_query.get(&app.world, entity3).unwrap(),
        Transform::from_translation(Vec3::new(0., 0., 0.))
    );
    assert!(marker_query.get(&app.world, entity3).is_ok());
}
