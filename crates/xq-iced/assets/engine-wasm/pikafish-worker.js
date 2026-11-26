// pikafish-worker.js — Pikafish (WASM) UCI engine inside a Web Worker.
//
// PWA spike for the Xiangqi AI game. The desktop app drives Pikafish as a
// child process over UCI (stdin/stdout); WASM has no child processes, so the
// engine runs here, in a dedicated Worker, and the Rust side
// (`crates/xq-iced/src/engine_wasm.rs`) talks to it with postMessage.
//
// Verified against release `pikafish-asyncify-1.0.0` of
// https://github.com/west-shell/Pikafish-wasm (see INTEGRATION.md in this
// directory for the full verification log). The important facts:
//
// - The build is single-threaded + Asyncify: NO SharedArrayBuffer and NO
//   COOP/COEP headers are needed.
// - The JS glue (`pikafish.js`, same directory) is a CommonJS factory:
//   `Pikafish(moduleArg)` returns a Promise resolving to `Module`.
// - Commands go in with `Module.ccall('command', null, ['string'], [line])`
//   — one UCI line per call. There is NO `send_command`/`read_stdout` in
//   this release (those exist only in the repo's `preamble.js`, which was
//   NOT baked into the release build).
// - Engine output does NOT come through `Module.print`. It arrives through
//   the `stdout` / `stderr` CHAR callbacks passed in `moduleArg` (one call
//   per character, 10 = newline). The `bench` summary goes to stderr.
// - `uci` / `isready` / `position` / `stop` answer synchronously inside
//   the ccall; `go` returns immediately (Asyncify unwind) and `bestmove`
//   arrives later through the stdout callback. Measured in Node 24:
//   `go depth 10` -> bestmove in ~215 ms; `stop` -> bestmove in ~1 ms.
// - NEVER call `Module.ccall` re-entrantly from inside the stdout/stderr
//   char callback: it corrupts the output stream (verified: `"uciok"` +
//   `"readyok"` merge into one line `"uciokreadyok"`). Follow-up engine
//   calls from an output callback must be deferred with `setTimeout(0)`.
//   Calls from `onmessage` or the startup promise are safe.
// - The three files `pikafish.js` / `pikafish.wasm` / `pikafish.data` must
//   sit next to this worker file (Trunk `copy-dir`, see INTEGRATION.md).
//   The `.data` file IS the NNUE net (`/pikafish.nnue`, zstd-compressed,
//   network code 0x6a448afa — same code as the desktop net).
//
// Protocol (all messages are JSON strings; Rust parses them with serde):
//
//   Rust -> worker:
//     {type:'go', req_id, fen, moves:[iccs...], depth?, movetime?,
//      multipv?, searchmoves?}   // exactly one search at a time
//     {type:'stop'}                  // answer the running search now
//     {type:'position', fen, moves}  // set position without searching
//     {type:'ucinewgame'}
//     {type:'setoption', name, value}
//     {type:'quit'}                  // engine quit + worker close()
//
//   Worker -> Rust:
//     {type:'ready', name}           // after uci + isready handshake
//     {type:'progress', text}        // engine .data download progress
//     {type:'info', req_id, depth, seldepth, multipv, score_cp|null,
//      mate|null, nodes, nps, time_ms, pv:[iccs...]}
//     {type:'bestmove', req_id, move, ponder?}
//     {type:'log', stream:'stdout'|'stderr', line}
//     {type:'error', req_id?, message}
//
// The worker enforces UCI discipline: one search at a time (`go` while
// busy is answered with an error, never queued); `position`/`setoption`/
// `ucinewgame` are rejected while a search runs, because the engine would
// ignore or mishandle them mid-search.

importScripts('pikafish.js');

let Module = null; // Emscripten module, set once the factory promise resolves.
let phase = 'loading'; // 'loading' -> 'uci' -> 'wait-ready' -> 'ready'
let engineName = '';
let searching = false; // a `go` is in flight; cleared by `bestmove`
let currentReqId = null;
let currentMultipv = 1;

// NEVER call `Module.ccall` from inside the stdout/stderr char callbacks
// (`onStdoutChar`/`onStderrChar`) or from `onEngineLine`: a re-entrant
// ccall corrupts the Emscripten stdout stream — verified in Node 24, the
// bytes of the just-finished line are replayed into the next one
// (`"uciok"` + `"readyok"` arrive merged as `"uciokreadyok"`). Always
// defer follow-up engine calls out of the output callback with `defer()`
// (a macrotask, so the outer ccall has fully returned). Calls made from
// `onmessage` or from the startup promise are safe: they never run inside
// an output callback.
function defer(fn) {
  setTimeout(fn, 0);
}

