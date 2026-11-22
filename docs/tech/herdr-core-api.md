# herdr-core 0.1.19 socket API

Findings from inspecting herdr-core 0.1.19 (`~/.local/bin/herdr-core`, `hc` is a symlink to it)
on this machine, 2026-11-22. Full machine-readable schema: [herdr-core-api-schema.json](herdr-core-api-schema.json)
(`hc schema`, an OpenRPC 1.3.2 document). Sibling document for classic herdr: [herdr-api.md](herdr-api.md).

## 0. Machine state at the time of inspection

- The core runs as `herdr-core serve` (pid 2740) with one holder process,
  `herdr-core hold --socket /run/user/1000/herdr-core/default.holder.c23281c9.sock`. The holder owns
  the PTYs, so terminals survive a core restart. `server.status` reports 6 live and 9 finished terminals.
- State lives in `~/.local/state/herdr-core/default/`: `core.json` (core id and name), `terminals.json`
  (registry), `screens/<addr>.snap` (saved screens), `docs.json` (the `doc.*` store), `peers.json`,
  `core.log` and `holder.log`. `~/.config/herdr-core/` (config.toml, templates/) does not exist,
  so defaults apply and no templates are defined.
- herdr-app (the window-arranging GUI) runs on another machine. It reaches this core with
  `ssh ... hc connect` (an `sshd: dev@notty` parent with a `herdr-core connect` child). It stores
  its layout in the doc `herdr-app` and tags terminals with the labels `herdr.space`, `herdr.tab`,
  `herdr.seen` and `where` (for example `"Dandelion / 1"`).
- The session that ran this inspection is itself terminal `t31` (`HC_TERMINAL_ID=t31`). Every write
  call ran with `HC_TERMINAL_ID` unset and with `no_parent`/`no_notify`. Without that, a probe would
  become a child of t31: hc would type `<hc>` notes into t31 and end the probe together with it.
- Writes went only to two probe terminals labelled `purpose=dandelion-probe`: `t37` (`bash --norc --noprofile`)
  and `t38` (`sleep 600`), both ended and removed. Existing terminals were only listed, fetched, and read with `lines: 0`.

## 1. Socket and framing

The socket is `/run/user/1000/herdr-core/default.sock` (`$HC_SOCKET`; the session name `default` comes
from `HC_SESSION`). The protocol is **JSON-RPC 2.0, one message per line**. Unlike classic herdr:

- `"jsonrpc":"2.0"` is accepted. The `id` can be a number or a string, and `params` can be omitted.
- **Several requests can be sent on one connection.** No `hello` is needed for plain calls.
- Errors use the JSON-RPC shape plus a stable `data.kind` and `data.hint`: `-32601` for an unknown
  method, `-32000` for domain errors. The CLI prints `{error:{kind,message,hint}}` on stderr and
  exits with code 3 (not_found), 7 (needs_target), 9 (blocked), 10 (status_mismatch) or 11 (finished).
- `hello {framed:true}` switches the connection to **framed mode** after the reply. Framed mode is
  required for `terminal.attach`; otherwise the call fails with `needs_framed`.
- `hc connect` speaks the same protocol over stdin and stdout (meant for `ssh box hc connect`). Verified:
  `printf '{"jsonrpc":"2.0","id":1,"method":"server.status"}\n' | hc connect` returns the status result.
- `default.holder.*.sock` is the private core-to-holder link (`holder_proto.rs`); clients do not use it.

Verified raw calls (Python, AF_UNIX, one connection, output trimmed):

