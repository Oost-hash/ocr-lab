export interface CatalogEntry {
  uniqueName: string;
  name: string;
  rarity: string;
}

export interface MatchedItem {
  name: string;
  score: number;
  xCenter: number;
  yCenter: number;
}
