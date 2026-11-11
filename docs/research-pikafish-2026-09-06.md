# Pikafish pin-down — research report

All facts verified live against the actual release binary on this Linux VM (no guessing; where the wiki and the binary disagree, the binary wins — see §4).

## 1. Latest official release

- **Repo:** `official-pikafish/Pikafish`
- **Tag / version:** `Pikafish-2026-09-06` (release name "Pikafish 2026-09-06")
- **Published:** 2026-09-06T10:22:29Z (confirmed newest of all releases; previous was `Pikafish-2026-01-02`)
- **Built from commit:** `4c17cee11f888ae1d48a9494f2e2239f019f0a1f` — the tag ref points directly at this commit (type: commit, not an annotated tag). Commit date 2026-09-06T06:03:41Z, author PikaCat, message "Tune search and evaluation parameters".
- **Release page:** https://github.com/official-pikafish/Pikafish/releases/tag/Pikafish-2026-09-06
- **Verification:** running `bench` on the downloaded Linux binary prints `Nodes searched : 2651062`, exactly the `Bench: 2651062` in that commit message — the binary matches the tagged source.

## 2. `pikafish.nnue` — download and sha256

- **There is no separate nnue download.** The release has exactly **one asset**, a 7z archive, and the net ships inside it:
  - https://github.com/official-pikafish/Pikafish/releases/download/Pikafish-2026-09-06/Pikafish.2026-09-06.7z (53,603,424 bytes)
  - My computed sha256 of the downloaded archive: `41952bbfe2520faceb5902c69e6ab4845cc999841d2b49a95cc1be7867a25e5b`
  - Inside: `pikafish.nnue`, 50,706,378 bytes
- **No sha256 is published anywhere I checked** — not in the release notes, not in the `official-pikafish/Networks` repo (its git tree contains only `README.md`, no net file, no hash), not on the wiki. So there is no official hash to pin against.
- **Self-computed sha256 of the extracted `pikafish.nnue`** (from the release archive above): `7d13d73569a9b571ba0eb20cf1596247bc2a42738967e61afef6482b231e900e` — usable as a local integrity check, but it is mine, not an upstream-published value.
- Live confirmation: the engine prints on startup `info string NNUE evaluation using pikafish.nnue (64MiB, (62083, 1024, 32, 32, 1))`.

## 3. Binary assets in the release

Single asset `Pikafish.2026-09-06.7z` containing universal binaries (release notes: "We have transitioned to universal binaries… automatically detect the features of your CPU… eliminating the need to manually choose between AVX2, AVX-512, etc."):

| File in archive | Size | Platform |
|---|---|---|
| `Pikafish-Linux-x86-64-universal` | 6,875,464 | Linux x86-64 (ELF 64-bit LSB, verified runnable on this VM) |
| `Pikafish-Windows-x86-64-universal.exe` | 6,944,256 | Windows x86-64 |
| `Pikafish-MacOS-universal` | 6,300,248 | macOS |
| `Pikafish-Android-arm64-universal` | 2,413,440 | Android arm64 |

- **No separate avx2 / sse41-popcnt Windows binaries exist in this release** — only the one universal `.exe`. (My computed sha256 of the Linux binary: `727828293830de8082a80b224bf47b32700c2d1a441908ea11b3c9197178c20c`.)
- Archive also bundles `Copying.txt` (GPLv3), `NNUE-License.md`, `README.md`, and a `Wiki/` folder (including `UCI-&-Commands.md`).

## 4. UCI options — ⚠️ wiki ≠ pinned release (important for the plan)

The live `uci` output of the pinned binary lists **only 11 options** (also confirmed in `src/ucioption.cpp` + `src/engine.cpp` at the tag commit, and in the bundled `Wiki/UCI-&-Commands.md`, lines 27–37):

