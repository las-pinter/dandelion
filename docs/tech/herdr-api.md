# herdr 0.9.5 socket API

Findings from inspecting herdr 0.9.5 (`~/.local/bin/herdr`) on this machine,
2026-11-22. Full machine-readable schema: [herdr-api-schema.json](herdr-api-schema.json)
(`herdr api schema --json`).

## 0. Machine state at the time of inspection

- The herdr 0.9.5 server was **not running**: `herdr status` reports
  `server: status: not running`, and `herdr session list` shows the `default` and
  `development` sessions as `stopped`.
- The running work on this machine is in a different product, **herdr-core**
  (`~/.local/bin/herdr-core`, alias `hc`, version 0.1.19). Its socket is
  `/run/user/1000/herdr-core/default.sock`, and its panes carry `HC_TERMINAL_ID`
  and `HC_TERMINAL_SOCKET` instead of `HERDR_*`. Its manual (`herdr-core --skill`)
  says: "It is not `herdr`, the older terminal multiplexer", and "use
  `herdr-app --skill`" for window arrangement. `herdr-app` is not installed here.
- To verify behaviour, an isolated named session was started with
  `herdr --session dandelion-probe server`. It is a separate server with its own
  socket under `~/.config/herdr/sessions/dandelion-probe/`. All write calls below
  ran in its single workspace `dandelion-probe`, which ran only `/bin/bash`. The
  session was then stopped and deleted (`herdr session stop|delete dandelion-probe`).
  The default herdr session and herdr-core were not touched.

## 1. Sockets and framing

Per session directory (`~/.config/herdr` for `default`, `~/.config/herdr/sessions/<name>/` for named sessions):

`herdr.sock` holds the public JSON API (the subject of this document). `herdr-client.sock` holds
the private binary client protocol (TUI attach, terminal streams). `herdr.sock.agent` is a
symlink to the SSH agent, and `herdr-server.log` is the server log.

Framing on `herdr.sock` is **newline-delimited JSON with one request per
connection**. It is not JSON-RPC.

- Request: `{"id":"<string>","method":"<name>","params":{...}}\n`, where `params` is required.
  Response: `{"id":..,"result":{"type":"<kind>",...}}` or `{"id":..,"error":{"code","message"}}`, then `\n`.
- The server closes the connection after the response. A second request on the
  same connection gets `EPIPE`, and a request with no trailing newline gets no response.
- The exception is `events.subscribe`, which keeps the connection open and streams events.
- A JSON-RPC style body (`"jsonrpc":"2.0","id":4`) is rejected with
  `invalid_request ... invalid type: integer 4, expected a string`.

Verified raw call (Python, AF_UNIX):

```
$ python3 -c '...connect(sock); sendall(b"{\"id\":\"1\",\"method\":\"ping\",\"params\":{}}\n"); readline()'
{"id":"1","result":{"type":"pong","version":"0.9.5","protocol":22,"capabilities":{"live_handoff":true,
 "detached_server_daemon":false,"endpoint_protocol_generation":1,"surface_interest":true,
 "health_check":true,"ssh_agent_registration":true}}}
```

Unknown method: `{"id":"5","error":{"code":"invalid_request","message":"invalid request: unknown variant `nope`, expected one of `ping`, ..."}}`.

The CLI uses the same socket and envelope (ids such as `cli:workspace:create`). Server errors are JSON on stderr with exit code 1, syntax errors exit 2, and with no server running: `{"id":"cli:api:snapshot","error":{"code":"server_not_running",...}}`.

Rust client: per call, connect a `UnixStream`, write one line, read one line, drop it. Keep one long-lived connection per `events.subscribe`.

## 2. Schema

`herdr api schema` prints `protocol: 22`, `schema_version: 1`, and the schemas
`error_response, event, request, subscription_event, success_response`.
`--json` gives the full JSON Schema (draft 2020-12, about 270 KB). `request` is a `oneOf` keyed on
`method`, and `success_response.result` is a `oneOf` keyed on `type`.
`ping` returns `version` and `protocol` for a runtime compatibility check.

