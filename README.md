# bevy_bundlication

> [!CAUTION]
> This crate has been deprecated by the phasing out of bundle structs in bevy, and bevy_replicon offering all features directly trough a combination of `replicate_as`, `replicate_with`, `replicate_bundle`, and `replicate_filtered`

Network replication for bevy based on a bundle pattern.
Replication group rules for [bevy_replicon](https://github.com/projectharmonia/bevy_replicon) using a bundle-like API.

## Goals

- Simplify the definition of replication groups
- Simplify bandwidth optimization

## Getting started

bevy_bundlication works with a pattern similar to a Bundle from bevy. Anything matching the bundle gets networked.
Each field needs to implement `NetworkedComponent`, this can be done manually or trough a blanket impl on types that have [`Component`](https://docs.rs/bevy/latest/bevy/ecs/component/trait.Component.html), [`Serialze`](https://docs.rs/serde/latest/serde/trait.Serialize.html) and [`Deserialize`](https://docs.rs/serde/latest/serde/trait.Deserialize.html).
For types where the blanket impl causes conflicts, the `#[bundlication(as = Wrapper)]` attribute can be used where `Wrapper` is a type that impletements `NetworkedWrapper<YourType>`.

Bundles can be registered to bevy_replicon using `replicate_bundle::<Bundle>()`.

```rust
use bevy::prelude::*;
use bevy_replicon::prelude::*;

#[derive(NetworkedBundle)]
pub struct PlayerPositionBundle {
    // The content of this field doesn't get sent, and it will be received as the default value,
    // it therefor requires neither Serialize/Deserialize nor NetworkedComponent
    #[bundlication(no_send, mode=Once)]
    pub player: Player,
    // This component is sent and spawned as is
    pub speed: Speed,
    // We replicate Transform, but it is serialized/deserialized using the logic of JustTranslation
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
        // To replicate the bundle, we register our bundle to bevy_replicon
        app.replicate_bundle::<PlayerPositionBundle>();
    }
}

use bevy_bundlication::prelude::*;
use serde::{Serialize, Deserialize};

// We need Default on Player because we use the no_send attribute
#[derive(Component, Default)]
pub struct Player(u128);

// Speed derives all required traits for the NetworkedBundle blanket impl
#[derive(Component, Serialize, Deserialize)]
pub struct Speed(f32);

// We define a type to network a type we don't own in a different way than its default behavior.
// This can also be used to network components without Serialize/Deserialize
// In this case we only network the translation part of Transform
#[derive(Serialize, Deserialize)]
pub struct JustTranslation(Vec3);

impl NetworkedWrapper<Transform> for JustTranslation {
    fn write_data(from: &Transform, w: impl std::io::Write, _: &SerializeCtx) -> Result<()> {
        serialize(w, &from.translation)?;
        Ok(())
    }

    fn read_new(r: impl std::io::Read, _: &mut DeserializeCtx) -> Result<Transform> {
        let translation: Vec3 = deserialize(r)?;
        Ok(Transform::from_translation(translation))
    }
}
```

## License

All code in this repository is dual-licensed under either:

* MIT License ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

at your option.
