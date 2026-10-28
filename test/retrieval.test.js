import { test } from "node:test"; import assert from "node:assert/strict";
import { add, search, forget, touch, strength, DAY, HALF_LIFE_DAYS, FORGET_BELOW } from "../src/memory/retrieval.js";
const now = 1000 * DAY;
test("strength halves every half-life", () => { assert.equal(strength({ at: now }, now), 1); assert.ok(Math.abs(strength({ at: now - HALF_LIFE_DAYS * DAY }, now) - 0.5) < 1e-9); });
test("recency breaks ties between equally similar memories", () => {
  let s = add([], { text: "old", vec: [1, 0], now: now - 60 * DAY }); s = add(s, { text: "new", vec: [1, 0], now });
  assert.equal(search(s, [1, 0], now, 2)[0].it.text, "new");
});
test("strong similarity still beats recency", () => {
  let s = add([], { text: "relevant-old", vec: [1, 0], now: now - 30 * DAY }); s = add(s, { text: "irrelevant-new", vec: [0, 1], now });
  assert.equal(search(s, [1, 0], now, 1)[0].it.text, "relevant-old");
});
test("forgetting drops decayed items only", () => {
  let s = add([], { text: "ancient", vec: [1], now: now - 90 * DAY }); s = add(s, { text: "fresh", vec: [1], now });
  const r = forget(s, now); assert.deepEqual(r.items.map((i) => i.text), ["fresh"]); assert.deepEqual(r.forgotten, ["ancient"]);
  assert.ok(strength({ at: now - 90 * DAY }, now) < FORGET_BELOW);
});
test("recalling an item refreshes it so it is not forgotten", () => {
  const s = touch(add([], { text: "used", vec: [1], now: now - 90 * DAY }), [`v${now - 90 * DAY}-0`], now);
  assert.equal(forget(s, now).items.length, 1);
});
