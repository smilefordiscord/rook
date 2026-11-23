use crate::tests::*;

/// The cap is 100 (creator-docs); an over-cap set keeping the first 100
/// and raising, and indexes outside the points raising, are Inference.
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

/// Positions come back as UDim2s in the path's parent, tangents as unit
/// vectors, the length and bounding box in pixels; with fewer than two
/// points the curve queries raise (Confirmed: creator-docs).
#[test]
fn path2d_curve_queries() {
    expect(
        r#"
        local f = Instance.new("Frame")
        f.Size = UDim2.fromOffset(200, 100)
        local p = Instance.new("Path2D")
        p.Thickness = 4
        p.Parent = f
        print(pcall(p.GetPositionOnCurve, p, 0.5))
        print(p:GetLength())
        p:SetControlPoints({
            Path2DControlPoint.new(UDim2.fromScale(0, 0.5)),
            Path2DControlPoint.new(UDim2.fromScale(1, 0.5)),
        })
        local half = p:GetPositionOnCurveArcLength(0.5)
        print(p:GetLength(), p:GetPositionOnCurve(0.25), math.abs(half.X.Scale - 0.5) < 1e-5, half.Y)
        print(p:GetTangentOnCurve(0), p:GetTangentOnCurveArcLength(1))
        local r = p:GetBoundingRect()
        print(r.Min, r.Max)
        f.Size = UDim2.fromOffset(100, 100)
        print(p:GetLength())
        "#,
        0,
        &[
            "false Path2D:GetPositionOnCurve() requires at least 2 control points",
            "0",
            "200 {0.15625, 0}, {0.5, 0} true 0.5, 0",
            "1, 0 1, 0",
            "-2, 48 202, 52",
            "100",
        ],
    );
}
