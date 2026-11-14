# LazyBuddy module API

Status: API v1, Windows x64. Business and decoder source belongs to independent repositories [news-service](https://github.com/LazyBuddyDev/news-service) and [hongguo-service](https://github.com/LazyBuddyDev/hongguo-service). Neither depends on Iced or desktop implementation. This host repository owns presentation templates, lifecycle and API clients, not service code. Own code is MIT, copyright 2026 Leager; third-party code retains its original licenses. Binary release and signing are separate publication steps.

This document is shared by the host and service repositories. Service-specific paths refer to the owning service repository. Each service's root module.json and README are its own build and package entrypoints.

## Package and process

`module.json` uses schema 1, `service_api: 1`, an immutable three-part package version, and target `windows-x86_64`. IDs use lowercase letters, digits and hyphens, beginning with a letter. Entrypoints are relative paths inside the package. Service feature manifests live at the root of each service repository. Python/media runtime manifests belong to hongguo-service/contracts. The host keeps only a generic fixture manifest under contracts/fixtures.

Feature packages declare a page template. `news_grid` declares `catalog_method` and `fetch_method`; `media_library` declares `search_method` and `favorites_method`. Template v1 includes the additional settings/account/media methods described below; custom services must implement the complete template they select. These templates provide presentation components, not arbitrary HTML or scripts.

Runtime packages have `kind: runtime` and no page. Dependencies pin exact package versions. The optional `runtime` field selects an interpreter dependency; the host runs that dependency's entrypoint/arguments followed by the feature entrypoint/arguments. Dependency directories are supplied as `LAZYBUDDY_DEPENDENCY_<UPPERCASE_ID_WITH_UNDERSCORES>_DIR`. Code and runtime files remain inside their own package directories; data goes under `LAZYBUDDY_DATA_DIR`.

The service reads one UTF-8 JSON object per stdin line and writes one reply per stdout line. stdout must contain no logs. Requests use `{"version":1,"id":"request-id","method":"health","args":{}}`; replies preserve `version`/`id` and contain either `ok:true,data` or `ok:false,error:{code}`. Clients must handle unknown error codes. Closing stdin requests shutdown; `lifecycle.suspend` lets the Hongguo service complete an already submitted transaction after stopping media. The desktop owns each service process tree and rejects replies from an old activation.

The host serializes transport requests. Expensive account work returns `operation:"pending"`; clients poll `account.status` for completion rather than blocking the transport. `accepted`, `pending`, confirmed account state, and actual decoder state have different meanings. An interrupted server write may still succeed upstream. Do not automatically resubmit it.

## News grid

| Method | Arguments | Result data |
| --- | --- | --- |
| `health` | none | `service`, `api_version` |
| `sources` | none | array of `id`, `name`, `category`, `subtitle` |
| `fetch` | `sources`: 1–4 source IDs | array of `id`, `ok`, successful `stories` or source `error` |
| `state` | none | `preferences`, successful `cache` keyed by source ID |
| `preferences` | none | `preferences` only; no content update |
| `configure` | complete `preferences` object | committed `preferences` |

Stories contain `title`, `url`, `badge`. Successful fetch rows include `updated` (Unix seconds) and `updated_ms` (Unix milliseconds), matching the saved cache. Failed refreshes preserve previous successful stories. Preferences include `auto_refresh`, `refresh_minutes` (1–60), `followed`, `hidden`, `order`, `default_followed`. ID arrays contain known unique sources; `order` specifies sources pinned ahead of the remaining catalog.

The service has no refresh timer. The client schedules at most one four-source batch, stops scheduling when hidden or inactive, and restores only interrupted visible sources. For an independent HTTP consumer, build the optional adapter documented in the news-service README; bearer authentication, loopback-only binding, request limits and Origin rejection apply.

## Media library

| Method | Arguments | Result data |
| --- | --- | --- |
| `search` | `query`, `page` | `items`, `page`, `has_more` |
| `account.restore/status/login/cancel/logout` | none | `state`, `name`, `qr_png`, account errors and operation fields |
| `favorites.read` | none | pending operation, then confirmed `favorites` |
| `favorites.set` | decimal-string `series`, boolean `favorite` | pending operation, then official read-back list or `favorites_error` |
| `progress.read` | none | account-isolated `history`, sync status |
| `progress.sync` | none | pending operation, then `history`, `sync_state`, `sync_error` |
| `cover.read` | decimal-string `series` | `series`, `width`, `height`, Base64 `rgba` |

List items use string `id`, `title`, total `episodes`, `has_cover`. `qr_png` is Base64 PNG; it is empty outside the QR stages. States include `idle`, `preparing`, `pending`, `scanned`, `verifying`, `signed_in`, `failed`, `cancelled`. Login is complete only after identity validation and secure persistence. Cancel does not delete saved credentials; logout does. Raw account IDs, cookies, device profiles, signed URLs and content keys are never public fields.

Async results identify `operation_method`. Poll while `operation == "pending"`; completion is delivered by `account.status`. `progress.read` remains available during an account transaction; other account operations may report `account_busy`. Identity restoration reads the local snapshot; the visible client explicitly requests cloud synchronization. Manual cloud synchronization during loading/playback reports `player_busy`. History is keyed by string series ID and contains `episode`, `time` in seconds and `updated` in Unix seconds. Cloud merge uses update time, including a newer rewind. Only an explicitly confirmed upload is marked synchronized internally.

Cover pixels are RGBA, at most 240×360, decoded in the service. Clients must bound Base64 length and validate dimensions and byte count before allocating. The service accepts a known series ID, not arbitrary image URLs.

## Media control and frames

`media.open` accepts `series`, `episode`, `quality`, a private D3D12 `pipe`, resume `position` in seconds, `volume` (0–1), `mute`, and `speed`. It returns `loading` promptly; poll `media.status` until `loaded` or `error`. A new open cancels the old source process. Identity, catalog and per-episode permissions are verified independently. A client must not infer permission for every episode from one playable episode.

Other methods are `media.play`, `media.pause`, `media.close`, `media.seek` (`value` in seconds), `media.speed` (`value`), `media.volume` (`value`), `media.mute` (boolean `value`) and `media.boost` (boolean `value`, temporary 2×). Status distinguishes selected `speed`, actual `actual_speed`, `boosted`, `position`, `duration`, `playing`, `loaded`, `loading`, `eos`, available episodes/qualities and actual decoded dimensions. `width`/`height` describe the input model; `decoded_width`/`decoded_height` come from the negotiated video output pad and remain null until available. `producer_pid` identifies the owned decoder process for the IPC consumer identity check; it is not the account service PID. `media.close` releases the source and decoder; it preserves the account's durable viewing record. EOS is a real decoder event, not a duration heuristic.

The optional media runtime provides GStreamer 1.26.11 and a JSON control worker. Frames use that version's D3D12 IPC protocol. Consumers wait for producer fences, import/copy into an owned GPU texture, and acknowledge only after GPU completion. The reference consumer is `crates/media-ipc`, independent of the Hongguo business service. Its Iced view is optional. This is a Windows GPU interface, not a language-neutral video byte stream; other clients can use the API without playback, or implement the matching consumer. See `docs/media-runtime.md` for the verified scope.

Hidden/capsule windows pause the established decoder without fetching a new source. Leaving the root page or opening settings releases media. Returning never autoplays. No inactive module may poll, refresh, fetch covers, preload or start a timer. An already submitted final progress transaction may finish its current confirmation, without spawning another round.

## Distribution

Packages are ZIPs with `module.json` at their root. The catalog records exact dependencies, HTTPS URL, byte size and SHA-256. `catalog.json.sig` is the raw 64-byte Ed25519 signature of the exact catalog bytes. The base package carries a separately trusted `module-catalog.json` with HTTPS `url` and a 32-byte public-key array. Signatures and hashes are verified before executing a download. Developer `install-local` is explicit and does not replace a failed signed download.

Installation stages the complete dependency closure, then atomically changes one registry. Versions are immutable. Removal recycles unreferenced runtime dependencies. CLI mutations are refused while a host is running; use its module manager or quit normally first. No external service source repository, key, signing identity or public release is created implicitly by exporting source or building a local development package.
