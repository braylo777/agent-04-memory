import { test } from "node:test"; import assert from "node:assert/strict";
import { select, render, PLAYBOOKS } from "../src/memory/playbooks.js";
test("selects playbooks by trigger, best match first", () => { assert.equal(select("Plan a trip and travel itinerary")[0].name, "plan-a-trip"); assert.equal(select("where for dinner?")[0].name, "recommend-food"); });
test("no trigger, no playbook; word boundaries respected", () => { assert.deepEqual(select("hello there"), []); assert.deepEqual(select("the debtor"), []); });
test("every playbook has triggers and steps; render lists steps", () => { assert.ok(PLAYBOOKS.every((b) => b.triggers.length && b.steps.length)); assert.match(render([PLAYBOOKS[0]]), /Playbook plan-a-trip: Ask dates/); });
