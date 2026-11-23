# Memory stores

Checked against Studio 0.748 on 2026-11-22, from the creator docs only: nothing here is measured in Studio yet.

MemoryStoreService's sorted maps, queues and hash maps, as Rook answers
them within one session. The structures are in `rook-lua`'s
`memory_stores.rs` and `memory_stores/`, what scripts call in
`api/methods/web/memory_stores.rs`, and Pages objects in
`api/methods/web/pages.rs`; the tests are `rook-lua`'s
`tests/scripting/memory_stores.rs`.

## Where the data lives

- Roblox keeps memory stores on its servers, shared by every server of
  a game, and Studio's playtests use a test scope of their own, isolated
  from the live game's (Confirmed: creator-docs "Memory stores", "Test and
  debug in Studio"). A Rook session is one server, so its stores live in
  the server's memory for the run and start empty (Approximation: Studio's
  test scope outlives a playtest).
- The same name reaches the same structure from any script
  (Confirmed: creator-docs MemoryStoreService). Asking for one twice
  gives the same object (Inference, as DataStoreService does); a queue
  asked for with another invisibility timeout is another object over the
  same queue.
- Only servers reach them: a client's call raises `InvalidClientAccess:
  MemoryStoreService must be called from server.` (Confirmed: the status
  code and its description, creator-docs "Memory stores"; that it is
  raised by the structure's methods rather than `GetSortedMap` and its
  other getters is Inference).
- Each call answers at once (Approximation: Roblox's go to its servers
  and yield for the round trip).
- Values are kept as the JSON Roblox converts them to, so a value read
  back is a copy, with `JSONEncode`'s rules for tables ([JSON](json.md)).
  A value JSON can't hold (an Instance, a datatype, a function) raises
  `Unable to convert value to json.`, and a nil value `Argument 2 missing
  or nil` (Inference).

## Expiration

Every item has an expiration in seconds, from 0 to 3,888,000 (45 days)
(Confirmed: creator-docs). It is whole seconds, a fraction cut off
(Inference: the parameter is an `int64`). An item is gone once the
session's clock (`time()`) reaches its time, so one set to expire after
0 seconds is never seen (Inference). Setting an item again starts its
time over.

Rook keeps each structure's deadlines in order and drops what has
expired when the structure is next used, so nothing scans for expired
items each frame.

## Sorted maps

- Items sort by sort key, then key: numeric sort keys first, then string
  ones, then items without one (Confirmed: creator-docs
  MemoryStoreSortedMap and "Memory store sorted map").
- `SetAsync(key, value, expiration, sortKey)` returns whether the key
  was there already; `GetAsync` returns the value and the sort key, nil
  for either that's missing; `RemoveAsync` and `GetSizeAsync` as
  documented.
- `GetRangeAsync(direction, count, lower, upper)` returns up to `count`
  items (1 to 200) strictly between the bounds, from the low end when
  ascending and the high end when descending, as `{key, value, sortKey}`
  tables. A bound is `{key = ..., sortKey = ...}` with one or both given
  (Confirmed). How a bound with one of them compares isn't documented:
  Rook takes one with both as that item's place, one with only a key as
  the place of an item with that key and no sort key, and one with only
  a sort key as lying past every item with that sort key (Inference).
- `UpdateAsync(key, transform, expiration)` calls the transform with the
  old value and sort key (nil for a new key) and stores what it returns;
  returning nil cancels (Confirmed). It returns what the transform
  returned, nil when it cancelled (Inference). One session has no other
  server to conflict with, so the transform runs once. A transform that
  raises an error raises `TransformCallbackFailed: Failed to invoke
  transformation callback function.` (Inference from the status code).
- Keys and string sort keys are at most 128 bytes, values at most 32 KB
  of JSON, a map at most 1,000,000 items and 100 MB (Confirmed: the
  limits; counting bytes and measuring a value as its JSON are Inference).

## Queues

- `GetQueue(name, invisibilityTimeout)`, 30 seconds by default
  (Confirmed: creator-docs).
- `AddAsync(value, expiration, priority)`: items are read highest
  priority first, then in the order they were added; `priority` is 0 by
  default (Confirmed).
- `ReadAsync(count, allOrNothing, waitTimeout)` reads up to `count`
  items (1 to 100) and returns their values and an id for
  `RemoveAsync`; with `allOrNothing`, fewer than `count` items read none
  (Confirmed). What it read is hidden from other reads for the queue
  object's invisibility timeout, then shows again unless removed
  (Confirmed). With nothing to read it tries again every two seconds
  until `waitTimeout` runs out, forever at -1 (its default), not at all
  at 0 (Confirmed); then it returns an empty array and no id (Inference).
  The id is an opaque string (Inference).
- `RemoveAsync(id)` removes the items that read hid, and does nothing
  once their invisibility has run out (Confirmed).
- `GetSizeAsync(excludeInvisible)` counts every item, or only those no
  read is hiding (Confirmed).
- A queue holds at most 1,000,000 items and 100 MB (Confirmed).

## Hash maps

- `SetAsync` (returning whether the key was there), `GetAsync`,
  `RemoveAsync` and `UpdateAsync` (the old value in, the new one out,
  nil cancels) as for sorted maps, without sort keys (Confirmed:
  creator-docs MemoryStoreHashMap). Keys are at most 128 bytes and values
  32 KB; a hash map has no item or size limit of its own (Confirmed).
- `ListItemsAsync(count)` (1 to 200) returns a MemoryStoreHashMapPages
  whose pages hold up to `count` `{key, value}` entries (Confirmed).
  Roblox promises no order; Rook lists keys in byte order and reads each
  page when the script advances to it, after the last key the page
  before gave (Approximation). `IsFinished` is true on the last page, and
  advancing past it raises `No pages to advance to` (Inference).

## Errors

Roblox's creator docs list each failure's status code and description,
not the message a script sees. Rook raises `<code>: <description>` with
the documented ones, such as `InvalidExpirationTime: The field
'expiration' time must be between 0 and 3,888,000.` and
`ItemValueSizeTooLarge: Value size exceeds limit (32 KB).`, and words
its own for the limits the docs give no description for: an empty or
too long key, a count out of range (Inference).

## What Rook lacks

- The game's memory quota (64 KB plus 1.2 KB a user) and request units
  (1,000 plus 120 a user each minute), which Studio's playtests have too
  (Confirmed: creator-docs), so nothing is throttled.
- `MemoryStoreService:GetDistributedCounter`, which the creator docs
  don't describe.

## Needs a Studio check

The error messages as scripts see them; whether a client's call fails at
the getters; the half-given range bounds; what `ReadAsync` returns when
it times out; an expiration of 0; how a value's size is measured; and
whether repeated getters return the same object.
