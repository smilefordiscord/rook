# HTTP requests

Checked against Studio 0.748 on 2026-11-23.

How HttpService's `GetAsync`, `PostAsync` and `RequestAsync` send a
script's requests to any site (`rook_lua`'s `api::methods::web::http`).
The engine sends them through the host's network
(`rook_platform::network`), each on a thread of its own, while the
calling script waits, as for Roblox's web APIs ([web
endpoints](../../design/web-endpoints.md)). Where they may go is checked
there too ([where a request may go](#where-a-request-may-go)). Sources are
`conformance/cases/scripting/http_requests.luau` and `client_http.luau`,
recorded in Studio 0.748 in a place with HTTP off, so they measure what
is refused before a request would go out; the rest comes from the creator
docs'
[HttpService](https://create.roblox.com/docs/reference/engine/classes/HttpService),
[In-game HTTP requests](https://create.roblox.com/docs/cloud-services/http-service),
`Enum.HttpError` and `Enum.HttpCompression`.

## The order of the checks

*Confirmed* (measured): a call is checked in this order, and the first
check that fails raises.

1. The arguments' casts: a missing or nil URL (or `PostAsync`'s missing
   `data`) raises `Argument <n> missing or nil`, a `nocache` or
   `compress` that isn't a boolean `Unable to cast string to bool`, a
   `content_type` that isn't an `Enum.HttpContentType` item, its number
   or its name `Unable to cast string to HttpContentType` (`Unable to
   cast Material to HttpContentType` for another enum's item), and
   `RequestAsync`'s options that aren't a table without a first item
   `Unable to cast string to Dictionary` (`Unable to cast Array to
   Dictionary` for a list).
2. Only a server sends requests: a client's call raises `Http requests
   can only be executed by game server`, even for a URL on Roblox's
   domains.
3. The URL, the options and the headers ([below](#what-is-checked)).
4. Roblox's own web domains ([where a request may go](#where-a-request-may-go)).
5. The place's saved `HttpService.HttpEnabled`, which Studio's **Allow
   HTTP Requests** game setting turns on. Off, a call raises `Http
   requests are not enabled. Enable via game settings`. *Confirmed* that
   it's off by default and that game scripts can't change it (its write
   security is `LocalUserSecurity`), and that every check above comes
   first. *Inference:* the message, as players report it: Studio's MCP,
   which records the cases, runs them as its Assistant, whose own HTTP
   permission (`Assistant HTTP permissions are not configured`) is what
   the recording shows at this point.
6. A server sends at most 500 requests a minute (*Confirmed*, creator
   docs `RequestAsync`); the next raises `Number of requests exceeded
   limit` until the oldest is a minute old. *Inference:* the minute is
   the last 60 seconds of game time, the message is the one the creator
   docs give for the Open Cloud limit, without "Open Cloud", and the
   limit comes after HttpEnabled.

## What is checked

All *Confirmed* (measured), with the messages as given.

- The URL is a string (Roblox also takes a Secret, which Rook doesn't
  have): a number, table or boolean raises `Invalid URL type, URL should
  be a string or secret.`; an empty one `Empty URL`; one that doesn't
  start `http://` or `https://`, in any case, `URL must be http` (`ftp:`,
  `file:`, no scheme at all). Anything else passes here, `http://` alone
  and a space in the host included, and is the network's to parse.
- `GetAsync` and `PostAsync`'s `headers` must be a table without a first
  item, or raise `headers value must be a dictionary!`; `RequestAsync`'s
  `Headers` likewise, or `Invalid request options, Headers should be a
  table`.
- Each header's value must be a string: `Header <name> must have its
  value be a string or secret!`. `User-Agent`, `Roblox-Id` and
  `Content-Length`, in any case, raise `Header "<name>" is not allowed!`,
  and so does `Content-Type` for `PostAsync`, whose `content_type` sets
  it. A space in a name raises `Header "<name>" has unallowed character "
  "`, a line break (`\n`, `\r`) in a value `Header <name> has unallowed
  character <c> in value <value>`. An empty name passes. *Inference:* the
  other characters outside HTTP's token set in a name raise as a space
  does, and a NUL in a value as a line break does.
- `PostAsync`'s `data` passes as a number or even a table, not only a
  string. *Approximation:* Rook sends a number as Luau writes it and
  anything but a string or number as no body, as what Roblox sends isn't
  measured.
- `RequestAsync`'s options take only `Url`, `Method`, `Headers`, `Body`,
  `Compress` and `Timeout`; any other key, a lowercase `url` included,
  raises `Unrecognized request option <key>`, found before a missing
  `Url`, which raises `Invalid request options`.
- `Method` is any HTTP token in any case (`get`, `PATCH`, `FOO`, even
  empty); one with a space raises `Invalid Http Method <method>`, and one
  that isn't a string `Invalid request options, Method should be a
  string`.
- `Body` must be a string, or raise `Invalid request options, Body should
  be a string`. Any `Body`, even an empty one, with `Method` `GET` or
  `HEAD` written in capitals raises `Invalid request options, Body should
  be empty for 'GET' or 'HEAD' method`; `get` passes. *Inference:* a
  `Body` without a `Method` raises as with `GET`.
- `Compress` passes whatever it is.
- `Timeout` must be a number, or raise `Timeout value must be an
  integer.`, above 0 and at most 120, or raise `Timeout value must be
  greater than 0 and no greater than 120 seconds.`

## Where a request may go

- Not to Roblox's own web domains (`roblox.com` and its subdomains, in
  any case, whatever the path or query holds): a URL there raises
  `HttpService is not allowed to access that Roblox resource`, and one on
  `apis.roblox.com` without an `x-api-key` header `Header "x-api-key"
  must be provided for Roblox resources!` (*Confirmed*, measured). Roblox
  lets a subset of Open Cloud's endpoints there through with a Secret API
  key; Rook has no Secrets, so those raise the first message
  ([stubs](../../guide/stubs.md)).
- Only to addresses on the public internet. A host whose name resolves
  only to addresses off it raises `HttpError: ConnectFail`: unspecified
  (`0.0.0.0`, `::`), loopback (`localhost`, `127.0.0.1`, `::1`), private
  (RFC 1918, `fc00::/7`), link-local (`169.254.0.0/16`, the cloud
  metadata address included, and `fe80::/10`), shared (CGNAT,
  `100.64.0.0/10`), multicast, broadcast, and the other ranges IANA keeps
  off the internet (documentation, benchmarking, reserved). An IPv4
  address inside IPv6 (`::ffff:127.0.0.1`, NAT64, 6to4) counts as
  itself. *Confirmed* (measured) that local addresses and names that
  don't resolve pass every check before HttpEnabled; *Inference:*
  Roblox's game servers reach only the internet, so connecting to any of
  these fails; the creator docs don't say.
- The check is on the addresses a name resolves to, not on the name,
  and the connection is made only to an address that passed, so a name
  that resolves to a local address, or changes what it resolves to
  between a check and the connection, can't get round it. A name with
  both kinds connects to its public ones.
- Every redirect is checked as the first URL is: one to Roblox's web
  domains raises `HttpService is not allowed to access that Roblox
  resource`, one to a local address `HttpError: ConnectFail`
  (*Inference*).
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
  (*Confirmed*, creator docs `RequestAsync`).
- `RequestAsync(options)` sends `options.Method` (any case, `GET` when
  left out) to `options.Url`, with `options.Headers` and `options.Body`.
  *Approximation:* Rook sends the method in capitals, and an empty one as
  `GET`. `Timeout` is the most the request may take, in seconds.
  *Approximation:* without one the limit is 60 seconds, Rook's own, as
  Roblox's isn't documented.
- `compress` true, or `Compress = Enum.HttpCompression.Gzip`, sends the
  body gzipped (*Confirmed*) with `Content-Encoding: gzip`
  (*Inference*).
- *Approximation:* Rook sends its own `User-Agent` (`Rook/<version>`) and
  no `Roblox-Id`, as its places have no id on Roblox.
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

The recording had HTTP off, so everything past HttpEnabled waits for a
Studio check of a place with HTTP enabled: its message, every message
tagged *Inference* above, what a request that passed the checks does
(local addresses, answers, statuses, redirects, `HttpError` items, the
limit per minute), how `nocache` caches, what `PostAsync` sends for a
table, how a lowercase method or an empty one is sent, Roblox's default
time limit, whether a server reached with HTTP/1.1 gives its own reason
phrase or header case, and the creator docs' port rule (none below 1024
but 80 and 443, nor 1194), which Rook doesn't apply.
