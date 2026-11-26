# Pikafish-WASM — Integration guide (PWA spike)

Engine for the Xiangqi AI game on iPhone (PWA): Pikafish compiled to
WebAssembly, running in a Web Worker. This directory holds the engine
assets + the worker glue; the Rust bridge is
`crates/xq-iced/src/engine_wasm.rs`. Nothing here is wired into the app
yet — this file is the exact remaining work.

Status: **Pikafish-WASM works — no fallback needed.** Verified end to end
in Node 24 on this VM (see §7 for the evidence log). The Fairy-Stockfish
fallback was NOT attempted (only needed if Pikafish-WASM had been blocked).

## 1. Assets in this directory

Release `pikafish-asyncify-1.0.0` of
https://github.com/west-shell/Pikafish-wasm (fork of official Pikafish,
branch `wasm`, GPL-3.0, single-threaded Asyncify build). Downloaded
2026-11-26 from the release page (URLs taken from the page's
`expanded_assets`, not guessed):

| File | Bytes | sha256 | Download URL |
|---|---|---|---|
| `pikafish.js` | 77,847 | `29643b40d24879e8a1d4042b8982fee1ffbefc7b5d1fabed612bbd0ea72ad4d3` | `https://github.com/west-shell/Pikafish-wasm/releases/download/pikafish-asyncify-1.0.0/pikafish.js` |
| `pikafish.wasm` | 845,871 | `2a44866abe24f0172422a8a172b268730004323a0c6c1df8af944c171bdbff3c` | `.../pikafish-asyncify-1.0.0/pikafish.wasm` |
| `pikafish.data` | 51,585,654 | `3cd15292bf8c979884262f57fc723959fc0dea43b4d8d544f88db5ceb2479e24` | `.../pikafish-asyncify-1.0.0/pikafish.data` |
| `pikafish-worker.js` | (glue, this spike) | — | written here, not downloaded |
| `INTEGRATION.md` | (this file) | — | written here |

Total download: 52,509,372 bytes (~50.1 MiB). The `.data` file IS the
NNUE net (see §6): Emscripten file-package metadata in `pikafish.js`
says `files:[{filename:"/pikafish.nnue",start:0,end:51585654}]` — one
file, the whole package.

## 2. Dependencies to add (Cargo.toml — NOT done here)

`xq-iced/Cargo.toml` currently has no wasm crates. Add, under
`[target.'cfg(target_arch = "wasm32")'.dependencies]` (or plain
`[dependencies]` if the team prefers — they are only *used* from
wasm32-gated code, but plain deps also compile on native and slow the
desktop build; target-gated is cleaner):

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "0.2"
web-sys = { version = "0.3", features = ["Worker", "MessageEvent", "console"] }
```

`serde` / `serde_json` / `xq-core` / `xq-engine` are already dependencies.
`js-sys` is NOT needed: the worker posts JSON strings and Rust parses
them with `serde_json`. Verified: `engine_wasm.rs` type-checks with
exactly these deps (`cargo check --target wasm32-unknown-unknown` in a
scratch crate, 2026-11-26; 8 `dead_code` warnings only, all "never used"
artifacts of the scratch harness).

## 3. Files to change for integration

1. `crates/xq-iced/src/main.rs` — one line: declare the new module.
   ```rust
   #[cfg(target_arch = "wasm32")]
   mod engine_wasm;
   ```
   (`engine_wasm.rs` itself is already gated with
   `#![cfg(target_arch = "wasm32")]` at the top; the declaration is
   still needed because this spike may not touch existing sources.)
