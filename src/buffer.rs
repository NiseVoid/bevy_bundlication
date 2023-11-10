use crate::{Identity, Tick};
use bevy::prelude::Resource;

/// The rule for which client receives a message
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SendRule {
    /// Send the message to all connections
    All,
    /// Send the message to all connections, except the one for specified client id
    Except(u32),
    /// Send the message to only the one for specified client id
    Only(u32),
    /// Send the message to a list of clients
    List(Vec<u32>),
}

impl SendRule {
    /// Filter to who the message is sent, used when values haven't changed
    pub fn filter_to(self, new_clients: &[u32]) -> Option<SendRule> {
        if new_clients.is_empty() {
            return None;
        }
        match self {
            Self::All => Some(Self::List(new_clients.to_vec())),
            Self::Except(c) => {
                let list: Vec<u32> = new_clients
                    .iter()
                    .filter(|new_c| **new_c != c)
                    .cloned()
                    .collect();
                if list.is_empty() {
                    return None;
                }
                Some(Self::List(list))
            }
            Self::Only(c) => {
                if new_clients.contains(&c) {
                    Some(self)
                } else {
                    None
                }
            }
            Self::List(mut list) => {
                list.retain(|c| new_clients.contains(c));
                if list.is_empty() {
                    return None;
                }
                Some(Self::List(list))
            }
        }
    }

    /// Check if the [SendRule] includes the given [Identity]
    pub fn includes(&self, ident: Identity) -> bool {
        match self {
            Self::All => true,
            Self::Except(client_id) => ident != Identity::Client(*client_id),
            Self::Only(client_id) => ident == Identity::Client(*client_id),
            Self::List(list) => {
                if let Identity::Client(client_id) = ident {
                    list.contains(&client_id)
                } else {
                    false
                }
            }
        }
    }
}

/// The key used to keep buffers separate
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferKey {
    /// The channel the packets are sent over
    pub channel: u8,
    /// The rule for who receives these packets
    pub rule: SendRule,
}

impl BufferKey {
    /// Construct a BufferKey
    pub fn new(channel: u8, rule: SendRule) -> Self {
        Self { channel, rule }
    }
}

const CAP: usize = 1198;

/// A resource that keeps a list of buffers to group packets on the same channel for the same
/// recipients. This reduces bandwidth use by only including the Tick once per packet and reducing
/// the number of separate packets sent (which might have extra per-message overhead)
#[derive(Resource, Default)]
pub struct Buffers {
    map: bevy::utils::HashMap<BufferKey, usize>,
    pub(crate) buffers: Vec<(BufferKey, Vec<u8>)>,
}

impl<'a> Buffers {
    pub(crate) fn clear(&mut self) {
        self.map.clear();
    }

    /// Return a mutable slice which has at least the specified number of bytes free.
    /// The bytes written should not exceed the specified length, otherwise packets could
    /// needlessly become too big to be sent in a single fragment
    pub fn reserve_mut(&'a mut self, key: BufferKey, len: usize, tick: Tick) -> &'a mut Vec<u8> {
        if let Some(n) = self.map.get_mut(&key) {
            if self.buffers[*n].1.len() + len <= CAP {
                return &mut self.buffers[*n].1;
            }
            self.buffers.push((key, Vec::with_capacity(CAP)));
            *n = self.buffers.len() - 1;
            self.buffers[*n].1.extend_from_slice(&(*tick).to_le_bytes());
            return &mut self.buffers[*n].1;
        }

        self.buffers.push((key.clone(), Vec::with_capacity(CAP)));
        let n = self.buffers.len() - 1;
        self.buffers[n].1.extend_from_slice(&(*tick).to_le_bytes());
        self.map.insert(key, n);
        &mut self.buffers[n].1
    }
}
