# Observing and controlling Claude Code CLI sessions

How Dandelion (same Linux user `dev`) can observe and control interactive
`claude` sessions. Verified read-only on 2026-11-22: Claude Code 2.1.330 (one
session on 2.1.329), Linux 6.17, herdr / `herdr-core`. "From the binary" means
read from the bundled JS in `~/.local/share/claude/versions/<ver>`: internal,
not a documented API.

## 1. PID -> session id, cwd, transcript

### 1a. Session registry: `~/.claude/sessions/<pid>.json` (primary source)

Claude Code writes one JSON file per live process and rewrites it on every
status change.

```
$ cat ~/.claude/sessions/2157410.json        # trimmed
{
 "pid": 2157410,
 "sessionId": "<session-id>",
 "cwd": "/home/dev/dandelion",
 "startedAt": 1795363351158,
 "procStart": "9056156",                    # /proc/<pid>/stat starttime, guards PID reuse
 "version": "2.1.330",
 "kind": "interactive",                     # interactive | bg | daemon | daemon-worker
 "entrypoint": "cli",
 "messagingSocketPath": "/run/user/1000/cc-socks/2157410.sock",
 "name": "dandelion-a6",
 "agent": "<agent-name>",
 "status": "busy",                          # busy | shell | idle | waiting
 "updatedAt": 1795372085544,
 "statusUpdatedAt": 1795372085544
}
```

The same directory also holds `<pid>.<hash>.key` (mode 0600) with the fields
`peerToken`, `procStart` and `pidDomain`. That is the credential for the
peer-messaging socket (see section 5). Do not read or copy it.

Reliability: high for discovery. The file can outlive a crashed process, so
check that `/proc/<pid>` exists and that its starttime (field 22 of
`/proc/<pid>/stat`) equals `procStart`. The schema is internal: the binary
validates `kind` and `status` against fixed lists, and optional fields
(`waitingFor`, `state`, `detail`, `logPath`, `bridgeSessionId`, ...) are only
written when set.

### 1b. Process-level cross-checks

```
$ ps -eo pid,ppid,tty,args | grep [c]laude
1081982    2805 pts/1  claude --agent <agent-name> --resume <session-id>
2157410 2042408 pts/6  claude --agent <agent-name>
$ readlink /proc/2157410/cwd     -> /home/dev/dandelion
$ readlink /proc/2157410/exe     -> /home/dev/.local/share/claude/versions/2.1.330
$ readlink /proc/2157410/fd/0    -> /dev/pts/6
```

- `argv` has `--resume <id>` only for resumed sessions.
- `/proc/<pid>/cwd` is the launch cwd; transcript lines and hook input carry the
  current `cwd`. `/proc/<pid>/exe` gives the exact running version.
- Open fds: the `.jsonl` is not held open (appended per write). The only
  session-specific fd is `/tmp/claude-1000/<slug>/<sessionId>/tasks`.
- Environment: no Claude-specific variables; under herdr `HC_TERMINAL_ID=t31`,
  `HC_TERMINAL_SOCKET=/run/user/1000/herdr-core/default.sock`, `HC_SCOPE`.

### 1c. Transcript path

`~/.claude/projects/<slug>/<sessionId>.jsonl`, where `<slug>` is the cwd with
every non-alphanumeric character replaced by `-`. Examples:
`/home/dev/dandelion` -> `-home-dev-dandelion`, and
`/home/dev/.herdr/worktrees/x` -> `-home-dev--herdr-worktrees-x`.

```
$ ls -la ~/.claude/projects/-home-dev-dandelion/
-rw------- 766526 Nov 22 20:28 <session-id>.jsonl
drwxrwxr-x        Nov 22 20:28 <session-id>/subagents/agent-<id>.jsonl (+ .meta.json)
drwxrwxr-x                     .../tool-results/
```

Most robust: glob `~/.claude/projects/*/<sessionId>.jsonl` (a resumed session
keeps its original project dir). Hooks provide `transcript_path` directly.

### 1d. herdr-core view (outside Claude)

