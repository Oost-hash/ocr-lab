import type { MatchedItem } from "./types.js";

// Match logic runs on the Rust side via the pipeline command.
// This module provides types and helper functions for the frontend.

export type { MatchedItem } from "./types.js";

export function sortByScore(items: MatchedItem[]): MatchedItem[] {
  return [...items].sort((a, b) => b.score - a.score);
}

export function topN(items: MatchedItem[], n: number): MatchedItem[] {
  return sortByScore(items).slice(0, n);
}
