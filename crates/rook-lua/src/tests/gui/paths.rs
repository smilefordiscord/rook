use crate::tests::*;

/// The cap is 100 (creator-docs). Studio 0.748: with no parent a point
/// can't be inserted, and after `SetControlPoints` a removal raises from
/// the spline, the point gone anyway; past the cap the first 100 stay.
#[test]
fn path2d_control_points() {
    expect(
        r#"
        local p = Instance.new("Path2D")
        print(p:GetMaxControlPoints(), #p:GetControlPoints())
        p:SetControlPoints({ Path2DControlPoint.new(UDim2.fromOffset(1, 2)), Path2DControlPoint.new(UDim2.fromOffset(3, 4)) })
        print(#p:GetControlPoints(), p:GetControlPoint(2).Position)
        print(pcall(p.GetControlPoint, p, 5))
        print(pcall(p.InsertControlPoint, p, 1, Path2DControlPoint.new(UDim2.fromOffset(9, 9))))
        print(pcall(p.RemoveControlPoint, p, 2))
        print(#p:GetControlPoints())
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
            "false Attempting to use Path2D with invalid parent",
            "false Spline removing control point at index 1 access out of bounds",
            "1",
            "false Variant cast failed",
            "false Path2D:SetControlPoints() only supports 100 control points.",
            "100",
            "false Path2D:InsertControlPoint() only supports 100 control points.",
        ],
    );
}

/// Confirmed (creator-docs): each of the four setters fires
/// ControlPointChanged, a look property doesn't; `Thickness` is kept
/// within 0 to 100. A query builds the spline again, so the setters that
/// follow it don't raise.
#[test]
fn path2d_changes_fire_and_thickness_is_kept_in_range() {
    expect(
        r#"
        local f = Instance.new("Frame")
        local p = Instance.new("Path2D")
        p.Parent = f
        local fired = 0
        p.ControlPointChanged:Connect(function() fired += 1 end)
        p:SetControlPoints({ Path2DControlPoint.new(), Path2DControlPoint.new() })
        p:GetLength()
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

/// Studio 0.748: positions come back as UDim2s of the parent's size,
/// tangents as the segment's derivative, the length in pixels and the
/// bounding rect round the points; with no segments the curve queries
/// raise.
#[test]
fn path2d_curve_queries() {
    expect(
        r#"
        local f = Instance.new("Frame")
        f.Size = UDim2.fromOffset(200, 100)
        local p = Instance.new("Path2D")
        p.Thickness = 4
        print(pcall(p.GetLength, p))
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
            "false Attempting to use Path2D with invalid parent",
            "false Path2D:GetPositionOnCurve() attempting to query with zero segments",
            "0",
            "200 {0.25, 0}, {0.5, 0} true 0.5, 0",
            "200, 0 200, 0",
            "-2.5, 47.5 202.5, 52.5",
            "100",
        ],
    );
}

/// Studio 0.748: the constructor takes no, one or three UDim2s, and its
/// table is read-only; a point prints its members.
#[test]
fn path2d_control_point_values() {
    expect(
        r#"
        local P = Path2DControlPoint
        local function e(...)
            local ok, m = pcall(...)
            return ok, (tostring(m):gsub("^.-:%d+: ", ""))
        end
        print(P.new(UDim2.new(0.5, 10, 0.25, -20), UDim2.fromOffset(-5, 0), UDim2.new(0.1, 3, 0, 4)))
        print(e(P.new, UDim2.new(), UDim2.new()))
        print(e(P.new, nil, nil, UDim2.new()))
        print(e(function() P.x = 1 end))
        "#,
        0,
        &[
            "Path2DControlPoint { Position = {0.5, 10}, {0.25, -20}, LeftTangent = {0, -5}, {0, 0}, RightTangent = {0.100000001, 3}, {0, 4} }",
            "false Trying to create a Path2DControlPoint with wrong number of arguments",
            "false invalid argument #1 to 'new' (UDim2 expected, got nil)",
            "false attempt to modify a readonly table",
        ],
    );
}

/// The points are the path's `PropertiesSerialize`, which `Clone` copies;
/// a clone's points are its own after that.
#[test]
fn path2d_clone_keeps_its_points() {
    expect(
        r#"
        local p = Instance.new("Path2D")
        p:SetControlPoints({ Path2DControlPoint.new(UDim2.fromOffset(1, 2)), Path2DControlPoint.new(UDim2.fromScale(1, 1)) })
        local c = p:Clone()
        print(#c:GetControlPoints(), c:GetControlPoint(1).Position, c:GetControlPoint(2).Position)
        c:RemoveControlPoint(1)
        print(#c:GetControlPoints(), #p:GetControlPoints())
        "#,
        0,
        &["2 {0, 1}, {0, 2} {1, 0}, {1, 0}", "1 2"],
    );
}
