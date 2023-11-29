use bevy_bundlication::prelude::*;

use bevy::{prelude::*, reflect::TypePath};
use serde::{Deserialize, Serialize};

#[derive(
    Component, Clone, Deref, DerefMut, Serialize, Deserialize, Debug, PartialEq, Eq, Default,
)]
pub struct Number(u8);

#[allow(dead_code)]
#[derive(NetworkedBundle, TypePath)]
struct NumberBundle {
    number: Number,
}

#[test]
fn test_client_authority_despawn() {
    let mut app = App::new();
    app.add_plugins(ServerNetworkingPlugin::new(13));

    let e1 = app.world.spawn_client(1, Authority::Client(1)).id();
    let e2 = app.world.spawn_client(2, Authority::Client(2)).id();
    let e3 = app.world.spawn_with_id(1, 1, Authority::Free).id();
    let e4 = app.world.spawn_with_id(1, 2, Authority::Server).id();

    let mut msgs = ServerMessages::default();
    msgs.input.push((
        1,
        vec![
            1, 0, 0, 0, // Tick
            0, 0, 1, 0, 0, 0, // e1
            0, 0, 2, 0, 0, 0, // e2
            0, 1, 1, 0, 0, 0, // e4
            0, 1, 2, 0, 0, 0, // e4
        ],
    ));
    app.insert_resource(msgs);

    app.update();

    assert!(app.world.get_entity(e1).is_none());
    assert!(app.world.get_entity(e2).is_some());
    assert!(app.world.get_entity(e3).is_none());
    assert!(app.world.get_entity(e4).is_some());
    let map = app.world.resource::<IdentifierMap>();
    assert_eq!(map.n_alive(), 2);
    assert_eq!(map.n_total(), 4);
}

#[test]
fn test_client_sends_only_right_despawns() {
    let mut app = App::new();
    app.add_plugins(ClientNetworkingPlugin::new(13));
    app.insert_resource(Identity::Client(1));

    let e1 = app.world.spawn_client(1, Authority::Client(1)).id();
    let e2 = app.world.spawn_client(2, Authority::Client(2)).id();
    let e3 = app.world.spawn_with_id(1, 1, Authority::Free).id();
    let e4 = app.world.spawn_with_id(1, 2, Authority::Server).id();
    app.world.spawn_with_id(1, 3, Authority::Free);

    // We ignore the initial data
    app.init_resource::<ClientMessages>();
    app.update();

    // We despawn entities 1-4
    for entity in [e1, e2, e3, e4] {
        app.world.despawn(entity);
    }

    app.init_resource::<ClientMessages>();
    app.update();

    let msgs = app.world.resource::<ClientMessages>();
    assert_eq!(msgs.output.len(), 1);
    assert_eq!(
        msgs.output[0],
        (
            13,
            vec![
                1, 0, 0, 0, //Tick
                0, 0, 1, 0, 0, 0, //e1
                0, 1, 1, 0, 0, 0, //e3
            ]
        )
    );
}

#[test]
fn test_early_despawn() {
    let mut app = App::new();
    app.add_plugins(ClientNetworkingPlugin::new(13))
        .register_bundle::<ServerToAll, NumberBundle, 1>();

    let e1 = app.world.spawn_client(1, ()).id();

    let mut msgs = ClientMessages::default();
    msgs.input.extend_from_slice(&[
        vec![
            1, 0, 0, 0, // Tick
            1, 0, 1, 0, 0, 0, 1, 1, 0, // update e1
        ],
        vec![
            5, 0, 0, 0, // Tick
            0, 0, 1, 0, 0, 0, // despawn e1
        ],
        vec![
            2, 0, 0, 0, // Tick
            1, 0, 1, 0, 0, 0, 1, 2, 0, // update e1
        ],
    ]);
    app.insert_resource(msgs);

    app.update();

    assert!(app.world.get_entity(e1).is_none());
    let map = app.world.resource::<IdentifierMap>();
    assert_eq!(map.n_alive(), 0);
    assert_eq!(map.n_total(), 1);
}

#[test]
fn test_late_despawn() {
    let mut app = App::new();
    app.add_plugins(ClientNetworkingPlugin::new(13))
        .register_bundle::<ServerToAll, NumberBundle, 1>();

    app.world.spawn_client(1, ());

    let mut msgs = ClientMessages::default();
    msgs.input.extend_from_slice(&[
        vec![
            27, 0, 0, 0, // Tick
            1, 0, 1, 0, 0, 0, 1, 2, 0, // update e1
        ],
        vec![
            5, 0, 0, 0, // Tick
            0, 0, 1, 0, 0, 0, // despawn e1
        ],
    ]);
    app.insert_resource(msgs);

    app.update();

    let map = app.world.resource::<IdentifierMap>();
    assert_eq!(map.n_alive(), 1);
    assert_eq!(map.n_total(), 1);
}
