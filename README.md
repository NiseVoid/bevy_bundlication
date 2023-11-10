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

## Future plans

- More optimizations
- Getting rid of Identifier in favor of a as-needed system to match predicted and real entities
- Add per-entity per-bundle client authority control
- Client-side packet buffering (to reduce jitter and support accurate interpolation)
- Per-entity visibility control

License

All code in this repository is dual-licensed under either:

- MIT License (LICENSE-MIT or http://opensource.org/licenses/MIT)
- Apache License, Version 2.0 (LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0)

at your option.
