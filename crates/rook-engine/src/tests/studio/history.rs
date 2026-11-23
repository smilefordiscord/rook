//! ChangeHistoryService (docs/behavior/studio/history.md).

use super::*;

#[test]
fn waypoints_undo_and_redo() {
    let dm = editing(
        r#"
        local chs = game:GetService("ChangeHistoryService")
        chs.OnUndo:Connect(function(name) print("undid", name) end)
        chs.OnRedo:Connect(function(name) print("redid", name) end)
        local brick = workspace.Brick
        chs:SetWaypoint("Start")
        brick.Transparency = 0.5
        brick.Name = "Glass"
        chs:SetWaypoint("Glass")
        brick.Parent = nil
        chs:SetWaypoint("Delete")
        print(chs:GetCanUndo())
        chs:Undo()
        print(brick.Parent == workspace, brick.Name)
        chs:Undo()
        print(brick.Name, brick.Transparency, chs:GetCanRedo())
        chs:Redo()
        print(brick.Name, brick.Transparency)
        brick.Name = "Changed"
        chs:SetWaypoint("Rename")
        print(chs:GetCanRedo())
        "#,
    );
    assert_eq!(
        lines(&dm),
        [
            "true Delete",
            "undid Delete",
            "true Glass",
            "undid Glass",
            "Brick 0 true Glass",
            "redid Glass",
            "Glass 0.5",
            // A new step drops what was put aside to redo.
            "false",
        ]
    );
    dm.undo();
    dm.run_extra_script("print(workspace:FindFirstChildOfClass('Part').Name)")
        .unwrap();
    assert_eq!(lines(&dm), ["undid Rename", "Glass"]);
}

/// Measured in Studio 0.748: `Append` keeps its edits as a step of its
/// own, named as its recording, like `Commit`.
#[test]
fn recordings_commit_cancel_and_append() {
    let dm = editing(
        r#"
        local chs = game:GetService("ChangeHistoryService")
        local brick = workspace.Brick
        local id = chs:TryBeginRecording("Move")
        print(string.match(id, "^recording_%d+$") ~= nil, chs:IsRecordingInProgress(), chs:TryBeginRecording("Other"))
        brick.Position = Vector3.new(0, 10, 0)
        chs:FinishRecording(id, Enum.FinishRecordingOperation.Commit)
        local cancelled = chs:TryBeginRecording("Paint")
        brick.Color = Color3.new(1, 0, 0)
        chs:FinishRecording(cancelled, Enum.FinishRecordingOperation.Cancel)
        print(brick.Color == Color3.new(1, 0, 0))
        local more = chs:TryBeginRecording("Move more")
        brick.Position = Vector3.new(0, 20, 0)
        chs:FinishRecording(more, Enum.FinishRecordingOperation.Append)
        print(chs:GetCanUndo())
        chs:Undo()
        print(brick.Position.Y, chs:GetCanUndo())
        "#,
    );
    assert_eq!(
        lines(&dm),
        ["true true nil", "false", "true Move more", "10 true Move"]
    );
}

/// Measured in Studio 0.748: edits made outside a recording join the next
/// step (a recording's, kept or cancelled with it) or, when `Undo` comes
/// first, are undone with the newest; finishing any recording drops the
/// redo steps.
#[test]
fn edits_outside_recordings_join_the_next_step() {
    let dm = editing(
        r#"
        local chs = game:GetService("ChangeHistoryService")
        local brick = workspace.Brick
        brick.Name = "Loose"
        local id = chs:TryBeginRecording("Fade")
        brick.Transparency = 0.5
        chs:FinishRecording(id, Enum.FinishRecordingOperation.Commit)
        chs:Undo()
        print(brick.Name, brick.Transparency)
        chs:Redo()
        brick.Name = "Again"
        chs:Undo()
        print(brick.Name, brick.Transparency, chs:GetCanRedo())
        brick.Name = "Cancelled"
        local nothing = chs:TryBeginRecording("Nothing")
        print(chs:GetCanRedo())
        chs:FinishRecording(nothing, Enum.FinishRecordingOperation.Cancel)
        print(brick.Name, chs:GetCanRedo())
        "#,
    );
    assert_eq!(
        lines(&dm),
        ["Brick 0", "Brick 0 true Fade", "true Fade", "Brick false"]
    );
}

/// Studio 0.748's errors and `OnRecordingFinished`'s arguments: the
/// display name is the name when none was given.
#[test]
fn finishing_recordings() {
    let mut dm = editing(
        r#"
        local chs = game:GetService("ChangeHistoryService")
        local commit = Enum.FinishRecordingOperation.Commit
        print(pcall(chs.FinishRecording, chs, "nope", commit))
        chs.OnRecordingFinished:Connect(function(name, display, id, op, options)
            print(name, display, string.match(id, "^recording_%d+$") ~= nil, op, options)
        end)
        local id = chs:TryBeginRecording("Paint", "Paint parts")
        print(pcall(chs.FinishRecording, chs, "nope", commit))
        chs:FinishRecording(id, commit)
        task.wait()
        chs:FinishRecording(chs:TryBeginRecording("Move"), commit)
        "#,
    );
    dm.step();
    assert_eq!(
        lines(&dm),
        [
            "false No recording in progress.",
            "false Invalid recording identifier.",
            "Paint Paint parts true Enum.FinishRecordingOperation.Commit nil",
            "Move Move true Enum.FinishRecordingOperation.Commit nil",
        ]
    );
}

/// What the editor and the engine write isn't an edit: the camera, the
/// plugins' GUI, layout.
#[test]
fn only_the_place_is_recorded() {
    let mut dm = editing(
        r#"
        local info = DockWidgetPluginGuiInfo.new(Enum.InitialDockState.Right, true)
        local widget = plugin:CreateDockWidgetPluginGui("Panel", info)
        local label = Instance.new("TextLabel")
        label.Text = "hi"
        label.Parent = widget
        "#,
    );
    let at = rook_types::CFrame::from_position(glam::Vec3::new(0.0, 50.0, 0.0));
    dm.set_edit_camera(at, at, 70.0);
    dm.step();
    dm.undo();
    assert!(dm.studio().history.undo.is_empty());
    assert!(dm.studio().history.redo.is_empty());
}