Methods relevant to Dandelion (103 in total; plugin, layout, copy-mode and client-UI methods are omitted):

| Area | Methods |
|---|---|
| Session/server | `ping`, `session.snapshot`, `server.agent_manifests`, `server.stop`, `server.reload_config` |
| Workspaces | `workspace.create/list/get/focus/rename/move/report_metadata/close` |
| Worktrees | `worktree.list/create/open/remove` |
| Tabs | `tab.create/list/get/focus/rename/move/close` |
| Panes | `pane.split/list/get/current/focus/rename/move/close/read/send_text/send_keys/send_input/process_info/layout/wait_for_output`, `pane.report_agent`, `pane.release_agent`, `pane.report_metadata` |
| Agents | `agent.list/get/read/explain/send_keys/rename/focus/start/prompt/wait`, `agent.view.set/clear` |
| Events | `events.subscribe`, `events.wait` |
| Notifications | `notification.show` (toast: `title`, `body`, `position`, `sound`) |

Sessions are not exposed over the socket: each session is its own server and socket, listed by `herdr session list` or the directory layout.

Main shapes:
- `AgentInfo`: `pane_id, terminal_id, workspace_id, tab_id, agent, name, agent_status, completion_seq, state_change_seq, revision, focused, cwd, foreground_cwd, title, terminal_title, tokens, agent_session, interactive_ready, launch_pending`.
- `PaneInfo` adds `label, scroll{...,viewport_rows}`; `SessionSnapshot` has `version, protocol, workspaces[], tabs[], panes[], layouts[], agents[], focused_*_id`.

```
$ herdr --session dandelion-probe api snapshot   # after creating 1 workspace, 2 tabs
{'agents': 1, 'focused_pane_id': 'w1:p1', 'layouts': 2, 'panes': 3, 'protocol': 22,
 'tabs': 2, 'version': '0.9.5', 'workspaces': 1}
```

## 3. Events and subscriptions

Herdr pushes events. Polling is not needed for state changes. Send `events.subscribe` with
`{"subscriptions":[{"type":...},...]}`. The server answers
`{"id":"sub1","result":{"type":"subscription_started"}}` and then streams one JSON
event per line on the same connection.

Subscription types: `workspace.created/updated/metadata_updated/renamed/moved/reordered/closed/focused`,
`worktree.created/opened/removed`, `tab.created/closed/focused/renamed/moved`,
`pane.created/closed/updated/focused/moved/exited/agent_detected/scroll_changed`,
`layout.updated`, and two parameterised types:
- `pane.agent_status_changed` requires `pane_id` (optional `agent_status`). Omitting
  `pane_id` fails the whole subscribe with `missing field pane_id`, so it cannot subscribe to
  every pane at once.
- `pane.output_matched` takes `{pane_id, source, match:{type:substring|regex,value}, lines?, strip_ansi?}`.
  If the text is already on screen, it fires immediately on subscribe.

Verified stream (a fake agent was reported in the probe pane, see section 4):

```
{"id":"sub1","result":{"type":"subscription_started"}}
{"data":{"pane":{...,"agent":"pi","agent_status":"working",...},"type":"pane_updated"},"event":"pane_updated"}
{"data":{"agent":"pi","pane_id":"w1:p1","type":"pane_agent_detected","workspace_id":"w1"},"event":"pane_agent_detected"}
{"data":{"agent":"pi","agent_status":"working","pane_id":"w1:p1","workspace_id":"w1"},"event":"pane.agent_status_changed"}
{"data":{"agent":"pi","agent_status":"idle","pane_id":"w1:p1","workspace_id":"w1"},"event":"pane.agent_status_changed"}
{"data":{"matched_line":"PROBE_DONE","pane_id":"w1:p1","read":{...}},"event":"pane.output_matched"}
{"data":{"type":"workspace_closed","workspace":{...},"workspace_id":"w1"},"event":"workspace_closed"}
{"data":{"pane_id":"w1:p3","type":"pane_closed","workspace_id":"w1"},"event":"pane_closed"}
```

