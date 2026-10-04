import { describe, expect, it } from "vitest";
import { buildTagSuggestions, createTagFuse } from "../suggestionService";

describe("suggestionService", () => {
  it("returns empty list when there is no active token", () => {
    const fuse = createTagFuse(["cat", "car"]);

    const suggestions = buildTagSuggestions({
      activeToken: null,
      usedTags: new Set(),
      fuse
    });

    expect(suggestions).toEqual([]);
  });

  it("deduplicates suggestions and excludes already used tags", () => {
    const fuse = createTagFuse(["Cat", "cat", "car", "camera"]);

    const suggestions = buildTagSuggestions({
      activeToken: { start: 0, end: 2, query: "ca", negative: false },
      usedTags: new Set(["car"]),
      fuse,
      suggestionLimit: 10
    });

    expect(suggestions.map((entry) => entry.value.toLowerCase())).toContain("cat");
    expect(suggestions.map((entry) => entry.value.toLowerCase())).not.toContain("car");
    expect(new Set(suggestions.map((entry) => entry.value.toLowerCase())).size).toBe(suggestions.length);
  });

  it("keeps exact query available even if query is already used", () => {
    const fuse = createTagFuse(["cat"]);

    const suggestions = buildTagSuggestions({
      activeToken: { start: 0, end: 3, query: "cat", negative: false },
      usedTags: new Set(["cat"]),
      fuse
    });

    expect(suggestions).toHaveLength(1);
    expect(suggestions[0].value).toBe("cat");
  });

  it("respects suggestion limit", () => {
    const fuse = createTagFuse(["cat", "car", "camera", "cabin"]);

    const suggestions = buildTagSuggestions({
      activeToken: { start: 0, end: 2, query: "ca", negative: false },
      usedTags: new Set(),
      fuse,
      suggestionLimit: 2
    });

    expect(suggestions).toHaveLength(2);
  });

  it("does not build suggestions for has-no-tags metatag queries", () => {
    const fuse = createTagFuse(["tags", "tagstone", "cat"]);

    const suggestions = buildTagSuggestions({
      activeToken: { start: 0, end: 4, query: "tags", negative: false },
      usedTags: new Set(),
      fuse
    });

    expect(suggestions).toEqual([]);
  });

  it("does not build suggestions for group-name metatag queries", () => {
    const fuse = createTagFuse(["gn:trip", "trip-2026", "cat"]);

    const suggestions = buildTagSuggestions({
      activeToken: { start: 0, end: 12, query: "gN:trip-2026", negative: false },
      usedTags: new Set(),
      fuse
    });

    expect(suggestions).toEqual([]);
  });
});

