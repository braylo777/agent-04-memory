import { test } from "node:test"; import assert from "node:assert/strict";
import { upsert, active, history, render } from "../src/memory/semantic.js";
test("new facts are added", () => assert.deepEqual(active(upsert([], [{ key: "name", value: "Sam" }], 1)).map((f) => f.value), ["Sam"]));
test("a changed fact supersedes (not deletes) the old one", () => {
  let f = upsert([], [{ key: "location", value: "Denver" }], 1); f = upsert(f, [{ key: "Location", value: "Lisbon" }], 2);
  assert.equal(f.length, 2); const [old, cur] = history(f, "location");
  assert.equal(old.superseded_at, 2); assert.equal(old.superseded_by, cur.id); assert.equal(cur.supersedes, old.id);
  assert.deepEqual(active(f).map((x) => x.value), ["Lisbon"]); assert.doesNotMatch(render(f), /Denver/);
});
test("the same value only confirms; junk is ignored", () => {
  let f = upsert([], [{ key: "diet", value: "vegetarian" }], 1); f = upsert(f, [{ key: "diet", value: "Vegetarian " }, null, { key: 3 }, { key: "x", value: "" }], 5);
  assert.equal(f.length, 1); assert.equal(f[0].confirmed_at, 5);
});
test("supersede chains across three values", () => {
  let f = []; for (const [i, v] of ["A", "B", "C"].entries()) f = upsert(f, [{ key: "job", value: v }], i + 1);
  assert.deepEqual(active(f).map((x) => x.value), ["C"]); assert.equal(f.filter((x) => x.superseded_at).length, 2);
});
