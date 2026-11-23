use crate::tests::*;

/// A player who joined directly has no join or teleport data.
#[test]
fn a_direct_join_has_no_join_or_teleport_data() {
    let (out, _) = run_with_ann(
        r#"
        local data = game:GetService("Players").Ann:GetJoinData()
        print(typeof(data), next(data))
        print(game:GetService("TeleportService"):GetLocalPlayerTeleportData())
        "#,
        0,
        None,
        |_| "{}",
    );
    assert_eq!(out, ["table nil", "nil"]);
}