```
$ herdr-core get t31    # trimmed
{"address":"t31","foreground":{"argv":["claude","--agent",...],"cwd":"/home/dev/dandelion","pid":2157410},
 "status":{"app":"claude-code","state":"working","msg":"<spinner text>"},"status_ms":1795372085544, ...}
```

`status_ms` matches `statusUpdatedAt` in the session file exactly, so herdr-core
probably derives its status from `~/.claude/sessions` (this is inferred).

## 2. Session state (working / idle / waiting)

### 2a. Session file `status` + `waitingFor` (best passive source)

From the binary (2.1.330):
- `status` is one of `busy`, `shell` (idle, shell command running), `idle`,
  `waiting` (SDK mapping: `running -> busy`, `requires_action -> waiting`).
- When `waiting`, `waitingFor` is `"permission prompt"` for a tool permission
  dialog, `"input needed"` for AskUserQuestion-style or `dialog:*` prompts, or
  `"dialog open"` for other modal dialogs.
- Written on change: watch `~/.claude/sessions/` with inotify (or poll).

Observed: both live sessions `"busy"`; `waiting`/`idle` not exercised (read-only).

### 2b. Transcript tail

JSONL. Top-level `type` values in a live 12k-line file include `assistant`,
`user`, `attachment`, `system` (subtypes `turn_duration`, `compact_boundary`,
`local_command`, ...), `queue-operation`, `cost-state`, `mode`,
`permission-mode`, `agent-setting`, `api-request`, `data-flow-records`,
`file-history-*`, `pr-link`.

Heuristics (not authoritative): a last `assistant` with `stop_reason ==
"tool_use"` and no `tool_result` means a tool is running OR a permission prompt
is pending (indistinguishable); `system`/`turn_duration` marks turn end (idle);
`user` records get `permissionDecision` (`{decision:"accept", source:"config"|
"user_temporary",...}`) only after a decision. Bookkeeping records follow
messages, so filter by `type` rather than reading the last line.

Reliability: medium. It is good for history and activity, poor for "waiting for
permission".

### 2c. Hooks (documented, push-based)

Hook events in 2.1.330 (from the binary) include `PreToolUse`, `PostToolUse`,
`Notification`, `UserPromptSubmit`, `SessionStart`, `SessionEnd`, `Stop`,
`StopFailure`, `SubagentStart/Stop`, `PreCompact`, `PermissionRequest`,
`PermissionDenied`, `Elicitation`, `CwdChanged` (33 in total).

From code.claude.com/docs/en/hooks: common input `session_id`, `prompt_id`, `transcript_path`, `cwd`,
  `permission_mode`, `effort.level`, `hook_event_name`, and also `agent_id` and
  `agent_type` for subagents.
- State mapping: `UserPromptSubmit` -> working; `Stop` -> idle (carries
  `last_assistant_message`, do not store); `PermissionRequest` -> waiting;
  `Notification` with `notification_type` `permission_prompt` (about 6 s after
  the prompt), `idle_prompt`, `elicitation_dialog`, `agent_needs_input` ->
  waiting; `SessionStart` (`source`, `model`) / `SessionEnd` (`reason`) ->
  lifecycle.
- Hooks receive no PID. Correlate on `session_id` against the session files.
  A command hook can also report `$PPID`.

Reliability: high and documented, but hooks load at startup, so only sessions
started after the settings change report.

### 2d. Terminal screen: `herdr-core read <t>` (TUI scraping, fallback only)

## 3. Tokens, cost, model

- **Per-message usage** (transcript `assistant` records): `message.model`,
  `message.usage.{input_tokens, output_tokens, cache_read_input_tokens,
  cache_creation_input_tokens, cache_creation.{ephemeral_5m_input_tokens,
  ephemeral_1h_input_tokens}, output_tokens_details.thinking_tokens,
  server_tool_use.*, service_tier, speed}`. The record also carries
  `requestedModel` and `effort`.
  One response is split into several records sharing `message.id` and repeating
  `usage` (sample: 2552 records, 1238 ids). **Deduplicate by `message.id`.**
  Example (this session, deduplicated): 41 messages, `claude-opus-5-5`, input 96,
  output 39430, cache_read 3233619, cache_creation 129107. Subagent usage is in
  `<sessionId>/subagents/agent-*.jsonl`, added separately.