2. `crates/xq-iced/src/ai_worker.rs` — add a wasm32-only worker. Do NOT
   rewrite `AiWorker`: it is thread-based (`std::thread::spawn`,
   `SharedEngine` behind a mutex, blocking `Engine::best_move`) and
   cannot run on `wasm32` (single thread, no blocking wait). Add a new
   struct, e.g. `WasmAiWorker`, that:
   - owns a `WasmEngine` (`engine_wasm::WasmEngine::new("engine-wasm/pikafish-worker.js")`);
   - exposes the same surface the screens use (`tx` / `rx` shape is up
     to the integrator, but keep `AiRequest` / `AiResponse` /
     `response_is_current` / `alloc_req_id` semantics unchanged so the
     screens do not fork): translate `AiRequest::BestMove` →
     `WasmEngine::request_best_move`, `Analyze` →
     `request_analyze`, `GradeMove` → `request_best_move` +
     `request_score_move` (best move + played-move score, then
     `grade_from_delta_with` / `explain_tags` exactly like
     `run_grade_on`), `Stop` → `WasmEngine::stop()`;
   - translates `WasmEngineEvent::Info/BestMove/Analysis` →
     `AiResponse::Info/BestMove/Analysis` (same `req_id`; reuse
     `to_analysis_line` for the `tags` on analysis lines — the bridge
     leaves `tags` empty on purpose);
   - polls with `WasmEngine::drain_events` / `try_recv` from the Iced
     update loop (e.g. a timer tick subscription) — NEVER a blocking
     `rx.recv()` on wasm; and drains `WasmEngineEvent::Error` into the
     existing fallback path (swap to `BuiltinEngine` + banner, same
     policy as `fallback_action`). Note: `BuiltinEngine` is pure Rust
     (`xq-core`/`xq-engine` already compile for `wasm32` — verified by
     the scratch `cargo check`), but on wasm its search would run on the
     UI thread; treat it as a last-resort fallback only.
   - `Level` → limits: reuse `xq_pikafish::levels::pikafish_limits(level)`
     (public, pure Rust) to build the `EngineRequest` limits, the same
     table the desktop engine uses; `grade_limits()` / `hint_limits()`
     are `time_ms(1500)` on the desktop Pikafish engine.
3. `crates/xq-iced/src/screens/*.rs` (`play.rs`, `practice.rs`,
   `learn.rs`) — wherever they construct `AiWorker::new(...)`, add a
   `cfg(target_arch = "wasm32")` branch constructing `WasmAiWorker`
   instead. The `AiRequest`/`AiResponse` types and the `req_id`
   discipline do not change, so screen logic should not need edits
   beyond construction (+ the engine-ready loading state: the PWA must
   wait for `WasmEngineEvent::Ready` — the 51.6 MB `.data` download —
   before the first AI move; show the worker's `progress` text if a
   loading indicator is wanted).
4. `crates/xq-iced/src/engine_manager.rs` — on wasm there is no child
   process to spawn/locate (`xq-pikafish::locate` / `PikafishEngine::spawn`
   are native-only). The wasm path must not call it; `WasmEngine`
   replaces the `SharedEngine` on this target.

## 4. Trunk: shipping the assets

The worker + the three engine files must be served as static files next
to each other, under the same directory, from the PWA origin:

```
dist/
  ...
  engine-wasm/
    pikafish.js
    pikafish.wasm
    pikafish.data
    pikafish-worker.js
```

Trunk mechanism (pick one; the build owner decides):

```html
<!-- in index.html -->
<link data-trunk rel="copy-dir" href="assets/engine-wasm" />
```

Trunk copies `assets/engine-wasm/` to `dist/engine-wasm/` (the
`copy-dir` target keeps the directory name). Then the Rust side creates
the worker with the page-relative URL
`"engine-wasm/pikafish-worker.js"`. The worker itself does
`importScripts('pikafish.js')` and Emscripten fetches `pikafish.data`
relative to the *worker script* URL, so all four files must stay
co-located — do not split them across directories.

Constraints that follow from the verified build (do not "fix" these):

- Single-threaded Asyncify build: NO `SharedArrayBuffer`, NO
  COOP/COEP headers, NO `Cross-Origin-Isolation` needed. Plain static
  hosting works.
- Do NOT set the `Threads` UCI option above 1 (the build has no thread
  pool; the option exists but is unsupported). The worker already sets
  `Hash 64` at startup; leave everything else at defaults.
- The `.data` file is fetched at engine startup (~51.6 MB). First AI
  move on a fresh PWA install waits for the download + ~250–520 ms
  module init (measured in Node; browser will be similar + network).
  The service worker should cache all four files so later launches are
  instant (the other subagent owns the PWA shell — flag it).

## 5. End-to-end test (after integration)

1. `trunk serve` (or `trunk build --release` + a static server).
2. Open the app, open DevTools → Network: confirm `engine-wasm/`
   serves all four files with 200 (not 404, not HTML fallback).
3. DevTools → Console: no worker errors; the worker posts
   `{type:'ready', name:'Pikafish dev-20261103-nogit'}` (visible if the
   app logs `WasmEngineEvent::Log` / `Ready`).
4. Start a game vs AI (any level): the AI move must appear in ~1 s at
   Medium (movetime 1000). The first move of the session additionally
   waits for the `.data` download.
