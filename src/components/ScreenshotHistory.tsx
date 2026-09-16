import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface ScreenshotRecord {
  id: string;
  filename: string;
  sha256: string;
  preprocess: boolean;
  timestamp: string;
}

interface Props {
  onSelect?: (record: ScreenshotRecord) => void;
}

export default function ScreenshotHistory({ onSelect }: Props) {
  const [records, setRecords] = useState<ScreenshotRecord[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadHistory();
  }, []);

  const loadHistory = async () => {
    try {
      const data = await invoke<ScreenshotRecord[]>("load_screenshot_history");
      setRecords(data);
    } catch (e) {
      console.error("Failed to load history:", e);
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await invoke("delete_screenshot_record", { id });
      setRecords((prev) => prev.filter((r) => r.id !== id));
    } catch (e) {
      console.error("Failed to delete record:", e);
    }
  };

  const formatDate = (timestamp: string) => {
    return new Date(timestamp).toLocaleString();
  };

  if (loading) {
    return (
      <div className="screenshot-history">
        <div className="loading">Loading history...</div>
      </div>
    );
  }

  if (records.length === 0) {
    return (
      <div className="screenshot-history">
        <div className="empty">No history yet</div>
      </div>
    );
  }

  return (
    <div className="screenshot-history">
      <h3>History ({records.length})</h3>
      <ul>
        {records.map((record) => (
          <li key={record.id}>
            <button
              className="select-btn"
              onClick={() => onSelect?.(record)}
            >
              {record.filename}
            </button>
            <span className="timestamp">{formatDate(record.timestamp)}</span>
            <span className="preprocess">
              {record.preprocess ? "PP" : "RAW"}
            </span>
            <button
              className="delete-btn"
              onClick={() => handleDelete(record.id)}
            >
              &times;
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