- **Cost**: `cost-state` records hold `totalCostUSD`, `modelUsage.<model>.{inputTokens,
  outputTokens, thinkingTokens, cacheRead/CreationInputTokens, costUSD}`,
  `totalAPIDuration`, `totalLinesAdded/Removed`. Written only around exit/resume
  (sample: 4 records, all 2 days old). **Not live**: compute usage x price.
- **statusLine**: the statusLine command receives JSON with `model.display_name`,
  `context_window.used_percentage` and `rate_limits.*`. That much was seen in
  the current statusLine command in `~/.claude/settings.json`. The JSON also
  carries a `cost` object (unverified here). Only one statusLine is configured,
  and herdr does not use it, so it could be wrapped. It runs only while the TUI
  renders.
- Model in use: the latest `assistant.message.model`, `SessionStart.model`, or
  the statusLine `model`.

## 4. Pending permission prompt and external approve/deny

**Details**: the `PermissionRequest` hook input has `tool_name`, `tool_input`
(for example `{command, description}` for Bash), `mcp_server` for MCP tools, and
`permission_suggestions[]`. The session file reports only
`waitingFor: "permission prompt"` and not the tool. In the transcript, the
pending call is the last unmatched `tool_use` block (`name`, `input`), but its
content must not be shown verbatim.

**Approve/deny via hook (documented API)**: a `PermissionRequest` hook replies with

```json
{"hookSpecificOutput":{"hookEventName":"PermissionRequest",
  "decision":{"behavior":"allow"}}}            // or "deny" + "message", "interrupt"
```

Allow can also carry `updatedInput` and `updatedPermissions`. The docs say the
hook runs "when Claude Code is about to ask you for permission". With an
external prompt tool the docs say "whichever decides first applies", but they
do not say whether the TUI dialog stays blocked while a hook is pending. The
default hook timeout is 600 s. If no decision is returned, the normal dialog
appears. Dandelion could therefore hold the HTTP request open until a user
clicks in the Dandelion UI, but this stalls the TTY dialog: unverified, test it
first.

**Keystrokes**: writing to `/dev/pts/N` writes to the screen, not the input;
`TIOCSTI` is disabled (`dev.tty.legacy_tiocsti = 0`); the PTY master belongs to
`herdr-core hold`. So keys must go through herdr (`herdr-core keys <t> enter|esc`,
`herdr-core send`, `herdr agent send-keys`). Depends on dialog layout; fragile.

## 5. Local listening ports

```
$ ss -ltnp | grep claude
LISTEN 127.0.0.1:45913  users:(("claude",pid=2157410,fd=30))
LISTEN 127.0.0.1:44311  users:(("claude",pid=1081982,fd=26))
$ curl -si http://127.0.0.1:45913/          -> HTTP/1.1 404 "not found"
$ curl -si -X POST http://127.0.0.1:45913/easel -> 404 "not found"
```

