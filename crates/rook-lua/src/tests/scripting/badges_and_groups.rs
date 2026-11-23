use crate::tests::*;

/// With no endpoints the session is all there is: badges are had by those
/// it awarded them to, and no group exists.
#[test]
fn badges_and_groups_answer_locally_without_endpoints() {
    let (out, requests) = run_with_ann(
        r#"
        local Badges = game:GetService("BadgeService")
        local Groups = game:GetService("GroupService")
        print(Badges:UserHasBadgeAsync(5, 100))
        print(Badges:AwardBadgeAsync(5, 100), Badges:AwardBadgeAsync(5, 100), Badges:AwardBadge(6, 100))
        print(Badges:UserHasBadgeAsync(5, 100), Badges:UserHasBadge(5, 101))
        print(table.concat(Badges:CheckUserBadgesAsync(5, { 99, 100, 101 }), ","))
        print(#Groups:GetGroupsAsync(5))
        local roles = Groups:GetRolesInGroupAsync(5, 7)
        print(roles.IsMember, #roles.Roles)
        print(pcall(Groups.GetGroupInfoAsync, Groups, 7))
        local ann = game:GetService("Players").Ann
        print(ann:IsInGroupAsync(7), ann:GetRankInGroupAsync(7), ann:GetRoleInGroupAsync(7))
        "#,
        1,
        None,
        |_| "{}",
    );
    assert_eq!(
        out,
        [
            "false",
            "true false false",
            "true false",
            "100",
            "0",
            "false 0",
            "false GroupService:GetGroupInfoAsync() failed: Unknown group 7",
            "false 0 Guest",
        ]
    );
    assert!(requests.is_empty());
}

fn host(url: &str) -> &'static str {
    match url.strip_prefix("http://web").unwrap_or(url) {
        "/groups/v1/users/5/groups/roles" => {
            r#"{"data":[{"group":{"id":7,"name":"Builders"},"role":{"id":70,"name":"Admin","rank":255},"isPrimaryGroup":true}]}"#
        }
        "/groups/v2/users/5/groups/roles" => {
            r#"{"data":[{"group":{"id":7},"role":{"id":70,"name":"Admin","rank":255}}]}"#
        }
        "/groups/v1/groups/7" => {
            r#"{"id":7,"name":"Builders","description":"We build","owner":{"userId":5,"username":"Ann"}}"#
        }
        "/groups/v1/groups/7/roles" => {
            r#"{"groupId":7,"roles":[{"id":71,"name":"Admin","rank":255},{"id":70,"name":"Member","rank":1}]}"#
        }
        "/badges/v1/users/5/badges/100/award" => r#"{"awarded":true}"#,
        "/badges/v1/users/5/badges/awarded-dates?badgeIds=99,100" => {
            r#"{"data":[{"badgeId":100}]}"#
        }
        _ => "{}",
    }
}

/// With an endpoint each member asks its domain in Roblox's shape and
/// returns what Roblox would make of the answer.
#[test]
fn badges_and_groups_ask_the_configured_host() {
    let (out, requests) = run_with_ann(
        r#"
        local Badges = game:GetService("BadgeService")
        local Groups = game:GetService("GroupService")
        local g = Groups:GetGroupsAsync(5)[1]
        print(g.Name, g.Id, g.Rank, g.Role, g.IsPrimary, g.IsInClan)
        local roles = Groups:GetRolesInGroupAsync(5, 7)
        print(roles.IsMember, roles.Roles[1].Id, roles.Roles[1].Name, roles.Roles[1].Rank)
        local info = Groups:GetGroupInfoAsync(7)
        print(info.Name, info.Id, info.Owner.Name, info.Owner.Id, info.Description)
        print(info.Roles[1].Name, info.Roles[1].Rank, info.Roles[2].Name)
        print(Badges:AwardBadgeAsync(5, 100))
        print(table.concat(Badges:CheckUserBadgesAsync(5, { 99, 100 }), ","))
        "#,
        8,
        Some("http://web"),
        host,
    );
    assert_eq!(
        out,
        [
            "Builders 7 255 Admin true false",
            "true 70 Admin 255",
            "Builders 7 Ann 5 We build",
            "Member 1 Admin",
            "true",
            "100",
        ]
    );
    assert_eq!(
        requests,
        [
            "GET http://web/groups/v1/users/5/groups/roles",
            "GET http://web/groups/v2/users/5/groups/roles",
            "GET http://web/groups/v1/groups/7",
            "GET http://web/groups/v1/groups/7/roles",
            "POST http://web/badges/v1/users/5/badges/100/award",
            "GET http://web/badges/v1/users/5/badges/awarded-dates?badgeIds=99,100",
        ]
    );
}

/// Measured: a client's `AwardBadgeAsync` returns false rather than
/// raising, once its arguments cast; `CheckUserBadgesAsync` refuses ids
/// that aren't numbers and more than 10.
#[test]
fn awarding_on_a_client_and_checking_bad_lists() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.world_mut().is_client = true;
    let source = r#"
        local Badges = game:GetService("BadgeService")
        print(Badges:AwardBadgeAsync(5, 100))
        print(pcall(Badges.AwardBadgeAsync, Badges, 5, "abc"))
        print(pcall(Badges.CheckUserBadgesAsync, Badges, 5, { "abc" }))
        print(pcall(Badges.CheckUserBadgesAsync, Badges, 5, { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11 }))
    "#;
    rt.run_source("Workspace.Script", source, None).unwrap();
    let out = finish(&rt, 0);
    assert_eq!(
        out,
        [
            "false",
            "false Unable to cast string to int64",
            "false CheckUserBadgesAsync: badgeIds must be numeric.",
            "false CheckUserBadgesAsync: Cannot query more than 10 badges.",
        ]
    );
}