```
option name Debug Log File type string default <empty>
option name NumaPolicy type string default auto
option name Threads type spin default 1 min 1 max 1024
option name Hash type spin default 16 min 1 max 33554432
option name Clear Hash type button
option name Ponder type check default false
option name MultiPV type spin default 1 min 1 max 128
option name Move Overhead type spin default 10 min 0 max 5000
option name nodestime type spin default 0 min 0 max 10000
option name UCI_ShowWDL type check default false
option name EvalFile type string default pikafish.nnue
```

**The following options do NOT exist in this release** — `setoption` replies `No such option: …` for each, and greps of `src/uci.cpp`, `src/ucioption.cpp`, `src/engine.cpp`, `src/search.cpp`, `src/evaluate.cpp`, `src/main.cpp` at commit `4c17cee` return zero hits:

- `Skill Level`, `UCI_LimitStrength`, `UCI_Elo`, `Repetition Rule`, `Sixty Move Rule`, `Rule60MaxPly`, `Mate Threat Depth`, `ScoreType`, `Draw Rule`, `LU_Output`

The wiki page https://www.pikafish.com/wiki/reference/uci-options.html **does** document them (Skill Level: default 20, range 0–20; UCI_LimitStrength: default off; UCI_Elo: range 1280–3133; MultiPV: 1–128; Rule60MaxPly: default 120, range 1–150; Repetition Rule values: `AsianRule`, `SkyRule`, `ComputerRule`, `ChineseRule`, `AllowChase`, `YitianRule`, `NoJudgement`; Sixty Move Rule: default on; Ponder: background thinking; EvalFile: NNUE path, default `pikafish.nnue`; NumaPolicy: default `auto`). **But those values describe some other build/variant, not the pinned 2026-09-06 release** — do not rely on them. Consequences for the plan: difficulty cannot be set via `UCI_Elo`/`Skill Level` with this binary; strength limiting must be done by the app itself (e.g. `go depth`/`go movetime` caps, or MultiPV + random pick). The options that *do* exist and are usable: `EvalFile` (string, default `pikafish.nnue`), `Threads` (spin 1–1024, default 1), `Hash` (spin 1–33554432 MB, default 16), `MultiPV` (spin 1–128, default 1), `Ponder` (check, default false).

## 5. UCI protocol — verified live on this VM

- `uci` → `id name Pikafish 2026-09-06`, `uciok`; `isready` → `readyok`.
- `position fen <fen> moves <m1> …` accepted (startpos FEN used: `rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1`). `position startpos moves …` also accepted.
- Move notation is ICCS-like coordinates, files a–i, ranks 0–9, e.g. `h2e2`, `a6a5` (bundled wiki `UCI-&-Commands.md` line 103 documents the `position [fen <fenstring> | startpos] moves <move1> …` syntax; live test confirms).
- `go movetime 1000` → `bestmove a6a5` (after `h2e2`).
- `go depth 5` → `bestmove a3a4` (from startpos).
- `go searchmoves` is **supported**: `go depth 6 searchmoves a6a5` → `bestmove a6a5 ponder e2e6` (only the listed move returned). Wiki line 279–281 documents it with example `go infinite searchmoves h2e2 h0g2`.
- `go infinite` + `stop` → `bestmove a6a5` (stop works).
- `bestmove` format: `bestmove <move> ponder <move>`.
- Release notes warn of strict validation: an invalid FEN/UCI command makes the engine print `info string CRITICAL ERROR` and terminate immediately (not live-tested, cited from release notes).

## 6. License note (affects shipping)

Engine is GPLv3 (`Copying.txt` in archive). The NNUE weights are **not** GPL: per `official-pikafish/Networks` README and `NNUE-License.md`, the `pikafish.nnue` weights are for legal, **non-commercial use only without permission** (commercial use only by listed permitted parties). Relevant if the app is ever distributed commercially.

## Scratch files (not in the repo)

Per instructions, nothing was written to `~/workspace/xiangqi`. Research artifacts live only in `/tmp` (ephemeral): `/tmp/pikafish.7z`, `/tmp/pikafish/` (extracted release incl. Linux binary + `pikafish.nnue`), `/tmp/uci.cpp`, `/tmp/ucioption.cpp`, `/tmp/pf_*.cpp`.