function post(obj) {
  postMessage(JSON.stringify(obj));
}

function sendToEngine(cmd) {
  Module.ccall('command', null, ['string'], [cmd]);
}

// --- stdout / stderr char sinks (Emscripten calls one per character) ---

let outBuf = '';
function onStdoutChar(code) {
  if (code === 10) {
    const line = outBuf;
    outBuf = '';
    onEngineLine(line);
  } else if (code !== 13) {
    outBuf += String.fromCharCode(code);
  }
}

let errBuf = '';
function onStderrChar(code) {
  if (code === 10) {
    const line = errBuf;
    errBuf = '';
    post({ type: 'log', stream: 'stderr', line });
  } else if (code !== 13) {
    errBuf += String.fromCharCode(code);
  }
}

// --- engine output parsing ---

function parseInfo(line) {
  // `info depth D seldepth S multipv M score cp N|mate N [bound] nodes X
  //  nps Y hashfull H tbhits T time T pv m1 m2 ...`
  const t = line.split(' ');
  const info = {
    depth: 0,
    seldepth: 0,
    multipv: 1,
    score_cp: null,
    mate: null,
    nodes: 0,
    nps: 0,
    time_ms: 0,
    pv: [],
  };
  for (let i = 1; i < t.length; i++) {
    const k = t[i];
    if (k === 'depth') info.depth = parseInt(t[++i], 10) || 0;
    else if (k === 'seldepth') info.seldepth = parseInt(t[++i], 10) || 0;
    else if (k === 'multipv') info.multipv = parseInt(t[++i], 10) || 1;
    else if (k === 'score') {
      const kind = t[++i];
      const v = parseInt(t[++i], 10);
      if (!Number.isNaN(v)) {
        if (kind === 'cp') info.score_cp = v;
        else if (kind === 'mate') info.mate = v;
      }
    } else if (k === 'lowerbound' || k === 'upperbound') {
      // No value follows; nothing to skip.
    } else if (k === 'nodes') info.nodes = parseInt(t[++i], 10) || 0;
    else if (k === 'nps') info.nps = parseInt(t[++i], 10) || 0;
    else if (k === 'hashfull') i++;
    else if (k === 'tbhits') i++;
    else if (k === 'time') info.time_ms = parseInt(t[++i], 10) || 0;
    else if (k === 'pv') {
      info.pv = t.slice(i + 1);
      break;
    } else if (k === 'string') {
      break; // free text to end of line
    }
    // Unknown single tokens are ignored (their value, if any, is just
    // another unknown token on the next iteration).
  }
  return info;
}

function onEngineLine(line) {
  if (line === '') return;
  if (line.startsWith('id name ')) {
    engineName = line.slice('id name '.length);
    return;
  }
  if (line === 'uciok' && phase === 'uci') {
    // Deferred: never ccall from inside the output callback (see `defer`).
    // Hash 16 MB (not the desktop app's 64 MB): the engine runs
    // inside a browser tab next to the 57 MB app WASM, and on iOS
    // Safari the tab is killed when its memory grows too far
    // ("A problem repeatedly occurred", Hieu 2026-11-26) — 16 MB
    // is plenty for the movetime-limited searches this app sends.
    // Do NOT touch Threads: this build is single-threaded and has
    // no thread pool.
    defer(() => {
      sendToEngine('setoption name Hash value 16');
      sendToEngine('isready');
    });
    phase = 'wait-ready';
    return;
  }
  if (line === 'readyok' && phase === 'wait-ready') {
    phase = 'ready';
    post({ type: 'ready', name: engineName });
    return;
  }
  if (line.startsWith('bestmove ')) {
    const t = line.split(' ');
    const move = t[1] || '';
    const ponder = t[2] === 'ponder' ? t[3] || null : null;
    const reqId = currentReqId;
    searching = false;
    currentReqId = null;
    currentMultipv = 1;
    // No MultiPV restore here: every `go` sets its own MultiPV first
    // (see the `go` handler), so there is no cross-request state to
    // restore — and a restore sent from this output callback would be
    // the same re-entrant-ccall hazard as the handshake (see `defer`).
    if (reqId !== null) {
      post({ type: 'bestmove', req_id: reqId, move, ponder });
    }
    return;
  }
  if (line.startsWith('info depth ')) {
    if (searching && currentReqId !== null) {
      const info = parseInfo(line);
      post({ type: 'info', req_id: currentReqId, ...info });
    }
    return;
  }
  // Everything else (id author, option lines, `info string ...`) is
  // forwarded for the engine log, not interpreted.
  post({ type: 'log', stream: 'stdout', line });
}

