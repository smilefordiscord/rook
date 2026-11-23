//! Path2D's control points, and TextService:GetTextBoundsAsync.

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
