import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface TraceEvent {
  event: string;
  elapsed_us: number;
  cycle: number;
  detail: unknown;
}

type StageState = "complete" | "failed" | "active" | "pending";

interface PipelineStage {
  key: string;
  label: string;
  description: string;
  state: StageState;
  at?: number;
  note?: string;
}

interface Snapshot {
  started: boolean;
  active: boolean;
  overlay_ready: boolean;
  path: string;
  events: TraceEvent[];
  write_error: string | null;
}

function detailRecord(event: TraceEvent | undefined): Record<string, unknown> | null {
  if (!event || typeof event.detail !== "object" || event.detail === null || Array.isArray(event.detail)) return null;
  return event.detail as Record<string, unknown>;
}

function findEvent(events: TraceEvent[], name: string) {
  return events.find((event) => event.event === name);
}

function findLastEvent(events: TraceEvent[], name: string, before = Number.POSITIVE_INFINITY) {
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const event = events[index];
    if (event.event === name && event.elapsed_us <= before) return event;
  }
  return undefined;
}

function buildPipeline(events: TraceEvent[]): PipelineStage[] {
  const trigger = findEvent(events, "ee_trigger_accepted");
  const hints = findEvent(events, "squad_wait_end");
  const captureStart = findEvent(events, "capture_ocr_start");
  const capture = events.find((event) => {
    const detail = detailRecord(event);
    return event.event === "ocr_span_end" && detail?.span === "capture_warframe_reward_area";
  });
  const confirmed = findEvent(events, "rewards_confirmed");
  const ocr = findLastEvent(events, "capture_ocr_end", confirmed?.elapsed_us);
  const rewards = findEvent(events, "relic-rewards");
  const overlay = events.find((event) => {
    if (event.event !== "frontend:cards_paint_opportunity") return false;
    const names = detailRecord(event)?.names;
    return Array.isArray(names) && names.length > 0;
  });
  const dismiss = findEvent(events, "dismiss_received") ?? findEvent(events, "auto_dismiss");
  const offscreen = events.find((event) => {
    const detail = detailRecord(event);
    return event.event === "overlay_window_moved" && detail?.offscreen === true
      && event.elapsed_us >= (dismiss?.elapsed_us ?? Number.POSITIVE_INFINITY);
  });
  const attempts = events.filter((event) => event.event === "capture_ocr_start").length;
  const stage = (
    key: string,
    label: string,
    description: string,
    complete: TraceEvent | undefined,
    active: boolean,
    note?: string,
  ): PipelineStage => ({
    key,
    label,
    description,
    state: complete ? "complete" : active ? "active" : "pending",
    at: complete?.elapsed_us,
    note,
  });

  return [
    stage("trigger", "Trigger", "EE.log accepted", trigger, false),
    stage("hints", "Hints", "Squad data", hints, Boolean(trigger && !hints), detailRecord(hints)?.hint ? `squad ${detailRecord(hints)?.hint}` : undefined),
    stage("capture", "Capture", "Warframe pixels", capture, Boolean(hints && captureStart && !capture)),
    stage("ocr", "OCR", "Match and retries", confirmed ? ocr : undefined, Boolean(capture && !confirmed), attempts ? `${attempts} attempt${attempts === 1 ? "" : "s"}` : undefined),
    stage("confirm", "Confirm", "Rewards locked", confirmed, false),
    stage("overlay", "Overlay", "Cards paint-ready", overlay, Boolean((confirmed ?? rewards) && !overlay)),
    stage("dismiss", "Dismiss", "Window off-screen", offscreen, Boolean(dismiss)),
  ];
}

function eventMessage(event: TraceEvent | undefined) {
  const message = detailRecord(event)?.message;
  return typeof message === "string" ? message : "";
}