```
-> {"jsonrpc":"2.0","id":1,"method":"terminal.list","params":{}}          # equivalent of `hc ls`
<- {"id":1,"jsonrpc":"2.0","result":{"hidden_finished":9,"terminals":[{"address":"t1","argv":["/bin/bash"],
    "cols":146,"rows":73,"cwd":"/home/dev/<project>","foreground":{"argv":["bash"],"name":"bash","pid":2801,..},
    "keep":true,"labels":{"herdr.space":"sp..","herdr.tab":"tb..","herdr.seen":"155","where":"Persona Agents / 1"},
    "state":"live","status":null,"turn":155,..}, ...]}}
-> {"jsonrpc":"2.0","id":3,"method":"hello","params":{"generation":1}}
<- {"result":{"core":{"id":"88a464..","name":"dev-virtualbox"},"features":["framed","attach","events","status",
    "shell","resize_notifications","automatic_input"],"framed":false,"generation":1,"methods":[...32...],"version":"0.1.19"}}
-> {"jsonrpc":"2.0","id":4,"method":"nope"}
<- {"error":{"code":-32601,"data":{"hint":"see `hc schema`","kind":"unknown_method"},"message":"unknown method \"nope\""},"id":4,"jsonrpc":"2.0"}
-> {"jsonrpc":"2.0","id":5,"method":"terminal.get","params":{"terminal":"t999"}}
<- {"error":{"code":-32000,"data":{"hint":"see `hc ls --all`","kind":"not_found"},"message":"no terminal \"t999\""},..}
```

The CLI's piped JSON is not the API shape. For example, `hc ls --json` prints a condensed record
(`address, agent, cwd, state, status, turn, where`). Dandelion should use the socket directly.

### Introspection and documentation

- `hc schema` and `rpc.discover` return the full OpenRPC document (66 KB): 32 methods with params, results
  and error kinds. `hello` lists methods and features. Other docs: `hc --help`, `hc <cmd> --help`, and
  `hc --skill` (a 279-line manual bundled in the binary). There are no man pages.

### Methods (32)

| Area | Methods |
|---|---|
| Handshake/server | `hello`, `server.status`, `server.stop`, `server.restart`, `rpc.discover` |
| Terminals | `terminal.run/list/get/read/attach/send/keys/interrupt/wait/kill/rm/keep/label/put`, `templates.list` |
| Events | `events.subscribe` |
| Shared state | `doc.get`, `doc.set` (opaque JSON, versioned compare-and-set, at most 1 MiB) |
| Remote / no terminal | `machine.add/list/connect/rename/rm`, `machine.exec`, `file.read/write/edit` |

## 2. Data model

There are no workspaces, tabs or panes: the only object is the **terminal**. Layout belongs to
clients (herdr-app keeps its spaces and tabs in a doc and in labels).

- Address: `t7`, never reused on this core. Some addresses are `t1b`/`t2f`-style. Other forms are a unique
  `name` (1-32 of `a-z 0-9 - _ .`) and `machine/t7` for terminals on added machines.
- `Terminal` fields: `address, name, argv, cwd, state (live|finished), created_ms, finished_ms, exit_code,
  signal, reason (exited|killed|server_stop|signaled|spawn_pending|...), pid, foreground{pid,name,argv,cwd},
  cols, rows, labels{}, keep, parent, notify, rm, template, first_message, identity ("ghostty"),
  title, screen (exact|inexact), status, status_ms, records{}, reported, turn, submit, stop, paste_gap_ms`.
- Labels are free-form `key=value` and never interpreted. `terminal.label` sets them; `key=` removes one.
  `terminal.list` and `events.subscribe` filter on them with AND semantics.
- Templates: Markdown files in `~/.config/herdr-core/templates/<name>.md`, with frontmatter
  `name, description, command` and a body that becomes the first message. `templates.list` returned
  `{"templates":[]}` here.
