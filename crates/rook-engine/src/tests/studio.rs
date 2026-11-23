//! Editing places as Studio does (docs/behavior/studio/): Edit mode,
//! plugins and what they make, the selection and the change history.

mod history;
mod plugins;

use crate::tests::*;

/// A place file with a Part named `Brick` in Workspace and a Script in
/// ServerScriptService that prints when it runs.
fn place_file() -> Vec<u8> {
    let s = Server::new(&Options::default()).unwrap();
    let ws = service(&s, "Workspace");
    add_under(&s, ws, "Part", "Brick", "");
    add_script(&s, "ServerScriptService", "Game", "print('the place ran')");
    s.save_place(rook_formats::Format::Binary).unwrap()
}

/// The place opened for editing, with `source` loaded as a plugin and a
/// frame run.
fn editing(source: &str) -> DataModel {
    let mut dm = DataModel::edit_place(&place_file(), &Options::default()).unwrap();
    dm.load_plugin_source("TestPlugin", source).unwrap();
    dm.step();
    dm
}