5. Sanity positions: startpos → any legal first move (`c3c4`/`g3g4`/
   `h2e2`/`b2e2` all seen in testing); Undo while the AI is thinking
   must not hang (stale `req_id` dropped); `stop` (e.g. exiting to the
   menu mid-think) must answer at once.
6. FEN → worker → bestmove → board, directly in the browser console
   (bypasses the Rust side, for isolating worker problems):
   ```js
   const w = new Worker('engine-wasm/pikafish-worker.js');
   w.onmessage = e => console.log(e.data);
   w.postMessage(JSON.stringify({type:'go', req_id:1,
     fen:'rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1',
     moves:[], movetime:1000}));
   // expect: {"type":"ready",...} then info lines then {"type":"bestmove","req_id":1,"move":"g3g4",...}
   ```

## 6. Verified API shape (what the code really does)

Read from the release `pikafish.js` + the fork's `wasm` branch source,
then confirmed by running it in Node 24. The upstream `preamble.js`
(`Module.send_command` / `Module.read_stdout`, `wasm_uci_execute`) is
NOT in this release — do not use it; the release exports a different
function (`_command`, one string-pointer argument).

- Load: `require('./pikafish.js')` is a CommonJS factory
  `(moduleArg) => Promise<Module>`. Pass `{ stdout, stderr, locateFile,
  setStatus }` as `moduleArg`.
- Send one UCI line: `Module.ccall('command', null, ['string'], [line])`.
  `Module.cwrap` and `Module._malloc` do NOT exist in this build — use
  `ccall` for everything. `Module._command` exists but takes a raw
  pointer; `ccall` is the supported path.
- Receive: output arrives through the `stdout` / `stderr` CHAR callbacks
  (one call per character, `10` = newline, `13` = `\r`). It does NOT go
  through `Module.print` — verified: a `print` handler receives nothing
  while the char callbacks stream the full `uci` response. The `bench`
  summary (`Total time`, `Nodes searched`, `Nodes/second`) goes to
  **stderr**, as do the `Position: N/49` lines.
- `uci` → `id name Pikafish dev-20261103-nogit` + 11 options
  (`Debug Log File`, `NumPolicy`, `Threads=1`, `Hash=16`, `Clear Hash`,
  `Ponder`, `MultiPV=1`, `Move Overhead=10`, `nodestime=0`,
  `UCI_ShowWDL`, `EvalFile=pikafish.nnue`) + `uciok`. `isready` →
  `readyok`. Both answer **synchronously inside the ccall** — register
  any waiter BEFORE sending, or read the lines that already arrived.
- `go` returns immediately (Asyncify unwind; measured: ccall returned
  after ~77 ms for `go depth 10`); `bestmove` arrives later through the
  stdout callback. `go` accepts standard UCI: `depth`, `movetime`,
  `movetime`+`depth` together, `searchmoves`, `infinite`.
- `stop` during `go infinite` → `bestmove` in ~1 ms (event-driven
  measurement; a 25 ms polling harness measured 26 ms, i.e. the poll
  interval dominated).
- Engine version is a **dev build** (`dev-20261103-nogit`), not the
  desktop `2026-09-06` release. Keep that in mind when comparing
  strength/behaviour with desktop.
- **Re-entrancy hazard (verified, with a failing-then-fixed test):**
  calling `ccall('command', …)` from INSIDE the stdout/stderr char
  callback corrupts the output stream — the bytes of the just-finished
  line are replayed into the next one (`"uciok"` + `"readyok"` arrive
  merged as `"uciokreadyok"`, and the handshake never completes).
  Follow-up engine calls from an output callback MUST be deferred with
  `setTimeout(0)` (verified clean). Calls from `onmessage` or the
  startup promise are safe. `pikafish-worker.js` implements this
  (`defer()`); it also sets MultiPV explicitly before EVERY `go`
  instead of restoring it after `bestmove`, so no restore call is ever
  needed from an output callback.

## 7. NNUE verification (network code 0x6a448afa)

Method: both files are zstd-compressed (magic `28 b5 2f fd`). No `zstd`
CLI and no Python `zstandard` module exist in this VM, so both were
stream-decompressed with Node 24's built-in `node:zlib`
`createZstdDecompress`, hashing the decompressed bytes.

