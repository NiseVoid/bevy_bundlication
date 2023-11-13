use bevy_bundlication::prelude::*;

use bevy::{prelude::*, reflect::TypePath};
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
pub struct Number(u8);

#[allow(dead_code)]
#[derive(NetworkedBundle, TypePath)]
struct NumberBundle {
    number: Number,
}

#[test]
fn test_remote_components() {
    let mut app = App::new();
    app.add_plugins(ClientNetworkingPlugin::new(0));
    app.register_bundle::<ServerToAll, NumberBundle, 0>();

    // This entity has the complete bundle
    let e1 = app.world.spawn_client(1, Number(5)).id();
    let e2 = app
        .world
        .spawn_client(2, (Number(6), Remote::new(Number(2))))
        .id();

    let mut msgs = ClientMessages::default();

    msgs.input.push(vec![
        1, 0, 0, 0, // Tick
        1, 0, 1, 0, 0, 0, 12, // 1
        1, 0, 2, 0, 0, 0, 13, // 2
    ]);

    app.insert_resource(msgs);

    app.update();

    assert_eq!(app.world.entity(e1).get::<Number>(), Some(&Number(12)));
    assert_eq!(app.world.entity(e2).get::<Number>(), Some(&Number(6)));
    assert_eq!(
        app.world.entity(e2).get::<Remote<Number>>().map(|n| **n),
        Some(Number(13))
    );
    assert_eq!(
        app.world
            .entity(e2)
            .get::<Remote<Number>>()
            .map(|n| n.tick()),
        Some(Tick(1))
    );
}
