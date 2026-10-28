import { test } from "node:test"; import assert from "node:assert/strict";
import { append, compact, context, MAX_TURNS, KEEP_TURNS, SUMMARY_CHARS } from "../src/memory/working.js";
const w0 = { summary: "", turns: [] };
const fill = (n) => { let w = w0; for (let i = 0; i < n; i++) w = append(w, "u" + i, "a" + i); return w; };
test("no compaction at or under MAX_TURNS", () => { const w = fill(MAX_TURNS / 2); assert.equal(compact(w), w); });
test("compaction keeps the newest turns and summarizes the rest", () => {
  const w = compact(fill(5)); assert.equal(w.turns.length, KEEP_TURNS); assert.equal(w.turns.at(-1).content, "a4"); assert.match(w.summary, /user: u0/);
});
test("summary is bounded and context leads with it", () => {
  let w = w0; for (let i = 0; i < 200; i++) w = compact(append(w, "x".repeat(100), "y"));
  assert.ok(w.summary.length <= SUMMARY_CHARS + 1); assert.equal(context(w)[0].role, "system");
});
