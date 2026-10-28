import { test } from "node:test"; import assert from "node:assert/strict";
import { turn, remember, recall, age, cron } from "../src/agent.js";
import { load, REM_INDEX } from "../src/memory/store.js";
import { MAX_TURNS } from "../src/memory/working.js";
import { kv, fakeAI } from "./helpers.js";
import worker from "../src/index.js";
const DAY = 864e5;
test("turn writes semantic facts and supersedes on change", async () => {
  const env = { KV: kv(), AI: fakeAI((m) => (m.includes("Denver") ? '[{"key":"location","value":"Denver"}]' : m.includes("Lisbon") ? '[{"key":"location","value":"Lisbon"}]' : "[]")) };
  await turn(env, "abcd", "I live in Denver", 1); const r = await turn(env, "abcd", "I moved to Lisbon", 2);
  assert.deepEqual(r.modules.semantic.active.map((f) => f.value), ["Lisbon"]); assert.deepEqual(r.modules.semantic.superseded.map((f) => f.value), ["Denver"]);
});
test("episodic: a similar later task recalls the past outcome into the prompt", async () => {
  const env = { KV: kv(), AI: fakeAI(undefined, () => "Booked a 3-day Rome trip") };
  await turn(env, "abcd", "Plan my Rome trip", 1);
  env.AI = fakeAI(undefined, (inp) => (assert.match(inp.messages[0].content, /Episodic memory[\s\S]*Rome trip[\s\S]*Booked a 3-day Rome trip/), "ok"));
  const r = await turn(env, "abcd", "Another Rome trip please", 2); assert.equal(r.used.episodes.length, 1);
});
test("procedural: matching playbook is injected", async () => {
  const env = { KV: kv(), AI: fakeAI(undefined, (inp) => (assert.match(inp.messages[0].content, /Playbook recommend-food/), "ok")) };
  assert.deepEqual((await turn(env, "abcd", "Ideas for dinner?", 1)).used.playbooks, ["recommend-food"]);
});
test("working memory is compacted after the turn, never mid-turn", async () => {
  let seen = 0; const env = { KV: kv(), AI: fakeAI(undefined, (inp) => { seen = inp.messages.length; return "ok"; }) };
  for (let i = 0; i < 5; i++) await turn(env, "abcd", "msg " + i, i);
  assert.equal(seen, 1 + MAX_TURNS + 1); // system + full 8-turn buffer + new message
  assert.equal((await load(env.KV, "abcd")).working.turns.length, 4);
});
test("prospective: reminder scheduled in chat fires via cron once", async () => {
  const env = { KV: kv(), AI: fakeAI() };
  await turn(env, "abcd", "Remind me in 1 minute to stretch", 0);
  assert.deepEqual(await env.KV.get(REM_INDEX, "json"), ["abcd"]);
  assert.equal((await cron(env, 30e3)).fired, 0); assert.equal((await cron(env, 61e3)).fired, 1);
  const m = await load(env.KV, "abcd"); assert.equal(m.inbox[0].text, "stretch"); assert.deepEqual(await env.KV.get(REM_INDEX, "json"), []);
});
test("retrieval: recency-aware recall and forgetting after aging", async () => {
  const env = { KV: kv(), AI: fakeAI() }, now = 1000 * DAY;
  await remember(env, "abcd", "My gym is Planet", now); await age(env, "abcd", 60, now); await remember(env, "abcd", "My gym is Equinox", now);
  assert.match((await recall(env, "abcd", "which gym", 3, now)).results[0].text, /Equinox/);
  await age(env, "abcd", 90, now); const r = await recall(env, "abcd", "which gym", 3, now);
  assert.equal(r.results.length, 0); assert.equal(r.forgotten.length, 2);
});
test("HTTP: validation and memory view", async () => {
  const env = { KV: kv() }, post = (p, b) => worker.fetch(new Request("https://x" + p, { method: "POST", body: JSON.stringify(b) }), env);
  assert.equal((await post("/api/chat", { session: "abcd" })).status, 400);
  assert.equal((await post("/api/age", { session: "abcd", days: 0 })).status, 400);
  assert.equal((await worker.fetch(new Request("https://x/api/memory?session=no"), env)).status, 400);
  const v = await (await worker.fetch(new Request("https://x/api/memory?session=abcd"), env)).json();
  assert.deepEqual(Object.keys(v), ["working", "semantic", "episodic", "procedural", "retrieval", "prospective"]);
});
