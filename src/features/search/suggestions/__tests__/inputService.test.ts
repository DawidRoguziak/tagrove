import { describe, expect, it } from "vitest";
import { applySuggestionToValue } from "../inputService";

describe("inputService", () => {
  it("replaces active token and keeps negative prefix", () => {
    const result = applySuggestionToValue(
      "dog -ca bird",
      { start: 4, end: 7, query: "ca", negative: true },
      "cat"
    );

    expect(result).toEqual({
      nextValue: "dog -cat bird",
      nextCaret: 8
    });
  });

  it("replaces active token without negative prefix", () => {
    const result = applySuggestionToValue(
      "ca dog",
      { start: 0, end: 2, query: "ca", negative: false },
      "car"
    );

    expect(result).toEqual({
      nextValue: "car dog",
      nextCaret: 3
    });
  });
});
