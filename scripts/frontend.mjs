export function selectFrontend(value = process.env.MEDIATAGGER_FRONTEND) {
  if (value === undefined) return "react";
  if (value === "react" || value === "solid") return value;
  throw new Error(`Unsupported MEDIATAGGER_FRONTEND: ${value}. Expected react or solid.`);
}
