import { useMemo } from "react";

interface Props {
  result: string;
}

interface ParsedResult {
  run_id: string;
  source_kind: string;
  total_ms: number;
  stages: Array<{ name: string; elapsed_ms: number }>;
  terminal_outcome?: string;
  attempts?: Array<{
    attempt: number;
    elapsed_ms: number;
    outcome: string;
    item_count?: number;
    retry_delay_ms?: number;
    error?: string;
  }>;
  is_complete: boolean;
  skip: boolean;
  items: string[];
  positions: number[];
  debug: string;
}

export default function ResultView({ result }: Props) {
  const parsed = useMemo<ParsedResult | null>(() => {
    try {
      return JSON.parse(result);
    } catch {
      return null;
    }
  }, [result]);

  if (!parsed) {
    return (
      <div className="result-view">
        <div className="raw-result">{result}</div>
      </div>
    );
  }

  const itemNames = parsed.items.map((path) => {
    const parts = path.split("/");
    return parts[parts.length - 1];
  });

  return (
    <div className="result-view">
      <div className="status">
        {parsed.skip ? (
          <span className="skip">&#9888; Skipped (relic selection screen)</span>
        ) : parsed.is_complete ? (
          <span className="complete">&#10003; Complete ({parsed.items.length} cards)</span>
        ) : (
          <span className="partial">&#9888; Partial ({parsed.items.length} cards)</span>
        )}
      </div>

      {parsed.items.length > 0 && (
        <div className="items">
          <h3>Matched Items</h3>
          <ul>
            {itemNames.map((name, i) => (
              <li key={i}>
                <span className="item-name">{name}</span>
                <span className="item-pos">
                  x={parsed.positions[i]?.toFixed(3)}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {parsed.stages.length > 0 && (
        <div className="timing">
          <div className="timing-heading">
            <h3>Pipeline Timing</h3>
            <span>{parsed.total_ms} ms total</span>
          </div>
          <ul>
            {parsed.stages.map((stage) => (
              <li key={stage.name}>
                <span>{stage.name.replace(/_/g, " ")}</span>
                <span>{stage.elapsed_ms} ms</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {parsed.attempts && parsed.attempts.length > 0 && (
        <div className="attempts">
          <div className="timing-heading">
            <h3>Capture Attempts</h3>
            <span>{parsed.terminal_outcome ?? "finished"}</span>
          </div>
          <ul>
            {parsed.attempts.map((attempt) => (
              <li key={attempt.attempt}>
                <span>#{attempt.attempt} {attempt.outcome}</span>
                <span>
                  {attempt.elapsed_ms} ms
                  {attempt.retry_delay_ms ? `, retry ${attempt.retry_delay_ms} ms` : ""}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="debug">
        <h3>Debug Info</h3>
        <pre>{parsed.debug}</pre>
      </div>
    </div>
  );
}
