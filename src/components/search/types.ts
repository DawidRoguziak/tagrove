export interface ActiveToken {
  start: number;
  end: number;
  query: string;
  negative: boolean;
}

export interface TagSuggestion {
  value: string;
  indices: ReadonlyArray<readonly [number, number]>;
}
