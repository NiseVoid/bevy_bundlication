use bevy_bundlication::{prelude::*, LastUpdate};

use std::time::{Duration, Instant};

use bevy::{prelude::*, reflect::TypePath};
use criterion::{criterion_group, criterion_main, Criterion};
use serde::{Deserialize, Serialize};

#[derive(Component, Default)]
struct Player;
#[derive(Component, Clone, Serialize, Deserialize)]
struct Name(String);
#[derive(Component, Clone, Serialize, Deserialize)]
struct Hp(u16);
#[derive(Component, Clone, Serialize, Deserialize)]
struct Position(Vec3);
#[derive(Component, Clone, Serialize, Deserialize)]
struct Rotation(Quat);
#[derive(Component, Clone, Serialize, Deserialize)]
struct InputHistory(Vec<Input>);
#[derive(Component, Clone, Serialize, Deserialize)]
struct PartyFinderSettings {
    dps: bool,
    tank: bool,
    healer: bool,
}

#[derive(Clone, Serialize, Deserialize)]
enum Input {
    None,
    Up,
    Left,
    Down,
    Right,
    Jump,
    Attack,
}

#[derive(Bundle, NetworkedBundle, TypePath)]
struct PlayerBundle {
    #[networked(no_send)]
    player: Player,
    name: Name,
    hp: Hp,
    pos: Position,
    rot: Rotation,
    hist: InputHistory,
    finder_settings: PartyFinderSettings,
}

impl PlayerBundle {
    fn generate(i: u32) -> Self {
        Self {
            player: Player,
            name: Name(String::from("Tester ") + &i.to_string()),
            hp: Hp(1000),
            pos: Position(Vec3::new(20., 18., 12. + 2.8 * i as f32)),
            rot: Rotation(Quat::from_rotation_y(i as f32)),
            hist: InputHistory(vec![
                Input::Up,
                Input::None,
                Input::None,
                Input::Down,
                Input::Attack,
            ]),
            finder_settings: PartyFinderSettings {
                tank: false,
                healer: true,
                dps: true,
            },
        }
    }
}

const ENTITIES: u32 = 1000;

fn init_app(app: &mut App) {
    app.register_bundle::<ServerToAll, PlayerBundle, 0>();
    app.world.send_event(NewConnection(Identity::Client(1)));

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

    app.update();
}

fn spawn_entities(app: &mut App) {
    for i in 0..ENTITIES {
        app.world.spawn_client(i, PlayerBundle::generate(i));
    }
}

fn copy_messages(server_app: &mut App, client_app: &mut App) {
    let mut client_msgs = ClientMessages::default();
    for msg in server_app
        .world
        .resource_mut::<ServerMessages>()
        .output
        .drain(..)
    {
        client_msgs.input.push(msg.2)
    }
    client_app.insert_resource(client_msgs);
}

#[derive(Component, Default, serde::Serialize, serde::Deserialize, Clone)]
pub struct Number(u8);

#[derive(Bundle, NetworkedBundle, TypePath)]
pub struct ConstBundle<const I: u8> {
    number: Number,
}

fn register_many_bundles(app: &mut App) {
    app.register_bundle::<ServerToAll, ConstBundle<0>, 0>();
    app.register_bundle::<ServerToAll, ConstBundle<1>, 0>();
    app.register_bundle::<ServerToAll, ConstBundle<2>, 0>();
    app.register_bundle::<ServerToAll, ConstBundle<3>, 0>();
    app.register_bundle::<ServerToAll, ConstBundle<4>, 0>();
    app.register_bundle::<ServerToAll, ConstBundle<5>, 0>();
    app.register_bundle::<ServerToAll, ConstBundle<6>, 0>();
    app.register_bundle::<ServerToAll, ConstBundle<7>, 0>();
    app.register_bundle::<ServerToAll, ConstBundle<8>, 0>();
    app.register_bundle::<ServerToAll, ConstBundle<9>, 0>();
    app.register_bundle::<ServerToAll, ConstBundle<10>, 0>();
}

fn spawn_numbers(app: &mut App) {
    for i in 0..ENTITIES {
        app.world.spawn_client(i, Number((i % 255) as u8));
    }
}

