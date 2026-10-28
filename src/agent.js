import { chat, text, ask, parseJSON } from "./core.js";
import { load, save, REM_INDEX } from "./memory/store.js";
import * as working from "./memory/working.js";
import * as semantic from "./memory/semantic.js";
import * as episodic from "./memory/episodic.js";
import * as procedural from "./memory/playbooks.js";
import * as retrieval from "./memory/retrieval.js";
import * as prospective from "./memory/prospective.js";

// Due reminders move to the inbox. Runs at the start of every request and from the cron trigger.
export function deliver(m, now) {
  const { reminders, fired } = prospective.fireDue(m.reminders, now);
  return { ...m, reminders, inbox: [...m.inbox, ...fired.map((r) => ({ text: r.text, due_at: r.due_at, fired_at: r.fired_at }))].slice(-20), fired };
}

export async function trackReminderSession(kv, sid, on) {
  const idx = (await kv.get(REM_INDEX, "json")) || [];
  const next = on ? [...new Set([...idx, sid])] : idx.filter((s) => s !== sid);
  if (next.length !== idx.length) await kv.put(REM_INDEX, JSON.stringify(next));
}

export async function turn(env, sid, message, now = Date.now()) {
  let m = deliver(await load(env.KV, sid), now);
  ({ items: m.vectors } = retrieval.forget(m.vectors, now)); // forgetting: decayed items drop out
  const [qv] = await retrieval.embed(env, [message]);
  const hits = retrieval.search(m.vectors, qv, now, 3);
  m.vectors = retrieval.touch(m.vectors, hits.map((h) => h.it.id), now); // recalled items are refreshed
  const pastEpisodes = episodic.byIds(m.episodes, hits.filter((h) => h.it.kind === "episode" && h.sim > 0.6).map((h) => h.it.ref));
  const notes = hits.filter((h) => h.it.kind === "note" && h.sim > 0.6).map((h) => h.it.text);
  const books = procedural.select(message);
  const reminder = prospective.parse(message, now);
  const system = [
    "You are a friendly assistant with several kinds of memory. Use them naturally; never invent facts.",
    `Semantic memory (current facts about the user):\n${semantic.render(m.facts)}`,
    pastEpisodes.length ? `Episodic memory (relevant past tasks and outcomes):\n${episodic.render(pastEpisodes)}` : "",
    notes.length ? `Retrieved notes:\n${notes.map((n) => "- " + n).join("\n")}` : "",
    books.length ? `Procedural memory (follow these playbooks):\n${procedural.render(books)}` : "",
    reminder ? `A reminder was just scheduled for ${new Date(reminder.due_at).toISOString()}: "${reminder.text}". Confirm it briefly.` : "",
    m.inbox.length ? `Reminders that came due: ${m.inbox.slice(-3).map((r) => r.text).join("; ")}` : "",
  ].filter(Boolean).join("\n\n");
  // Mid-turn: the working buffer is used as-is.
  const reply = text(await chat(env, [{ role: "system", content: system }, ...working.context(m.working), { role: "user", content: message }]));
  // After the turn: write memories, then compact working memory.
  const extracted = parseJSON(await ask(env, semantic.EXTRACT_PROMPT, message, { max_tokens: 200 }), []);
  m.facts = semantic.upsert(m.facts, Array.isArray(extracted) ? extracted : [], now);
  const { episodes, episode } = episodic.log(m.episodes, { task: message, outcome: reply, now });
  m.episodes = episodes;
  const [ev] = await retrieval.embed(env, [episodic.describe(episode)]);
  m.vectors = retrieval.add(m.vectors, { text: episodic.describe(episode), kind: "episode", ref: episode.id, vec: ev, now });
  if (reminder) { m.reminders = prospective.schedule(m.reminders, reminder, now); await trackReminderSession(env.KV, sid, true); }
  m.working = working.compact(working.append(m.working, message, reply));
  delete m.fired;
  await save(env.KV, sid, m);
  return { reply, modules: view(m, now), used: { episodes: pastEpisodes.map((e) => e.id), playbooks: books.map((b) => b.name), notes, reminder_scheduled: !!reminder } };
}

export async function remember(env, sid, note, now = Date.now()) {
  const m = await load(env.KV, sid);
  const [v] = await retrieval.embed(env, [note]);
  m.vectors = retrieval.add(m.vectors, { text: note, kind: "note", vec: v, now });
  await save(env.KV, sid, m);
  return { stored: true, items: m.vectors.length };
}

export async function recall(env, sid, query, k = 3, now = Date.now()) {
  const m = await load(env.KV, sid);
  const { items, forgotten } = retrieval.forget(m.vectors, now);
  const [qv] = await retrieval.embed(env, [query]);
  const hits = retrieval.search(items, qv, now, k, (it) => it.kind === "note");
  m.vectors = retrieval.touch(items, hits.map((h) => h.it.id), now);
  await save(env.KV, sid, m);
  const r = (x) => +x.toFixed(4);
  return { results: hits.map((h) => ({ text: h.it.text, similarity: r(h.sim), recency: r(h.recency), score: r(h.score) })), forgotten };
}

// Demo/eval helper: shift every timestamp in ONE session back by `days`, to show decay and recency without waiting weeks.
export async function age(env, sid, days, now = Date.now()) {
  const m = await load(env.KV, sid), d = days * retrieval.DAY, back = (o, ks) => { for (const k of ks) if (typeof o[k] === "number") o[k] -= d; };
  m.vectors.forEach((it) => back(it, ["at", "used_at"])); m.episodes.forEach((e) => back(e, ["at"]));
  await save(env.KV, sid, m);
  return { aged_days: days };
}

export function view(m, now = Date.now()) {
  return {
    working: { summary: m.working.summary, turns: m.working.turns.length },
    semantic: { active: semantic.active(m.facts), superseded: m.facts.filter((f) => f.superseded_at) },
    episodic: m.episodes.slice(-5).reverse().map(({ id, task, outcome, at }) => ({ id, task, outcome, at })),
    procedural: procedural.PLAYBOOKS.map((b) => b.name),
    retrieval: m.vectors.map((it) => ({ kind: it.kind, text: it.text, strength: +retrieval.strength(it, now).toFixed(3) })).slice(-8).reverse(),
    prospective: { pending: prospective.pending(m.reminders), inbox: m.inbox.slice(-5).reverse() },
  };
}

export async function cron(env, now = Date.now()) {
  const idx = (await env.KV.get(REM_INDEX, "json")) || [];
  let fired = 0;
  for (const sid of idx) {
    const m = deliver(await load(env.KV, sid), now);
    const n = m.fired.length; delete m.fired;
    if (n) { fired += n; await save(env.KV, sid, m); }
    if (!prospective.pending(m.reminders).length) await trackReminderSession(env.KV, sid, false);
  }
  return { sessions: idx.length, fired };
}
