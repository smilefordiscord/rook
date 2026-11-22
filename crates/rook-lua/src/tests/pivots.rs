use crate::tests::*;

/// Doors that swing by `SetPrimaryPartCFrame(GetPrimaryPartCFrame() * r)`
/// every frame (a 61k-part place's) used to balloon and vanish: rounding error in
/// the rotation tripled with each move until parts were orthonormalized.
#[test]
fn repeated_model_moves_keep_parts_rigid() {
    expect(
        r#"
        local m = Instance.new("Model")
        local door = Instance.new("Part")
        door.Anchored = true
        door.CFrame = CFrame.new(-837.6, 5766.9, 1323.1) * CFrame.Angles(0.3, 1.47, -0.2)
        door.Parent = m
        local hinge = door:Clone()
        hinge.CFrame = door.CFrame * CFrame.new(2, 0, 0)
        hinge.Parent = m
        m.PrimaryPart = door
        m.Parent = workspace
        for _ = 1, 600 do
            m:SetPrimaryPartCFrame(m:GetPrimaryPartCFrame() * CFrame.Angles(0, math.rad(-6), 0))
        end
        local function scale(p) return math.round(p.CFrame.RightVector.Magnitude * 1e4) / 1e4 end
        print(scale(door), scale(hinge), math.round((door.Position - hinge.Position).Magnitude * 10) / 10)
        "#,
        0,
        &["1 1 2"],
    );
}

/// Doors that turn every part about `GetModelCFrame()` swing about the
/// bounding box center (turned to the primary part), not the primary part
/// itself: an invisible part beside the hinge pulls it there.
#[test]
fn model_cframe_is_the_bounding_box() {
    expect(
        r#"
        local m = Instance.new("Model")
        local door = Instance.new("Part")
        door.Size = Vector3.new(0.6, 7.7, 5)
        door.CFrame = CFrame.new(10, 5, 0) * CFrame.Angles(0, math.rad(30), 0)
        door.Parent = m
        local stabilizer = Instance.new("Part")
        stabilizer.Size = Vector3.new(0.6, 8, 5)
        stabilizer.CFrame = door.CFrame * CFrame.new(0, 0.15, 4)
        stabilizer.Parent = m
        m.PrimaryPart = door
        local box, size = m:GetBoundingBox()
        local p = door.CFrame:ToObjectSpace(m:GetModelCFrame())
        local function r(v) return math.round(v * 100) / 100 end
        print(m:GetModelCFrame() == box, r(p.X), r(p.Y), r(p.Z), r(size.X), r(size.Y), r(size.Z))
        "#,
        0,
        &["true 0 0.15 2 0.6 8 9"],
    );
}

/// Without a primary part the box turns with the saved pivot, so a light
/// spun about `GetModelCFrame()`'s X axis turns about the pivot's.
#[test]
fn model_cframe_turns_with_the_pivot() {
    expect(
        r#"
        local m = Instance.new("Model")
        local p = Instance.new("Part")
        p.Size = Vector3.new(1, 2, 3)
        p.Parent = m
        m.WorldPivot = CFrame.new(5, 0, 0) * CFrame.Angles(0, 0, math.rad(90))
        local c = m:GetModelCFrame()
        local function r(v) return math.round(v * 100) / 100 end
        print(r(c.XVector.Y), r(c.Position.X))
        "#,
        0,
        &["1 0"],
    );
}

/// An Attachment's Position, Orientation and World views read and set its
/// CFrame, under its part.
#[test]
fn attachment_views_of_cframe() {
    expect(
        "local p = Instance.new('Part') p.Position = Vector3.new(10, 0, 0) \
         local a = Instance.new('Attachment', p) a.Position = Vector3.new(0, 2, 0) \
         print(a.CFrame.Position, a.WorldPosition) \
         a.Orientation = Vector3.new(0, 90, 0) \
         print(math.round(a.Axis.Z), math.round(a.WorldAxis.Z)) \
         a.WorldPosition = Vector3.new(0, 0, 0) \
         print(a.Position)",
        0,
        &["0, 2, 0 10, 2, 0", "-1 -1", "-10, 0, 0"],
    );
}

