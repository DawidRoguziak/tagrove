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

  it("collects used tags from value and excluded tags", () => {
    const used = collectUsedTags("cat -dog", ["  BIRD ", ""]);

    expect(used).toEqual(new Set(["cat", "dog", "bird"]));
  });
});
