use bevy_bundlication::prelude::*;
use SendRule::*;

use bevy::{ecs::system::Command, prelude::*, reflect::TypePath};
use serde::{Deserialize, Serialize};

#[derive(Event, TypePath, PartialEq, Eq, Debug)]
pub struct BroadcastEvent {
    about: Entity,
    value: i32,
}

#[derive(Serialize, Deserialize)]
pub struct NetworkedTestEvent {
    about: Identifier,
    value: u16,
}

impl NetworkedEvent for BroadcastEvent {
    type As = NetworkedTestEvent;

    fn from_networked(_: Tick, map: &IdentifierMap, networked: Self::As) -> IdentifierResult<Self> {
        let about = map.get_alive(&networked.about)?;
        Ok(Self {
            about,
            value: networked.value as i32,
        })
    }

    fn to_networked(&self, _: Tick, map: &IdentifierMap) -> IdentifierResult<Self::As> {
        let about = map.from_entity(&self.about)?;
        Ok(Self::As {
            about,
            value: self.value.max(0) as u16,
        })
    }
}

#[derive(Event, TypePath, PartialEq, Eq, Debug)]
pub struct TargetedEvent {
    target: Entity,
    value: i32,
}

impl NetworkedEvent for TargetedEvent {
    type As = NetworkedTestEvent;

    fn from_networked(_: Tick, map: &IdentifierMap, networked: Self::As) -> IdentifierResult<Self> {
        let about = map.get_alive(&networked.about)?;
        Ok(Self {
            target: about,
            value: networked.value as i32,
        })
    }

    fn to_networked(&self, _: Tick, map: &IdentifierMap) -> IdentifierResult<Self::As> {
        let about = map.from_entity(&self.target)?;
        Ok(Self::As {
            about,
            value: self.value.max(0) as u16,
        })
    }
}

#[test]
fn test_send_events() {
    let mut app = App::new();
    app.add_plugins(ServerNetworkingPlugin::new(0));
    app.init_resource::<ServerMessages>();
    app.register_event::<ServerToClient, TargetedEvent, 17>();
    app.register_event::<ServerToClient, BroadcastEvent, 23>();

    let e1 = app.world.spawn_client(1, ()).id();
    let e2 = app.world.spawn_client(2, ()).id();

    app.update();

    SendEvent {
        event: TargetedEvent {
            target: e1,
            value: 10,
        },
        channel: 17,
        rule: SendRule::Only(1),
    }
    .apply(&mut app.world);
    SendEvent {
        event: TargetedEvent {
            target: e2,
            value: 11,
        },
        channel: 18,
        rule: SendRule::Only(2),
    }
    .apply(&mut app.world);

    SendEvent {
        event: BroadcastEvent {
            about: e1,
            value: 30,
        },
        channel: 23,
        rule: SendRule::All,
    }
    .apply(&mut app.world);
    SendEvent {
        event: BroadcastEvent {
            about: e1,
            value: 31,
        },
        channel: 23,
        rule: SendRule::All,
    }
    .apply(&mut app.world);

    app.update();

    let mut msgs = app.world.resource_mut::<ServerMessages>();
    assert_eq!(msgs.output.len(), 3);

    println!("{:?}", msgs.output);

    assert!(msgs.output.contains(&(
        17,
        Only(1),
        vec![
            1, 0, 0, 0, // Tick
            2, 0, 1, 0, 0, 0, 10, 0, // Event 1
        ]
    )));
    assert!(msgs.output.contains(&(
        18,
        Only(2),
        vec![
            1, 0, 0, 0, //Tick
            2, 0, 2, 0, 0, 0, 11, 0, // Event 2
        ],
    )));
    assert!(msgs.output.contains(&(
        23,
        All,
        vec![
            1, 0, 0, 0, // Tick
            1, 0, 1, 0, 0, 0, 30, 0, // Event 4
            1, 0, 1, 0, 0, 0, 31, 0, // Event 5
        ]
    )));
    msgs.output.clear();
}

#[test]
fn test_receive_events() {
    let mut app = App::new();
    app.add_plugins(ServerNetworkingPlugin::new(0));
    app.init_resource::<ServerMessages>();
    app.add_event::<NetworkEvent<BroadcastEvent>>();
    app.register_event::<ClientToServer, BroadcastEvent, 13>();
    app.add_event::<NetworkEvent<TargetedEvent>>();
    app.register_event::<ClientToServer, TargetedEvent, 13>();

    let e1 = app.world.spawn_client(1, ()).id();
    let e2 = app.world.spawn_client(2, ()).id();

    let mut msgs = ServerMessages::default();
    msgs.input.push((
        1,
        vec![
            2, 0, 0, 0, // Tick
            2, 0, 2, 0, 0, 0, 10, 0, // Event 1
        ],
    ));
    msgs.input.push((
        2,
        vec![
            3, 0, 0, 0, //Tick
            2, 0, 1, 0, 0, 0, 11, 0, // Event 2
        ],
    ));
    msgs.input.push((
        1,
        vec![
            7, 0, 0, 0, // Tick
            1, 0, 1, 0, 0, 0, 12, 0, // Event 3
            1, 0, 3, 0, 0, 0, 13, 0, // Event 4
        ],
    ));
    app.world.insert_resource(msgs);

    app.update();

    let events: Vec<_> = app
        .world
        .resource_mut::<Events<NetworkEvent<TargetedEvent>>>()
        .drain()
        .collect();
    assert_eq!(events.len(), 2);
    assert!(events.contains(&NetworkEvent {
        tick: Tick(2),
        sender: Identity::Client(1),
        event: TargetedEvent {
            target: e2,
            value: 10
        },
    }));
    assert!(events.contains(&NetworkEvent {
        tick: Tick(3),
        sender: Identity::Client(2),
        event: TargetedEvent {
            target: e1,
            value: 11
        },
    }));

    let events: Vec<_> = app
        .world
        .resource_mut::<Events<NetworkEvent<BroadcastEvent>>>()
        .drain()
        .collect();
    assert_eq!(events.len(), 1);
    assert!(events.contains(&NetworkEvent {
        tick: Tick(7),
        sender: Identity::Client(1),
        event: BroadcastEvent {
            about: e1,
            value: 12
        },
    }));
}
