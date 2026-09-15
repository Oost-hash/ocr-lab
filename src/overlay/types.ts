import type { MatchedItem } from "../match/types.js";

export interface OverlayPayload {
  matches: MatchedItem[];
  imageWidth: number;
  imageHeight: number;
}

export interface OverlayState {
  visible: boolean;
  matches: MatchedItem[];
}
