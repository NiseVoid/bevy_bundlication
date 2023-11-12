# bevy_bundlication

Network replication for bevy based on a bundle pattern.

## Goals

- High performance replication
- Minimizing bandwidth overhead
- Features to synchronize predicted and remote entities
    - Rollback is out of scope
- Support well-scoped client authority

## Non-goals

- Being the easiest crate for less performance and bandwidth critical uses

## Getting started

bevy_bundlication works with a pattern similar to a Bundle from bevy. Anything matching the bundle, with the required component to get picked up by networking (Identifier) is sent according to the rules it was registered with. Each field needs to implement [`bevy::ecs::component::Component`](https://docs.rs/bevy/latest/bevy/ecs/component/trait.Component.html), [`serde::Serialze`](https://docs.rs/serde/latest/serde/trait.Serialize.html), [`serde::Deserialize`](https://docs.rs/serde/latest/serde/trait.Deserialize.html) and `Clone` (for now). The bundle also needs to derive `Bundle` (for now) and `TypePath`.

Bundles need to be registered to the app, and can have extra rules on fields.

```rust
#[derive(Component, Default)]
pub struct Player;

#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Speed(f32);

#[derive(Serialize, Deserialize)]
pub struct JustTranslation(Vec3);

impl NetworkedWrapper<Transform> for JustTranslation {
    fn from_component(_: Tick, _: &IdentifierMap, t: &Transform) -> IdentifierResult<Self> {
        Ok(Self(t.translation))
    }

    fn to_component(self, _: Tick, _: &IdentifierMap) -> IdentifierResult<Transform> {
        Ok(Transform::from_translation(self))
    }
}

#[derive(NetworkedBundle, Bundle, TypePath)]
pub struct PlayerPositionBundle {
    // This field doesn't get sent, but it will get spawned (with the default value)
    #[networked(no_send)]
    pub player: Player,
    // This components is queried and spawned as Transform, but sent according to
    // the logic of JustTranslation
    #[networked(as = JustTranslation)]
    pub translation: Transform,
    // This component is sent and spawned as is
    pub speed: Speed,
}

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        // We register the bundle to be sent from the server to all clients.
        // These registers need to be identical on both the client and server.
        // but don't worry, the client won't try to send server data,
        // nor would the server accept it
        app.register_bundle::<ServerToAll, PlayerPositionBundle, 0>();

        // Other rules for how a bundle is sent are:
        // - ServerToOwner and ServerToObserver, which both check the Identifier (or Owner, if it exists)
        // - ClientToServer, for client authority which requires an Authority::Free or
        //      matching Authority::Client(x) on the client
    }
}
```

## Future plans

- More optimizations
- Getting rid of Identifier in favor of a as-needed system to match predicted and real entities
- Add per-entity per-bundle client authority control
- Client-side packet buffering (to reduce jitter and support accurate interpolation)
- Per-entity visibility control

## License

All code in this repository is dual-licensed under either:

* MIT License ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

at your option.
