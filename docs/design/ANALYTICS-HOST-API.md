# Analytics host API for W4

This note records the host wire shapes affected by W3. Tauri invoke argument names use camelCase; JSON reply fields and event payloads use the names shown below.

## Changed command replies

### `analytics_knowledge_get`

Request:

```json
{"matchId":"12345"}
```

Reply keeps the existing wrapper and adds an on-screen-only `personal_averages` projection. `digest` contains match facts and has no `lanes[].personal_average`, `meta.result.own_side`, or `meta.result.own_result`. `viewer` carries the selected own hero and the session-only `own_side` / `own_result` values. The wrapper fields are:

```json
{
  "digest": {"knowledge_version": 4, "meta": {"result": {"winner": "radiant"}}, "lanes": []},
  "bundle_hash": "<sha256>",
  "bundle_available": true,
  "own_hero_id": "npc_dota_hero_axe",
  "viewer": {"own_hero": "npc_dota_hero_axe", "own_side": "radiant", "own_result": "win"},
  "personal_averages": {
    "top": null,
    "mid": {"value": 4.5, "n": 6, "label": "ค่าเฉลี่ยของคุณ", "source": "ประวัติของคุณในเครื่อง", "trial_label": "ทดลอง · เกณฑ์ชั่วคราว"},
    "bot": null
  },
  "notes": [],
  "reports": [],
  "chat_history_estimated_tokens": 0,
  "chat_history_token_budget": 12000,
  "chat_history_over_budget": false,
  "provisional_threshold_label": "เกณฑ์ชั่วคราว",
  "report_cost_estimate_usd": null,
  "report_cost_reason": "provider token pricing is not verified in this build"
}
```

The host builds provider snapshots from `digest` plus the session block. It never includes `personal_averages` in a provider request. Old cached digests are scrubbed before use. An old AI report is not reused in provider context unless both prompt and knowledge versions match the current versions and the report is not stale.

### `analytics_chat_submit`

Request and immediate reply are unchanged:

```json
{"matchId":"12345","question":"Why did mid fall behind?","selectedHero":null,"scrubberTimeS":null}
```

```json
"<request-id>"
```

Earlier turns remain in the local transcript, including tool calls and tool results. Provider request snapshots retain their text turns and remove earlier `tool_use` / `tool_result` or function-call pairs. Tool blocks from the active request remain until that request finishes. The history-token budget is applied to this projected snapshot.

### `match_history_clear`

Request has no arguments. Reply remains:

```json
{"cleared":true}
```

In addition to local and OpenDota history, the host deletes lane rows, calibration output, pending lane transaction data, and the separate lane de-duplication token file.
The lane rows use schema 2. A valid legacy schema-1 lane store is atomically cleared once on its first read or write; later schema-2 rows are retained. The separate `lane-history-seen.json` file stores bounded HMAC-SHA-256 tokens of bundle hashes keyed by the installation key, never the raw bundle hash.

## New commands

### `analytics_match_data_delete`

Supply exactly one target. Use `matchId` for a match (the host derives its keyed analytics folder), or `orphanKey` returned by `analytics_orphans_list` for a folder with no library entry.

```json
{"matchId":"12345"}
```

```json
{"orphanKey":"<64 lowercase hex characters>"}
```

Reply:

```json
{"deleted":true}
```

`deleted` is false when the requested analytics folder is already absent. Invalid target combinations, match IDs or folder keys are rejected with a Tauri command error. The host accepts only validated match keys and deletes one direct child of the app-data `analytics` directory. It does not accept a path and does not touch the replay library or replay bundles.

### `analytics_orphans_list`

Request has no arguments. Reply contains opaque keys only; it does not return match IDs or paths.

```json
{"orphan_keys":["<64 lowercase hex characters>"]}
```

Only direct, valid keyed directories without a `ReplayLibrary` entry are returned. Malformed names and paths that do not resolve directly inside the analytics directory are excluded.

## Analytics events

### `analytics://tool-activity`

Emitted before and after each tool execution. `status` is `started`, `completed`, or `failed`; `tool` is an allowlisted tool name or `unknown`. Inputs, outputs, and tool IDs are not emitted.

```json
{"request_id":"<request-id>","tool":"get_deaths","status":"started"}
```

### `analytics://complete`

Cancellation completions may include a stable `reason_code`; a match barrier uses `match_phase_started`. The same request can still include the existing `error`, usage, and byte-count fields.

```json
{"request_id":"<request-id>","error":"Request cancelled","reason_code":"match_phase_started"}
```

## Chart text sent in RULES

`kohbor-chart` accepts `type`, optional `title` / `unit`, and inline named series. Bar charts use `categories` and `values`; line charts use `points` with numeric `x` and numeric-or-null `y`. Host RULES include one example of each. They no longer ask the model to use chart references because the current host path does not resolve them.

```kohbor-chart
{"type":"bar","categories":["Radiant","Dire"],"series":[{"name":"Hero kills","values":[12,9]}]}
```

```kohbor-chart
{"type":"line","from":0,"to":600,"series":[{"name":"Axe","points":[{"x":0,"y":300},{"x":600,"y":4200}]}]}
```

