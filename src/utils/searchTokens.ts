/** Search syntax only. Stored tag names are passed to IPC without these quotes. */
export function serializeTagToken(tag: string, negative = false): string {
  const literal = /^-|^(?:tags(?::.*)?|gn:.*)$/i.test(tag) || /["\\]/.test(tag);
  const value = literal ? `"${tag.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"` : tag;
  return `${negative ? "-" : ""}${value}`;
}

export function readTagToken(raw: string) {
  const negative = raw.startsWith("-") && raw.length > 1;
  const body = negative ? raw.slice(1) : raw;
  const quoted = body.startsWith('"');
  if (!quoted) return { value: body.toLowerCase(), negative, quoted, complete: true };
  let value = "";
  for (let index = 1; index < body.length; index++) {
    const char = body[index];
    if (char === '"') {
      return { value: value.toLowerCase(), negative, quoted, complete: index === body.length - 1 };
    }
    if (char === "\\") {
      const next = body[++index];
      if (next !== '"' && next !== "\\") return { value, negative, quoted, complete: false };
      value += next;
    } else value += char;
  }
  return { value: value.toLowerCase(), negative, quoted, complete: false };
}
