import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { e2eWindowTitle } from "../build-e2e.js";

const root = path.resolve(import.meta.dirname, "../..");
const publisher = JSON.parse(await fs.readFile(path.join(root, "packaging/flatpak/publisher.json"), "utf8"));
const pkg = JSON.parse(await fs.readFile(path.join(root, "package.json"), "utf8"));
const evidence = path.join(root, "artifacts/about", new Date().toISOString().replaceAll(":", "-"));
const run = promisify(execFile);
const suite = process.env.MEDIATAGGER_ABOUT === "1" ? describe : describe.skip;

async function invoke(command, args = {}) {
  return browser.executeAsync((name, payload, done) => {
    window.__TAURI_INTERNALS__.invoke(name, payload).then(
      value => done({ value }), error => done({ failure: String(error) })
    );
  }, command, args);
}

suite("About and repository links", () => {
  before(async () => {
    await $(".filter-input").waitForDisplayed({ timeout: 20000 });
    assert.equal((await invoke("plugin:window|title", { label: "main" })).value, e2eWindowTitle);
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await browser.refresh();
    await $('button[aria-label="Open settings"]').waitForDisplayed();
    await $('button[aria-label="Open settings"]').click();
    await fs.mkdir(evidence, { recursive: true });
  });

  it("shows the build version, exact offline documents and last-section focus in both themes and languages", async () => {
    for (const language of ["en", "pl"]) {
      await $('a[href="#settings-language"]').click();
      await $("#settings-language-select").selectByAttribute("value", language);
      for (const theme of ["light", "dark"]) {
        await $('a[href="#settings-appearance"]').click();
        await $(`.theme-choice:has(input[value="${theme}"])`).click();
        await $('a[href="#settings-about"]').click();
        await browser.waitUntil(async () => await $('a[href="#settings-about"]').getAttribute("aria-current") === "location");
        assert.equal(await browser.execute(() => document.activeElement.id), "settings-about");
        assert.equal(await $("html").getAttribute("lang"), language);
        assert.equal(await $("html").getAttribute("data-theme"), theme);
        assert.equal(await $('[data-testid="about-version"]').getText(), pkg.version);
        assert.equal(await $("#settings-about h2").getText(), language === "en" ? "About" : "O aplikacji");
        await browser.saveScreenshot(path.join(evidence, `${language}-${theme}.png`));
      }
    }
    for (const [index, file] of ["LICENSE", "PRIVACY.md"].entries()) {
      const summary = await $(`#settings-about details:nth-of-type(${index + 1}) summary`);
      await summary.click();
      const document = await $(`#settings-about details:nth-of-type(${index + 1}) pre`);
      assert.equal(await document.getProperty("textContent"), await fs.readFile(path.join(root, file), "utf8"));
      assert.equal(await document.getAttribute("lang"), "en");
      await summary.click();
    }
  });

  it("rejects URLs outside the repository, file opening and explicit application selection", async () => {
    for (const url of ["https://example.org/", "https://github.com/another/tagrove", `${publisher.repository}-other`,
      "https://github.com.evil.invalid/DawidRoguziak/tagrove", "file:///tmp/tagrove-test", "mailto:test@example.org"]) {
      const result = await invoke("plugin:opener|open_url", { url });
      assert.match(result.failure, /not allowed|forbidden/i, url);
    }
    assert.match((await invoke("plugin:opener|open_url", { url: publisher.repository, with: "firefox" })).failure, /not allowed|forbidden/i);
    assert.match((await invoke("plugin:opener|open_path", { path: "/tmp/tagrove-test" })).failure, /not allowed|forbidden/i);
  });

  it("shows a selectable address when the native opener rejects a clicked link", async () => {
    // Fault injection changes only the IPC URL, so the real native capability rejects it.
    // The component must catch that rejection and keep its original address available.
    await browser.execute(() => {
      const original = window.fetch;
      window.__tagroveRestoreFetch = () => { window.fetch = original; };
      window.fetch = (resource, options) => {
        if (decodeURIComponent(String(resource)).includes("plugin:opener|open_url")) {
          const payload = JSON.parse(options.body);
          return original(resource, { ...options, body: JSON.stringify({ ...payload, url: "https://example.org/" }) });
        }
        return original(resource, options);
      };
    });
    try {
      await $("#settings-about button:first-of-type").click();
      await $('#settings-about [role="alert"]').waitForDisplayed();
      const input = await $('#settings-about input[readonly]');
      assert.equal(await input.getValue(), publisher.repository);
      await input.scrollIntoView();
      const rect = await browser.execute(() => {
        const r = document.querySelector("#settings-about input").getBoundingClientRect();
        return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
      });
      const window = await browser.getWindowRect();
      await run("python3", ["e2e/native-input.py", "focus", String(Math.round(window.x + rect.x)), String(Math.round(window.y + rect.y))]);
      assert.equal(await browser.execute(() => document.hasFocus()), true);
      await browser.keys(["Control", "a"]);
      await browser.saveScreenshot(path.join(evidence, "native-rejection.png"));
      assert.deepEqual(await browser.execute(() => {
        const field = document.querySelector("#settings-about input");
        return [field.selectionStart, field.selectionEnd];
      }), [0, publisher.repository.length]);
    } finally {
      await browser.execute(() => {
        window.__tagroveRestoreFetch();
        delete window.__tagroveRestoreFetch;
      });
    }
  });
});
