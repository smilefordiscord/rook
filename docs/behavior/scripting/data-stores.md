# Data stores

Checked against Studio 0.748 on 2026-11-23, from the creator docs only:
no data store request has been measured in Studio (see
[what needs a Studio check](#what-needs-a-studio-check)).

How `DataStoreService` and its objects behave in Rook. The backing, budgets
and listings are `rook_lua`'s `data_stores` module; the methods are
`api::methods::data_stores`, with the requests' waiting in
`crates/rook-lua/src/api/luau/datastore.luau`. Sources are the creator docs' class pages
(DataStoreService, GlobalDataStore, DataStore, OrderedDataStore, the
options, key info and pages classes) and their pages "Data store error
codes and limits" and "Versioning, listing, and caching". Each rule below
is Confirmed (stated by those docs) unless tagged otherwise.

Data lives in memory for one server's run: a new run starts with empty
stores. Saving them between runs is [M12 step 3](../../roadmap/m12-server-hosting.md).
Requests answer at once, without the network's delay (Approximation), unless
a budget makes them wait.

## Stores and scopes

- `GetDataStore(name, scope)` and `GetOrderedDataStore` return the same
  object for the same name and scope, and `GetGlobalDataStore` always the
  same one, which uses the scope `u`. Its class is `DataStore`, the API
  dump's return type (Inference).
- Names and scopes are 1 to 50 bytes (`string.len`), and so are keys.
  The errors for names and scopes are Rook's wording (Inference): see
  [errors](#errors).
- A store keeps every scope's keys as `scope/key`. With a
  `DataStoreOptions` whose `AllScopes` is true, the scope must be `""`,
  and keys are read, written and listed in that form; a key without a
  scope raises an error (its wording is Rook's).
- On a client every `Get…DataStore` raises `DataStore can't be accessed
  from client` (Inference: the message scripts report).

## Values

A standard store keeps a value as the JSON it serializes to, so a read
returns a fresh copy shaped as [`JSONEncode`](json.md) would write it: a
table with a length keeps 1 to that length and drops its other keys, and
any other table's number keys become strings.

- What JSON can't hold raises `104: Cannot store X in data store. Data
  stores can only accept valid UTF-8 characters.`, with X the value's
  `typeof`, or `Array` or `Dictionary` for a table holding something that
  can't be stored. That covers instances and other datatypes, functions,
  strings that aren't UTF-8, NaN and infinities, and tables that hold
  themselves. Inference: the docs word 104 "Can't store X in DataStore";
  this is the text scripts report, and that X names the outer table, and
  that NaN is refused rather than stored as null, are unchecked.
- A value whose JSON is longer than 4,194,304 bytes raises `105:
  Serialized value exceeds 4MB limit.` (the limit is the docs'; the
  wording of X is Inference). Nesting deeper than 400 can't be stored
  (the docs' `InvalidJson`), and raises 104 here (Inference).
- Writing nil raises `Argument 2 missing or nil` (Inference).

An ordered store keeps integers. Anything else raises `103: X is not
allowed in data stores.`, X being `double` for a fractional number and the
`typeof` of anything else (Inference: the docs word 103 "Can't allow X in
DataStore").

## Key info and versions

Each write to a standard key appends a version, and each key keeps all of
its versions. Ordered stores have no versions, metadata or user IDs: their
reads return only the value, and `RemoveAsync` deletes the key for good.

- `GetAsync` returns the latest value and a `DataStoreKeyInfo`, or nil and
  nil when the key was never written or its latest version is deleted.
  `UpdateAsync`, `IncrementAsync`, `RemoveAsync`, `GetVersionAsync` and
  `GetVersionAtTimeAsync` return the same pair. `SetAsync` returns the new
  version's string.
- A key info's `Version` is its version's string, `UpdatedTime` when that
  version was written, `CreatedTime` when the key was first written
  (Inference), both in Unix milliseconds of the game's clock.
  `GetUserIds` and `GetMetadata` are what that version was written with.
- `SetAsync(key, value, userIds, options)` and `IncrementAsync` take user
  IDs (an array of whole numbers, at most 4: Inference from older docs) and the
  metadata of a `DataStoreSetOptions` or `DataStoreIncrementOptions`
  (`SetMetadata`); a write without them leaves the new version with none.
- `RemoveAsync` returns the latest value and its key info and appends a
  deleted version (a tombstone). A key already deleted returns nil and nil
  and gets no new version (Inference).
- `GetVersionAsync(key, version)` reads a version by its string, and
  `GetVersionAtTimeAsync(key, time)` the version current at a Unix time in
  milliseconds; both give nil for a deleted version. The time must be
  above 0 and at most ten minutes ahead, or the call raises `Timestamp must
  be positive and not more than ten minutes in the future.`
- `RemoveVersionAsync(key, version)` removes a version for good: reads and
  listings no longer find it. A string that names no version does nothing
  (Inference).
- Version strings take the shape of Roblox's: the time in .NET ticks as
  hex, the version's number in the key, the ticks again and `01`
  (Approximation: only the shape and their order are Roblox's).

## UpdateAsync

The transform gets the current value (nil for none) and its key info (nil
for none, and always for an ordered store), and returns the new value, then
user IDs and metadata, which replace the key's: returning neither clears
them. A nil value cancels the write, and `UpdateAsync` returns nil
(Inference). The transform may not yield: one that does is stopped and the
call raises `UpdateAsync's transform function can't yield` (Rook's
wording). An error in the transform is raised from `UpdateAsync` as it is.

## IncrementAsync

Adds `delta` (default 1) to an integer value, a missing key counting as 0;
a value that isn't an integer raises 103 (Inference). A fractional delta is
cut to its whole part, as Roblox passes it as an `int` (Inference). A sum
past a 64-bit integer raises and leaves the value as it was (Inference:
the docs give no limit).

## Pages

Listings return `Pages` objects that read one page at a time from the live
stores, continuing from where the last page ended.

| Method | Pages | Items | Default page size |
|---|---|---|---|
| `OrderedDataStore:GetSortedAsync(ascending, pageSize, min, max)` | `DataStorePages` | tables of `key` and `value` | none: 1 to 100 |
| `DataStore:ListKeysAsync(prefix, pageSize, cursor, excludeDeleted)` | `DataStoreKeyPages` | `DataStoreKey` (`KeyName`) | 50 |
| `DataStore:ListVersionsAsync(key, sortDirection, minDate, maxDate, pageSize)` | `DataStoreVersionPages` | `DataStoreObjectVersionInfo` | 1024 |
| `DataStoreService:ListDataStoresAsync(prefix, pageSize, cursor)` | `DataStoreListingPages` | `DataStoreInfo` | 32 |

- `IsFinished` is true on the last page, and `AdvanceToNextPageAsync`
  there raises `No pages to advance to.` Advancing is a request of the
  method the pages came from, and spends its budget.
- `GetCurrentPage` returns a new table each call, with new item objects
  (Inference).
- `GetSortedAsync`'s bounds are integers or nil (`106: MinValue must be an
  integer.`, `106: MaxValue must be an integer.`), the maximum at least
  the minimum (`107: MaxValue must be greater than or equal to
  MinValue.`), and a page size outside 1 to 100 raises `106: PageSize must
  be within a predefined range.` Entries with the same value come in key
  order, reversed when descending (Inference).
- `ListKeysAsync` lists keys in key order (Inference), deleted ones too
  unless `excludeDeleted`; then it checks at most 512 keys a page, so a
  page can come back empty with more to come.
- `ListVersionsAsync` lists oldest first, or newest first for
  `Enum.SortDirection.Descending`, from `minDate` to `maxDate` (Unix
  milliseconds, inclusive: Inference; 0 for no bound), deleted versions
  included with `IsDeleted`.
- `ListDataStoresAsync` lists the stores holding a key, by name (Inference:
  standard stores only).
- `DataStoreKeyPages.Cursor` and `DataStoreListingPages.Cursor` are where
  the next page starts, empty on the last page; passed back as the
  `cursor` argument they continue from there. Rook's cursor is the last key
  or store name read (Approximation: Roblox's are opaque strings).

## The read cache

A key is cached for 4 seconds after a read, and `SetAsync` and
`UpdateAsync` cache it again from their write. A `GetAsync` of a cached key,
unless its `DataStoreGetOptions.UseCache` is false, is answered without a
request and spends no budget (Inference from "without a backend request").
With one server, the cache always holds the stored value, so only budgets
show it.

## Budgets

Each kind of request spends from a budget that refills over each minute,
by the number of players in the server (the docs' server limits):

| Budget | Requests | Per minute |
|---|---|---|
| `StandardRead` | `GetAsync`, `GetVersionAsync`, `GetVersionAtTimeAsync`, `UpdateAsync` | 60 + 40 per player |
| `StandardWrite` | `SetAsync`, `IncrementAsync`, `UpdateAsync` | 60 + 40 per player |
| `StandardList` | `ListDataStoresAsync`, `ListKeysAsync`, `ListVersionsAsync` | 5 + 2 per player |
| `StandardRemove` | `RemoveAsync` | 60 + 40 per player |
| `RemoveVersionAsync` | `RemoveVersionAsync` | 5 + 2 per player |
| `OrderedRead` | an ordered store's `GetAsync`, `BatchGetAsync` (one per key), `UpdateAsync` | 60 + 40 per player |
| `OrderedWrite` | an ordered store's `SetAsync`, `IncrementAsync`, `UpdateAsync` | 30 + 5 per player |
| `OrderedList` | `GetSortedAsync` | 5 + 2 per player |
| `OrderedRemove` | an ordered store's `RemoveAsync` | 30 + 5 per player |

- `UpdateAsync` spends from both its read and its write budget.
- `GetRequestBudgetForRequestType` gives the whole requests a budget can
  make now. `OrderedList` gives 0, as its enum item's docs say. The legacy
  request types give the budget now covering the functions they name
  (Inference): `GetAsync` and `GetVersionAsync` `StandardRead`,
  `SetIncrementAsync` `StandardWrite`, `UpdateAsync` the smaller of those
  two, `GetSortedAsync` the `OrderedList` budget, `SetIncrementSortedAsync`
  `OrderedWrite`, `ListAsync` `StandardList`, `RemoveVersionAsync` its
  own, and `OnUpdate` one of 30 + 5 per player (Inference from older docs).
- `SetRateLimitForRequestType(type, base, perPlayer)` sets a budget's
  limit, within the docs' constraints table (0 to 10000 and 0 to 200 for
  the modern types; the legacy types' own ranges); `UpdateAsync` and
  `OnUpdate` can't be set. Out-of-range limits raise an error (Inference,
  Rook's wording).
- Approximation: the docs give servers a one-time startup burst and don't
  say how big, nor how much budget can build up; Rook's budgets start
  with, and hold at most, one minute's worth.

When a budget is spent, the request waits in one of four queues (get, set,
ordered get, ordered set) and runs, in the order queued, once its budget
refills; the call yields until then, and warns `DataStore request was added
to queue. If request queue fills, further requests will be dropped. Try
sending fewer requests.Key = <key>` (Inference: the warning scripts report;
without `Key = …` for requests without a key). A queue holds 30 requests;
another is dropped with `301: GetAsync request dropped. Request was
throttled but queue was full.` (302 `SetAsync`, 303 `IncrementAsync`, 304
`UpdateAsync`, 305 `GetSorted`, 306 `RemoveAsync`), or for the methods
without a number `<Method> request dropped. Request was throttled.`

## Errors

The numbered errors read `<code>: <message>`; the docs give the messages
and the prefix is Inference. Beyond those above:

| Error | Raised for |
|---|---|
| `101: Key name can't be empty.` | An empty key |
| `102: Key name exceeds the 50 character limit.` | A key over 50 bytes |
| `511: Metadata attribute size exceeds 300 limit.` | Metadata over 300 bytes as JSON, or a key over 50 or a value over 250 (the docs' metadata limits; X's wording is Inference) |
| `512: UserID size exceeds 4 limit.` | More than 4 user IDs |
| `513: Attribute userId format is invalid.` | A user ID that isn't a number |
| `513: Attribute metadata format is invalid.` | Metadata that isn't a table of string keys and storable values |
| `DataStore name can't be empty.`, `DataStore name can't be longer than 50 characters.`, and the same for `scope` | Names and scopes (Inference) |
| `BatchGetAsync is only supported on OrderedDataStore.` | `BatchGetAsync` on a standard store (the docs say it errors; Rook's wording) |

## Not modeled

- Per-key throughput (25 MB read and 4 MB written per key a minute) and
  the experience-wide limits, which take many servers to reach.
- `GlobalDataStore:OnUpdate` (deprecated) is not implemented.
- Versions are kept for the run; Roblox deletes deleted versions after 30
  days.

## What needs a Studio check

Data store requests in Studio need API access turned on and write to a real
experience's stores, so none of this has been recorded. Worth measuring,
in a place made for it:

- The exact messages: 103, 104 (and which table X names), 105, 511 and
  512, the name and scope errors, the client error, the `AllScopes` and
  `BatchGetAsync` errors, out-of-range rate limits, and a yielding
  transform.
- Whether NaN and infinities are refused, and what a nil write raises.
- `GetGlobalDataStore`'s class and name; what an ordered store's
  `SetAsync` returns.
- `UpdateAsync`'s results when its transform returns nil; `RemoveAsync`
  on a deleted key; `RemoveVersionAsync` with an unknown version.
- Key info times after a key is removed and written again.
- The budgets a fresh server reports, how far they build up, the startup
  burst, how fast they refill, and the legacy request types' answers.
- Whether a cached read spends budget.
- Version strings and cursors as Studio shows them.
