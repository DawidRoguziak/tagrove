import { cleanup } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import i18n, { changeAppLanguage } from "../../../i18n/core";
import { createSectionOperationState } from "../operationStateService";
import { renderSolid } from "../../../test/renderSolid";
import { settle } from "../../../test/settle";
import { DuplicateDeleteConfirmDialog } from "../DuplicateDeleteConfirmDialog";
import { DuplicatesSection } from "../sections/DuplicatesSection";
import { ScanSettingsSection } from "../sections/ScanSettingsSection";
import { renderSettings } from "./renderSettings";
import { LightboxInfoPanel } from "../../lightbox/LightboxInfoPanel";
import { asset } from "../../lightbox/__tests__/fixtures";
import { SearchTagatorWrapper } from "../../search/SearchTagatorWrapper";
import { SuggestionClient } from "../../search/suggestions/SuggestionClient";
import { vi } from "vitest";

vi.mock("../../../platform/tauri/tags", () => ({
  listTags: vi.fn().mockResolvedValue({ items: [], total: 0 })
}));

afterEach(async () => {
  cleanup();
  await changeAppLanguage("en");
});

const dialogText = () =>
  document.getElementById("duplicate-delete-confirm-heading")?.parentElement?.querySelector("p")
    ?.textContent;

describe("duplicate delete confirmation plurals", () => {
  it.each([
    ["en", 1, 1, "Apply 1 queued change?\n\nThis will permanently delete 1 file from disk."],
    ["en", 2, 2, "Apply 2 queued changes?\n\nThis will permanently delete 2 files from disk."],
    ["pl", 1, 1, "Zastosować 1 zmianę z kolejki?\n\nTo trwale usunie 1 plik z dysku."],
    ["pl", 2, 2, "Zastosować 2 zmiany z kolejki?\n\nTo trwale usunie 2 pliki z dysku."],
    ["pl", 5, 5, "Zastosować 5 zmian z kolejki?\n\nTo trwale usunie 5 plików z dysku."],
    ["pl", 5, 1, "Zastosować 5 zmian z kolejki?\n\nTo trwale usunie 1 plik z dysku."]
  ] as const)("%s changes=%i deletes=%i", async (language, count, deleteCount, expected) => {
    await changeAppLanguage(language);
    renderSettings(() => (
      <DuplicateDeleteConfirmDialog
        open
        count={count}
        deleteCount={deleteCount}
        isOperationLocked={false}
        onCancel={() => {}}
        onConfirm={() => {}}
      />
    ));
    expect(dialogText()).toBe(expected);
  });
});

describe("duplicates last result plurals", () => {
  it.each([
    ["en", 1, 1, "Last result: 1 group / 1 asset"],
    ["en", 2, 5, "Last result: 2 groups / 5 assets"],
    ["pl", 1, 1, "Ostatni wynik: 1 grupa / 1 zasób"],
    ["pl", 2, 2, "Ostatni wynik: 2 grupy / 2 zasoby"],
    ["pl", 5, 5, "Ostatni wynik: 5 grup / 5 zasobów"]
  ] as const)("%s groups=%i assets=%i", async (language, groups, assets, expected) => {
    await changeAppLanguage(language);
    const view = renderSettings(() => (
      <DuplicatesSection
        isOperationLocked={false}
        operationState={createSectionOperationState("duplicates")}
        duplicateGroups={groups}
        duplicateAssets={assets}
        onStart={() => {}}
      />
    ));
    expect(view.container.textContent).toContain(expected);
  });
});

describe("localized video tool status", () => {
  it.each([
    ["en", "ffmpeg: available", "ffprobe: not available"],
    ["pl", "ffmpeg: dostępny", "ffprobe: niedostępny"]
  ] as const)("%s", async (language, first, second) => {
    await changeAppLanguage(language);
    const view = renderSolid(ScanSettingsSection, {
      scanRoots: [],
      isOperationLocked: false,
      thumbnailBulkRunning: false,
      cancelThumbnailRunning: false,
      operationState: createSectionOperationState("scan"),
      videoToolStatus: { ffmpeg_available: true, ffprobe_available: false },
      onSetAutoScan: vi.fn(),
      onPickFolder: vi.fn(),
      onRescanRoot: vi.fn(),
      onRemoveRoot: vi.fn(),
      onRescanAll: vi.fn(),
      onRenderAllThumbnails: vi.fn(),
      onRenderFailedThumbnails: vi.fn(),
      onCancelThumbnailRender: vi.fn()
    } as never);
    const spans = [...view.getByTestId("video-tool-status").querySelectorAll("span")];
    expect(spans.map((node) => node.textContent?.trim())).toEqual([first, second]);
  });
});

