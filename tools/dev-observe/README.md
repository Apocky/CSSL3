# § dev-observe

Local-only development evidence ledger for CSSLv3/LoA work.

## Commands

```powershell
python tools/dev-observe/dev_observe.py run --repo CSSLv3 --phase P0 --cwd compiler-rs -- cargo test -p csslc linker
python tools/dev-observe/dev_observe.py review --limit 20
python tools/dev-observe/dev_observe.py latest
python tools/render-v3/v3_present_bench.py self-test
python tools/render-v3/v3_present_bench.py analyze --log compiler-rs/target/v3_depthresolve_5000.log --image compiler-rs/target/present_capture_v3_world1024_active128_depthresolve.ppm --compare-image compiler-rs/target/present_capture_v3_world1024_active128_depthresolve_yaw500.ppm --out-json compiler-rs/target/v3_depthresolve_bench_report.json --print-csl --fail-on-budget
cd compiler-rs; $env:LOA_RENDER_V3='1'; $env:LOA_V3_MIXED_BENCH='1'; $env:LOA_V3_PRESENT_TELEMETRY='1'; $env:LOA_QUICK_QUIT='5000'; .\target\debug\loa-runtime.exe *> .\target\v3_depthresolve_mixed_5000.log
cd ..; python tools/render-v3/v3_present_bench.py analyze --log compiler-rs/target/v3_depthresolve_mixed_5000.log --out-json compiler-rs/target/v3_depthresolve_mixed_bench_report.json --print-csl --fail-on-budget
python tools/render-v3/v3_present_bench.py run --frames 1500 --mixed-bench --window-mode windowed --send-input --analyze --print-csl --fail-on-budget
tools/render-v3/run_graphical_bench.cmd -Quick
tools/render-v3/run_graphical_bench.cmd -Frames 5000 -Open
tools/render-v3/run_graphical_bench.cmd -StressRoom hair -Frames 5000 -Open
tools/render-v3/run_stress_lab_bench.cmd -Quick -Open
tools/render-v3/run_stress_lab_bench.cmd -Frames 5000 -Open
python tools/dev-observe/dev_observe.py run --repo CSSLv3 --phase p1.graphical_bench_wrapper_quick -- tools/render-v3/run_graphical_bench.cmd -Quick -NoBuild -AllowBudgetFail
```

## Output

- `tools/dev-observe/logs/dev_runs.jsonl` — append-only run events
- `tools/dev-observe/logs/latest.json` — latest event snapshot
- `compiler-rs/target/v3_*_bench_report.json` — Render V3 benchmark verdicts with p99/p99.9 gates and evidence gaps
- `compiler-rs/target/v3_graphical_bench_*_dashboard.html` — operator-facing graphical benchmark dashboard with capture/diff previews
- `compiler-rs/target/v3_stress_lab_*_menu.html` — operator stress-lab menu linking per-room dashboards
- `compiler-rs/target/v3_stress_lab_*_suite.json` — suite summary across selected stress rooms
- MCP tools in `tools/grok-mcp-harness/dev_observe_tools.py` read the same ledger.

## Discipline

- Raw absolute paths are redacted to repo labels before logging.
- Full command/cwd are represented by short hashes.
- Review output leads with failures, warnings, slow feedback loops, and next actions.
- Render V3 evidence is classified separately from readiness. Passing 144/240 Hz budgets is not a production claim unless full-frame artifacts, state diffs, mixed simulation/input telemetry, and accepted utilization evidence are present.
- `LOA_V3_MIXED_BENCH=1` adds deterministic synthetic gameplay/input perturbation fields. It removes the "no mixed sim" gap, but it is still synthetic input; real raw-input latency/jitter remains a separate gate.
- V3 telemetry includes process CPU/RAM fields on Windows and real `raw_input_latency_us` only when actual winit keyboard/mouse events occurred before the frame. Automated quick-quit runs usually keep the real-input gap open.
- `run --send-input` attempts controlled OS-window key injection. Treat it as diagnostic only unless the report actually contains `raw_input_latency_us` rows.
- `run_graphical_bench.cmd` is the post-pass operator command. It builds `loa-runtime`, opens the graphical V3 path, captures yaw A/B full frames, runs a readback-free timing pass, generates JSON/CSL evidence, converts PPM artifacts to BMP previews, and writes an HTML dashboard.
- `-Quick` is a fast local sanity pass, not a release-grade production claim. Use `-Frames 5000 -Open` for the current full operator gate.
- Budget failures still generate the report/dashboard/images before returning a non-zero exit. Add `-AllowBudgetFail` when the goal is to collect failed-run evidence through `dev_observe` without stopping the outer command.
- `LOA_V3_STRESS_ROOM` / `-StressRoom` selects actual V3 crystal-field layouts: `shells`, `printer`, `material`, `hair`, `portal`, `arena`.
- Reports include `active_culling_proof` when the graphical runner declares the deterministic 1024-world → bounded-active-scene culling invariant; this replaces the old brute-force active≥1000 evidence gap without pretending the active cap is unbounded.
- `run_stress_lab_bench.cmd` runs multiple stress rooms and emits a menu dashboard. Treat it as the post-pass comparison suite; quick mode is sanity, full 5000-frame mode is the current benchmark gate.