| | Desktop `engine/pikafish.nnue` | WASM `pikafish.data` (= `/pikafish.nnue`) |
|---|---|---|
| Compressed bytes | 50,706,378 (sha256 `7d13d735…e900e`, matches `engine/PIKAFISH_VERSION.toml`) | 51,585,654 (sha256 `3cd15292…e24`) |
| Decompressed bytes | 66,077,882 | 66,082,730 |
| First 4 bytes (LE u32) | **0x6a448afa** | **0x6a448afa** |
| Decompressed sha256 | `e0af7813ce304337…` | `2f6c8ed917f1ade65…` |
| First differing byte | — | offset 238 (first 32 header bytes identical) |

Verdict: the claim "mạng NNUE mã 0x6a448afa trùng mạng desktop" is
**half true**. The network CODE matches (same 4-byte magic, same
architecture header), but the two nets are NOT byte-identical: the
decompressed weights differ (first diff at byte 238, sizes differ by
4,848 bytes). They are most likely different training runs of the same
architecture. Practical impact: eval can differ slightly from desktop
(same startpos, `movetime 1000`: desktop played `b2e2`, WASM played
`g3g4` — both are normal opening moves; the desktop run reached depth
20 vs 16, so the difference is confounded by search depth). Before
treating WASM-vs-desktop strength as equal, spot-check bestmove
agreement on a few known positions; do not assume it.

## 8. Performance measurements (this VM, 1 thread, startpos)

| Test | Desktop Pikafish 2026-09-06 (native) | WASM dev-20261103 |
|---|---|---|
| `go movetime 1000` | depth 20, 576,877 nodes, **576,877 nps**, `b2e2` | depth 16, 179,415 nodes, **179,235 nps**, `g3g4` |
| `go depth 10` | — | `c3c4`, 215 ms total (ccall returned after 77 ms) |
| `stop` during `go infinite` | — | `bestmove` in ~1 ms |
| `bench 16 1 8` (49 positions) | ~600–700k nps on bench positions | 199,351 nodes in 868–1163 ms, **171k–230k nps** |

The claimed Asyncify cost of ~23% NPS (bench 7372 ms vs 5976 ms)
**did not reproduce here**: WASM runs at ~0.31× desktop NPS on the
same machine/position (~69% slower). Caveats: different engine versions
(dev-20261103 vs 2026-09-06), different net weights (§7), and a
throttled shared-CPU VM — but the gap is far beyond 23%. Consequence:
at the same movetime the PWA engine searches ~4 fewer plies than
desktop. Difficulty levels are movetime-based, so they still function;
absolute strength per level is lower. Whether to raise PWA movetimes
to compensate is a product decision for Hieu, not this spike.

## 9. Worker test evidence (Node 24, 2026-11-26)

`pikafish-worker.js` was driven through a Node harness simulating a
`DedicatedWorkerGlobalScope` (`importScripts`/`postMessage`/`close`
shimmed; engine files loaded from this directory):

```
READY: Pikafish dev-20261103-nogit
busy rejected OK: engine busy
info sample: depth 10 score_cp 36 pv [ 'c3c4', 'b7c7', 'h2e2' ]
bestmove: g3g4 ponder: b7e7 | took 1003 ms
stop bestmove: b2e2 | stop latency 26 ms (polling harness; ~1 ms event-driven, §6)
analyze bestmove: g3g4 | multipv values seen: [ 1, 2, 3 ] | info lines: 24
log lines forwarded: 24
WORKER TEST ALL OK
```

The Rust bridge (`engine_wasm.rs`) was type-checked for
`wasm32-unknown-unknown` (scratch crate, same `xq-core`/`xq-engine`
path deps). The full browser path (Trunk → worker → `bestmove` → board)
is NOT yet tested — that needs the other subagent's build (§5).

## 10. Open points / risks

- The WASM engine is a dev build (`dev-20261103-nogit`) with a
  different net from desktop (§7): do not present PWA strength as
  identical to desktop without a spot-check.
- NPS ~0.31× desktop on this VM (§8): consider retuning PWA movetimes.
- `bench` default is 49 positions (not 16) and its summary goes to
  stderr — only relevant if anyone re-benches in the browser.
- First launch downloads 51.6 MB (`.data`); the PWA shell must cache
  the four files, or every fresh install pays that cost.
- The worker loads `pikafish.js` with `importScripts` (classic worker
  script, not a module worker). Do not convert it to
  `type: 'module'` without re-testing: the Emscripten glue expects a
  classic worker/global scope.