From the binary, the listener is `Bun.serve({hostname:"127.0.0.1",port:0})`
with the debug line `$.tool.register: loopback MCP server listening on
127.0.0.1:<port>`. This is the in-process MCP-over-HTTP server for tools that
plugins or mods register (here the easel mod's tools). Path `/<pluginName>?v=<ver>`,
POST only, `Authorization: Bearer <secret>` generated in memory and never
written to disk; an auth mismatch returns 404, same as a missing route.

**It is not usable** by Dandelion: it is not an IDE bridge, it carries no
session state, and it has no reachable secret.

Other IPC: `/run/user/1000/cc-socks/<pid>.sock` (0600) is the undocumented
cross-session peer-messaging socket (`peerProtocol: 1`), authenticated by
`peerToken` in the `.key` file; not probed. No `~/.claude/ide/*.lock` exists.

## 6. Recommended v1 design

1. **Discovery and state: watch `~/.claude/sessions/*.json` with inotify.**
   Validate against `/proc/<pid>` + `procStart`; take `sessionId`, `cwd`, `name`,
   `agent`, `version`, `status`, `waitingFor`; resolve the transcript by glob.
   Works for already-running sessions with zero config changes.
2. **Usage: tail the transcript** from the last offset.
   Parse `assistant` records only, dedupe on `message.id`, sum `usage` per model
   (plus `subagents/*.jsonl`), apply a Dandelion price table. Never store `content`.
3. **Optional: install Dandelion hooks** in `~/.claude/settings.json`, using
   `type: "http"` POSTs to `http://127.0.0.1:<dandelion>/hooks/claude` with a
   bearer token via `allowedEnvVars`. The alternative is a command hook with
   `async: true` that adds `$PPID`.
   Events: `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `Stop`,
   `Notification`, `PermissionRequest` (observe-only first), `PostToolUse`,
   `SubagentStart/Stop`. Merge beside herdr's existing `SessionStart` entry.
4. **Control in v1: none, or keystrokes via herdr-core** behind explicit user
   action. A decision-returning `PermissionRequest` hook is v2, after testing
   how it interacts with the TUI dialog.

Pros: steps 1-2 need no settings changes and cover already-running sessions;
hooks are the only documented interface and carry permission tool details.

Cons: session-file and transcript schemas are internal and evolve; hooks apply
only to sessions started after install; a slow http hook delays Claude
(`PermissionRequest`/`PreToolUse` block, default timeout 600 s, so set a short
`timeout`); editing `~/.claude/settings.json` competes with the user, herdr and
`/config`.

May break across versions: path/schema of `sessions/<pid>.json` and the
`status`/`waitingFor` strings; transcript record types and the
`message.id` duplication; when `cost-state` is written; the slug rule; the
permission dialog's key layout. Hook payloads are documented and additive, so
least likely to break. Read `version` from the session file and log unknown
enum values instead of failing.

## Summary table

| Field | Source | Verified |
|---|---|---|
| PID -> sessionId, cwd, name, version | `~/.claude/sessions/<pid>.json` | yes |
| PID liveness / reuse guard | `/proc/<pid>/stat` vs `procStart` | yes (field present) |
| Running binary version | `/proc/<pid>/exe` | yes |
| Transcript path | `~/.claude/projects/<slug>/<sessionId>.jsonl` | yes |
| PID -> terminal | `/proc/<pid>/environ` `HC_TERMINAL_ID`, `herdr-core get` | yes |
| Working/idle/waiting | session file `status`, `waitingFor` | partially (`busy` seen; enum from binary) |
| State via hooks | `UserPromptSubmit`/`Stop`/`Notification`/`PermissionRequest` | no (docs only) |
| State via transcript | `stop_reason`, `turn_duration` | yes (heuristic) |
| Tokens per session | transcript `message.usage`, deduplicated by `message.id` | yes |
| Live cost | computed; `cost-state` is not live | yes (cost-state not live) |
| Model | `assistant.message.model`, `requestedModel` | yes |
| Pending permission details | `PermissionRequest` hook input | no (docs only) |
| Approve/deny | hook `decision.behavior`, or herdr-core keys | no |
| Loopback TCP port | plugin-tool MCP server, bearer secret in memory | yes (404 probe + binary) |
| Peer socket `cc-socks/<pid>.sock` | cross-session messaging, token in `.key` | no (not probed) |

## Open questions

- Does a pending `PermissionRequest` http hook keep the TUI dialog hidden until
  it answers, or do both race? Test this in a throwaway session.
- When exactly is `cost-state` written (exit, resume, `/cost`)? Is there any
  live cost source other than statusLine?
- Can `waitingFor` hold values beyond `permission prompt`, `input needed` and
  `dialog open`, for example for plan approval or elicitation?
- Is the peer-messaging socket protocol (`peerProtocol: 1`) stable or intended
  for third parties? It could deliver prompts without TTY injection.
- Does herdr-core read `~/.claude/sessions` or use its own hook? The installed
  herdr hook is a no-op here because `HERDR_ENV` is unset in the claude
  environment.
