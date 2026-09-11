import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const suite = process.env.MEDIATAGGER_SETTINGS_LANGUAGE === "1" ? describe : describe.skip;
const execFileAsync = promisify(execFile);
const evidence = path.resolve("artifacts/settings-language", new Date().toISOString().replace(/[:.]/g, "-"));

suite("Separate language settings card", function () {
  this.timeout(90000);

  before(async () => {
    await fs.mkdir(evidence, { recursive: true });
    console.log("SETTINGS_LANGUAGE_EVIDENCE", evidence);
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
  });

  for (const language of ["en", "pl"]) {
    for (const width of [1440, 600, 320]) {
      it(`lays out both cards and persists ${language} at ${width}px`, async () => {
        await browser.refresh();
        await browser.setWindowSize(width, 600);
        await $("#open-settings-button").waitForDisplayed();
        await $("#open-settings-button").click();
        const link = await $('.settings-nav a[href="#settings-language"]');
        await link.waitForDisplayed();
        await link.click();
        assert.equal(await $("#settings-language").isFocused(), true);
        await browser.waitUntil(async () => await link.getAttribute("aria-current") === "location");
        await $("#settings-language-select").selectByAttribute("value", language);
        await browser.waitUntil(async () => await $("html").getAttribute("lang") === language);
        assert.equal(await $("#settings-language h2").getText(), language === "en" ? "Language" : "Język");
        assert.equal(await $("#settings-language p").getText(), language === "en" ? "Choose application language." : "Wybierz język aplikacji.");
        assert.equal(await $("#settings-appearance p").getText(), language === "en" ? "Choose application theme." : "Wybierz motyw aplikacji.");
        await browser.refresh();
        await $("#open-settings-button").waitForDisplayed();
        assert.equal(await $("html").getAttribute("lang"), language);
        await $("#open-settings-button").click();
        await $("#settings-language-select").waitForExist();
        assert.equal(await $("#settings-language-select").getValue(), language);
        await $('.settings-nav a[href="#settings-appearance"]').click();
        await browser.pause(250);
        const { x, y, width: windowWidth, height } = await browser.getWindowRect();
        await execFileAsync("import", ["-window", "root", "-crop", `${windowWidth}x${height}+${x}+${y}`, path.join(evidence, `${language}-${width}.png`)]);
        const geometry = await browser.execute(() => {
          const appearance = document.querySelector("#settings-appearance");
          const language = document.querySelector("#settings-language");
          const text = language.querySelector("section > div").getBoundingClientRect();
          const select = language.querySelector("select").getBoundingClientRect();
          return {
            adjacent: appearance.nextElementSibling === language,
            themeHasSelect: !!appearance.querySelector("select"),
            stacked: select.top >= text.bottom,
            sideBySide: select.left >= text.right,
            previewHeights: [...appearance.querySelectorAll(".theme-preview")].map(element => element.getBoundingClientRect().height),
            fits: [...document.querySelectorAll("#settings-appearance, #settings-language, #settings-language-select, .window-controls")].every(element => {
              const rect = element.getBoundingClientRect();
              // WebKit can place a fractional grid edge within the final viewport pixel.
              return rect.left >= -1 && rect.right <= innerWidth + 1 && element.scrollWidth <= element.clientWidth;
            }),
            bounds: [...document.querySelectorAll("#settings-appearance, #settings-language, #settings-language-select, .window-controls")].map(element => ({ id: element.id, left: element.getBoundingClientRect().left, right: element.getBoundingClientRect().right, scroll: element.scrollWidth, client: element.clientWidth, viewport: innerWidth })),
            stored: localStorage.getItem("media-tagger.language")
          };
        });
        assert.equal(geometry.adjacent, true);
        assert.equal(geometry.themeHasSelect, false);
        assert.equal(width >= 1024 ? geometry.sideBySide : geometry.stacked, true);
        assert.deepEqual(geometry.previewHeights, [64, 64]);
        assert.equal(geometry.fits, true, JSON.stringify(geometry.bounds));
        assert.equal(geometry.stored, language);

      });
    }
  }
});
