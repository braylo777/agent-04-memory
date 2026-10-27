import { test } from "node:test";
import assert from "node:assert/strict";
import { chatWithMemory, mergeFacts, SHORT_N } from "../src/memory.js";

const kv = () => { const m = new Map(); return { get: async (k) => (m.has(k) ? JSON.parse(m.get(k)) : null), put: async (k, v) => m.set(k, v), delete: async (k) => m.delete(k) }; };
test("mergeFacts dedupes case-insensitively", () => assert.deepEqual(mergeFacts(["Likes tea"], ["likes tea", "Lives in Austin"]), ["Likes tea", "Lives in Austin"]));
test("facts persist across turns and reach the prompt", async () => {
  let call = 0;
  const env = { KV: kv(), AI: { run: async (_m, inp) => {
    call++;
    if (inp.messages[0].content.startsWith("Extract")) return { response: call === 2 ? '["Name is Sam"]' : "[]" };
    if (call === 3) assert.match(inp.messages[0].content, /Name is Sam/);
    return { response: "ok" };
  } } };
  await chatWithMemory(env, "abc123", "Hi, I'm Sam");
  const r = await chatWithMemory(env, "abc123", "What's my name?");
  assert.deepEqual(r.memory.facts, ["Name is Sam"]); assert.equal(r.memory.short_term_turns, 4);
});
test("short-term window is capped", async () => {
  const env = { KV: kv(), AI: { run: async () => ({ response: "[]" }) } };
  let r; for (let i = 0; i < 8; i++) r = await chatWithMemory(env, "abcd", "msg " + i);
  assert.equal(r.memory.short_term_turns, SHORT_N);
});
