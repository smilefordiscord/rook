pub(crate) mod globals;
pub(crate) mod instance;
pub(crate) mod methods;
pub(crate) mod properties;
pub(crate) mod scripts;
pub(crate) mod signal;

pub(crate) use methods::physics::models::pivot_of;

use mlua::Lua;

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    crate::runtime::actors::install(lua)?;
    instance::install(lua)?;
    signal::install(lua)?;
    globals::install(lua)?;
    scripts::install(lua)?;
    methods::content::assets::install(lua)?;
    methods::web::requests::install(lua)?;
    methods::gui::text::install(lua)?;
    methods::web::http::install(lua)?;
    methods::web::memory_stores::install(lua)?;
    methods::web::messaging::install(lua)?;
    methods::remotes::install(lua)?;
    methods::teleport::install(lua)?;
    methods::gui::text_chat::install(lua)?;
    methods::web::marketplace::install(lua)?;
    crate::runtime::actors::note_builtins(lua)?;
    Ok(())
}
