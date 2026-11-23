use crate::tests::*;

/// A Path2D's points are its `PropertiesSerialize`, so they reach clients
/// with the path and follow the server's changes, as its other
/// properties do.
#[test]
fn a_server_paths_points_reach_its_clients() {
    let mut server = Server::new(&Options::default()).unwrap();
    add_script(
        &server,
        "ServerScriptService",
        "Main",
        r#"
        local p = Instance.new("Path2D")
        p.Name = "Route"
        p:SetControlPoints({
            Path2DControlPoint.new(UDim2.fromOffset(1, 2)),
            Path2DControlPoint.new(UDim2.new(0.5, 3, 1, 4), UDim2.fromOffset(-5, 0), UDim2.fromOffset(5, 0)),
        })
        p.Parent = game:GetService("ReplicatedStorage")
        task.wait(0.3)
        p:RemoveControlPoint(1)
        "#,
    );
    let first = service(&server, "ReplicatedFirst");
    add_under(
        &server,
        first,
        "LocalScript",
        "Watch",
        r#"
        local p = game:GetService("ReplicatedStorage"):WaitForChild("Route")
        local b = p:GetControlPoint(2)
        print(#p:GetControlPoints(), p:GetControlPoint(1).Position, b.Position, b.LeftTangent, b.RightTangent)
        task.wait(0.5)
        print(#p:GetControlPoints(), p:GetControlPoint(1).Position)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Builderman", 1).unwrap();
    session.run_for(1.0);
    let client = session.client_mut(index).unwrap();
    assert_eq!(
        lines(client),
        [
            "2 {0, 1}, {0, 2} {0.5, 3}, {1, 4} {0, -5}, {0, 0} {0, 5}, {0, 0}",
            "1 {0.5, 3}, {1, 4}",
        ]
    );
}
