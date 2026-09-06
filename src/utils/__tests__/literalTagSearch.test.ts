import { expect, it } from "vitest";
import { parseSearchFilter } from "../media";
import { serializeTagToken } from "../searchTokens";
import { buildFilterInput, putTagInMode } from "../../components/tag-list/services/tagListSelectionService";
import { applySuggestionToValue } from "../../components/search/services/inputService";
import { collectUsedTags, readActiveToken } from "../../components/search/services/tokenService";
import { buildTagSuggestions, createTagFuse } from "../../components/search/services/suggestionService";
import { buildFilterDescriptor } from "../../components/app/services/filterService";

const names = ["tags", "tags:3", "gn:trip", "-holiday", "-", 'say"hello', "path\\photo", '"', "\\", "normal"];
it.each(names)("round trips stored tag %s through Tag list, typed search and suggestion replacement", name => {
  for (const mode of ["include", "exclude"] as const) {
    const query = buildFilterInput(putTagInMode({ included: {}, excluded: {} }, name, mode));
    const expected = { mode: "tags", include: mode === "include" ? [name] : [], exclude: mode === "exclude" ? [name] : [], metaFilter: null, validationError: null };
    expect(parseSearchFilter(query)).toEqual(expected);
    const quoted = `${mode === "exclude" ? "-" : ""}${JSON.stringify(name)}`;
    expect(parseSearchFilter(quoted)).toEqual(expected);
    const draft = `cat ${mode === "exclude" ? "-" : ""}"ta bird`;
    const token = readActiveToken(draft, 7)!;
    const inserted = applySuggestionToValue(draft, token, name);
    expect(inserted.nextValue).toBe(`cat ${serializeTagToken(name, mode === "exclude")} bird`);
    expect(inserted.nextCaret).toBe(inserted.nextValue.length - 5);
    expect(collectUsedTags(query, [])).toEqual(new Set([name]));
    const filters = { mediaKind: "all" as const, favoritesOnly: false };
    expect(buildFilterDescriptor({ ...filters, filterInput: query })).toEqual(buildFilterDescriptor({ ...filters, filterInput: quoted }));
  }
});

it("keeps quoted operator names eligible for autocomplete without confusing literal minus names", () => {
  const token = readActiveToken('-"tags"', 5)!;
  expect(token).toMatchObject({ query: "tags", negative: true, literal: true });
  expect(buildTagSuggestions({ activeToken: token, usedTags: new Set(), fuse: createTagFuse(["tags", "tags:3"]) }).map(item => item.value)).toEqual(["tags", "tags:3"]);
  expect(collectUsedTags('"-holiday"', ["-trip"])).toEqual(new Set(["-holiday", "-trip"]));
});

it("preserves unquoted operators and rejects unfinished or invalid quoted tags", () => {
  expect(parseSearchFilter("tags:3").metaFilter).toEqual({ type: "hasNoTags", tagCount: 3 });
  expect(parseSearchFilter("gn:Trip 2026").metaFilter).toEqual({ type: "groupName", groupName: "Trip 2026" });
  expect(parseSearchFilter('-tags').exclude).toEqual(["tags"]);
  expect(parseSearchFilter('"tags" -"gn:trip"').validationError).toBeNull();
  for (const query of ['"tags', '"ta gs"', '"bad\\q"', '"tags"suffix', '""']) {
    expect(parseSearchFilter(query).validationError).toBe("tagInvalidCharacters");
  }
});
