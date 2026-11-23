//! `universe.toml`: the places of a game, in the folder of the place the
//! player opens (Rook's own file; Roblox keeps a universe on its website).
//!
//! ```toml
//! game_id = 4410        # game.GameId; 0 if left out
//!
//! [places]              # PlaceId = file, beside this one
//! 1001 = "Lobby.rbxl"
//! 1002 = "Arena.rbxl"
//! ```

use std::path::{Component, Path, PathBuf};

use super::{PlaceSource, Universe};

/// The manifest's name, in the opened place's folder.
pub const MANIFEST: &str = "universe.toml";

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("{path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("{path}: {what}")]
    Invalid { path: PathBuf, what: String },
}

impl Universe {
    /// The universe the place file at `place` belongs to, and its
    /// PlaceId: what the manifest beside it says, or the place alone with
    /// PlaceId 0 and no places to teleport to (as an unpublished place in
    /// Studio) without one.
    pub fn open(place: &Path) -> Result<(Self, i64), ManifestError> {
        let folder = place.parent().unwrap_or(Path::new(""));
        let path = folder.join(MANIFEST);
        let files = &rook_platform::get().files;
        if !files.exists(&path) {
            return Ok((Self::new(0), 0));
        }
        let text = files
            .read_to_string(&path)
            .map_err(|source| ManifestError::Read {
                path: path.clone(),
                source,
            })?;
        let universe = Self::parse(&text, folder).map_err(|e| match e {
            Parse::Toml(source) => ManifestError::Parse {
                path: path.clone(),
                source,
            },
            Parse::Invalid(what) => ManifestError::Invalid {
                path: path.clone(),
                what,
            },
        })?;
        let name = place.file_name();
        let id = universe
            .places
            .iter()
            .find_map(|(&id, source)| match source {
                PlaceSource::File(file) if file.file_name() == name => Some(id),
                _ => None,
            });
        if id.is_none() {
            tracing::warn!(
                "{} doesn't name {}: it plays as PlaceId 0",
                path.display(),
                place.display()
            );
        }
        Ok((universe, id.unwrap_or(0)))
    }

    /// A manifest's text, its files in `folder`.
    fn parse(text: &str, folder: &Path) -> Result<Self, Parse> {
        let table: toml::Table = text.parse().map_err(Parse::Toml)?;
        let game_id = match table.get("game_id") {
            None => 0,
            Some(toml::Value::Integer(id)) if *id >= 0 => *id,
            Some(_) => return Err(Parse::Invalid("game_id must be a whole number".into())),
        };
        let mut universe = Self::new(game_id);
        let places = match table.get("places") {
            Some(toml::Value::Table(places)) => places,
            None => return Ok(universe),
            Some(_) => return Err(Parse::Invalid("[places] must be a table".into())),
        };
        for (key, value) in places {
            let id = key
                .parse::<i64>()
                .ok()
                .filter(|&id| id > 0)
                .ok_or_else(|| Parse::Invalid(format!("{key} isn't a PlaceId")))?;
            let toml::Value::String(file) = value else {
                return Err(Parse::Invalid(format!("place {id} must name a file")));
            };
            // Only files in the manifest's folder or below: a manifest
            // shared with a place mustn't make a teleport read a file
            // elsewhere on the machine, or reach a network share.
            let file = Path::new(file);
            let mut parts = file.components();
            if !parts.all(|c| matches!(c, Component::Normal(_))) || file.file_name().is_none() {
                return Err(Parse::Invalid(format!(
                    "place {id} must name a file in this folder, not {}",
                    file.display()
                )));
            }
            universe.add_place(id, PlaceSource::File(folder.join(file)));
        }
        for key in table.keys() {
            if key != "game_id" && key != "places" {
                tracing::warn!("{MANIFEST}: `{key}` means nothing to Rook");
            }
        }
        Ok(universe)
    }
}

#[derive(Debug)]
enum Parse {
    Toml(toml::de::Error),
    Invalid(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_names_its_game_and_places() {
        let text = "game_id = 4410\n[places]\n1001 = \"Lobby.rbxl\"\n1002 = \"Arena.rbxl\"\n";
        let universe = Universe::parse(text, Path::new("games")).unwrap();
        assert_eq!(universe.game_id, 4410);
        assert!(universe.has_place(1001) && universe.has_place(1002));
        assert!(!universe.has_place(1003));
        let Some(PlaceSource::File(path)) = universe.places.get(&1002) else {
            panic!("a file");
        };
        assert_eq!(path, &Path::new("games").join("Arena.rbxl"));
    }

    /// A place's file is in the manifest's folder or below it, never a
    /// path leaving it, an absolute path or a network share.
    #[test]
    fn place_ids_are_positive_numbers_naming_files() {
        for bad in [
            "[places]\nlobby = \"a.rbxl\"",
            "[places]\n0 = \"a.rbxl\"",
            "[places]\n5 = 7",
            "game_id = \"x\"",
            "[places]\n5 = \"../a.rbxl\"",
            "[places]\n5 = \"games/../../a.rbxl\"",
            "[places]\n5 = \"/a.rbxl\"",
            "[places]\n5 = \"//host/share/a.rbxl\"",
            "[places]\n5 = \"\"",
        ] {
            assert!(
                matches!(Universe::parse(bad, Path::new("")), Err(Parse::Invalid(_))),
                "{bad}"
            );
        }
    }
}
