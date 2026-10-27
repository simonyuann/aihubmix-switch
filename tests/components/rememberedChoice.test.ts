import { afterEach, describe, expect, it, vi } from "vitest";
import {
  readRememberedChoice,
  writeRememberedChoice,
} from "@/components/usage/rememberedChoice";

const KEY = "cc-switch.usage.test";
const METRICS = ["tokens", "requests", "cost"] as const;

describe("remembered usage choices", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.removeItem(KEY);
  });

  it("reads back what was written", () => {
    writeRememberedChoice(KEY, "cost");
    expect(readRememberedChoice(KEY, METRICS, "tokens")).toBe("cost");
  });

  it("falls back when nothing or an unknown value is stored", () => {
    expect(readRememberedChoice(KEY, METRICS, "tokens")).toBe("tokens");
    localStorage.setItem(KEY, "latency");
    expect(readRememberedChoice(KEY, METRICS, "tokens")).toBe("tokens");
  });

  it("keeps working when storage is unavailable", () => {
    // Native Storage methods live on its prototype; the Node fallback owns them.
    const storageApi = Object.prototype.hasOwnProperty.call(
      window.localStorage,
      "getItem",
    )
      ? window.localStorage
      : Object.getPrototypeOf(window.localStorage);
    vi.spyOn(storageApi, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    vi.spyOn(storageApi, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    expect(() => writeRememberedChoice(KEY, "cost")).not.toThrow();
    expect(readRememberedChoice(KEY, METRICS, "tokens")).toBe("tokens");
  });
});
