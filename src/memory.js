import { chat, text, ask, parseJSON } from "./core.js";

// Memory layout per session in KV (expires after 7 days):
//   short-term: last N chat turns   long-term: durable facts extracted by the LLM
export const SHORT_N = 10, MAX_FACTS = 20, TTL = 60 * 60 * 24 * 7;
export const key = (sid) => `mem:${sid}`;
export async function load(kv, sid) { return (await kv.get(key(sid), "json")) || { turns: [], facts: [] }; }
export async function save(kv, sid, m) { await kv.put(key(sid), JSON.stringify(m), { expirationTtl: TTL }); }

export function mergeFacts(old, add) {
  const seen = new Set(old.map((f) => f.toLowerCase()));
  const out = [...old];
  for (const f of add || []) if (typeof f === "string" && f.trim() && !seen.has(f.toLowerCase())) { out.push(f.trim()); seen.add(f.toLowerCase()); }
  return out.slice(-MAX_FACTS);
}

export async function chatWithMemory(env, sid, message) {
  const m = await load(env.KV, sid);
  const system = `You are a friendly assistant with memory.\nKnown long-term facts about the user:\n${m.facts.map((f) => "- " + f).join("\n") || "(none yet)"}\nUse them naturally when relevant.`;
  const reply = text(await chat(env, [{ role: "system", content: system }, ...m.turns, { role: "user", content: message }]));
  // Memory writer: extract durable facts (name, preferences, goals) from this exchange.
  const extracted = parseJSON(await ask(env, 'Extract durable facts about the USER (name, preferences, goals, location, job) from the message. Each fact must be a full short sentence, e.g. [\"Name is Sam\", \"Lives in Austin\"]. Reply ONLY with a JSON array, or [] if none.', message, { max_tokens: 150 }), []);
  m.facts = mergeFacts(m.facts, Array.isArray(extracted) ? extracted : []);
  m.turns = [...m.turns, { role: "user", content: message }, { role: "assistant", content: reply }].slice(-SHORT_N);
  await save(env.KV, sid, m);
  return { reply, memory: { facts: m.facts, short_term_turns: m.turns.length } };
}