- Machines: other cores reached over ssh or `--via <cmd>`. `machine.list` returned `{"machines":[]}`.
  `peers.json` records one inbound peer core (herdr-app's).
- Parent and children: a terminal started from another terminal is its child by default. It ends with
  the parent unless `outlive_parent` is set, and the parent receives typed notes unless `no_notify` is set.

### Agent identification and states

herdr-core does **no screen detection** and keeps no list of agent kinds. Status comes only from
the program itself through **OSC 7501**:

```
ESC ] 7501 ; state=<s>:app=<name>[:id=<rec>][:kind=<k>][:progress=<0-100>][:title=<b64>][:msg=<b64>] BEL
```

- `state`: `idle | working | done | blocked | error`. `kind` (blocked only): `permission | question | auth`.
  `title` and `msg` are base64. A report with a non-base64 `msg` was ignored in the probe. `state=clear`
  clears the status. Records with an `id` land in `records{}`, and the root record becomes `status`.
- Programs probe with `ESC]7501;?`; the core answers `]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth`.
- Claude Code 2.1.330 emits these natively with `app=claude-code` (found in its binary). The
  `~/.claude/hooks/herdr-agent-state.sh` hook belongs to classic herdr and is a no-op here.
- `turn` increases each time the status leaves `working`. `status_ms` is the time of the last change.
  `reported:false` means the program never reported, so it has no agent status.
- The "agent" shown by `hc ls` is `status.app`, or else the foreground process name.

Verified on the probe (bash printing the sequence):

```
$ hc send dandelion-probe "printf '\033]7501;state=working:app=dandelion-probe:msg=cHJvYmluZw==\007'"
$ hc get dandelion-probe   -> "status":{"app":"dandelion-probe","msg":"probing","state":"working"},"reported":true
live t3 (real agent):        "status":{"app":"claude-code","kind":"question","msg":"<redacted>","state":"blocked"}
live t31 (real agent):       "status":{"app":"claude-code","msg":"<redacted>","state":"working"},
                             "records":{"a94..":{"app":"claude-code","state":"working","title":"<redacted>"},..}
```

`status.msg` and `title` can contain conversation text, so Dandelion should treat them as sensitive.

## 3. Events

`events.subscribe {terminal?, children_of?, labels?}` (CLI: `hc events`) returns a snapshot of
the matching terminals and a `seq`. On a plain connection, the connection then carries only
notifications, one per line:

```
<- {"id":"s1","jsonrpc":"2.0","result":{"channel":null,"seq":3753,"terminals":[<Terminal>...]}}
<- {"jsonrpc":"2.0","method":"event","params":{"seq":3754,"terminal":"t37","type":"labels","data":{"labels":{"extra":"1","purpose":"dandelion-probe"}}}}
<- {"jsonrpc":"2.0","method":"event","params":{"seq":3733,"terminal":"t37","type":"status","data":{"records":{},"reported":true,"status":{"app":"dandelion-probe","msg":"probing","state":"working"}}}}
<- {"jsonrpc":"2.0","method":"event","params":{"seq":3735,"terminal":"t37","type":"turn","data":{"state":"blocked","turn":1}}}
<- {"jsonrpc":"2.0","method":"event","params":{"seq":3745,"terminal":"t37","type":"size","data":{"cols":80,"rows":24}}}
<- {"jsonrpc":"2.0","method":"event","params":{"seq":3760,"terminal":"t38","type":"foreground","data":{"argv":["sleep","600"],"cwd":"/tmp","name":"sleep","pid":2556940}}}
<- {"jsonrpc":"2.0","method":"event","params":{"seq":3759,"terminal":"t38","type":"created","data":<Terminal>}}
<- {"jsonrpc":"2.0","method":"event","params":{"seq":3758,"terminal":"t37","type":"finished","data":{"reason":"killed","exit_code":null,"signal":1,..}}}
<- {"jsonrpc":"2.0","method":"event","params":{"seq":3762,"terminal":"t37","type":"removed","data":{}}}
<- {"jsonrpc":"2.0","method":"heartbeat","params":{"seq":3705}}      # every 15 s (HC_HEARTBEAT_MS)
```

- Types seen: `created, foreground, status, turn, size, labels, finished, removed`. Types documented but not
  seen: `lost, restarted, keep, title, doc`. `foreground` is emitted but not in the schema's list.
- `seq` is global and monotonic. A subscriber that falls behind gets `missed` and a fresh snapshot; events
  whose `seq` is not above the snapshot's should then be ignored. There is no replay from a given seq.
- A single subscription with no filter covers every terminal, including status changes. Classic herdr
  needed one subscription entry per pane. All live agents' status changes were observed on one stream.
- In framed mode, events arrive as JSON lines on a channel with credit (not exercised).
- No event carries terminal output. Output comes from `terminal.attach`, `terminal.read`, or `terminal.wait {output}`.

## 4. Reading, and streaming a terminal into a browser

`terminal.read {terminal, lines=0}` returns **plain text, no ANSI** (`{text, complete, how, why}`).
`how` is `screen`, `history` or `stitched`. Finished terminals can still be read.

```
$ hc read dandelion-probe --json
{"complete":true,"how":"screen","text":"bash-5.2$ printf '\\033[31mRED\\033[0m PROBE_OK\\n'\nRED PROBE_OK\nbash-5.2$"}
read t3 (blocked claude), lines 0:  {"complete":true,"how":"screen"}  73 lines, no ESC bytes
```

Caution: with `lines > 0` on an **idle full-screen app**, hc *scrolls the app* to stitch history
and then restores it (`how: stitched`). That is input to the program. While the program is
`working|blocked|error`, it returns the screen only (`complete:false, why:<state>`).
Dandelion should poll existing agents with `lines: 0` only.

### Live stream: `terminal.attach` (what `hc attach` uses)

There is no JSON or NDJSON terminal bridge like classic herdr's `terminal session observe|control`.
The stream is binary frames on the main socket after `hello {framed:true}`, verified with strace on
`hc attach` and with a raw Python client:

- Frame: `channel u32 LE | kind u8 | length u32 LE | payload`. Channel 0 carries JSON-RPC messages
  (each one frame, no newline). `terminal.attach` returns `{channel, resize_notifications, automatic_input, terminal}`.
- Frame kinds verified: `0 DATA` (output from the core; keystrokes from the client), `1 CREDIT` (u32 LE bytes
  consumed, from client to core), `2 CLOSE` (empty; the core echoes it), `3 RESIZE` (`cols u16 LE, rows u16 LE`).
  Documented but numbers not verified: `FOCUS`, `FACTS` (presumably 4/5). `6 AUTOMATIC_INPUT` is documented
  as accepted when `automatic_input:true`.
- Flow control: the core sent exactly **262144 bytes** (256 KiB) without credit and then stopped.
  `hc attach` sends CREDIT frames after it consumes output. A web relay must do the same.
- The first DATA frame is a full repaint (`\x18\e[!p`, mode resets, `\e[H\e[2J`, ...), then live bytes.
  Both can go straight into xterm.js `term.write()`. `history: N` prepends N scrollback lines (main screen only).
- `observe:true` never types or resizes. A non-observer's attach size and RESIZE resize the real terminal.
  Verified: the probe was 200x120 (headless default), 80x24 after attach, and 100x30 after RESIZE.
  It went back to 200x120 after CLOSE. Several viewers of different sizes were not tested.
- Detach: CLOSE (kind 2) or drop the socket (`hc attach`: Ctrl+\\). Program loss ends the stream even with `keep`; reattach after `restarted`.

```
-> {"jsonrpc":"2.0","id":0,"method":"hello","params":{"framed":true}}\n
-> 00000000 00 d3000000 {"id":1,"jsonrpc":"2.0","method":"terminal.attach","params":{"terminal":"dandelion-probe","cols":80,"rows":24,"observe":false,"history":0,"display":{...}}}
<- ch0 kind0 {"result":{"automatic_input":true,"channel":1,"resize_notifications":false,"terminal":{...}}}
<- ch1 kind0 len1099 b'\x18\x1b[!p\x1b[?2026l...'                 # first paint
-> ch1 kind0 b'echo RAW_DATA_OK\r'
<- ch1 kind0 b'echo RAW_DATA_OK\r\n\x1b[?2004l\rRAW_DATA_OK\r\n\x1b[?2004hbash-5.2$ '
-> ch1 kind3 b'd\x00\x1e\x00'            # 100x30; hc get then showed cols 100 rows 30
-> ch1 kind2 b''     <- ch1 kind2 b''    # close; size reverted to 200x120
```

Plan for Dandelion: one framed connection per browser view (tokio `UnixStream`). Relay DATA to a WebSocket,
return CREDIT as bytes are forwarded, map xterm `onData`/`onResize` to DATA/RESIZE; `observe:true` for read-only views.

## 5. Spawning and control

- `terminal.run {argv | line, name?, cwd?, labels?, env?, size?{cols,rows}, display?, keep?, rm?,
  no_parent?, no_notify?, outlive_parent?, parent?, on?, wait?, timeout?}`. With no argv and no line, it runs the shell.
  If `argv[0]` names a template, the core starts its command, waits up to 60 s for readiness, then sends
  the body plus the task as the first message (`first_message.sent/reason`). The default size is 200x120.
  Verified raw: `{"argv":["sleep","600"],"name":"dandelion-probe-2","no_parent":true,"no_notify":true,
  "size":{"cols":100,"rows":30}}` returned `{"address":"t38","cols":100,..}`. Via the CLI:
  `hc run --bg --no-parent --no-notify --name dandelion-probe --label purpose=dandelion-probe bash --norc --noprofile`
  returned `t37`. Template runs and agent launches were not verified (no templates; no AI agents may be started).
- `terminal.send {terminal, text, force?, if?: idle|working, wait?, timeout?}` pastes the text and presses
  Enter once nobody has typed for about 3 s. It returns `{started, status, turn}`. Verified:
  `{"started":null,"status":null,"turn":0}` (no reporter). A blocked program is refused:
  `{"kind":"blocked","message":"t37 is blocked (permission)","hint":"typing would answer its dialog ... or --force"}`.
  `--if idle` against `working` returned `status_mismatch`, "nothing was typed".
- `terminal.keys {terminal, keys:[...]}` takes names such as `enter esc tab up ctrl+c alt+x f5 a`, encoded for the
  program's keyboard mode. Verified: `e c h o space k e y s o k enter` typed `echo keysok`. `_` is
  rejected (`unknown key "_"`), so raw bytes need attach DATA.
- `terminal.interrupt {terminal, timeout?}` presses esc for reporters and ctrl+c for other programs, and never
  escalates. Verified: `{"key":"ctrl+c","outcome":"stopped"}` (bash with sleep), `{"key":"ctrl+c","outcome":"exited"}`
  (bare sleep), and `{"key":"esc","outcome":"still_working"}` (fake reporter).
- `terminal.wait {terminal?, output?, children?: any|all, timeout?}` blocks server-side. It returns
  `{ended, reason: status|exit|output|timeout|not_started|message|children, status, turn, exit_code, signal}`.
  Verified: `reason:"status"` (blocked report after 2.2 s), `reason:"timeout"` (`ended:false`), and
  `reason:"output"`. The output match also hits the echoed command line, so `--output X` matched the typed
  `echo X` before the program printed X.
- `terminal.kill` hangs up, then terminates, then kills, and returns the final Terminal (verified: `reason:"killed","signal":1`).
  `terminal.rm` works on finished terminals only. A live terminal gives `{"kind":"live","hint":"kill it first"}` (verified).

## 6. Versioning and relationship to herdr / herdr-app

- Compatibility signals: `hello.generation` (1; a mismatch gives `unsupported_generation`), `version` ("0.1.19"),
  `features[]`, and the OpenRPC `info.version`. `AttachResult.resize_notifications` and `automatic_input`
  are per-path capabilities. Dandelion should call `hello {generation:1}` and check `features` for `attach`/`events`/`status`.
- `server.status` lists holders: after `hc update`, older holders keep their terminals until they end
  (`current:false`). `hc update` and `server restart` keep programs running, but streams must reattach.
- The `hc --skill` manual says herdr-core is "not `herdr`, the older terminal multiplexer". Window arrangement
  belongs to herdr-app (`herdr-app --skill`; not installed here). Herdr-app is a client of this API: it uses
  labels and `doc.*`.
- Differences from classic herdr 0.9.5:

| Topic | herdr 0.9.5 | herdr-core 0.1.19 |
|---|---|---|
| Envelope | custom `{id,method,params}`; string id | JSON-RPC 2.0; numeric or string id |
| Connection | one request per connection | many requests; framed mode via `hello` |
| Schema | `herdr api schema --json` (JSON Schema) | `hc schema` / `rpc.discover` (OpenRPC) |
| Objects | session > workspace > tab > pane, agents | flat terminals + labels; layout in clients |
| Agent state | screen detection manifests + hooks; `idle working blocked done unknown` | program self-report via OSC 7501; `idle working done blocked error`, plus `kind`/`msg`/`progress` |
| Per-agent status events | one subscription per pane | one unfiltered subscribe covers all |
| Heartbeat / resync | none seen | `heartbeat` every 15 s, `missed` + snapshot |
| Read | `text` or `ansi`, sources visible/recent/... | plain text only; screen + N history lines |
| Live stream | CLI NDJSON bridge (`terminal session observe/control`), base64 | binary frames on the API socket, credit flow control |
| Interrupt | `send_keys ctrl+c` | `terminal.interrupt` with outcome report |

## Call v1 uses

| Call v1 uses | method | verified |
|---|---|---|
| Handshake / version check | `hello` | yes (raw) |
| Core status | `server.status` | yes (raw, `hc connect`) |
| List terminals | `terminal.list` | yes (raw) |
| One terminal | `terminal.get` | yes (raw + CLI) |
| Live updates incl. agent status | `events.subscribe` | yes (raw + `hc events`) |
| Read screen/history (text) | `terminal.read` | yes (raw + CLI; `lines:0` on others' terminals) |
| Live view (read-only) | `terminal.attach observe:true` (framed) | yes (raw) |
| Interactive terminal | `terminal.attach` + DATA/RESIZE/CREDIT/CLOSE | yes (raw, probe only) |
| Spawn terminal | `terminal.run` | yes (raw + CLI, plain programs) |
| Spawn agent from template | `terminal.run` with template | no |
| Send prompt | `terminal.send` | yes (CLI, probe) |
| Press keys | `terminal.keys` | yes (CLI, probe) |
| Interrupt | `terminal.interrupt` | yes (raw + CLI, probe) |
| Wait for state/output | `terminal.wait` | yes (raw + CLI, probe) |
| Kill / remove | `terminal.kill`, `terminal.rm` | yes (CLI kill, raw rm) |
| Labels | `terminal.label` | yes (CLI, probe) |
| Templates list | `templates.list` | yes (empty) |
| herdr-app layout | `doc.get herdr-app` | yes (read-only, structure only) |

## Open questions

1. Numeric kinds and payloads of FOCUS, FACTS and AUTOMATIC_INPUT (6). The maximum frame size and the exact
   CREDIT window semantics (initial 256 KiB measured; is credit cumulative or a delta?).
2. Several simultaneous non-observer attaches with different sizes: who wins the size, and what does
   `resize_notifications:true` change for observers?
3. Does the size really revert to the headless default after the last viewer detaches, for terminals that
   herdr-app has open? Seen only on the probe.
4. `lost`, `restarted`, `keep`, `title`, `doc` and `missed` event payloads were not observed.
5. Framed-mode event channel (credit on JSON lines) is untested; plain mode worked for a long-lived subscriber.
   `terminal.put` (file beside a terminal, e.g. a pasted image) is untested.
6. Template launches and real agent readiness (`first_message`, 60 s ready gate) are unverified.
7. Should Dandelion set its own labels (for example `dandelion.*`) or a doc to coexist with herdr-app's `herdr.*` labels?
8. Access control: the socket is mode 0775 in a 0700 directory, and any process of user `dev` has full control.
   Dandelion's web layer must add its own authentication.
