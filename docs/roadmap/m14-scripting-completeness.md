# M14 Scripting completeness

Driven by the corpus run (`reports/run/`): today 60.4% of places with
server scripts run 10 seconds without a script error. Many errors are the
scripts' own (old syntax modern Roblox rejects too, missing children);
the target counts only errors Rook causes, checked against Studio.

1. Classify the corpus run's errors: Rook's (unimplemented members,
   wrong behavior) against the scripts' own, with a sample of each
   checked in Studio.
2. The unimplemented members by places affected, starting with
   `InsertService:LoadAsset` (54 places) and `require(assetId)` (17).
3. Services that talk to Roblox's website, answered locally and
   offline: MarketplaceService, BadgeService, GroupService, Players'
   web lookups.
4. TeleportService between places in one folder (multi-place games).
5. HttpService, opt-in per place: [HTTP requests](../behavior/scripting/http-requests.md).
   - *Scope*: `GetAsync`, `PostAsync` and `RequestAsync` sent from the
     servers of places whose saved `HttpEnabled` is on, with Roblox's
     refusals (clients, HTTP off, URLs that aren't HTTP, Roblox's own
     domains), its limit per minute, `Compress`, `Timeout`, and its
     answers and errors. Also in: `nocache`'s caching, and Open Cloud
     through HttpService with `GetSecret` and the Secret datatype. Out:
     `CreateWebStreamClient`, which Roblox runs in Studio only, and a
     host's switch that keeps a public server's HTTP off (M20 step 3).
   - *Evidence*: the creator docs' HttpService, In-game HTTP requests,
     `Enum.HttpError` and `Enum.HttpCompression`. Nothing is measured in
     Studio yet: a place with HTTP enabled sending to a server on the same
     machine settles each Inference on the page.
   - *Design* (2026-11-22):
     - Established engines run a script's request off the main thread
       and hand the answer back on the main loop: Unity's
       `UnityWebRequest.SendWebRequest` completes an async operation
       the game polls each frame, Godot's `HTTPRequest` node emits
       `request_completed` from its process step. Rook already does this
       for Roblox's web APIs: a request waits in the World's outbox,
       the engine sends it on a thread through `rook_platform::network`,
       and the answer resumes the waiting script at the frame's web
       step. HttpService takes the same path, so a script waits on the
       network one way only, and headless runs still repeat exactly.
     - Failures are typed (`rook_platform::network::NetworkError`) by
       the kinds an HTTP client tells apart, which HttpService raises
       as their `Enum.HttpError` items.
     - The limit per minute is a queue of send times per DataModel,
       trimmed from its front as calls come.
     - Cost: nothing per frame while no request is in flight; each
       request holds a thread while it waits, as Roblox's web calls do
       in Rook, at most 500 a minute.
   - *Done when*: a conformance case recorded in Studio against a local
     server gets the same answers, statuses and errors in Rook, and
     `nocache`'s caching and Open Cloud with Secrets are done.
6. Modern Luau: Parallel Luau (Actor, `task.desynchronize`, SharedTable),
   `buffer`, and the libraries and syntax of the Luau live Roblox runs.
7. Client scripts in the corpus run too, once characters and GUI exist.
8. **Match Studio** in the script runtime M1 built: `BindableFunction`
   resuming as Studio does, `SignalBehavior.AncestryDeferred`'s own rules,
   the place datatype errors begin with, and `DataStoreService`'s versions,
   `ListKeysAsync`, budgets and `DataStorePages`.
   - *Evidence*: the [stubs](../guide/stubs.md) rows taken over by this
     step, and the open Approximations and Inferences
     on the scripting pages.
   - *Done when*: the [Match Studio](README.md#match-studio) bar holds
     for steps 1 to 7 and the runtime M1 built, and no stubs row names
     this step.