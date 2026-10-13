import type { UnlistenFn } from "@tauri-apps/api/event";

/** Register independent listeners together and clean up partial failures. */
export async function subscribeAll(subscriptions: Promise<UnlistenFn>[]): Promise<UnlistenFn> {
  const results = await Promise.allSettled(subscriptions);
  const disposers = results.flatMap(result => result.status === "fulfilled" ? [result.value] : []);
  const dispose = () => disposers.splice(0).forEach(unlisten => unlisten());
  const failure = results.find(result => result.status === "rejected");
  if (failure?.status === "rejected") { dispose(); throw failure.reason; }
  return dispose;
}
