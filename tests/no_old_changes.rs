use bevy_bundlication::{prelude::*, LastUpdate};

use bevy::{prelude::*, reflect::TypePath};
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Serialize, Deserialize, Debug, PartialEq, Eq, Default)]
pub struct Number(u8);

#[derive(NetworkedBundle, Bundle, TypePath)]
struct NumberBundle {
    number: Number,
}

type Last = LastUpdate<NumberBundle>;

#[test]
fn test_no_old_changes() {
    let mut app = App::new();
    app.add_plugins(ClientNetworkingPlugin::new(0));
    app.register_bundle::<ServerToAll, NumberBundle, 0>();

    // This entity has the complete bundle
    let e1 = app.world.spawn_with_id(1, 1, ()).id();
    let e2 = app.world.spawn_with_id(1, 2, Last::new(Tick(3))).id();

    let mut msgs = ClientMessages::default();

    msgs.input.extend_from_slice(&[
        vec![1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1],
        vec![5, 0, 0, 0, 1, 1, 1, 0, 0, 0, 5],
        vec![2, 0, 0, 0, 1, 1, 2, 0, 0, 0, 2],
        vec![3, 0, 0, 0, 1, 1, 2, 0, 0, 0, 3],
        vec![7, 0, 0, 0, 1, 1, 3, 0, 0, 0, 7],
    ]);

    app.insert_resource(msgs);

    app.update();

    assert_eq!(app.world.entity(e1).get::<Number>(), Some(&Number(5)));
    assert_eq!(
        app.world.entity(e1).get::<Last>(),
        Some(&Last::new(Tick(5)))
    );
    assert_eq!(app.world.entity(e2).get::<Number>(), None);
    assert_eq!(
        app.world.entity(e2).get::<Last>(),
        Some(&Last::new(Tick(3)))
    );
    let e3 = app.world.resource::<IdentifierMap>().get_id(1, 3).unwrap();
    assert_eq!(app.world.entity(e3).get::<Number>(), Some(&Number(7)));
    assert_eq!(
        app.world.entity(e3).get::<Last>(),
        Some(&Last::new(Tick(7)))
    );
}
