use crate::*;

/// A plugin that adds test functionality to the server
pub struct TestServerPlugin;

impl Plugin for TestServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            receive_messages::<ServerToClient, ServerMessages>.in_set(InternalSet::ReceiveMessages),
        )
        .add_systems(
            PostUpdate,
            send_buffers::<ServerToClient, ServerMessages>.in_set(InternalSet::SendBuffers),
        );
    }
}

/// A plugin that adds test functionality to the client
pub struct TestClientPlugin;

impl Plugin for TestClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            receive_messages::<ClientToServer, ClientMessages>.in_set(InternalSet::ReceiveMessages),
        )
        .add_systems(
            PostUpdate,
            send_buffers::<ClientToServer, ClientMessages>.in_set(InternalSet::SendBuffers),
        );
    }
}

/// The input and output for a client
#[derive(Resource, Clone, Default)]
pub struct ClientMessages {
    /// The input, a list of packet bytes
    pub input: Vec<Vec<u8>>,
    /// The output, a list of channel and bytes
    pub output: Vec<(u8, Vec<u8>)>,
}

impl<Dir: Direction> NetImpl<Dir> for ClientMessages {
    fn receive_messages(&mut self, world: &mut World, handlers: &Handlers<Dir::Reverse>, _: &[u8]) {
        for b in self.input.drain(..) {
            handlers.process(world, Identity::Server, &b);
        }
    }

    fn send_messages(&mut self, msgs: impl Iterator<Item = (BufferKey, Vec<u8>)>) {
        for (BufferKey { channel, .. }, buf) in msgs {
            self.output.push((channel, buf));
        }
    }
}

/// The input and output for a server
#[derive(Resource, Default)]
pub struct ServerMessages {
    /// The input, a list of client id and bytes
    pub input: Vec<(u32, Vec<u8>)>,
    /// The output, a list of channels, [`Identity`]s and bytes
    pub output: Vec<(u8, Identity, Vec<u8>)>,
}

impl<Dir: Direction> NetImpl<Dir> for ServerMessages {
    fn receive_messages(&mut self, world: &mut World, handlers: &Handlers<Dir::Reverse>, _: &[u8]) {
        for (client_id, b) in self.input.drain(..) {
            handlers.process(world, Identity::Client(client_id), &b);
        }
    }

    fn send_messages(&mut self, msgs: impl Iterator<Item = (BufferKey, Vec<u8>)>) {
        for (
            BufferKey {
                channel,
                destination,
            },
            buf,
        ) in msgs
        {
            self.output.push((channel, destination, buf));
        }
    }
}
