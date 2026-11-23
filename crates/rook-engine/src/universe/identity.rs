//! Which server a DataModel is: the `DataModel` properties scripts read
//! (`PlaceId`, `GameId`, `JobId`, `PrivateServerId`,
//! `PrivateServerOwnerId`). The DataModel's own properties are the one
//! record of it; a session reads them back rather than keeping a copy.

use rook_types::Value;

use crate::DataModel;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerIdentity {
    pub place_id: i64,
    pub game_id: i64,
    pub job_id: String,
    /// A reserved server's id; empty for a public server.
    pub private_server_id: String,
}

impl ServerIdentity {
    /// A server of `place_id` in `universe`, with a new `JobId`.
    pub fn new_server(
        universe: &super::Universe,
        place_id: i64,
        private_server_id: String,
    ) -> Self {
        Self {
            place_id,
            game_id: universe.game_id,
            job_id: universe.new_id(),
            private_server_id,
        }
    }

    /// Make `dm` this server, as it loads: no signal fires. A reserved
    /// server's `PrivateServerOwnerId` is 0 (creator-docs
    /// ReserveServerAsync), and Rook has no other private servers.
    pub fn apply(&self, dm: &DataModel) {
        let mut w = dm.runtime().world_mut();
        let dom = &mut w.dom;
        let root = dom.root();
        for (name, value) in [
            ("PlaceId", Value::Int64(self.place_id)),
            ("GameId", Value::Int64(self.game_id)),
            ("JobId", Value::String(self.job_id.clone())),
            (
                "PrivateServerId",
                Value::String(self.private_server_id.clone()),
            ),
            ("PrivateServerOwnerId", Value::Int64(0)),
        ] {
            match dom.property_id(root, name) {
                Some(id) => dom.load_property(root, id, value),
                None => tracing::warn!("DataModel has no {name}"),
            }
        }
    }

    /// The server `dm` is.
    pub fn read(dm: &DataModel) -> Self {
        let w = dm.runtime().world();
        let dom = &w.dom;
        let root = dom.root();
        let text = |name| dom.string_property(root, name).unwrap_or_default();
        let number = |name| dom.number_property(root, name).unwrap_or(0.0) as i64;
        Self {
            place_id: number("PlaceId"),
            game_id: number("GameId"),
            job_id: text("JobId"),
            private_server_id: text("PrivateServerId"),
        }
    }
}
