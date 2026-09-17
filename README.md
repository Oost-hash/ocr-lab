# FrameForge OCR Lab

## Production snapshot

`src-tauri/src/prod-code/` contains byte-for-byte copies from
`../project/src-tauri/src/` (`C:\frameforge\project` in this workspace).
The original relative file paths and line endings are preserved.
Do not add instrumentation or lab adapters inside this directory.

From the lab root:

```powershell
pnpm prod:sync   # Refresh the selected files from the project working tree
pnpm prod:check  # Verify every copied byte and the recorded SHA-256 hashes
```

`prod-code-manifest.json` records the source commit, relevant working-tree
changes, and each file's hash. The working-tree files are the source of truth;
the commit alone does not identify a snapshot if the source has local changes.
The sync command rejects unexpected files in `prod-code` rather than removing
them. `.gitattributes` disables Git line-ending conversion for these copies.

### Live production cycle

1. Start Warframe and the lab (`pnpm tauri dev`).
2. Open the live tab and select **Start live run** before opening a relic.
3. Complete a relic run. The original watcher reads the actual
   `%LOCALAPPDATA%\Warframe\EE.log` and captures the actual Warframe window.
4. Inspect the timeline, including the reward and dismiss events.
5. Select **Stop** when finished. Restart the lab for another recording run;
   a run can contain multiple relic cycles. This prevents outstanding production
   safety timers from affecting a restarted run in the same process.

The Control Panel is the owner window. Closing it exits the entire OCR Lab
process, including Virtual Game, overlays, watchers and backend threads. Closing
an auxiliary window does not exit the lab.

The live run observes both production relic paths. Opening Warframe's relic
selection grid runs the production era OCR and relic recommendation overlay,
which makes that path testable without completing a mission. The reward-card
capture/OCR path still starts only when the post-mission reward screen appears.
Inventory quantities are projected from production's persisted
`inventory_state_cache.json` using the same startup inclusion rules as FrameForge.

The memory trigger is optional and defaults to off, as it does in production.
It uses the original `monitor` functions and platform implementation. It
prepares the overlay; EE.log still starts the OCR reward session.

The live path now executes the original reward watcher, squad-hint wait,
capture, `extract_reward_items_twophase`, retry/confirmation rules and cleanup
timers. The original `useOverlays` hook, `Overlay.tsx`, catalog construction,
targeted item lookup and window show/hide functions handle frontend delivery.
Production files remain unchanged. `build.rs` selects complete Rust items by
name and copies their original source bytes into `OUT_DIR`; this avoids pulling
in unrelated trade, inventory-scanning and network services. It fails if a
selected item is missing. `lab/production_modules.rs` supplies imports and
delegating observation wrappers.

### Measurements and run files

The configured runs directory receives a `live-<timestamp>-<pid>` folder:

- `timeline.jsonl`: timestamps from a single backend monotonic clock, UTC,
  the current observed cycle number, and event details.
- `inputs.json`: the cached data used to initialize the run.
- `production-captures/`: the original production diagnostic captures.
- `temp/`: original session logs, memory scan diagnostics and debug screenshots.
- `inventory-changes.log`: reward inventory changes within the lab.

`TEMP` and `TMP` are isolated for the lab process before worker startup, so the
original production diagnostic filenames do not collide with another app.
Inventory writes affect lab state and lab logs. FrameForge caches are read-only.

The timeline records:

- Relevant EE.log batches when the original watcher processes them, followed
  by accepted triggers, original trigger lines and catalog prefilter context.
- Squad-hint wait, each capture/OCR attempt, original OCR tracing spans and
  the original retry diagnostics.
- `relic-trigger`, `relic-rewards`, frontend diagnostics, data IPC receipt,
  window moves, DOM card commits and paint opportunities.
- Dismissal, delayed offscreen movement and safety cleanup.

An event's cycle number is the cycle active **when observed**, not guaranteed
ownership: an old production safety timer can fire during a later cycle.
The global monotonic clock retains that ordering. Memory scan durations remain
in the original session log. The UI shows the latest 100 recorded events;
the JSONL file keeps the full recording. Disk-write errors are shown in the UI.

DOM commits and double-requestAnimationFrame markers do not prove physical
display visibility. Compare them with window moves and a screen recording.
**Mark game reward screen** adds a manual reference including reaction time;
it is not a game-synchronized timestamp. EE.log timestamps are retained as text
and are not assumed to share the backend clock.

### Lab boundaries

The lab bootstraps from local item, relic-reward, recipe, quantity and price
caches. Missing required item/reward caches prevent startup; optional missing
data is recorded. Original catalog construction and corrections run against
that data, but the ExportRecipes blueprint map starts empty, and no production
inventory worker, live crafting updates, cache refresh or network price calls
run. Cache-only prices can be stale; their date is recorded. These differences
matter when comparing with a fully populated production app. Instrumentation
also has a small overhead.

The live mode uses the original fixed EE.log path and timing rules, not the old
lab `ee_log`/`trigger_delay_ms` settings. The former observer remains in
`lab/live_trigger.rs` for reference but is not compiled or started.

Static file/Virtual Game runs still use `lab/ocr.rs`, a detailed timing wrapper
around production primitives (`execution_kind: lab_instrumented_production_ocr`).
Their retry loop and tool overlay are lab behavior, independent of the live path.

## Checks

```powershell
pnpm prod:check
cargo check --manifest-path src-tauri/Cargo.toml
pnpm build
```