Event names are inconsistent: generic events use underscores (`pane_updated`), parameterised ones use dots (`pane.agent_status_changed`).

There is **no event stream of raw pane output** on `herdr.sock`. `pane_output_changed` exists
only as an `events.wait` match kind. For output, use `pane.output_matched` or the terminal stream (section 5).

`events.wait` makes a single blocking call. In 0.9.5 it only supports
`pane_agent_status_changed`. Other kinds return
`unsupported_event_wait_match: events.wait currently supports pane agent status matches`.

```
{"id":"x","method":"events.wait","params":{"match_event":{"event":"pane_agent_status_changed",
 "pane_id":"w1:p1","agent_status":"working"},"timeout_ms":3000}}
-> {"result":{"type":"wait_matched","event":{"event":"pane_agent_status_changed","data":{...,"agent_status":"working","agent":"pi"}}}}
```

`agent.wait` (`herdr agent wait <t> [--until S]... [--timeout MS]`) blocks server-side until the
agent's status is in `until`. The default is the settled set `idle|done|blocked`. It returns
`agent_info` or the error `timeout`. Verified: the call blocked while the agent was `working` and
returned `{"type":"agent_info","agent":{...,"agent_status":"idle","completion_seq":2,...}}` when
the state was reported as idle. `agent.prompt` with `wait` adds a gate: a `working` or `blocked`
state must be observed within 5 s, otherwise it returns `agent_prompt_stalled`.

## 4. Agent model

- `AgentStatus` (API): `idle | working | blocked | done | unknown`. Reporters may set `idle | working | blocked | unknown`. `done` means idle with an unseen completion; focus commands mark it seen, and reads do not. `completion_seq` and `state_change_seq` are monotonic counters for de-duplication.
- Kinds accepted by `agent start` in 0.9.5: `pi claude codex gemini cursor devin agy cline omp
  mastracode opencode copilot kimi kiro droid amp grok hermes kilo qodercli qwen letta maki muse`.
- Detection works in two ways:
  1. Screen detection uses per-agent TOML manifests in `~/.local/state/herdr/agent-detection/remote/*.toml`,
     which herdr auto-updates (for example `claude.toml 2026.10.11.1`). Each manifest has prioritised
     rules over screen regions (`osc_title`, `bottom_non_empty_lines(N)`, `prompt_box_body`, ...).
  2. Hook reporting: integrations (`herdr integration install claude`, ...) or any process call
     `pane.report_agent {pane_id, source, agent, state, seq}` and `pane.release_agent`.
- `herdr agent explain` shows which rule matched. It also works offline on a captured screen:

```
$ herdr agent explain --file work.txt --agent claude --json     # work.txt: "✻ Thinking… (12s · esc to interrupt)"
{"agent":"claude","manifest_version":"2026.10.11.1","matched_rule":{"id":"live_turn_working",
 "priority":970,"region":"bottom_non_empty_lines(12)","state":"working"},"state":"working",...}
$ herdr agent explain --file idle.txt --agent claude
state: idle   rule: none   fallback_reason: default_known_agent_idle_fallback
```

Fake agent without an AI process, verified in the probe:

```
$ herdr pane report-agent w1:p1 --source dandelion-probe --agent pi --state working --seq 1
$ herdr agent list
{"result":{"agents":[{"agent":"pi","agent_status":"working","pane_id":"w1:p1","state_change_seq":1,...}],"type":"agent_list"}}
$ herdr agent rename w1:p1 probe      -> {"type":"agent_info","agent":{...,"name":"probe"}}
```

Agent-surface commands check that the agent is the pane's foreground process. With bash in the
foreground, `agent prompt` returned `agent_not_ready: agent probe is no longer the pane
foreground process`, and `agent send-keys` returned `agent probe is not an active named agent`.
In those cases the pane surface (`pane.send_keys`) must be used.

## 5. Reading output and streaming a terminal

