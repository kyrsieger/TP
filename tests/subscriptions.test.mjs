import assert from "node:assert/strict";
import test from "node:test";
import { loadTs } from "./load-ts.mjs";

const { subscribeAll } = loadTs("src/services/subscriptions.ts");

test("a failed registration cleans up both completed and late subscriptions", async () => {
  const disposed = [];
  let finishLate;
  const error = new Error("listener failed");
  const subscribing = subscribeAll([
    Promise.resolve(() => disposed.push("early")),
    Promise.reject(error),
    new Promise(resolve => { finishLate = () => resolve(() => disposed.push("late")); }),
  ]);
  finishLate(); await assert.rejects(subscribing, error);
  assert.deepEqual(disposed, ["early", "late"]);
});

test("group disposal releases each listener once even after repeated cleanup", async () => {
  let count = 0;
  const dispose = await subscribeAll([Promise.resolve(() => count++), Promise.resolve(() => count++)]);
  dispose(); dispose(); assert.equal(count, 2);
});