describe("localized lightbox media kind", () => {
  it.each([
    ["en", "video", "Video"],
    ["en", "image", "Image"],
    ["pl", "image", "Obraz"],
    ["ja", "video", "動画"]
  ] as const)("%s %s", async (language, kind, label) => {
    await changeAppLanguage(language);
    const view = renderSolid(LightboxInfoPanel, { selected: asset(1, { kind }) });
    expect(view.getByTestId("lightbox-info-panel").textContent).toContain(`: ${label}`);
    expect(view.getByTestId("lightbox-info-panel").textContent).not.toMatch(/: (image|video)\b/);
  });
});

describe("localized search syntax hint", () => {
  it.each([
    ["en", "cat vacation -dog\ntags\ntags:3\ngN:Trip 2026"],
    ["pl", "kot wakacje -pies\ntags\ntags:3\ngN:Wakacje 2026"]
  ] as const)("%s", async (language, hint) => {
    await changeAppLanguage(language);
    const view = renderSolid(SearchTagatorWrapper, {
      client: new SuggestionClient(),
      filterInput: "",
      onFilterChange: vi.fn(),
      knownTags: [],
      mediaKind: "all" as const,
      onMediaKindChange: vi.fn(),
      onSearchSubmit: vi.fn(),
      onApplyTagListSearch: vi.fn().mockResolvedValue(undefined),
      onClearAll: vi.fn(),
      favoritesOnly: false,
      onFavoritesOnlyChange: vi.fn()
    } as never);
    await settle(() => {});
    expect(view.container.querySelector(".workspace-search-field")?.getAttribute("title")).toBe(hint);
  });
});

describe("count strings use plural keys", () => {
  const expectations: Array<["en" | "pl", string, number, string]> = [
    ["en", "workspace.itemCount", 1, "1 item"],
    ["en", "workspace.itemCount", 2, "2 items"],
    ["pl", "workspace.itemCount", 1, "1 element"],
    ["pl", "workspace.itemCount", 2, "2 elementy"],
    ["pl", "workspace.itemCount", 5, "5 elementów"],
    ["en", "settings.actions.summary.duplicateApplied", 1, "Applied 1 change. Remaining groups: 3"],
    ["en", "settings.actions.summary.duplicateApplied", 5, "Applied 5 changes. Remaining groups: 3"],
    ["pl", "settings.actions.summary.duplicateApplied", 1, "Zastosowano 1 zmianę. Pozostałe grupy: 3"],
    ["pl", "settings.actions.summary.duplicateApplied", 2, "Zastosowano 2 zmiany. Pozostałe grupy: 3"],
    ["pl", "settings.actions.summary.duplicateApplied", 5, "Zastosowano 5 zmian. Pozostałe grupy: 3"],
    ["en", "settings.actions.summary.foldersAdded", 1, "1 folder added to scan list."],
    ["en", "settings.actions.summary.foldersAdded", 2, "2 folders added to scan list."],
    ["pl", "settings.actions.summary.foldersAdded", 2, "Dodano 2 foldery do listy skanowania."],
    ["pl", "settings.actions.summary.foldersAdded", 5, "Dodano 5 folderów do listy skanowania."],
    ["en", "settings.actions.summary.duplicateRecoveryRequired", 1, "The batch could not be fully recovered. 1 item has a reported recovery path; rescan before retrying."],
    ["en", "settings.actions.summary.duplicateRecoveryRequired", 2, "The batch could not be fully recovered. 2 items have a reported recovery path; rescan before retrying."]
  ];
  it.each(expectations)("%s %s %i", async (language, key, count, expected) => {
    await changeAppLanguage(language);
    expect(i18n.t(key, { count, formattedCount: String(count), groups: 3 })).toBe(expected);
  });
});
