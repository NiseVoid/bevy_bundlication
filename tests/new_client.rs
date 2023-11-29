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
fn test_new_client() {
    let mut app = App::new();
    app.add_plugins(ServerNetworkingPlugin::new(0));
    app.insert_resource(Tick(1));
    app.init_resource::<ServerMessages>();
    app.register_bundle::<ServerToAll, NumberBundle, 1>();

    // This entity has the complete bundle
    let e = app.world.spawn_client(1, Number(2)).id();
    app.world.spawn_with_id(1, 1, Number(5));
    app.world.spawn_with_id(1, 2, Number(7));

    // We don't care about the initial updates, so we just reset those
    app.update();
    app.world.remove_resource::<ServerMessages>();
    app.init_resource::<ServerMessages>();

    // We up the tick, change one element and connect a new client
    app.insert_resource(Tick(2));
    **app.world.entity_mut(e).get_mut::<Number>().unwrap() = 3;
    app.world.send_event(Connected(Identity::Client(13)));
    app.world.send_event(StartReplication(Identity::Client(13)));

    app.update();

    let msgs = app.world.resource::<ServerMessages>();
    // Now we expect the changed entity to get broadcast, while the new clients also gets updated
    // about the other two entities
    assert_eq!(msgs.output.len(), 1);
    assert!(msgs.output.contains(&(
        0,
        Identity::Client(13),
        vec![
            2, 0, 0, 0, //Tick
            1, // Entity
            0, 1, 0, 0, 0, 1, 3, // 1
            0, 1, // end, entity
            1, 1, 0, 0, 0, 1, 5, // 2
            0, 1, // end, entity
            1, 2, 0, 0, 0, 1, 7, // 3
            0, // end
        ]
    ),));
}