`pane.read` / `agent.read`: `{pane_id|target, source, lines?, format: text|ansi, strip_ansi}`.
- `source`: `visible` (viewport), `recent` (with soft wraps), `recent_unwrapped`
  (wraps joined, best for logs), and `detection` (the detector's plain snapshot).
- `format:"ansi", strip_ansi:false` returns SGR-coloured text. The result includes
  `revision` and `truncated`.

```
{"method":"pane.read","params":{"pane_id":"w1:p1","source":"recent","lines":5,"format":"ansi","strip_ansi":false}}
-> {"type":"pane_read","read":{"format":"ansi","text":"\u001b[0m\u001b[1m\u001b[38;5;6m/tmp\u001b[0m ... \u001b[38;5;1mred\u001b[0m PROBE_..."
```

The live terminal stream is not available on `herdr.sock`. `herdr agent attach` and
`herdr terminal attach <terminal_id>` attach a TTY to the pane through `herdr-client.sock`.
That socket uses a private binary protocol (strace shows u32 little-endian length-prefixed
frames, a handshake, then the target id `w1:p2`, then ANSI frames). Its version is
`private_protocol: 22`.

The CLI also provides a **JSON bridge** that suits xterm.js:

- `herdr terminal session observe <target> [--cols N --rows N]` (read-only)
- `herdr terminal session control <target> [--takeover] [--cols N --rows N]` (read-write)

Both print NDJSON frames on stdout:

```
{"type":"terminal.frame","seq":1,"full":true,"width":80,"height":24,"encoding":"ansi","bytes":"<base64>"}
{"type":"terminal.frame","seq":2,"full":false,...,"bytes":"<base64>"}   # decoded: "\e[?2026h...\e[1;8Hecho\e[1;13HOBSERVED_''LINE..."
{"type":"terminal.closed","reason":"detached"}
```

`full:true` frames are complete repaints (`\e[2J...`); later frames are diffs inside synchronized output (`\e[?2026h ... \e[?2026l`). Decoded bytes can go to `term.write()` as they are.

`control` reads NDJSON commands on stdin. The following were verified:
```
{"type":"terminal.input","text":"echo CONTROL_OK\r"}
{"type":"terminal.input","bytes":"ZWNobyBCNjRfT0sK"}        # base64; text and bytes are mutually exclusive
{"type":"terminal.resize","cols":100,"rows":30}             # resizes the real pane (viewport_rows became 30)
```
The binary also names `terminal.scroll` (lines > 0), `terminal.mouse` and a release command.
Their field shapes were not verified. Closing stdin detaches, which produces
`terminal.closed reason=detached`.

Plan for Dandelion: one `herdr terminal session observe|control` child per browser view, relaying stdout and stdin over a WebSocket.

## 6. Spawning, interrupting, killing

- Workspace: `workspace.create {cwd?, label?, env?, focus:false}` returns `.workspace`, `.tab` and `.root_pane` (verified: `w1`, `w1:t1`, `w1:p1`).
- Tab: `tab.create {workspace_id, label?}` returns `.tab` and `.root_pane` (verified).
- Pane: `pane.split {target_pane_id?, direction: right|down, cwd?, env?, ratio?, focus:false}`
  returns `.pane` (verified via CLI, `w1:p2`).
- Worktree: `worktree.create {cwd|workspace_id, branch?, base?, path?, label?, focus, trust_repository}`
  creates a git worktree and a linked workspace. Not verified, to avoid writing into a repo.
  `worktree.list --cwd /home/dev/dandelion` was verified and is read-only.
- Agent: `agent.start {name, kind, pane_id, args?, timeout_ms? (3000 < t <= 300000)}`. It needs an
  idle shell pane and returns after the agent is detected and ready. Errors include
  `agent_not_ready`. Not verified, because the probe must not run an AI agent.
- Run a command: `herdr pane run <pane> "<cmd>"`. This is CLI sugar for `send_text` plus Enter, or
  `pane.send_input {pane_id, text, keys:["enter"]}`. Verified via CLI.
- Interrupt: `pane.send_keys {pane_id, keys:["ctrl+c"]}` (verified: `sleep 300` was replaced by
  `/bin/bash` in `pane.process_info`). For a live agent, `agent.send_keys {target, keys:["esc"]}`
  or `["ctrl+c"]`.
- Kill: `pane.close {pane_id}`, `tab.close {tab_id}` and `workspace.close {workspace_id, close_group?}` return `{"type":"ok"}` without confirmation (pane and workspace verified); `pane_closed` and `workspace_closed` events follow.
- `pane.process_info` lists foreground processes (pid, argv, cwd), useful before signalling.

## 7. Versioning and stability

- Client and server report `version` (0.9.5), `protocol` (22, the private protocol) and
  `endpoint_protocol_generation` (1). `schema_version` is 1. `herdr status` shows
  `endpoint_compatible` and `private_protocol_compatible`. Dandelion should check `ping`
  at startup and refuse protocol versions it does not know.
- `events.wait` is documented by the server itself as partial ("currently supports ...").
- Agent detection manifests update remotely and independently of the binary (the cached version
  is reported in `agent explain`), so state classification can change without an upgrade.
- herdr, herdr-core and herdr-app: the herdr-core manual calls `herdr` "the older terminal
  multiplexer". herdr-core (`hc`) keeps programs and agents alive in persistent terminals and has
  its own CLI and socket (`hc ls/read/send/keys/wait/events/attach`). herdr-app is referenced as
  the window-arranging layer and is not installed here. The 0.9.5 binary's `--help` still links to herdr.dev.

## Call v1 uses

| Call v1 uses | method | verified |
|---|---|---|
| Health/version check | `ping` | yes (raw) |
| Full state on connect | `session.snapshot` | yes (CLI) |
| List workspaces/tabs/panes | `workspace.list`, `tab.list`, `pane.list` | yes (workspace.list CLI); tab/pane list no |
| List/get agents | `agent.list`, `agent.get` | yes |
| Live updates | `events.subscribe` | yes (raw) |
| Per-pane agent status | `events.subscribe` `pane.agent_status_changed` | yes (raw) |
| Read pane/agent output | `pane.read`, `agent.read` | yes (pane raw + agent CLI) |
| Explain state | `agent.explain` | yes (CLI, live and --file) |
| Live terminal view | `herdr terminal session observe` (CLI bridge) | yes |
| Interactive terminal | `herdr terminal session control` (CLI bridge) | yes (input, resize) |
| Create workspace | `workspace.create` | yes (CLI) |
| Create tab | `tab.create` | yes (raw) |
| Split pane | `pane.split` | yes (CLI) |
| Create worktree | `worktree.create` | no |
| Start agent | `agent.start` | no |
| Prompt agent | `agent.prompt` | no (only the not-foreground error path) |
| Wait for agent | `agent.wait` | yes (CLI) |
| Interrupt | `pane.send_keys` / `agent.send_keys` | pane yes; agent no |
| Rename agent | `agent.rename` | yes (CLI) |
| Focus | `agent.focus` / `pane.focus` | no |
| Close pane/workspace | `pane.close`, `workspace.close` | yes (raw) |
| Notify | `notification.show` | no |

## Open questions

1. Target product: the owner's live agents run under herdr-core (`hc` 0.1.19), not herdr 0.9.5.
   Should Dandelion integrate with herdr, with herdr-core, or with both? herdr-core's socket protocol was not inspected.
2. Can several `events.subscribe` connections be open at once, and how many? Is there a
   heartbeat, and is there a replay or resume after a disconnect? None was seen.
3. Subscribing to `pane.agent_status_changed` for every pane needs one entry per `pane_id`, so the
   subscription has to be redone as panes appear. Alternatively, `pane_updated` may carry the status changes (to confirm).
4. `terminal session observe` with `--cols/--rows`: does it resize the pane like `control`
   does, or does it render a virtual view? Several observers with different sizes have not been tested.
5. Field shapes of `terminal.scroll`, `terminal.mouse` and the release command in `control`, and what `--takeover` does.
6. Is there a documented, stable public form of `herdr-client.sock`, or only the CLI bridge?
7. Real-agent behaviour of `agent.start` and `agent.prompt` (ready gate, `agent_prompt_stalled`) is unverified.
8. `revision` stayed 0 on every pane and read in the probe. Its semantics are unknown.
