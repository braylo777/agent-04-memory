// Live eval of the deployed agent (Llama 3.3 + BGE on Workers AI). Every case uses a fresh session that is deleted after.
//  recall: tell a fact, then ask; pass if the reply contains it.
//  supersede: tell a fact, change it, ask; pass only if the new value is the active fact, the old one is marked superseded (kept, not deleted), and the reply uses the new value.
//  recency: store an older and a newer conflicting note (older aged 45 days), query; pass if the newer ranks first.
//  forgetting: store a note, age it 90 days, store a fresh one, query; pass if the old note was forgotten and the fresh one is returned.
//  reminders: schedule via chat, wait past due, read memory; pass if the reminder fired into the inbox.
import fs from "node:fs";
import { call, pct, ratio, has, sleep } from "./lib.mjs";
const BASE = process.env.BASE || "https://agent-04-memory.braylo777.workers.dev";
const C = JSON.parse(fs.readFileSync(new URL("./cases.json", import.meta.url)));
const lat = { chat: [], recall: [] }, R = { recall: [], supersede: [], recency: [], forgetting: [], reminders: [] };
let n = 0; const sid = () => "ev" + Date.now().toString(36) + n++;
const post = async (p, b, gap) => { const r = await call(BASE + p, { method: "POST", body: JSON.stringify(b) }, gap); if (/4006|neurons/.test(r.body?.error || "")) quotaHit = true; return r; };
const chat = async (session, message) => { const r = await post("/api/chat", { session, message }); if (r.status === 200) lat.chat.push(r.ms); if (/4006|neurons/.test(r.body?.error || "")) quotaHit = true; return r; };
let quotaHit = false; // Workers AI daily free allocation exhausted: stop and record remaining cases as not run
const QUOTA = "Workers AI daily free allocation exhausted (shared account); excluded from rates";
const mem = async (session) => (await call(BASE + "/api/memory?session=" + session, {}, 500)).body;
const done = (s) => post("/api/forget", { session: s }, 500);

for (const c of C.recall) { if (quotaHit) { R.recall.push({ ...c, not_run: QUOTA }); continue; } const s = sid(); await chat(s, c.tell); const b = await chat(s, c.ask);
  R.recall.push({ ...c, ...(quotaHit ? { not_run: QUOTA } : {}), reply: b.body?.reply ?? b.body?.error, pass: b.status === 200 && has(b.body.reply, c.expect) }); await done(s); console.log("recall", R.recall.at(-1).pass); }
for (const c of C.supersede) { if (quotaHit) { R.supersede.push({ ...c, not_run: QUOTA }); continue; } const s = sid(); await chat(s, c.tell); await chat(s, c.change); const b = await chat(s, c.ask);
  const sem = b.body?.modules?.semantic, act = (sem?.active || []).map((f) => f.value).join(" | "), sup = (sem?.superseded || []).map((f) => f.value).join(" | ");
  const store_ok = has(act, [c.new]) && !has(act, [c.old]) && has(sup, [c.old]), reply_ok = has(b.body?.reply, [c.new]);
  R.supersede.push({ ...c, ...(quotaHit ? { not_run: QUOTA } : {}), active: act, superseded: sup, reply: b.body?.reply, store_ok, reply_ok, pass: store_ok && reply_ok }); await done(s); console.log("supersede", store_ok, reply_ok); }
for (const c of C.recency) { if (quotaHit) { R.recency.push({ ...c, not_run: QUOTA }); continue; } const s = sid(); await post("/api/remember", { session: s, note: c.old }); await post("/api/age", { session: s, days: 45 }, 500);
  await post("/api/remember", { session: s, note: c.new }); const r = await post("/api/recall", { session: s, query: c.q }); if (r.status === 200) lat.recall.push(r.ms);
  const res = r.body?.results || [], cosineFirst = [...res].sort((a, b) => b.similarity - a.similarity)[0]?.text;
  R.recency.push({ ...c, ...(quotaHit ? { not_run: QUOTA } : {}), results: res, pass: has(res[0]?.text, [c.expect]), cosine_only_would_pass: has(cosineFirst, [c.expect]) }); await done(s); console.log("recency", R.recency.at(-1).pass); }
for (const c of C.forgetting) { if (quotaHit) { R.forgetting.push({ ...c, not_run: QUOTA }); continue; } const s = sid(); await post("/api/remember", { session: s, note: c.old }); await post("/api/age", { session: s, days: 90 }, 500);
  await post("/api/remember", { session: s, note: c.fresh }); const r = await post("/api/recall", { session: s, query: c.q }); if (r.status === 200) lat.recall.push(r.ms);
  const texts = (r.body?.results || []).map((x) => x.text), forgot = (r.body?.forgotten || []).includes(c.old);
  R.forgetting.push({ ...c, ...(quotaHit ? { not_run: QUOTA } : {}), results: texts, forgotten: r.body?.forgotten, pass: forgot && !texts.includes(c.old) && texts.includes(c.fresh) }); await done(s); console.log("forget", R.forgetting.at(-1).pass); }
for (const c of C.reminders) { if (quotaHit) { R.reminders.push({ ...c, not_run: QUOTA }); continue; } const s = sid(); const r = await chat(s, c.msg); const due = r.body?.modules?.prospective?.pending?.[0]?.due_at;
  if (due) await sleep(Math.max(0, due - Date.now()) + 3000); const m = await mem(s);
  const inbox = m?.prospective?.inbox || [], fired = inbox.some((x) => has(x.text, [c.expect]));
  R.reminders.push({ ...c, ...(quotaHit ? { not_run: QUOTA } : {}), scheduled: !!due, inbox, pending_after: m?.prospective?.pending?.length, pass: !!due && fired && !m.prospective.pending.length }); await done(s); console.log("remind", R.reminders.at(-1).pass); }

const ran = (k) => R[k].filter((x) => !x.not_run);
const m = (k) => ({ ...ratio(ran(k).filter((x) => x.pass).length, ran(k).length), not_run: R[k].length - ran(k).length });
const out = { project: "agent-04-memory", base: BASE, models: { llm: "@cf/meta/llama-3.3-70b-instruct-fp8-fast", embed: "@cf/baai/bge-base-en-v1.5" }, run_at: new Date().toISOString(),
  n_cases: Object.values(R).reduce((a, x) => a + x.length, 0),
  metrics: { recall_accuracy: m("recall"), supersede_correctness: m("supersede"),
    supersede_store_only: ratio(ran("supersede").filter((x) => x.store_ok).length, ran("supersede").length),
    recency_ranking_accuracy: m("recency"), recency_cosine_only_baseline: ratio(ran("recency").filter((x) => x.cosine_only_would_pass).length, ran("recency").length),
    forgetting_accuracy: m("forgetting"), reminder_firing: m("reminders"),
    latency_ms: { chat: { n: lat.chat.length, p50: pct(lat.chat, 50), p95: pct(lat.chat, 95) }, recall: { n: lat.recall.length, p50: pct(lat.recall, 50), p95: pct(lat.recall, 95) }, note: "end-to-end from the eval client" } },
  notes: "Recency and forgetting use /api/age to shift one session's timestamps back (no waiting weeks). Reminders were delivered by the on-request check; the */5 cron path is covered by unit tests.", cases: R };
fs.writeFileSync(new URL("./results.json", import.meta.url), JSON.stringify(out, null, 2));
console.log(JSON.stringify(out.metrics, null, 2));
