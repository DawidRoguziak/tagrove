import { describe, expect, it } from "vitest";
import { SECTION_READY_MESSAGES, createSectionOperationState } from "../operationStateService";

describe("operationStateService", () => {
  it("creates default state for each settings section", () => {
    for (const section of ["scan", "importExport", "danger", "duplicates"] as const) {
      const state = createSectionOperationState(section);

      expect(state).toEqual({
        loading: false,
        message: SECTION_READY_MESSAGES[section],
        progress: null
      });
    }
  });
});
