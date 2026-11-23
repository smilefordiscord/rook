# Messaging

Checked against Studio 0.748 on 2026-11-22, from the creator docs only: nothing here is measured in Studio yet.

MessagingService within one session: what a server publishes reaches the
same server's subscribers. The code is `rook-lua`'s
`api/methods/web/messaging.rs`; the tests are `rook-lua`'s
`tests/scripting/messaging.rs`.

## Publishing and subscribing

- Roblox carries messages between a game's servers, typically within 1
  to 2 seconds, and delivery is best effort (Confirmed: creator-docs
  MessagingService). A Rook session is one server, so a message reaches
  that server's own subscribers, as it does in a Studio playtest
  (Inference: Studio's playtest is one server of its own).
- `SubscribeAsync(topic, callback)` returns an RBXScriptConnection; its
  `Disconnect` unsubscribes, and a stopped script's subscriptions end
  with it (Confirmed: creator-docs). A topic can be subscribed to more
  than once, and each callback is called, the newest first as with any
  signal's handlers.
- A callback gets one table, `{Data, Sent}`: the published value and the
  Unix time it was sent, in seconds (Confirmed: creator-docs). Rook gives
  whole seconds (Inference).
- A message arrives in the frame after it was published, after
  Heartbeat with `task.wait`'s threads, never during `PublishAsync`
  (Inference: on Roblox it makes a round trip through Roblox's servers).
- The message travels as JSON, so each callback gets a copy of its own
  with `JSONEncode`'s rules for tables ([JSON](json.md)) (Inference).
- `PublishAsync` and `SubscribeAsync` return at once (Approximation:
  Roblox's yield until Roblox's servers have the message or the
  subscription).

## Limits and errors

- Topics are 1 to 80 characters and messages at most 1 kB (Confirmed:
  creator-docs); Rook counts a message's bytes as JSON, up to 1,024
  (Inference).
- A topic out of range, a message too large or one JSON can't hold
  (an Instance, a datatype, a function) raises an error, and so does a
  call on a client (Inference: Roblox's wording isn't documented; Rook's
  names the member and the reason).

## What Rook lacks

- The rate limits (messages sent per server and received per topic and
  game each minute, subscriptions and subscribe requests per server), so
  nothing is throttled.

## Needs a Studio check

Whether a playtest's server receives its own messages and how many
frames later; the error messages; whether `Sent` has a fraction; and
whether a subscription made after a message was published, before it
arrived, receives it (Rook's does).
