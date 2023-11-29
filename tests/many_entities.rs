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
fn test_many_entities() {
    let mut app = App::new();
    app.add_plugins(ServerNetworkingPlugin::new(0));
    app.init_resource::<ServerMessages>();
    app.register_bundle::<ServerToAll, NumberBundle, 1>();

    #[allow(clippy::unnecessary_to_owned)]
    for label in app
        .world
        .resource::<bevy::app::MainScheduleOrder>()
        .labels
        .to_vec()
    {
        app.edit_schedule(label, |schedule| {
            schedule.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
        });
    }

    // We spawn 25k entities
    for i in 0..25000 {
        app.world.spawn_client(i, Number((i % 255) as u8));
    }
    app.world.send_event(NewConnection(Identity::Client(0)));

    app.update();

    let mut msgs = app.world.remove_resource::<ServerMessages>().unwrap();
    // We have 25k entities, each at 9 bytes (packet type (1), Identifier (5), bundle id (1), Number (1), entity end (1))
    // Packets can be at most 1200 bytes, that means we need over 188 packets
    println!("{}", msgs.output.len());
    assert!(msgs.output.len() == 204);

    let mut client_msgs = ClientMessages::default();
    for msg in msgs.output.drain(..) {
        client_msgs.input.push(msg.2)
    }

    let mut app = App::new();
    app.add_plugins(ClientNetworkingPlugin::new(0));
    app.init_resource::<ClientMessages>();
    app.register_bundle::<ServerToAll, NumberBundle, 1>();

    #[allow(clippy::unnecessary_to_owned)]
    for label in app
        .world
        .resource::<bevy::app::MainScheduleOrder>()
        .labels
        .to_vec()
    {
        app.edit_schedule(label, |schedule| {
            schedule.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
        });
    }

    app.insert_resource(client_msgs);

    app.update();

    assert_eq!(app.world.entities().len(), 25000);
}
