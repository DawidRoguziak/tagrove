export interface SelectionRange {
  startIndex: number;
  endIndex: number;
}

export type BulkSelectionInteraction =
  | { type: "click"; assetId: number; assetIndex: number; ctrlLike: boolean; shift: boolean }
  | { type: "clear" }
  | { type: "rectangle-start" }
  | { type: "rectangle-cancel" }
  | { type: "rectangle-commit"; ranges: SelectionRange[]; additive: boolean };

export type BulkSelectionHandler = (interaction: BulkSelectionInteraction) => void | Promise<void>;
