import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";

const frontend = process.env.MEDIATAGGER_FRONTEND ?? "react";
const suite = frontend === "solid" ? describe : describe.skip;
const shellSuite = process.env.MEDIATAGGER_SOLID_SHELL === "1" ? describe : describe.skip;
const evidence = path.resolve("artifacts/solid2/stage2", frontend);

suite("Solid desktop bootstrap", () => {
  it("mounts and reloads inside the isolated native window", async () => {
    await $("main h1").waitForDisplayed();
    assert.equal(await $("main h1").getText(), "Tagrove");
    assert.equal(await $('[role="status"]').getText(), "Solid 2 shell ready");
    await browser.refresh();
    await $('[role="status"]').waitForDisplayed();
    assert.equal(await $('[role="status"]').getText(), "Solid 2 shell ready");
    await fs.mkdir(evidence, { recursive: true });
    await browser.saveScreenshot(path.join(evidence, "shell.png"));
  });
});

async function navigateSection(id) {
  // On narrow layouts the reference nav scrolls above the sticky header.
  await browser.execute(() => { document.querySelector("main").scrollTop = 0; });
  await $(`.settings-nav a[href="#settings-${id}"]`).click();
}

shellSuite("Shell appearance and language parity", function () {
  this.timeout(90000);
  before(async () => { await fs.mkdir(evidence, { recursive: true }); });
  for (const width of [1440, 600, 320]) {
    for (const language of ["en", "pl"]) {
      for (const theme of ["light", "dark"]) {
        it(`preserves ${language}/${theme} controls at ${width}px`, async () => {
          await browser.setWindowSize(width, 900);
          await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
          await browser.refresh();
          await $("#open-settings-button").waitForDisplayed();
          await $("#open-settings-button").click();
          await $("#settings-language-select").waitForExist();
          await navigateSection("appearance");
          await $(`#settings-appearance input[value="${theme}"]`).click();
          await navigateSection("language");
          await $("#settings-language-select").selectByAttribute("value", language);
          await browser.waitUntil(async () => await $("html").getAttribute("lang") === language && await $("html").getAttribute("data-theme") === theme);
          assert.equal(await $("#settings-language h2").getText(), language === "en" ? "Language" : "Język");
          assert.equal((await $$("#settings-language-select option")).length, 11);
          await navigateSection("appearance");
          const measured = await browser.execute(() => {
            const selectors = [".window-controls button", ".theme-preview", "#settings-language-select"];
            const controls = selectors.map(selector => {
              const el = document.querySelector(selector);
              const rect = el.getBoundingClientRect(), style = getComputedStyle(el);
              return { selector, width: rect.width, height: rect.height, font: style.font, color: style.color, background: style.backgroundColor, borderRadius: style.borderRadius };
            });
            const fits = [...document.querySelectorAll("#settings-appearance, #settings-language, #settings-language-select, .window-controls")].every(el => {
              const rect = el.getBoundingClientRect(); return rect.left >= -1 && rect.right <= innerWidth + 1 && el.scrollWidth <= el.clientWidth;
            });
            return { controls, fits, theme: localStorage.getItem("media-tagger.theme"), language: localStorage.getItem("media-tagger.language") };
          });
          assert.equal(measured.fits, true); assert.equal(measured.theme, theme); assert.equal(measured.language, language);
          await fs.writeFile(path.join(evidence, `${language}-${theme}-${width}.json`), JSON.stringify(measured, null, 2));
          await browser.saveScreenshot(path.join(evidence, `${language}-${theme}-${width}.png`));
          await browser.refresh();
          await $("#open-settings-button").waitForDisplayed();
          assert.equal(await $("html").getAttribute("lang"), language); assert.equal(await $("html").getAttribute("data-theme"), theme);
          await $("#open-settings-button").click();
          await $("#settings-language-select").waitForExist();
          assert.equal(await $("#settings-language-select").getValue(), language);
          await browser.keys("Escape");
          await $("#open-settings-button").waitForDisplayed();
          await browser.waitUntil(async () => $("#open-settings-button").isFocused());
        });
      }
    }
  }
  after(async () => {
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh(); await browser.setWindowSize(1440, 900);
  });
});