#[test]
fn model_scale_to_scales_about_the_pivot() {
    expect(
        r#"
        local m = Instance.new("Model")
        local a = Instance.new("Part")
        a.Size = Vector3.new(2, 2, 2)
        a.CFrame = CFrame.new(0, 0, 0)
        a.Parent = m
        local b = Instance.new("Part")
        b.Size = Vector3.new(2, 4, 2)
        b.CFrame = CFrame.new(10, 0, 0)
        b.Parent = m
        local at = Instance.new("Attachment")
        at.Position = Vector3.new(0, 1, 0)
        at.Parent = b
        m.PrimaryPart = a
        m.Parent = workspace
        print(m:GetScale())
        m:ScaleTo(0.5)
        print(m:GetScale(), a.Size, b.Size, b.Position, at.Position)
        m:ScaleTo(1)
        print(m:GetScale(), b.Size, b.Position)
        print(pcall(m.ScaleTo, m, 0))
        m:ScaleTo(0 / 0)
        print(m:GetScale(), b.Size)
        "#,
        0,
        &[
            "1",
            "0.5 1, 1, 1 1, 2, 1 5, 0, 0 0, 0.5, 0",
            "1 2, 4, 2 10, 0, 0",
            "false Only positive non-zero scale values are allowed.",
            "1 2, 4, 2",
        ],
    );
}

/// What `ScaleTo` scales, as measured in Studio 0.748 on the same model: a
/// primitive mesh keeps its `Scale` (its part's size scales it already) and
/// a file mesh's scales; every mesh's offset, the PrimaryPart's pivot
/// offset (so the pivot is at 1, 0, 0), a weld constraint's offset and a
/// nested model's pivot and `Scale` scale too.
#[test]
fn model_scale_to_scales_meshes_welds_and_inner_models() {
    expect(
        r#"
        local m = Instance.new("Model")
        local a = Instance.new("Part")
        a.Anchored = true
        a.Size = Vector3.new(2, 2, 2)
        a.PivotOffset = CFrame.new(1, 0, 0)
        a.Parent = m
        local b = Instance.new("Part")
        b.Size = Vector3.new(2, 2, 2)
        b.CFrame = CFrame.new(10, 0, 0)
        b.Parent = m
        local sphere = Instance.new("SpecialMesh")
        sphere.MeshType = Enum.MeshType.Sphere
        sphere.Offset = Vector3.new(0, 1, 0)
        sphere.Parent = a
        local block = Instance.new("BlockMesh")
        block.Parent = b
        local file = Instance.new("SpecialMesh")
        file.MeshType = Enum.MeshType.FileMesh
        file.Parent = b
        local weld = Instance.new("WeldConstraint")
        weld.Part0, weld.Part1 = a, b
        weld.Parent = a
        local inner = Instance.new("Model")
        inner.WorldPivot = CFrame.new(10, 4, 0)
        inner.Parent = m
        m.PrimaryPart = a
        m.Parent = workspace
        m:ScaleTo(2)
        print(b.Position, a.PivotOffset.Position)
        print(sphere.Scale, sphere.Offset, block.Scale, file.Scale)
        print(inner.WorldPivot.Position, inner:GetScale())
        task.wait()
        print(b.Position)
        "#,
        1,
        &[
            "19, 0, 0 2, 0, 0",
            "1, 1, 1 0, 2, 0 1, 1, 1 2, 2, 2",
            "19, 8, 0 2",
            "19, 0, 0",
        ],
    );
}

#[test]
fn bulk_move_to_moves_each_part() {
    expect(
        r#"
        local a, b = Instance.new("Part"), Instance.new("Part")
        a.Anchored, b.Anchored = true, true
        a.Parent, b.Parent = workspace, workspace
        workspace:BulkMoveTo({ a, b }, { CFrame.new(1, 2, 3), CFrame.new(4, 5, 6) }, Enum.BulkMoveMode.FireCFrameChanged)
        print(a.Position, b.Position)
        print(pcall(workspace.BulkMoveTo, workspace, { a, b }, { CFrame.new() }))
        print(pcall(workspace.BulkMoveTo, workspace, { Instance.new("Folder") }, { Vector3.zero }))
        print(pcall(workspace.BulkMoveTo, workspace, { a }, { Vector3.zero }))
        print(pcall(workspace.BulkMoveTo, workspace, { 1, 2 }, { CFrame.new() }))
        print(pcall(workspace.BulkMoveTo, workspace, { a }, { CFrame.new() }, "bad"))
        "#,
        0,
        &[
            "1, 2, 3 4, 5, 6",
            "false Length of part list (2) and length of CFrame list (1) do not match.",
            "false Can only call BulkMoveTo on BaseParts, partList[1] is a Folder.",
            "false CFrame list must only contain CFrames, cframeList[1] is a Vector3.",
            "false Unable to cast value to Object",
            "false Unable to cast string to BulkMoveMode",
        ],
    );
}
