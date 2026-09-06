import { describe, expect, it, vi } from "vitest";
import { SuggestionClient } from "../SuggestionClient";
import type { SuggestionRequest, SuggestionResponse } from "../protocol";
import { readActiveToken } from "../../services/tokenService";
class ControlledWorker extends EventTarget implements Worker {
  onmessage: Worker["onmessage"] = null;
  onmessageerror: Worker["onmessageerror"] = null;
  onerror: Worker["onerror"] = null;
  messages: SuggestionRequest[] = [];
  terminate = vi.fn();
  postMessage(message: SuggestionRequest) {
    this.messages.push(message);
  }
  reply(requestId: number, version: number, value: string) {
    const data: SuggestionResponse = { requestId, version, suggestions: [{ value, indices: [] }] };
    this.onmessage?.call(this, new MessageEvent("message", { data }));
  }
  fail() {
    this.onerror?.call(this, new ErrorEvent("error"));
  }
}
const token = readActiveToken("cat", 3)!;
describe("SuggestionClient", () => {
  it("starts lazily, shares one worker, and coalesces pending searches by editor", async () => {
    const worker = new ControlledWorker();
    const factory = vi.fn(() => worker);
    const client = new SuggestionClient(factory);
    expect(factory).not.toHaveBeenCalled();
    const tags = ["cat", "car"];
    const first = client.search("one", tags, token, []);
    const superseded = client.search("two", tags, token, []);
    const latest = client.search("two", tags, token, ["car"]);
    expect(worker.messages.filter((message) => message.type === "search")).toHaveLength(1);
    await expect(superseded).resolves.toEqual([]);
    worker.reply(1, 1, "cat");
    await expect(first).resolves.toEqual([{ value: "cat", indices: [] }]);
    expect(worker.messages.filter((message) => message.type === "search")).toHaveLength(2);
    worker.reply(2, 1, "obsolete");
    worker.reply(3, 1, "car");
    await expect(latest).resolves.toEqual([{ value: "car", indices: [] }]);
    expect(factory).toHaveBeenCalledOnce();
    expect(worker.messages.filter((message) => message.type === "vocabulary")).toHaveLength(1);
    client.dispose();
    expect(worker.terminate).toHaveBeenCalledOnce();
  });
  it("discards old vocabulary results and can recover from worker failure", async () => {
    const firstWorker = new ControlledWorker();
    const nextWorker = new ControlledWorker();
    const factory = vi.fn().mockReturnValueOnce(firstWorker).mockReturnValueOnce(nextWorker);
    const client = new SuggestionClient(factory);
    const obsolete = client.search("one", ["old"], token, []);
    const current = client.search("one", ["cat"], token, []);
    const rejected = expect(current).rejects.toThrow("unavailable");
    firstWorker.reply(1, 1, "old");
    await expect(obsolete).resolves.toEqual([]);
    firstWorker.fail();
    await rejected;
    client.retry();
    const retried = client.search("one", ["cat"], token, []);
    nextWorker.reply(3, 3, "cat");
    await expect(retried).resolves.toEqual([{ value: "cat", indices: [] }]);
    client.dispose();
  });
  it("settles queued and active reads on cleanup", async () => {
    const worker = new ControlledWorker();
    const client = new SuggestionClient(() => worker);
    const tags = ["cat"];
    const active = client.search("one", tags, token, []);
    const queued = client.search("two", tags, token, []);
    client.dispose();
    await expect(active).resolves.toEqual([]);
    await expect(queued).resolves.toEqual([]);
    worker.reply(1, 1, "late");
    expect(worker.messages.filter((message) => message.type === "search")).toHaveLength(1);
  });
});