// --- incoming messages from Rust ---

function positionCommand(fen, moves) {
  let cmd = 'position fen ' + fen;
  if (moves && moves.length > 0) cmd += ' moves ' + moves.join(' ');
  return cmd;
}

function goCommand(msg) {
  // Mirrors `go_command` in crates/xq-pikafish/src/uci.rs: depth and
  // movetime caps are both sent when both are set; when neither is set
  // the engine gets a 1000 ms movetime (the desktop default).
  let cmd = 'go';
  if (msg.depth != null) cmd += ' depth ' + msg.depth;
  if (msg.movetime != null) cmd += ' movetime ' + msg.movetime;
  if (msg.depth == null && msg.movetime == null) cmd += ' movetime 1000';
  if (msg.searchmoves) cmd += ' searchmoves ' + msg.searchmoves;
  return cmd;
}

self.onmessage = (event) => {
  let msg = event.data;
  if (typeof msg === 'string') {
    try {
      msg = JSON.parse(msg);
    } catch (e) {
      post({ type: 'error', message: 'bad message: ' + e });
      return;
    }
  }
  if (!msg || typeof msg.type !== 'string') {
    post({ type: 'error', message: 'message without type' });
    return;
  }

  switch (msg.type) {
    case 'go': {
      if (phase !== 'ready') {
        post({ type: 'error', req_id: msg.req_id ?? null, message: 'engine not ready' });
        return;
      }
      if (searching) {
        post({ type: 'error', req_id: msg.req_id ?? null, message: 'engine busy' });
        return;
      }
      sendToEngine(positionCommand(msg.fen, msg.moves));
      const multipv = msg.multipv != null && msg.multipv > 1 ? msg.multipv : 1;
      // Always set MultiPV for THIS search (even 1): the engine keeps
      // the previous search's value, and restoring it after `bestmove`
      // would need a ccall from the output callback (see `defer`).
      sendToEngine('setoption name MultiPV value ' + multipv);
      currentMultipv = multipv;
      searching = true;
      currentReqId = msg.req_id ?? null;
      sendToEngine(goCommand(msg));
      return;
    }
    case 'stop': {
      // Only meaningful mid-search; the engine answers `bestmove`
      // (~1 ms measured) and the `searching` flag clears there.
      if (searching) sendToEngine('stop');
      return;
    }
    case 'position': {
      if (phase !== 'ready') {
        post({ type: 'error', message: 'engine not ready' });
        return;
      }
      if (searching) {
        post({ type: 'error', message: 'engine busy' });
        return;
      }
      sendToEngine(positionCommand(msg.fen, msg.moves));
      return;
    }
    case 'ucinewgame': {
      if (searching) {
        post({ type: 'error', message: 'engine busy' });
        return;
      }
      sendToEngine('ucinewgame');
      return;
    }
    case 'setoption': {
      if (searching) {
        post({ type: 'error', message: 'engine busy' });
        return;
      }
      sendToEngine('setoption name ' + msg.name + ' value ' + msg.value);
      return;
    }
    case 'quit': {
      try {
        sendToEngine('quit');
      } catch (e) {
        // Engine already gone; closing anyway.
      }
      close();
      return;
    }
    default:
      post({ type: 'error', message: 'unknown message type: ' + msg.type });
  }
};

// --- engine startup ---

Pikafish({
  stdout: onStdoutChar,
  stderr: onStderrChar,
  // All three files sit next to this worker; resolve the .wasm and the
  // .data (fetched by Emscripten) relative to the worker script URL.
  locateFile: (path) => path,
  setStatus: (text) => {
    // Download progress for the 51.6 MB .data file.
    if (text) post({ type: 'progress', text });
  },
})
  .then((mod) => {
    Module = mod;
    phase = 'uci';
    sendToEngine('uci');
  })
  .catch((e) => {
    post({ type: 'error', message: 'failed to load Pikafish WASM: ' + e });
  });
