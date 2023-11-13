use bevy_bundlication::prelude::*;

use bevy::{prelude::*, reflect::TypePath};
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
pub struct Number(u8);

#[allow(dead_code)]
#[derive(NetworkedBundle, TypePath)]
struct NumberBundle {
    number: Number,
}

#[test]
fn test_client_authority() {
    let mut app = App::new();
    app.add_plugins(ServerNetworkingPlugin::new(0));
    app.register_bundle::<ClientToServer, NumberBundle, 0>();

    // This entity has the complete bundle
    let e1 = app.world.spawn_client(1, Authority::Client(1)).id();
    let e2 = app.world.spawn_client(2, Authority::Client(2)).id();
    let e3 = app.world.spawn_with_id(1, 1, Authority::Client(2)).id();
    let e4 = app.world.spawn_with_id(1, 2, Authority::Free).id();
    let e5 = app.world.spawn_with_id(1, 3, Authority::Server).id();

    let mut msgs = ServerMessages::default();

    msgs.input.push((
        1,
        vec![
            1, 0, 0, 0, // Tick
            1, 0, 1, 0, 0, 0, 5, // 1
            1, 0, 2, 0, 0, 0, 6, // 2
            1, 1, 1, 0, 0, 0, 7, // 3
            1, 1, 2, 0, 0, 0, 8, // 4
            1, 1, 3, 0, 0, 0, 9, // 5
        ],
    ));

    app.insert_resource(msgs);

    app.update();

    assert_eq!(app.world.entity(e1).get::<Number>(), Some(&Number(5)));
    assert_eq!(app.world.entity(e2).get::<Number>(), None);
    assert_eq!(app.world.entity(e3).get::<Number>(), None);
    assert_eq!(app.world.entity(e4).get::<Number>(), Some(&Number(8)));
    assert_eq!(
        app.world.entity(e4).get::<Authority>(),
        Some(&Authority::Client(1))
    );
    assert_eq!(app.world.entity(e5).get::<Number>(), None);

    let mut msgs = ServerMessages::default();
    msgs.input.push((
        2,
        vec![
            2, 0, 0, 0, // Tick
            1, 1, 1, 0, 0, 0, 1, // 1
            1, 1, 2, 0, 0, 0, 2, // 2
            1, 1, 3, 0, 0, 0, 3, // 3
        ],
    ));
    app.insert_resource(msgs);

    app.update();

    assert_eq!(app.world.entity(e3).get::<Number>(), Some(&Number(1)));
    assert_eq!(app.world.entity(e4).get::<Number>(), Some(&Number(8)));
    assert_eq!(app.world.entity(e5).get::<Number>(), None);
}
