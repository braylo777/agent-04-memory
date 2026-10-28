import { test } from "node:test"; import assert from "node:assert/strict";
import { log, byIds, describe, render, MAX_EPISODES } from "../src/memory/episodic.js";
test("logs task and outcome", () => { const { episodes, episode } = log([], { task: "Plan Rome trip", outcome: "Made a 3-day plan", now: 1 }); assert.equal(episodes.length, 1); assert.match(describe(episode), /Rome.*3-day/); });
test("log is capped and lookups by id work", () => {
  let e = []; for (let i = 0; i < MAX_EPISODES + 5; i++) e = log(e, { task: "t" + i, outcome: "o", now: i }).episodes;
  assert.equal(e.length, MAX_EPISODES); assert.deepEqual(byIds(e, [e[0].id, "nope"]).map((x) => x.task), ["t5"]); assert.match(render([e[0]]), /"t5"/);
});
