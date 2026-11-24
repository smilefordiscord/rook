//! Plugins in Edit mode (docs/behavior/studio/plugins.md).

use super::*;

#[test]
fn edit_mode_runs_plugins_and_not_the_place() {
    let mut dm = editing(
        r#"
        local rs = game:GetService("RunService")
        print(typeof(plugin), plugin.Name, script.Parent == plugin, plugin.Parent)
        print(rs:IsStudio(), rs:IsEdit(), rs:IsRunning(), rs:IsRunMode(), rs:IsServer(), rs:IsClient())
        "#,
    );
    dm.step();
    assert_eq!(
        lines(&dm),
        [
            "Instance TestPlugin true nil",
            "true true false false true true"
        ]
    );
}

/// Measured in Studio 0.748's Edit mode: `Heartbeat`, `RenderStepped` and
/// `PreRender` fire, in that order; the simulation's events don't.
#[test]
fn edit_mode_fires_heartbeat_and_render_events() {
    let mut dm = editing(
        r#"
        local rs = game:GetService("RunService")
        local order = {}
        for _, name in { "Stepped", "PreSimulation", "PostSimulation", "Heartbeat", "RenderStepped", "PreRender" } do
            rs[name]:Connect(function() table.insert(order, name) end)
        end
        task.wait()
        table.clear(order)
        task.wait()
        task.wait()
        local from = table.find(order, "Heartbeat")
        print(#order, table.concat(order, " ", from, from + 2))
        "#,
    );
    for _ in 0..4 {
        dm.step();
    }
    assert_eq!(lines(&dm), ["6 Heartbeat RenderStepped PreRender"]);
}

/// Plugin members are the plugin's: a game script can't reach them.
#[test]
fn plugin_security_is_the_plugins_alone() {
    let dm = editing(r#"print(#game:GetService("Selection"):Get())"#);
    dm.run_extra_script(
        r#"print(pcall(function() return game:GetService("Selection"):Get() end))"#,
    )
    .unwrap();
    let out = lines(&dm);
    assert_eq!(out[0], "0");
    assert!(
        out[1].starts_with("false") && out[1].contains("lacking capability Plugin"),
        "{out:?}"
    );
}

/// When `SelectionChanged` fires, as measured in Studio 0.748: `Set`
/// unless both the selection and the list are empty, `Add` only when it
/// selects something new, `Remove` always. Neither keeps an instance
/// outside the DataModel.
#[test]
fn selection_changed_by_method() {
    let mut dm = editing(
        r#"
        local sel = game:GetService("Selection")
        local n = 0
        sel.SelectionChanged:Connect(function() n += 1 end)
        local brick, terrain = workspace.Brick, workspace.Terrain
        local loose = Instance.new("Part")
        local function count(label, f)
            local before = n
            f()
            task.wait()
            print(label, n - before, #sel:Get())
        end
        count("set twice", function() sel:Set({ brick, brick }) end)
        count("set same", function() sel:Set({ brick }) end)
        count("add selected", function() sel:Add({ brick }) end)
        count("add new", function() sel:Add({ terrain }) end)
        count("remove unselected", function() sel:Remove({ loose }) end)
        count("set empty", function() sel:Set({}) end)
        count("set empty again", function() sel:Set({}) end)
        count("set outside", function() sel:Set({ loose }) end)
        count("add outside", function() sel:Add({ loose }) end)
        print(pcall(sel.Set, sel, nil))
        print(pcall(sel.Set, sel, { brick, 1 }))
        "#,
    );
    for _ in 0..12 {
        dm.step();
    }
    assert_eq!(
        lines(&dm),
        [
            "set twice 1 1",
            "set same 1 1",
            "add selected 0 1",
            "add new 1 2",
            "remove unselected 1 2",
            "set empty 1 0",
            "set empty again 0 0",
            "set outside 1 0",
            "add outside 0 0",
            "false Expected table argument, got nil",
            "false Unable to cast value to Object",
        ]
    );
}

/// What leaves the DataModel leaves the selection at once, and
/// `SelectionChanged` fires for each instance that left (Measured in
/// Studio 0.748); coming back doesn't select it again.
#[test]
fn removed_instances_leave_the_selection() {
    let mut dm = editing(
        r#"
        local sel = game:GetService("Selection")
        local n = 0
        sel.SelectionChanged:Connect(function() n += 1 end)
        local folder = Instance.new("Folder")
        local a = Instance.new("Part")
        a.Parent = folder
        local b = Instance.new("Part")
        b.Parent = folder
        folder.Parent = workspace
        sel:Set({ workspace.Brick, a, b })
        task.wait()
        n = 0
        folder.Parent = nil
        print(#sel:Get())
        task.wait()
        print(n)
        folder.Parent = workspace
        task.wait()
        print(#sel:Get(), n)
        "#,
    );
    for _ in 0..4 {
        dm.step();
    }
    assert_eq!(lines(&dm), ["1", "2", "1 2"]);
}

#[test]
fn toolbar_buttons_click_and_show() {
    let dm = editing(
        r#"
        local toolbar = plugin:CreateToolbar("Tools")
        local button = toolbar:CreateButton("Paint", "Paints parts", "rbxassetid://1", "Paint!")
        button.Click:Connect(function()
            print("clicked", button.Parent == toolbar, toolbar.Parent == plugin)
            button:SetActive(true)
        end)
        "#,
    );
    let (button, info) = {
        let studio = dm.studio();
        let (&id, info) = studio.buttons.iter().next().unwrap();
        (id, info.clone())
    };
    assert_eq!(
        (info.text.as_str(), info.tooltip.as_str(), info.active),
        ("Paint!", "Paints parts", false)
    );
    dm.press_button(button);
    assert_eq!(lines(&dm), ["clicked true true"]);
    assert!(dm.studio().buttons[&button].active);
}

/// Activating one plugin deactivates another; its mouse hears the
/// viewport only while it's activated.
#[test]
fn activation_and_the_plugins_mouse() {
    let mut dm = editing(
        r#"
        local mouse = plugin:GetMouse()
        print(mouse.ClassName, mouse == plugin:GetMouse())
        plugin.Deactivation:Connect(function() print("deactivated") end)
        mouse.Button1Down:Connect(function() print("down", mouse.Target and mouse.Target.Name) end)
        plugin:Activate(true)
        print(plugin:IsActivated(), plugin:IsActivatedWithExclusiveMouse())
        "#,
    );
    dm.set_viewport(800, 600);
    let at = rook_types::CFrame::look_at(
        glam::Vec3::new(0.0, 20.0, 20.0),
        glam::Vec3::ZERO,
        glam::Vec3::Y,
    );
    dm.set_edit_camera(at, at, 70.0);
    dm.input(InputEvent::MouseMove { x: 400.0, y: 300.0 });
    dm.input(InputEvent::MouseButton {
        button: 1,
        down: true,
    });
    dm.run_extra_script("game:GetService('ChangeHistoryService')")
        .unwrap();
    dm.step();
    let out = lines(&dm);
    assert_eq!(out[..2], ["PluginMouse true", "true true"]);
    assert!(out[2].starts_with("down"), "{out:?}");
}

/// Edit mode has no top bar (measured in Studio 0.748's Edit DataModel):
/// `GetGuiInset` is (0, 0), a CoreUISafeInsets ScreenGui covers the
/// viewport from its top left, and screen and viewport points agree. The
/// plugin's Mouse gives the same coordinates, which once left out a
/// client's 58-pixel top bar.
#[test]
fn edit_mode_has_no_top_bar() {
    let mut dm = editing(
        r#"
        local gui = Instance.new("ScreenGui")
        gui.Parent = game:GetService("CoreGui")
        local mouse = plugin:GetMouse()
        mouse.Button1Down:Connect(function()
            local camera = workspace.CurrentCamera
            local p = camera.CFrame.Position + camera.CFrame.LookVector * 50
            print(game:GetService("GuiService"):GetGuiInset())
            print(gui.AbsolutePosition, gui.AbsoluteSize)
            print(mouse.X, mouse.Y, mouse.ViewSizeX, mouse.ViewSizeY)
            print(camera:WorldToScreenPoint(p) == camera:WorldToViewportPoint(p))
        end)
        plugin:Activate(true)
        "#,
    );
    dm.set_viewport(800, 600);
    dm.step();
    for e in (ScreenPoint { x: 120.0, y: 90.0 }).click() {
        dm.input(e);
    }
    dm.step();
    assert_eq!(
        lines(&dm),
        ["0, 0 0, 0", "0, 0 800, 600", "120 90 800 600", "true"]
    );
}

/// A host drawing its view in part of the window (an editor, between its
/// panels) makes that the camera's viewport, as Studio's 3D view is:
/// `ViewportSize` is its size and viewport points and the mouse's ray
/// count from its top left, while GUI, the Mouse's `X` and `Y` and screen
/// points stay in the window's coordinates.
#[test]
fn a_view_rect_is_the_cameras_viewport() {
    let mut dm = editing(
        r#"
        local gui = Instance.new("ScreenGui")
        gui.Parent = game:GetService("CoreGui")
        local mouse = plugin:GetMouse()
        mouse.Button1Down:Connect(function()
            local camera = workspace.CurrentCamera
            local look = camera.CFrame.LookVector
            local p = camera.CFrame.Position + look * 50
            local function near(a, b) return (a - b).Magnitude < 1e-3 end
            print(camera.ViewportSize, gui.AbsoluteSize)
            print(mouse.X, mouse.Y, mouse.ViewSizeX, mouse.ViewSizeY)
            print(near(mouse.UnitRay.Direction, look), mouse.Target and mouse.Target.Name)
            local v, s = camera:WorldToViewportPoint(p), camera:WorldToScreenPoint(p)
            print(math.round(v.X), math.round(v.Y), math.round(s.X), math.round(s.Y))
            print(near(camera:ScreenPointToRay(mouse.X, mouse.Y).Direction, look),
                near(camera:ViewportPointToRay(200, 150).Direction, look))
            print(game:GetService("UserInputService"):GetMouseLocation())
        end)
        plugin:Activate(true)
        "#,
    );
    dm.set_viewport(800, 600);
    dm.set_view_rect(Some([300.0, 50.0, 400.0, 300.0]));
    let at = rook_types::CFrame::look_at(
        glam::Vec3::new(0.0, 20.0, 20.0),
        glam::Vec3::new(0.0, 0.5, 0.0),
        glam::Vec3::Y,
    );
    dm.set_edit_camera(at, at, 70.0);
    dm.step();
    // The view's center.
    for e in (ScreenPoint { x: 500.0, y: 200.0 }).click() {
        dm.input(e);
    }
    dm.step();
    assert_eq!(
        lines(&dm),
        [
            "400, 300 800, 600",
            "500 200 400 300",
            "true Brick",
            "200 150 500 200",
            "true true",
            "200, 150",
        ]
    );
    // Back to the whole window, as a game draws.
    dm.set_view_rect(None);
    dm.run_extra_script("print(workspace.CurrentCamera.ViewportSize)")
        .unwrap();
    assert_eq!(lines(&dm), ["800, 600"]);
}

/// A plugin moving the current camera moves the editor's view, and it's
/// no edit to undo (measured in Studio 0.748).
#[test]
fn a_plugin_moves_the_edit_camera() {
    let dm = editing(
        r#"
        local chs = game:GetService("ChangeHistoryService")
        workspace.CurrentCamera.CFrame = CFrame.new(1, 2, 3)
        workspace.CurrentCamera.Focus = CFrame.new(1, 2, -7)
        workspace.CurrentCamera.FieldOfView = 50
        chs:SetWaypoint("Moved the view")
        print(chs:GetCanUndo())
        "#,
    );
    let (cframe, focus, fov) = dm.edit_camera().unwrap();
    assert_eq!(cframe.position, glam::Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(focus.position, glam::Vec3::new(1.0, 2.0, -7.0));
    assert_eq!(fov, 50.0);
    assert_eq!(lines(&dm), ["false"]);
}

/// A dock widget lays out in the area the editor gives it and takes the
/// mouse there.
#[test]
fn dock_widgets_lay_out_where_placed() {
    let mut dm = editing(
        r#"
        local info = DockWidgetPluginGuiInfo.new(Enum.InitialDockState.Left, true, false, 300, 200, 150, 100)
        local widget = plugin:CreateDockWidgetPluginGui("Panel", info)
        widget.Title = "Panel"
        local button = Instance.new("TextButton")
        button.Size = UDim2.fromScale(1, 0.5)
        button.Parent = widget
        button.Activated:Connect(function() print("pressed") end)
        print(widget.Parent == game:GetService("PluginGuiService"))
        "#,
    );
    let widget = *dm.studio().widgets.keys().next().unwrap();
    assert_eq!(
        dm.studio().widgets[&widget].dock,
        crate::studio::DockState::Left
    );
    dm.set_viewport(800, 600);
    dm.set_widget_rect(widget, Some([10.0, 100.0, 200.0, 300.0]));
    dm.step();
    dm.run_extra_script(
        "local b = game:GetService('PluginGuiService').Panel.TextButton print(b.AbsoluteSize, b.AbsolutePosition)",
    )
    .unwrap();
    // A click in the widget's top half presses the button.
    for e in (ScreenPoint { x: 50.0, y: 150.0 }).click() {
        dm.input(e);
    }
    dm.step();
    assert_eq!(lines(&dm), ["true", "200, 150 10, 100", "pressed"]);
}

/// `DockWidgetPluginGuiInfo` as Studio 0.748 prints, compares and checks
/// it: sizes are truncated to integers, out of range is the smallest; no
/// member can be read.
#[test]
fn dock_widget_info_prints_and_checks_arguments() {
    let dm = editing(
        r#"
        local Info, Dock = DockWidgetPluginGuiInfo, Enum.InitialDockState
        print(Info.new())
        print(Info.new(Dock.Float, true, false, 12.7, -2.7, 2^31, 0/0))
        print(Info.new(Dock.Left) == Info.new(2.5), Info.new() == Info.new(Dock.Left))
        print(pcall(Info.new, "Left"))
        print(pcall(Info.new, Dock.Left, nil))
        print(pcall(Info.new, Dock.Left, true, 1))
        print(pcall(Info.new, Dock.Left, true, true, "12"))
        print(pcall(Info.new, Dock.Left, true, true, 1, 2, 3, "x"))
        print(pcall(function() return Info.new().MinWidth end))
        print(pcall(function() Info.new().MinWidth = 1 end))
        "#,
    );
    assert_eq!(
        lines(&dm),
        [
            "InitialDockState:Right InitialEnabled:0 InitialEnabledShouldOverrideRestore:0 FloatingXSize:0 FloatingYSize:0 MinWidth:0 MinHeight:0",
            "InitialDockState:Float InitialEnabled:1 InitialEnabledShouldOverrideRestore:0 FloatingXSize:12 FloatingYSize:-2 MinWidth:-2147483648 MinHeight:-2147483648",
            "true false",
            "false DockWidgetPluginGuiInfo.new first argument expects InitialDockState input",
            "false DockWidgetPluginGuiInfo.new second arg should be a boolean for InitialEnabled.",
            "false DockWidgetPluginGuiInfo.new third arg should be a boolean for InitialEnabledShouldOverrideRestore.",
            "false DockWidgetPluginGuiInfo.new fourth argument expects an int for FloatingXSize.",
            "false DockWidgetPluginGuiInfo.new seventh argument expects an int for MinHeight.",
            "false 'MinWidth' is not a valid member of DockWidgetPluginGuiInfo",
            "false MinWidth is not a valid member of DockWidgetPluginGuiInfo",
        ]
    );
}

#[test]
fn settings_keep_json_values_per_plugin() {
    let dm = editing(
        r#"
        plugin:SetSetting("Grid", { size = 2, on = true })
        print(plugin:GetSetting("Grid").size, plugin:GetSetting("Missing"))
        "#,
    );
    assert_eq!(lines(&dm), ["2 nil"]);
    let studio = dm.studio();
    assert!(studio.settings_changed);
    assert_eq!(studio.settings["TestPlugin"]["Grid"]["size"], 2);
}

/// What a place file keeps: not Studio's own services.
#[test]
fn saving_leaves_out_studios_own() {
    let dm = editing("game:GetService('Selection') game:GetService('ChangeHistoryService')");
    let bytes = dm.save_place(rook_formats::Format::Binary).unwrap();
    let loaded = rook_formats::load(&bytes).unwrap();
    let names: Vec<String> = (loaded.dom.root().children().iter())
        .map(|&r| loaded.dom.get_by_ref(r).unwrap().class.to_string())
        .collect();
    for gone in [
        "CoreGui",
        "PluginGuiService",
        "Selection",
        "ChangeHistoryService",
    ] {
        assert!(!names.iter().any(|n| n == gone), "{gone} in {names:?}");
    }
    assert!(names.iter().any(|n| n == "Workspace"));
}

/// What an Output window reads: each line printed, once, a frame later,
/// with an empty context table (Measured in Studio 0.748); an error's
/// stack follows it as lines of its own.
#[test]
fn message_out_announces_output() {
    let mut dm = editing(
        r#"
        game:GetService("LogService").MessageOut:Connect(function(message, kind, context)
            if message == "hello" or message == "careful" then
                print("got", message, kind, typeof(context), next(context))
            elseif string.find(message, "boom", 1, true) or string.find(message, "^Stack") then
                print("got", kind)
            end
        end)
        print("hello")
        warn("careful")
        task.spawn(function() error("boom") end)
        "#,
    );
    dm.step();
    let out = lines(&dm);
    assert_eq!(
        out[out.len() - 5..],
        [
            "got hello Enum.MessageType.MessageOutput table nil",
            "got careful Enum.MessageType.MessageWarning table nil",
            "got Enum.MessageType.MessageError",
            "got Enum.MessageType.MessageInfo",
            "got Enum.MessageType.MessageInfo",
        ],
        "{out:?}"
    );
}

/// ReflectionService as Studio 0.748 answers for a Part: its own
/// properties first, each with its owner, category, permits and types;
/// `nil` for a class that doesn't exist.
#[test]
fn reflection_lists_properties_as_studio_does() {
    let dm = editing(
        r#"
        local rs = game:GetService("ReflectionService")
        local props = rs:GetPropertiesOfClass("Part")
        print(props[1].Name, props[1].Owner, props[1].Display.Category, props[1].Type.EnumType)
        for _, p in props do
            if p.Name == "Anchored" or p.Name == "Material" or p.Name == "CFrame" then
                print(p.Name, p.Owner, p.Display.Category, p.Permits.Write ~= nil, p.Serialized,
                    p.Type.EngineType, p.Type.ScriptType)
            end
        end
        for _, p in rs:GetPropertiesOfClass("Part", { ExcludeInherited = true }) do print("own", p.Name) end
        print(rs:GetPropertiesOfClass("Nope"), rs:GetClass("Nope"))
        "#,
    );
    assert_eq!(
        lines(&dm),
        [
            "Shape Part Part PartType",
            "Anchored BasePart Part true true bool boolean",
            "CFrame BasePart Transform true true CoordinateFrame CFrame",
            "Material BasePart Appearance true true Enum EnumItem",
            "own Shape",
            "own shape",
            "nil nil",
        ]
    );
}

/// A plugin reading where a part is after frames have trimmed the journal:
/// Edit mode builds no physics world, and the joints' reader keeps up all
/// the same (it read from the start and failed a debug check).
#[test]
fn reading_a_part_after_trims() {
    let mut dm = editing(
        r#"
        local n = 0
        game:GetService("RunService").Heartbeat:Connect(function()
            n += 1
            workspace.Brick.Name = "Brick"
            if n == 3 then
                print(workspace.Brick.CFrame.Position)
            end
        end)
        "#,
    );
    for _ in 0..3 {
        dm.step();
    }
    assert_eq!(lines(&dm), ["0, 0, 0"]);
}
