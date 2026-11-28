import { useSyncExternalStore } from "react";
import { readPreference, writePreference } from "@/lib/userPreferences";

const KEY = "aihubmix:advanced-interface";
const EVENT = "aihubmix:interface-changed";

export function getAdvancedInterface(): boolean {
  try {
    return readPreference(KEY) === "true";
  } catch {
    return false;
  }
}

export function setAdvancedInterface(enabled: boolean): void {
  writePreference(KEY, String(enabled));
  window.dispatchEvent(new Event(EVENT));
}

function subscribe(callback: () => void) {
  window.addEventListener(EVENT, callback);
  window.addEventListener("storage", callback);
  return () => {
    window.removeEventListener(EVENT, callback);
    window.removeEventListener("storage", callback);
  };
}

export function useAdvancedInterface(): boolean {
  return useSyncExternalStore(subscribe, getAdvancedInterface, () => false);
}
