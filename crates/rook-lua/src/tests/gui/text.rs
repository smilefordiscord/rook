//! TextService's measuring (docs/behavior/gui/text.md#measuring-text).

use crate::tests::*;

#[test]
fn get_text_bounds_params_defaults() {
    expect(
        r#"
        local p = Instance.new("GetTextBoundsParams")
        print(p.Name, p.Text == "", p.Size, p.Width, p.RichText)
        print(p.Font.Family, p.Font.Weight, p.Font.Style)
        "#,
        1,
        &[
            "Instance true 20 0 false",
            "rbxasset://fonts/families/SourceSansPro.json Enum.FontWeight.Regular Enum.FontStyle.Normal",
        ],
    );
}

/// With its fonts in the install (or none to wait for), the call returns
/// without yielding, as Rook's other Async members do when the answer is
/// at hand.
#[test]
fn get_text_bounds_answers_at_once_when_its_fonts_are_loaded() {
    expect(
        r#"
        local TextService = game:GetService("TextService")
        local p = Instance.new("GetTextBoundsParams")
        p.Text = "Hello"
        local done = false
        task.spawn(function()
            local b = TextService:GetTextBoundsAsync(p)
            print(typeof(b))
            done = true
        end)
        print(done)
        print(pcall(function() TextService:GetTextBoundsAsync() end))
        print(pcall(function() TextService:GetTextBoundsAsync(nil) end))
        print(pcall(function() TextService:GetTextBoundsAsync(Instance.new("Folder")) end))
        print(pcall(function() TextService:GetTextBoundsAsync("x") end))
        "#,
        1,
        &[
            "Vector2",
            "true",
            "false Argument 1 missing or nil",
            "false Argument 1 missing or nil",
            "false Argument 1 is invalid: expected 'GetTextBoundsParams' instance type",
            "false Unable to cast value to Object",
        ],
    );
}

/// A font by asset id: the call waits until the engine hands the font
/// back.
#[test]
fn get_text_bounds_waits_for_its_font() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.run_source(
        "Workspace.Script",
        r#"
        local p = Instance.new("GetTextBoundsParams")
        p.Text = "Hello"
        p.Font = Font.new("rbxassetid://1")
        print("asked")
        local b = game:GetService("TextService"):GetTextBoundsAsync(p)
        print("measured", typeof(b))
        "#,
        None,
    )
    .unwrap();
    assert_eq!(finish(&rt, 3), ["asked"]);
    let requests = rt.world_mut().gui.take_font_requests();
    assert_eq!(requests, ["rbxassetid://1"]);
    rt.deliver_font(&requests[0], None);
    assert_eq!(finish(&rt, 1), ["measured Vector2"]);
    assert!(rt.world().text_bounds_waiting.is_empty());
}

/// Studio 0.748: `Size` takes anything, kept within 1 to 100 and cut to a
/// whole number; a GetTextBoundsParams doesn't clone; GetTextSize casts
/// its arguments from the last.
#[test]
fn get_text_bounds_params_size_and_clone() {
    expect(
        r#"
        local p = Instance.new("GetTextBoundsParams")
        local sizes = {}
        for _, v in { 0, 1.5, 1000, 0 / 0, "x", "12" } do
            p.Size = v
            table.insert(sizes, tostring(p.Size))
        end
        p.Size = nil
        table.insert(sizes, tostring(p.Size))
        print(table.concat(sizes, " "))
        print(p:Clone())
        local TextService = game:GetService("TextService")
        print(pcall(TextService.GetTextSize, TextService))
        print(pcall(TextService.GetTextSize, TextService, "Hi", 14, "Nope", Vector2.zero))
        print(pcall(TextService.GetTextSize, TextService, "Hi", 14, Enum.Font.Arial, {}))
        "#,
        1,
        &[
            "1 1 100 -2147483648 1 12 1",
            "nil",
            "false Argument 4 missing or nil",
            "false Unable to cast string to Font",
            "false Unable to cast Array to Vector2",
        ],
    );
}

#[test]
fn text_bounds_match_text_size() {
    expect(
        r#"
        local ts = game:GetService("TextService")
        local params = Instance.new("GetTextBoundsParams")
        params.Text = "hello world"
        params.Size = 18
        params.Font = Font.fromEnum(Enum.Font.Gotham)
        local b = ts:GetTextBoundsAsync(params)
        print(typeof(b), b == ts:GetTextSize("hello world", 18, Enum.Font.Gotham, Vector2.new(1e7, 1e7)))
        "#,
        0,
        &["Vector2 true"],
    );
}
