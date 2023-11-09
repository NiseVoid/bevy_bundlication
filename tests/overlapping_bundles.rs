use bevy_bundlication::*;
use SendRule::*;

use bevy::{prelude::*, reflect::TypePath};
use serde::{Deserialize, Serialize};

#[derive(Component, Serialize, Deserialize, Default, Clone, Debug)]
struct Coordinates {
    x: u8,
    y: u8,
}

#[derive(Component, Deref, DerefMut, Default, Debug)]
struct Hp(i64);

#[derive(Serialize, Deserialize)]
struct HpSerialized(u16);

impl NetworkedComponent for Hp {
    type As = HpSerialized;

    fn to_networked(&self, _: Tick, _: &IdentifierMap) -> IdentifierResult<Self::As> {
        Ok(HpSerialized(self.0.max(0) as u16))
    }

    fn from_networked(_: Tick, _: &IdentifierMap, networked: Self::As) -> IdentifierResult<Self> {
        Ok(Self(networked.0 as i64))
    }
}

#[derive(NetworkedBundle, Bundle, TypePath, Default)]
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

    app.register_bundle::<ServerToOwner, Test2Bundle, 0>();
    app.register_bundle::<ServerToObserver, Test1Bundle, 0>();
    app.update();

    let mut msgs = app.world.resource_mut::<ServerMessages>();
    assert_eq!(msgs.output.len(), 5);
    assert!(msgs
        .output
        .contains(&(0, Except(1), vec![0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 5, 10])));
    assert!(msgs
        .output
        .contains(&(0, All, vec![0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 3, 2])));
    assert!(msgs
        .output
        .contains(&(0, Except(7), vec![0, 0, 0, 0, 1, 0, 7, 0, 0, 0, 2, 90])));
    assert!(msgs.output.contains(&(
        0,
        Only(1),
        vec![0, 0, 0, 0, 2, 0, 1, 0, 0, 0, 5, 10, 184, 11]
    )));
    assert!(msgs
        .output
        .contains(&(0, Only(7), vec![0, 0, 0, 0, 2, 0, 7, 0, 0, 0, 2, 90, 0, 0])));
    msgs.output.clear();

    // Edit the HP of Player 7, should create a message for player 7 with coordinates and HP
    **app.world.query::<&mut Hp>().get_mut(&mut app.world, entity7).unwrap() = 5;
    app.update();

    let mut msgs = app.world.resource_mut::<ServerMessages>();
    assert_eq!(msgs.output.len(), 1);
    assert!(msgs
        .output
        .contains(&(0, Only(7), vec![1, 0, 0, 0, 2, 0, 7, 0, 0, 0, 2, 90, 5, 0])));
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
    assert_eq!(msgs.output.len(), 2);
    assert!(msgs
        .output
        .contains(&(0, Except(1), vec![2, 0, 0, 0, 1, 0, 1, 0, 0, 0, 12, 10])));
    assert!(msgs.output.contains(&(
        0,
        Only(1),
        vec![2, 0, 0, 0, 2, 0, 1, 0, 0, 0, 12, 10, 184, 11]
    )));
    msgs.output.clear();

    // Edit the HP of the non-player entity, this should generate no messages
    **app.world.query::<&mut Hp>().get_mut(&mut app.world, entity2).unwrap() = 1234;
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
    assert_eq!(msgs.output.len(), 1);
    assert!(msgs
        .output
        .contains(&(0, All, vec![4, 0, 0, 0, 1, 1, 1, 0, 0, 0, 3, 8])));
    msgs.output.clear();
}
