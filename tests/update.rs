use bevy_bundlication::*;

use bevy::{prelude::*, reflect::TypePath};
use serde::{Deserialize, Serialize};

#[derive(
    Component, Clone, Deref, DerefMut, Serialize, Deserialize, Debug, PartialEq, Eq, Default,
)]
pub struct Number(u8);

fn add_numbers(current: &mut Number, new: Number) {
    current.0 += new.0
}

#[derive(NetworkedBundle, Bundle, TypePath, Default)]
struct SumBundle {
    #[networked(update = add_numbers)]
    sum: Number,
}

#[test]
fn test_update() {
    let mut app = App::new();
    app.add_plugins(ClientNetworkingPlugin::new(0));
    app.register_bundle::<ServerToAll, SumBundle, 0>();

    let e1 = app.world.spawn_client(1, ()).id();
    let e2 = app.world.spawn_client(2, Number(2)).id();

    let mut msgs = ClientMessages::default();

    msgs.input.push(vec![
        1, 0, 0, 0, // Tick
        1, 0, 1, 0, 0, 0, 5, // 1
        1, 0, 2, 0, 0, 0, 5, // 2
    ]);

    app.insert_resource(msgs);

    app.update();

    assert_eq!(app.world.entity(e1).get::<Number>(), Some(&Number(5)));
    assert_eq!(app.world.entity(e2).get::<Number>(), Some(&Number(7)));
}
