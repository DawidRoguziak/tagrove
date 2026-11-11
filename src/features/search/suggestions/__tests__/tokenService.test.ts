import { describe, expect, it } from "vitest";
import { collectUsedTags, normalizeTagToken, readActiveToken } from "../tokenService";

describe("tokenService", () => {
  it("normalizes tag tokens", () => {
    expect(normalizeTagToken("  -CaT  ")).toBe("cat");
  });

  it("reads active token around caret and keeps negative flag", () => {
    const value = "dog -ca bird";
    const token = readActiveToken(value, 6);

    expect(token).toEqual({
      start: 4,
      end: 7,
      query: "ca",
      negative: true
    });
  });

  it("returns null for empty or invalid token", () => {
    expect(readActiveToken("   ", 1)).toBeNull();
    expect(readActiveToken("-", 1)).toBeNull();
  });

  it("returns no active token while the anchored gN: rule consumes the input", () => {
    for (const value of ["gN:Trip 2026", "  gn:trip 2026", 'gN:"Trip 2026', "GN:", "gN:Trip 2026 "]) {
      for (let caret = 0; caret <= value.length; caret++) {
        expect(readActiveToken(value, caret)).toBeNull();
      }
    }
  });

  it("keeps active tokens when gN: is not at the start of the input", () => {
    expect(readActiveToken("cat gN:Trip 2026", 16)).toEqual({
      start: 12,
      end: 16,
      query: "2026",
      negative: false
    });
    expect(readActiveToken('"gn:trip" 2026', 14)).toEqual({
      start: 10,
      end: 14,
      query: "2026",
      negative: false
    });
    expect(readActiveToken("-gn:trip 2026", 13)).toEqual({
      start: 9,
      end: 13,
      query: "2026",
      negative: false
    });
  });

  it("collects used tags from value and excluded tags", () => {
    const used = collectUsedTags("cat -dog", ["  BIRD ", ""]);

    expect(used).toEqual(new Set(["cat", "dog", "bird"]));
  });
});
