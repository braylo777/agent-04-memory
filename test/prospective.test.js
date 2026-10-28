import { test } from "node:test"; import assert from "node:assert/strict";
import { parse, schedule, fireDue, pending } from "../src/memory/prospective.js";
test("parses relative reminders", () => {
  assert.deepEqual(parse("Remind me in 10 minutes to call Mom.", 0), { text: "call Mom", due_at: 600000 });
  assert.equal(parse("remind me after 2 days about the dentist", 0).due_at, 2 * 864e5); assert.equal(parse("hello", 0), null);
});
test("fires only due, unfired reminders, exactly once", () => {
  let r = schedule([], { text: "a", due_at: 10 }, 0); r = schedule(r, { text: "b", due_at: 100 }, 0);
  let x = fireDue(r, 50); assert.deepEqual(x.fired.map((f) => f.text), ["a"]); assert.deepEqual(pending(x.reminders).map((f) => f.text), ["b"]);
  x = fireDue(x.reminders, 60); assert.equal(x.fired.length, 0);
});