function buildRelicPickerPipeline(events: TraceEvent[]): PipelineStage[] {
  const trigger = findEvent(events, "relic_picker_detected") ?? findEvent(events, "relic_picker_test_trigger");
  const captureStart = events.find((event) => event.event === "ocr_span_start"
    && detailRecord(event)?.span === "capture_warframe_pixels");
  const capture = events.find((event) => event.event === "ocr_span_end"
    && detailRecord(event)?.span === "capture_warframe_pixels");
  const ocr = findEvent(events, "relic_picker_ocr_result");
  const ocrFailed = eventMessage(ocr).endsWith("None");
  const payload = findEvent(events, "relic_picker_payload_ready");
  const open = findEvent(events, "relic-pick-open");
  const paint = findEvent(events, "frontend:relic_picker_paint_opportunity");
  const dismiss = events.find((event) => event.event === "relic_picker_dismiss_detected"
    && event.elapsed_us >= (paint?.elapsed_us ?? open?.elapsed_us ?? Number.POSITIVE_INFINITY));
  const era = eventMessage(ocr).match(/Some\(\"([^\"]+)\"\)/)?.[1];
  const relics = detailRecord(open)?.relics;
  const renderedCards = detailRecord(paint)?.rendered_cards;
  const stage = (
    key: string,
    label: string,
    description: string,
    complete: TraceEvent | undefined,
    active: boolean,
    note?: string,
    failed = false,
  ): PipelineStage => ({
    key,
    label,
    description,
    state: failed ? "failed" : complete ? "complete" : active ? "active" : "pending",
    at: complete?.elapsed_us,
    note,
  });

  return [
    stage("trigger", "Trigger", "EE.log marker", trigger, false),
    stage("wait", "Wait", "Render delay", captureStart, Boolean(trigger && !captureStart), captureStart ? "production 400 ms" : undefined),
    stage("capture", "Capture", "Top-left pixels", capture, Boolean(captureStart && !capture)),
    stage("ocr", "Era OCR", "Lith / Meso / Neo / Axi", ocr, Boolean(capture && !ocr), ocrFailed ? "no era detected" : era, ocrFailed),
    stage("payload", "Payload", "Inventory recommendations", payload ?? open, Boolean(ocr && !ocrFailed && !payload && !open), Array.isArray(relics) ? `${relics.length} relics` : undefined),
    stage("paint", "Paint", "Recommendation cards", paint, Boolean(open && !paint), typeof renderedCards === "number" ? `${renderedCards} cards` : undefined),
    stage("dismiss", "Dismiss", "Picker closed", dismiss, Boolean(paint && !dismiss)),
  ];
}

function formatDuration(microseconds: number) {
  const milliseconds = microseconds / 1000;
  if (milliseconds < 1000) return `${milliseconds.toFixed(milliseconds < 10 ? 1 : 0)} ms`;
  return `${(milliseconds / 1000).toFixed(2)} s`;
}

export default function LivePipeline() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [memoryTrigger, setMemoryTrigger] = useState(false);
  useEffect(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    const refresh = async () => {
      try {
        const value = await invoke<Snapshot>("get_live_snapshot");
        if (!disposed) setSnapshot(value);
      } catch (error) { if (!disposed) setError(String(error)); }
      if (!disposed) timer = setTimeout(refresh, 500);
    };
    void refresh();
    return () => { disposed = true; clearTimeout(timer); };
  }, []);

  const run = async (command: string) => {
    setBusy(true);
    setError(null);
    try {
      await invoke(command, { memoryTrigger });
      setSnapshot(await invoke<Snapshot>("get_live_snapshot"));
    } catch (error) { setError(String(error)); }
    finally { setBusy(false); }
  };

  const latestCycle = snapshot?.events.reduce((latest, event) => Math.max(latest, event.cycle), 0) ?? 0;
  const cycleEvents = latestCycle > 0
    ? snapshot?.events.filter((event) => event.cycle === latestCycle) ?? []
    : [];
  const allEvents = snapshot?.events ?? [];
  const latestPickerIndex = allEvents
    .map((event) => event.event === "relic_picker_detected" || event.event === "relic_picker_test_trigger")
    .lastIndexOf(true);
  const latestRewardTrigger = findLastEvent(allEvents, "ee_trigger_accepted");
  const latestPickerTrigger = latestPickerIndex >= 0 ? allEvents[latestPickerIndex] : undefined;
  const pickerActive = Boolean(latestPickerTrigger
    && (!latestRewardTrigger || latestPickerTrigger.elapsed_us > latestRewardTrigger.elapsed_us));
  const displayEvents = pickerActive ? allEvents.slice(latestPickerIndex) : cycleEvents;
  const stages = pickerActive ? buildRelicPickerPipeline(displayEvents) : buildPipeline(displayEvents);
  const triggerAt = stages[0].at;
  const attempts = displayEvents.filter((event) => event.event === "capture_ocr_start").length;
  const lastEvent = displayEvents[displayEvents.length - 1];
  const relicPickerOpen = snapshot ? findLastEvent(snapshot.events, "relic-pick-open") : undefined;
  const lastStageAt = stages.reduce<number | undefined>((latest, stage) => stage.at === undefined
    ? latest : Math.max(latest ?? stage.at, stage.at), undefined);
  const elapsed = triggerAt !== undefined && lastStageAt !== undefined
    ? lastStageAt - triggerAt
    : undefined;
  let previousAt = triggerAt;

  return <div className="content live-scanner">
    <header className="live-header">
      <div>
        <span className="live-eyebrow">Production path observer</span>
        <h2>Relic pipeline latency</h2>
        <p>Relic picker: EE.log → era OCR → recommendations. Rewards: capture/OCR → overlay.</p>
      </div>
      <div className={`recording-state ${snapshot?.active ? "is-active" : ""}`}>
        <span className="recording-dot" />
        {snapshot?.active ? "Recording" : snapshot?.started ? "Stopped" : "Standby"}
      </div>
    </header>

    <div className="live-toolbar">
      <label className="checkbox-label">
        <input type="checkbox" checked={memoryTrigger} disabled={busy || snapshot?.started}
          onChange={(event) => setMemoryTrigger(event.target.checked)} />
        Early memory trigger
      </label>
      <button disabled={busy || !snapshot || snapshot.started} onClick={() => void run("start_live_production")}>Start live run</button>
      <button className="secondary" disabled={busy || !snapshot?.active} onClick={() => void run("stop_live_production")}>Stop</button>
      <button className="secondary" disabled={!snapshot?.active} onClick={() => {
        void invoke("record_lab_frontend", { name: "game_reward_screen_manual_marker", detail: {
          browser_epoch_ms: performance.timeOrigin + performance.now(), source: "manual click; includes reaction time",
        } }).catch((error) => setError(String(error)));
      }}>Mark screen visible</button>
    </div>
    {error && <div className="error">{error}</div>}
    {snapshot?.write_error && <div className="error">Timeline write failed: {snapshot.write_error}</div>}

    <section className="cycle-overview" aria-label="Latest relic pipeline">
      <div className="cycle-heading">
        <div>
          <span className="section-kicker">Latest pipeline</span>
          <h3>{pickerActive ? "Relic picker attempt" : latestCycle > 0 ? `Reward cycle ${latestCycle}` : snapshot?.active ? "Armed — waiting for relic picker or reward screen" : "Waiting for live run"}</h3>
        </div>
        <div className="cycle-metrics">
          <div><span>Elapsed</span><strong>{elapsed === undefined ? "—" : formatDuration(elapsed)}</strong></div>
          <div><span>{pickerActive ? "Pipeline" : "OCR attempts"}</span><strong>{pickerActive ? "Picker" : attempts || "—"}</strong></div>
          <div><span>Last signal</span><strong>{lastEvent?.event.replace("frontend:", "") ?? "No cycle events"}</strong></div>
        </div>
      </div>

      <ol className="pipeline-rail">
        {stages.map((stage) => {
          const delta = stage.at !== undefined && previousAt !== undefined ? stage.at - previousAt : undefined;
          if (stage.at !== undefined) previousAt = stage.at;
          return <li key={stage.key} className={`pipeline-stage stage-${stage.state}`}>
            <div className="stage-node" aria-hidden="true"><span /></div>
            <div className="stage-copy">
              <div className="stage-title-row">
                <strong>{stage.label}</strong>
                <span className="stage-state">{stage.state}</span>
              </div>
              <span className="stage-description">{stage.description}</span>
              <div className="stage-timing">
                {stage.at !== undefined && triggerAt !== undefined
                  ? <><b>+{formatDuration(stage.at - triggerAt)}</b>{delta !== undefined && delta > 0 && <span>Δ {formatDuration(delta)}</span>}</>
                  : <b>{stage.state === "active" ? "in progress" : "—"}</b>}
              </div>
              {stage.note && <span className="stage-note">{stage.note}</span>}
            </div>
          </li>;
        })}
      </ol>
    </section>

    <div className="live-status-strip">
      <span><i className={snapshot?.overlay_ready ? "ok" : "cold"} />Overlay {snapshot?.overlay_ready ? "initialized" : "cold"}</span>
      <span><i className={relicPickerOpen ? "ok" : "neutral"} />Relic picker {relicPickerOpen ? "overlay emitted" : "waiting"}</span>
      <span><i className={memoryTrigger ? "ok" : "neutral"} />Memory trigger {memoryTrigger ? "enabled" : "off"}</span>
      <span><i className={snapshot?.active ? "ok" : "neutral"} />{snapshot?.started ? "Run initialized" : "Not started"}</span>
    </div>
    {snapshot?.path && <p>Run files: <code>{snapshot.path}</code></p>}
    <p className="measurement-note">Paint-ready is a frontend marker, not proof of physical display. Use a recording or the manual marker for game-screen correlation.</p>
    <div className="live-timeline">
      <div className="timeline-heading">
        <div><span className="section-kicker">Evidence</span><h3>Raw timeline</h3></div>
        <span>Latest 100 events</span>
      </div>
      <table>
        <thead><tr><th>Cycle</th><th>Run ms</th><th>Event</th><th>Details</th></tr></thead>
        <tbody>{snapshot?.events.slice(-100).map((event, index) => <tr key={`${event.elapsed_us}-${index}`}>
          <td>{event.cycle}</td><td>{(event.elapsed_us / 1000).toFixed(1)}</td><td>{event.event}</td>
          <td><details><summary>Details</summary><pre>{JSON.stringify(event.detail, null, 2)}</pre></details></td>
        </tr>)}</tbody>
      </table>
    </div>
  </div>;
}
