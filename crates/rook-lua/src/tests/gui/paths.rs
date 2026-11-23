use crate::tests::*;

/// Measured in Studio: the cap is 100, an over-cap set keeps the first 100
/// and raises, and indexes outside the points raise.
#[test]
fn path2d_control_points() {
    expect(
        r#"
        local p = Instance.new("Path2D")
        print(p:GetMaxControlPoints(), #p:GetControlPoints())
        p:SetControlPoints({ Path2DControlPoint.new(UDim2.fromOffset(1, 2)), Path2DControlPoint.new(UDim2.fromOffset(3, 4)) })
        print(#p:GetControlPoints(), p:GetControlPoint(2).Position)
        print(pcall(p.GetControlPoint, p, 5))
        p:InsertControlPoint(1, Path2DControlPoint.new(UDim2.fromOffset(9, 9)))
        p:UpdateControlPoint(3, Path2DControlPoint.new(UDim2.fromOffset(7, 7)))
        p:RemoveControlPoint(2)
        local points = p:GetControlPoints()
        print(#points, points[1].Position, points[2].Position)
        print(pcall(p.SetControlPoints, p, { 1 }))
        local many = {}
        for i = 1, 105 do many[i] = Path2DControlPoint.new() end
        print(pcall(p.SetControlPoints, p, many))
        print(#p:GetControlPoints())
        print(pcall(p.InsertControlPoint, p, 1, Path2DControlPoint.new()))
        "#,
        0,
        &[
            "100 0",
            "2 {0, 3}, {0, 4}",
            "false Path2D:GetControlPoint() at index 5 access out of bounds",
            "2 {0, 9}, {0, 9} {0, 7}, {0, 7}",
            "false Variant cast failed",
            "false Path2D:SetControlPoints() only supports 100 control points.",
            "100",
            "false Path2D:InsertControlPoint() only supports 100 control points.",
        ],
    );
}

/// Confirmed (creator-docs): each of the four setters fires
/// ControlPointChanged, a look property doesn't; `Thickness` is kept
/// within 0 to 100.
#[test]
fn path2d_changes_fire_and_thickness_is_kept_in_range() {
    expect(
        r#"
        local p = Instance.new("Path2D")
        local fired = 0
        p.ControlPointChanged:Connect(function() fired += 1 end)
        p:SetControlPoints({ Path2DControlPoint.new(), Path2DControlPoint.new() })
        p:InsertControlPoint(3, Path2DControlPoint.new())
        p:UpdateControlPoint(1, Path2DControlPoint.new(UDim2.fromScale(1, 1)))
        p:RemoveControlPoint(2)
        p.Color3 = Color3.new(1, 0, 0)
        task.wait()
        print(fired)
        p.Thickness = 250
        print(p.Thickness)
        p.Thickness = -3
        print(p.Thickness)
        "#,
        2,
        &["4", "100", "0"],
    );
}
