//! A universe: the places of one game, which teleports move players
//! between (docs/behavior/scripting/teleports.md). It's the one place that
//! answers "which file is place N" and "which game is this": a folder's
//! `universe.toml` names them for the player ([`manifest`]), tests give
//! places in memory. Which server a teleport goes to is [`routing`]'s;
//! what a running server is, its [`ServerIdentity`].

pub mod identity;
pub mod manifest;
pub mod routing;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub use identity::ServerIdentity;
pub use manifest::ManifestError;

/// Where a place's file comes from.
#[derive(Debug, Clone)]
pub enum PlaceSource {
    File(PathBuf),
    Bytes(Arc<[u8]>),
}

impl PlaceSource {
    /// The file's bytes: a file is read now, so whoever loads the place
    /// reads it, off the thread that asked for it.
    pub fn read(&self) -> std::io::Result<Arc<[u8]>> {
        match self {
            Self::Bytes(b) => Ok(b.clone()),
            Self::File(path) => Ok(rook_platform::get().files.read(path)?.into()),
        }
    }
}

/// The places of one game.
#[derive(Debug, Default)]
pub struct Universe {
    /// `game.GameId` in each of its places (0: none, as an unpublished
    /// place's).
    pub game_id: i64,
    places: BTreeMap<i64, PlaceSource>,
    /// The ids made so far ([`Universe::new_id`]).
    ids: AtomicU64,
}

impl Universe {
    pub fn new(game_id: i64) -> Self {
        Self {
            game_id,
            ..Self::default()
        }
    }

    pub fn add_place(&mut self, place_id: i64, source: PlaceSource) {
        self.places.insert(place_id, source);
    }

    pub fn has_place(&self, place_id: i64) -> bool {
        self.places.contains_key(&place_id)
    }

    /// Where a place's file comes from.
    pub fn place_source(&self, place_id: i64) -> std::io::Result<PlaceSource> {
        self.places.get(&place_id).cloned().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("place {place_id} isn't in the universe"),
            )
        })
    }

    /// The bytes of a place's file, read now.
    pub fn place_bytes(&self, place_id: i64) -> std::io::Result<Arc<[u8]>> {
        self.place_source(place_id)?.read()
    }

    /// A new id in a GUID's form, for a server's `JobId` or a reserved
    /// server's `PrivateServerId`: counted rather than random, so runs
    /// repeat exactly (Roblox's are random).
    pub fn new_id(&self) -> String {
        let n = self.ids.fetch_add(1, Ordering::Relaxed) + 1;
        format!("{:08x}-0000-4000-8000-{:012x}", self.game_id as u32, n)
    }
}
