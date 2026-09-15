import type { MatchedItem } from "../match/types.js";

export function renderRewardCards(
  container: HTMLElement,
  matches: MatchedItem[],
  imageWidth: number,
  imageHeight: number,
): void {
  if (!matches || matches.length === 0) {
    container.innerHTML = `<div class="ov-empty">No items matched</div>`;
    return;
  }

  const cardW = Math.max(80, Math.round(imageWidth * 0.127 - 10));
  const cardTop = Math.round(imageHeight * 0.03);

  container.innerHTML = matches.map((m, i) => {
    const x = Math.round(m.xCenter * imageWidth - cardW / 2);
    const isBest = i === 0;
    return `
      <div class="ov-card" style="left:${x}px; top:${cardTop}px; width:${cardW}px;">
        ${isBest ? `<div class="ov-arrow">&#9650;</div>` : ""}
        <div class="ov-name">${escapeHtml(m.name)}</div>
        <div class="ov-score">${(m.score * 100).toFixed(0)}% match</div>
      </div>
    `;
  }).join("");
}

function escapeHtml(value: string): string {
  const el = document.createElement("span");
  el.textContent = value;
  return el.innerHTML;
}
