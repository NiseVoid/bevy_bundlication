use bevy_bundlication::prelude::*;

use std::io::{Read, Write};

use bevy::{prelude::*, reflect::TypePath};
use serde::{Deserialize, Serialize};

#[derive(Component, Serialize, Deserialize, Default, Debug)]
struct Coordinates {
    x: u8,
    y: u8,
}

#[derive(Component, Deref, DerefMut, Default, Debug)]
struct Hp(i64);

#[derive(Serialize, Deserialize)]
struct HpSerialized(u16);

impl NetworkedComponent for Hp {
    fn write_data(&self, w: impl Write, _: Tick, _: &IdentifierMap) -> IdentifierResult<()> {
        serialize(w, &HpSerialized(self.0.max(0) as u16)).unwrap();
        Ok(())
    }

    fn read_new(r: impl Read, _: Tick, _: &mut IdentifierManager) -> NetworkReadResult<Self> {
        let hp: HpSerialized = deserialize(r)?;
        Ok(Self(hp.0 as i64))
    }
}

#[allow(dead_code)]
#[derive(NetworkedBundle, TypePath, Default)]
struct Test1Bundle {
    coord: Coordinates,
}

#[derive(NetworkedBundle, Bundle, TypePath, Default)]
struct Test2Bundle {
    coord: Coordinates,
    hp: Hp,
}

#[test]
fn test_overlapping_bundles() {
    let mut app = App::new();
    app.add_plugins(ServerNetworkingPlugin::new(0));
    app.init_resource::<ServerMessages>();

    let entity1 = app
        .world
        .spawn_client(
            1,
            (Test2Bundle {
                coord: Coordinates { x: 5, y: 10 },
                hp: Hp(3000),
            },),
        )
        .id();
    let entity2 = app
        .world
        .spawn_with_id(
            // This is another entity type from client, so client 1 should be considered separate
            1,
            1,
            (Test2Bundle {
                coord: Coordinates { x: 3, y: 2 },
                hp: Hp(500),
            },),
        )
        .id();
    let entity7 = app
        .world
        .spawn_client(
            7,
            (Test2Bundle {
                coord: Coordinates { x: 2, y: 90 },
                hp: Hp(-3),
            },),
        )
        .id();
    app.world.send_event(Connected(Identity::Client(1)));
    app.world.send_event(Connected(Identity::Client(7)));
    app.world.send_event(StartReplication(Identity::Client(1)));
    app.world.send_event(StartReplication(Identity::Client(7)));

    app.register_bundle::<ServerToOwner, Test2Bundle, 0>();
    app.register_bundle::<ServerToObserver, Test1Bundle, 0>();
    app.update();

    let mut msgs = app.world.resource_mut::<ServerMessages>();
    assert_eq!(msgs.output.len(), 2);
    assert!(msgs.output.contains(&(
        0,
        Identity::Client(1),
        vec![
            0, 0, 0, 0, // Tick
            1, // Entity message
            0, 1, 0, 0, 0, // Identifier
            2, // Bundle
            5, 10, 184, 11, // Data
            0,  // End of Entity
            1,  // Entity message
            1, 1, 0, 0, 0, // Identifier
            1, // Bundle
            3, 2, // Data
            0, // End of Entity
            1, // Entity message
            0, 7, 0, 0, 0, // Identifier
            1, // Bundle
            2, 90, // Data
            0,  // End of Entity
        ]
    )));
    assert!(msgs.output.contains(&(
        0,
        Identity::Client(7),
        vec![
            0, 0, 0, 0, // Tick
            1, // Entity message
            0, 1, 0, 0, 0, // Identifier
            1, // Bundle
            5, 10, //Data
            0,  // End of Entity
            1,  // Entity message
            1, 1, 0, 0, 0, // Identifier
            1, // Bundle
            3, 2, // Data
            0, // End of Entity
            1, // Entity message
            0, 7, 0, 0, 0, // Identifier
            2, // Bundle
            2, 90, 0, 0, // Data
            0, // End of Entity
        ]
    )));
    msgs.output.clear();

    // Edit the HP of Player 7, should create a message for player 7 with coordinates and HP
    **app
        .world
        .query::<&mut Hp>()
        .get_mut(&mut app.world, entity7)
        .unwrap() = 5;
    app.update();

    let mut msgs = app.world.resource_mut::<ServerMessages>();
    assert_eq!(msgs.output.len(), 1);
    assert!(msgs.output.contains(&(
        0,
        Identity::Client(7),
        vec![
            1, 0, 0, 0, // Tick
            1, // Entity message
            0, 7, 0, 0, 0, // Identifier
            2, // Packet
            2, 90, 5, 0, // Data
            0, // End of Entity
        ]
    )));
    msgs.output.clear();

    // Edit the Coordinates of Player 1, should create a message for player 1 with coordinates
    // and HP, and a message for all others with only the new coordinates
    app.world
        .query::<&mut Coordinates>()
        .get_mut(&mut app.world, entity1)
        .unwrap()
        .x += 7;
    app.update();

    let mut msgs = app.world.resource_mut::<ServerMessages>();
    assert!(msgs.output.contains(&(
        0,
        Identity::Client(1),
        vec![
            2, 0, 0, 0, // Tick
            1, // Entity message
            0, 1, 0, 0, 0, // Identifier
            2, // Packet
            12, 10, 184, 11, // Data
            0,  // End of Entity
        ]
    )));
    assert!(msgs.output.contains(&(
        0,
        Identity::Client(7),
        vec![
            2, 0, 0, 0, // Tick
            1, // Entity message
            0, 1, 0, 0, 0, // Identifier
            1, // Packet
            12, 10, // Data
            0,  // End of Entity
        ]
    )));
    msgs.output.clear();

    // Edit the HP of the non-player entity, this should generate no messages
    **app
        .world
        .query::<&mut Hp>()
        .get_mut(&mut app.world, entity2)
        .unwrap() = 1234;
    app.update();

    let msgs = app.world.resource_mut::<ServerMessages>();
    assert_eq!(msgs.output.len(), 0);

    // Edit the Coordinates of the non-player entity, this should send the new coordinates to
    // everyone
    app.world
        .query::<&mut Coordinates>()
        .get_mut(&mut app.world, entity2)
        .unwrap()
        .y += 6;
    app.update();

    let mut msgs = app.world.resource_mut::<ServerMessages>();
    assert_eq!(msgs.output.len(), 2);
    for client in [1, 7] {
        assert!(msgs.output.contains(&(
            0,
            Identity::Client(client),
            vec![
                4, 0, 0, 0, // Tick
                1, // Entity message
                1, 1, 0, 0, 0, // Identifier
                1, // Packet
                3, 8, // Data
                0, // End of Entity
            ]
        )));
    }
    msgs.output.clear();
}
