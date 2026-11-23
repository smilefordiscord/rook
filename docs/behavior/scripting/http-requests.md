# HTTP requests

Checked against Studio 0.748 on 2026-11-22, from its creator docs only: no rule here is measured in Studio yet.

How HttpService's `GetAsync`, `PostAsync` and `RequestAsync` send a
script's requests to any site (`rook_lua`'s `api::methods::web::http`).
The engine sends them through the host's network
(`rook_platform::network`), each on a thread of its own, while the
calling script waits, as for Roblox's web APIs ([web
endpoints](../../design/web-endpoints.md)). Where they may go is checked
there too ([where a request may go](#where-a-request-may-go)). Sources are the creator docs'
[HttpService](https://create.roblox.com/docs/reference/engine/classes/HttpService),
[In-game HTTP requests](https://create.roblox.com/docs/cloud-services/http-service),
`Enum.HttpError` and `Enum.HttpCompression`.

## When a request is sent

- Only a server sends one. A client's call raises `Http requests can only
  be executed by game server`. *Confirmed* that only servers send them
  (creator docs); *Inference:* the message, as players report it.
- Only when the place's saved `HttpService.HttpEnabled` is on, which
  Studio's **Allow HTTP Requests** game setting turns on. Off, a call
  raises `Http requests are not enabled. Enable via game settings`.
  *Confirmed* that it's off by default and that game scripts can't
  change it (its write security is `LocalUserSecurity`); *Inference:*
  the message.
- The URL must be `http` or `https` (*Confirmed*, creator docs
  `RequestAsync`); any other, or none, raises `HttpError: InvalidUrl`
  (*Inference*).
- A URL on Roblox's own web domains (`roblox.com` and its subdomains)
  raises `HttpService is not allowed to access ROBLOX resources`
  (*Inference:* the rule and message as players report them). Roblox lets
  a subset of Open Cloud's endpoints on `apis.roblox.com` through with a
  Secret API key; Rook has no Secrets, so those are refused too
  ([stubs](../../guide/stubs.md)).
- A server sends at most 500 requests a minute (*Confirmed*, creator
  docs `RequestAsync`); the next raises `Number of requests exceeded
  limit` until the oldest is a minute old. *Inference:* the minute is
  the last 60 seconds of game time, and the message is the one the
  creator docs give for the Open Cloud limit, without "Open Cloud".

The calling thread waits for the answer, as every web call does in Rook,
while the frame loop carries on: no host's frame (the player's, a
server's, a headless run's) ever waits for one, so a slow or silent
server holds up only the script that asked. The answer resumes it on the
first frame after it arrives.

## Where a request may go

- Only to addresses on the public internet. A host whose name resolves
  only to addresses off it raises `HttpError: ConnectFail`: unspecified
  (`0.0.0.0`, `::`), loopback (`localhost`, `127.0.0.1`, `::1`), private
  (RFC 1918, `fc00::/7`), link-local (`169.254.0.0/16`, the cloud
  metadata address included, and `fe80::/10`), shared (CGNAT,
  `100.64.0.0/10`), multicast, broadcast, and the other ranges IANA keeps
  off the internet (documentation, benchmarking, reserved). An IPv4
  address inside IPv6 (`::ffff:127.0.0.1`, NAT64, 6to4) counts as
  itself. *Inference:* Roblox's game servers reach only the internet,
  so connecting to any of these fails; the creator docs don't say.
- The check is on the addresses a name resolves to, not on the name,
  and the connection is made only to an address that passed, so a name
  that resolves to a local address, or changes what it resolves to
  between a check and the connection, can't get round it. A name with
  both kinds connects to its public ones.
- Every redirect is checked as the first URL is: one to Roblox's web
  domains raises `HttpService is not allowed to access ROBLOX resources`,
  one to a local address `HttpError: ConnectFail` (*Inference*).
- Requests go straight to the host, never through a proxy the
  environment names, which would resolve the name out of the check's
  sight.
- *Rook extension:* the developer setting `httpLocalAddresses`
  ([run-time settings](../../design/data-and-logging.md#run-time-settings)),
  off by default, lets requests reach local addresses, so a place can
  talk to a backend of its developer's own on the same machine or the
  LAN. Roblox's own web domains stay refused.

## What is sent

- `GetAsync(url, nocache, headers)` sends `GET`. *Approximation:*
  `nocache` changes nothing, as Rook keeps no answers to reuse: every
  call is sent.
- `PostAsync(url, data, content_type, compress, headers)` sends `POST`
  with `data` as the body and `Content-Type` from `content_type`
  (`ApplicationJson` by default): `application/json`,
  `application/x-www-form-urlencoded` for `ApplicationUrlEncoded`,
  `application/xml`, `text/plain` and `text/xml` for the others
  (*Confirmed*, creator docs `RequestAsync`). A header in `headers`
  replaces it.
- `RequestAsync(options)` sends `options.Method` (any case, `GET` when
  left out) to `options.Url`, with `options.Headers` and `options.Body`.
  A `Body` with `GET` or `HEAD` raises `<method> requests can't have a
  Body` (*Confirmed* that those take none; *Inference:* that it raises,
  and the message). `Timeout` is in seconds, above 0 and at most the
  default limit, or it raises `Timeout must be greater than 0 and at
  most 60` (*Confirmed* range; *Inference:* the message).
  *Approximation:* the default limit is 60 seconds, Rook's own, as
  Roblox's isn't documented.
- `compress` true, or `Compress = Enum.HttpCompression.Gzip`, sends the
  body gzipped (*Confirmed*) with `Content-Encoding: gzip`
  (*Inference*).
- `User-Agent` and `Roblox-Id` are Roblox's own and `Content-Length`
  comes from the body (*Confirmed*, creator docs `RequestAsync`), so a
  script's values for them are dropped (*Inference:* rather than
  raised). *Approximation:* Rook sends its own `User-Agent`
  (`Rook/<version>`) and no `Roblox-Id`, as its places have no id on
  Roblox.
- Redirects are followed, up to 10 (*Approximation:* Roblox's count isn't
  documented), each checked ([where a request may go](#where-a-request-may-go)).
  Answers come decompressed; one over 16 MiB raises `HttpError:
  OutOfMemory` rather than arriving cut short (*Inference:* the enum's
  item for running out of memory on the answer; *Approximation:* Roblox's
  limit isn't documented).

## What comes back

- `GetAsync` and `PostAsync` return the body of a 2xx answer. Any other
  status raises `HTTP <code> (<reason>)`, as `HTTP 404 (Not Found)`, or
  `HTTP <code>` for a code with no standard reason (*Inference:* the
  message as players report it).
- `RequestAsync` returns a table whatever the status (*Confirmed*,
  creator docs):
  - `Success`: true for a 2xx status.
  - `StatusCode`: the status.
  - `StatusMessage`: the status's standard reason phrase (`Not Found`),
    whatever phrase the server wrote. *Inference:* HTTP/2 sends no
    phrase, so Roblox can only have the standard one there.
  - `Headers`: the answer's headers by lowercase name (HTTP/2 sends them
    so, creator docs), a header sent more than once as its values
    joined with `, ` (*Inference*).
  - `Body`: the body, as bytes.
- A request that gets no answer raises `HttpError: ` and the
  `Enum.HttpError` item for why (*Inference*):

  | Why | Raises |
  |---|---|
  | The URL can't be parsed | `HttpError: InvalidUrl` |
  | A header or the method has characters HTTP doesn't allow | `HttpError: Unknown` |
  | The host's name doesn't resolve | `HttpError: DnsResolve` |
  | Nothing takes the connection, or the host is local ([where a request may go](#where-a-request-may-go)) | `HttpError: ConnectFail` |
  | No answer within the time limit | `HttpError: TimedOut` |
  | Over 10 redirects | `HttpError: TooManyRedirects` |
  | A redirect without a usable `Location` | `HttpError: InvalidRedirect` |
  | TLS fails, the certificate included | `HttpError: SslConnectFail` |
  | The server closes the connection mid-answer | `HttpError: ConnectionClosed` |
  | The answer is over 16 MiB | `HttpError: OutOfMemory` |
  | Anything else | `HttpError: NetFail` |

## Not yet

Every message tagged *Inference* above, and whether Roblox raises for a
locked header or a `Body` with `GET`, wait for a Studio check of a place
with HTTP enabled. So do how `nocache` caches, whether a server reached
with HTTP/1.1 gives its own reason phrase or header case, and the
creator docs' port rule (none below 1024 but 80 and 443, nor 1194),
which Rook doesn't apply.
