use super::*;

/// `Instance.new(className, parent)`. Parent is set last, after the
/// instance exists (the parent argument is discouraged but supported).
pub(super) fn instance_new(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "new");
    let class_name = a.string(0)?;
    let parent = a.opt_instance(1)?;
    // Parenting is a write to `Parent` like any other, which
    // desynchronized code may not make (docs/behavior/scripting/parallel-luau.md).
    if parent.is_some() && crate::runtime::actors::in_parallel(lua) {
        return Err(ScriptError(
            "Property Instance.Parent is not safe to write in parallel".into(),
        ));
    }
    let db = rook_reflection::database();
    let class = db
        .class_by_name(&class_name)
        .filter(|c| !c.flags.not_creatable && !c.flags.service)
        .ok_or_else(|| {
            ScriptError(format!(
                "Unable to create an Instance of type \"{class_name}\""
            ))
        })?;
    let id = with_world(lua, |w| {
        let id = w.dom.create(class.id);
        if let Some(name) = new_name(class.name) {
            // A new instance has no parent, so renaming it can't fail.
            let _ = w.dom.set_name(id, name.to_owned());
        }
        id
    });
    if let Some(p) = parent {
        crate::api::methods::mutate(lua, |w| w.dom.set_parent(id, Some(p)))?;
    }
    Outcome::ret(lua, push_instance(lua, id)?)
}

pub(super) fn instance_table(lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("new", api_fn(lua, instance_new)?)?;
    Ok(t)
}
