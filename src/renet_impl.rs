use crate::*;

use bevy::prelude::resource_exists;
use bevy_renet::{
    renet::{ClientId, RenetClient, RenetServer, ServerEvent},
    {RenetReceive, RenetSend},
};

/// A plugin that adds renet support to a server
pub struct BundlicationRenetServerPlugin;

impl Plugin for BundlicationRenetServerPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            PreUpdate,
            (
                RenetReceive.in_set(InternalSet::ReadPackets),
                BundlicationSet::Receive.run_if(resource_exists::<RenetServer>),
            ),
        )
        .configure_sets(
            PostUpdate,
            (
                RenetSend.in_set(InternalSet::SendPackets),
                BundlicationSet::Send.run_if(resource_exists::<RenetServer>),
            ),
        )
        .add_systems(
            PreUpdate,
            (
                receive_messages::<ServerToClient, RenetServer>
                    .in_set(InternalSet::ReceiveMessages),
                read_events.after(InternalSet::ReceiveMessages),
            ),
        )
        .add_systems(
            PostUpdate,
            send_buffers::<ServerToClient, RenetServer>.in_set(InternalSet::SendBuffers),
        );
    }
}

fn read_events(
    mut renet_events: EventReader<ServerEvent>,
    mut connected: EventWriter<Connected>,
    mut disconnected: EventWriter<Disconnected>,
) {
    for event in renet_events.read() {
        match event {
            ServerEvent::ClientConnected { client_id, .. } => {
                connected.send(Connected(Identity::Client(client_id.raw() as u32)));
            }
            ServerEvent::ClientDisconnected { client_id, .. } => {
                disconnected.send(Disconnected(Identity::Client(client_id.raw() as u32)));
            }
        }
    }
}

/// A plugin that adds renet support to a client
pub struct BundlicationRenetClientPlugin;

impl Plugin for BundlicationRenetClientPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            PreUpdate,
            (
                RenetReceive.in_set(InternalSet::ReadPackets),
                BundlicationSet::Receive.run_if(resource_exists::<RenetClient>),
            ),
        )
        .configure_sets(
            PostUpdate,
            (
                RenetSend.in_set(InternalSet::SendPackets),
                BundlicationSet::Send.run_if(resource_exists::<RenetClient>),
            ),
        )
        .add_systems(
            PreUpdate,
            receive_messages::<ClientToServer, RenetClient>.in_set(InternalSet::ReceiveMessages),
        )
        .add_systems(
            PostUpdate,
            send_buffers::<ClientToServer, RenetClient>.in_set(InternalSet::SendBuffers),
        );
    }
}

impl<Dir: Direction> NetImpl<Dir> for RenetClient {
    fn receive_messages(
        &mut self,
        world: &mut World,
        handlers: &Handlers<Dir::Reverse>,
        channels: &[u8],
    ) {
        for channel in channels {
            while let Some(msg) = self.receive_message(*channel) {
                handlers.process(world, Identity::Server, &msg);
            }
        }
    }

    fn send_messages(&mut self, msgs: impl Iterator<Item = (BufferKey, Vec<u8>)>) {
        for (BufferKey { channel, .. }, buf) in msgs {
            self.send_message(channel, buf);
        }
    }
}

impl<Dir: Direction> NetImpl<Dir> for RenetServer {
    fn receive_messages(
        &mut self,
        world: &mut World,
        handlers: &Handlers<Dir::Reverse>,
        channels: &[u8],
    ) {
        for client_id in self.clients_id() {
            for channel in channels {
                while let Some(msg) = self.receive_message(client_id, *channel) {
                    handlers.process(world, Identity::Client(client_id.raw() as u32), &msg);
                }
            }
        }
    }

    fn send_messages(&mut self, msgs: impl Iterator<Item = (BufferKey, Vec<u8>)>) {
        for (BufferKey { channel, recipient }, buf) in msgs {
            let Identity::Client(client_id) = recipient else {
                continue;
            };
            self.send_message(ClientId::from_raw(client_id as u64), channel, buf);
        }
    }
}
