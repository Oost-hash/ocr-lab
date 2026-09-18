# Candidate Pipeline Scaffold

## Goal

Keep the production pipeline unchanged and provide a small, independent starting
point for a future replacement. OCR Lab can optionally run the candidate beside
production and show both in the existing live hero.

The initial candidate has no implementation. It returns `not_implemented` so a
maintainer can add Trigger, Capture, and Rendering behavior incrementally.

## Layout

```text
src-tauri/src/candidate/
  mod.rs
  pipeline.rs
```

Candidate code must not be added to `prod-code/` or `lab/`.

## Frontend

Two checkboxes select which pipeline to run. **Baseline** is enabled by default;
**Candidate** is disabled by default. Selecting both enables parallel mode and
shows both rows:

```text
Production  Trigger -> Wait -> Capture -> Era OCR -> Payload -> Paint -> Dismiss
Candidate   Trigger -> Not yet implemented
```

Candidate-only mode currently returns `not_implemented` without starting
Production. Enabling both must not alter production decisions, output, or
rendering.

Each row ends with **Total**, measured from the shared trigger through frontend
paint. In parallel mode, the slower total is red. A failed or
`not_implemented` pipeline loses to a successful pipeline and is red without
treating its missing duration as zero.

## Result Files

When the candidate checkbox is enabled, each observed relic-picker trigger
produces two files with the same schema:

```text
results/<trigger-id>/baseline.json
results/<trigger-id>/candidate.json
```

Both files are created when the shared trigger is observed. `candidate.json`
immediately reports `not_implemented`; `baseline.json` is updated after OCR and
again after frontend paint or failure evidence arrives.

```json
{
  "schema_version": 1,
  "pipeline": "baseline",
  "trigger_id": "picker-001",
  "status": "success",
  "total_ms": 518,
  "phases": {
    "trigger": { "status": "complete", "duration_ms": 0 },
    "capture": {
      "status": "complete",
      "duration_ms": 452,
      "era": "MESO",
      "raw_ocr": "MESO ERA",
      "screenshot": null
    },
    "rendering": {
      "status": "complete",
      "duration_ms": 66,
      "payload_count": 9,
      "rendered_count": 3
    }
  },
  "error": null
}
```

The candidate uses the same fields. Until implemented it returns:

```json
{
  "schema_version": 1,
  "pipeline": "candidate",
  "trigger_id": "picker-001",
  "status": "not_implemented",
  "total_ms": 0,
  "phases": {
    "trigger": { "status": "complete", "duration_ms": 0 },
    "capture": { "status": "not_implemented", "duration_ms": null },
    "rendering": { "status": "not_reached", "duration_ms": null }
  },
  "error": "Candidate pipeline is not yet implemented"
}
```

## Data Policy

Keep only data needed to compare correctness and latency:

- The shared trigger ID.
- Status and duration for Trigger, Capture, and Rendering.
- OCR result and raw OCR text.
- A screenshot reference when something fails.
- Payload and rendered counts.
- A specific error reason.

Do not duplicate full catalogs, complete cache snapshots, repeated frontend
events, or full payloads in each result file.
