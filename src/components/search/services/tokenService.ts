import type { ActiveToken } from "../types";

export function normalizeTagToken(token: string): string {
  return token.trim().toLowerCase().replace(/^-/, "");
}

export function readActiveToken(input: string, caret: number): ActiveToken | null {
  const position = Math.max(0, Math.min(caret, input.length));

  let start = position;
  while (start > 0 && !/\s/.test(input[start - 1])) {
    start -= 1;
  }

  let end = position;
  while (end < input.length && !/\s/.test(input[end])) {
    end += 1;
  }

  const rawToken = input.slice(start, end).trim();
  if (!rawToken) {
    return null;
  }

  const negative = rawToken.startsWith("-");
  const query = normalizeTagToken(rawToken);
  if (!query) {
    return null;
  }

  return { start, end, query, negative };
}

export function collectUsedTags(value: string, excludedTags: string[]): Set<string> {
  const valueTokens = value
    .split(/\s+/)
    .map((token) => normalizeTagToken(token))
    .filter(Boolean);

  const excludedTokens = excludedTags
    .map((token) => normalizeTagToken(token))
    .filter(Boolean);

  return new Set([...valueTokens, ...excludedTokens]);
}
