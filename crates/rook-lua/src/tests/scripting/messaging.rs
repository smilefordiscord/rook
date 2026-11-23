use crate::tests::*;

/// Published messages reach the topic's subscribers in a later frame,
/// each as a `{Data, Sent}` table of its own.
#[test]
fn published_messages_reach_subscribers_next_frame() {
    expect(
        r#"
        local ms = game:GetService("MessagingService")
        local a = ms:SubscribeAsync("News", function(message)
            print("a", message.Data.text, message.Data.seen)
        end)
        -- Connected last, so called first.
        ms:SubscribeAsync("News", function(message)
            message.Data.seen = true
            print("b", message.Data.text, typeof(message.Sent), message.Sent > 0)
        end)
        ms:SubscribeAsync("Other", function() print("other") end)
        print(typeof(a), a.Connected)
        ms:PublishAsync("News", { text = "hi" })
        print("published")
        task.wait(0.1)
        a:Disconnect()
        print(a.Connected)
        ms:PublishAsync("News", { text = "again" })
        "#,
        12,
        &[
            "RBXScriptConnection true",
            "published",
            "b hi number true",
            "a hi nil",
            "false",
            "b again number true",
        ],
    );
}

#[test]
fn publish_checks_topics_and_sizes() {
    expect(
        r#"
        local ms = game:GetService("MessagingService")
        local function try(f) print((select(2, pcall(f)))) end
        try(function() ms:PublishAsync("", 1) end)
        try(function() ms:PublishAsync(string.rep("t", 81), 1) end)
        try(function() ms:PublishAsync("t", string.rep("x", 1100)) end)
        try(function() ms:PublishAsync("t", workspace) end)
        try(function() ms:SubscribeAsync("t") end)
        "#,
        0,
        &[
            "MessagingService:PublishAsync() topic must be 1 to 80 characters long",
            "MessagingService:PublishAsync() topic must be 1 to 80 characters long",
            "MessagingService:PublishAsync() message is larger than 1kB",
            "MessagingService:PublishAsync() message can't be converted to JSON",
            "Argument 2 missing or nil",
        ],
    );
}