fn replication(c: &mut Criterion) {
    c.bench_function("entities send", |b| {
        b.iter_custom(|iter| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..iter {
                let mut server_app = App::new();
                server_app.add_plugins(ServerNetworkingPlugin::new(0));
                server_app.init_resource::<ServerMessages>();

                init_app(&mut server_app);
                spawn_entities(&mut server_app);

                let instant = Instant::now();
                server_app.update();
                elapsed += instant.elapsed();

                let mut client_app = App::new();
                client_app.add_plugins(ClientNetworkingPlugin::new(0));
                client_app.init_resource::<ClientMessages>();
                init_app(&mut client_app);

                copy_messages(&mut server_app, &mut client_app);
                client_app.update();
                assert_eq!(client_app.world.entities().len(), ENTITIES);
            }

            elapsed
        })
    });

    c.bench_function("entities receive", |b| {
        b.iter_custom(|iter| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..iter {
                let mut server_app = App::new();
                server_app.add_plugins(ServerNetworkingPlugin::new(0));
                server_app.init_resource::<ServerMessages>();

                init_app(&mut server_app);
                spawn_entities(&mut server_app);

                server_app.update();

                let mut client_app = App::new();
                client_app.add_plugins(ClientNetworkingPlugin::new(0));
                client_app.init_resource::<ClientMessages>();
                init_app(&mut client_app);

                copy_messages(&mut server_app, &mut client_app);

                let instant = Instant::now();
                client_app.update();
                elapsed += instant.elapsed();

                assert_eq!(client_app.world.entities().len(), ENTITIES);
            }

            elapsed
        })
    });

    c.bench_function("many unused bundles", |b| {
        b.iter_custom(|iter| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..iter {
                let mut server_app = App::new();
                server_app.add_plugins(ServerNetworkingPlugin::new(0));
                server_app.init_resource::<ServerMessages>();

                register_many_bundles(&mut server_app);
                init_app(&mut server_app);
                spawn_entities(&mut server_app);

                let instant = Instant::now();
                server_app.update();
                elapsed += instant.elapsed();

                let mut client_app = App::new();
                client_app.add_plugins(ClientNetworkingPlugin::new(0));
                client_app.init_resource::<ClientMessages>();
                register_many_bundles(&mut client_app);
                init_app(&mut client_app);

                copy_messages(&mut server_app, &mut client_app);
                client_app.update();
                assert_eq!(client_app.world.entities().len(), ENTITIES);
            }

            elapsed
        })
    });

    c.bench_function("send overlapping bundles", |b| {
        b.iter_custom(|iter| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..iter {
                let mut server_app = App::new();
                server_app.add_plugins(ServerNetworkingPlugin::new(0));
                server_app.init_resource::<ServerMessages>();

                register_many_bundles(&mut server_app);
                init_app(&mut server_app);
                spawn_numbers(&mut server_app);

                let instant = Instant::now();
                server_app.update();
                elapsed += instant.elapsed();

                let mut client_app = App::new();
                client_app.add_plugins(ClientNetworkingPlugin::new(0));
                client_app.init_resource::<ClientMessages>();
                register_many_bundles(&mut client_app);
                init_app(&mut client_app);

                copy_messages(&mut server_app, &mut client_app);
                client_app.update();
                assert_eq!(client_app.world.entities().len(), ENTITIES);
            }

            elapsed
        })
    });

    c.bench_function("receive overlapping bundles", |b| {
        b.iter_custom(|iter| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..iter {
                let mut server_app = App::new();
                server_app.add_plugins(ServerNetworkingPlugin::new(0));
                server_app.init_resource::<ServerMessages>();

                register_many_bundles(&mut server_app);
                init_app(&mut server_app);
                spawn_numbers(&mut server_app);

                server_app.update();

                let mut client_app = App::new();
                client_app.add_plugins(ClientNetworkingPlugin::new(0));
                client_app.init_resource::<ClientMessages>();
                register_many_bundles(&mut client_app);
                init_app(&mut client_app);

                copy_messages(&mut server_app, &mut client_app);

                let instant = Instant::now();
                client_app.update();
                elapsed += instant.elapsed();

                assert_eq!(client_app.world.entities().len(), ENTITIES);
            }

            elapsed
        })
    });

    c.bench_function("update overlapping bundles", |b| {
        b.iter_custom(|iter| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..iter {
                let mut server_app = App::new();
                server_app.add_plugins(ServerNetworkingPlugin::new(0));
                server_app.init_resource::<ServerMessages>();

                register_many_bundles(&mut server_app);
                init_app(&mut server_app);
                spawn_numbers(&mut server_app);

                server_app.update();

                let mut client_app = App::new();
                client_app.add_plugins(ClientNetworkingPlugin::new(0));
                client_app.init_resource::<ClientMessages>();
                register_many_bundles(&mut client_app);
                init_app(&mut client_app);

                copy_messages(&mut server_app, &mut client_app);
                let mut update_messages = (*client_app.world.resource::<ClientMessages>()).clone();
                for msg in update_messages.input.iter_mut() {
                    msg[4] = 1;
                }

                client_app.update();
                assert_eq!(client_app.world.entities().len(), ENTITIES);

                client_app.insert_resource(update_messages);

                let instant = Instant::now();
                client_app.update();
                elapsed += instant.elapsed();

                let number_id = client_app.world.component_id::<Number>().unwrap();
                let update_id = client_app.world.component_id::<LastUpdate<()>>().unwrap();
                for arch in client_app.world.archetypes().iter() {
                    if arch.contains(number_id) {
                        assert!(arch.contains(update_id));
                        for e in arch.entities() {
                            let update = unsafe {
                                client_app
                                    .world
                                    .get_by_id(e.entity(), update_id)
                                    .unwrap()
                                    .deref::<LastUpdate<()>>()
                            };
                            assert_eq!(**update, Tick(1));
                        }
                        break;
                    }
                }
            }

            elapsed
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(50).measurement_time(Duration::from_secs(10));
    targets = replication
}
criterion_main!(benches);
