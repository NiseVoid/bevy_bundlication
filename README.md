# bevy_bundlication

Network replication for bevy based on a bundle pattern.
Replication group rules for [bevy_replicon](https://github.com/projectharmonia/bevy_replicon) using a bundle-like API.

## Goals

- Simplify the definition of replication groups
- Simplify bandwidth optimization

## Getting started

bevy_bundlication works with a pattern similar to a Bundle from bevy. Anything matching the bundle gets networked.
Each field needs to implement `NetworkedComponent`, this can be done manually or trough a blanket impl on types that have [`Component`](https://docs.rs/bevy/latest/bevy/ecs/component/trait.Component.html), [`Serialze`](https://docs.rs/serde/latest/serde/trait.Serialize.html) and [`Deserialize`](https://docs.rs/serde/latest/serde/trait.Deserialize.html).
For types where the blanket impl causes conflicts, the `#[bundlication(as = Wrapper)]` attribute can be used where `Wrapper` is a type that impletements `NetworkedWrapper<YourType>`.

Bundles can be registered to bevy_replicon using `replicate_group::<Bundle>()`.

```rust
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_bundlication::prelude::*;
use serde::{Serialize, Deserialize};

#[derive(Component, Default)]
pub struct Player(u128);

#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Speed(f32);

#[derive(Serialize, Deserialize)]
pub struct JustTranslation(Vec3);

impl NetworkedWrapper<Transform> for JustTranslation {
    fn write_data(
        from: &Transform,
        w: impl std::io::Write,
        ctx: &SerializeCtx,
    ) -> BincodeResult<()> {
        serialize(w, &from.translation)?;
        Ok(())
    }

    fn read_new(
        r: impl std::io::Read,
        ctx: &mut DeserializeCtx, // This context can be used for entity mapping or check the message tick
    ) -> BincodeResult<Transform> {
        let translation: Vec3 = deserialize(r)?;
        Ok(Transform::from_translation(translation))
    }
}

#[derive(NetworkedBundle)]
pub struct PlayerPositionBundle {
    // This content of this field doesn't get sent, but it will get spawned (with the default value)
    #[bundlication(no_send)]
    pub player: Player,
    // This component is sent and spawned as is
    pub speed: Speed,
    // This components is queried and spawned as Transform, but sent according to
    // the logic of JustTranslation
    #[bundlication(as = JustTranslation)]
    pub translation: Transform,
    // If we also use this as a bundle and have fields we don't want replicon to consider, we can
    // add the skip attribute
    #[bundlication(skip)]
    pub skipped: GlobalTransform,
}

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        // We can now register this bundle to bevy_replicon
        app.replicate_group::<PlayerPositionBundle>();
    }
}
```

## License

All code in this repository is dual-licensed under either:

* MIT License ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

at your option.
