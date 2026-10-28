import { json, limited, body, cap } from "./core.js";
import { turn, remember, recall, age, view, deliver, cron } from "./agent.js";
import { load, save, key } from "./memory/store.js";
import page from "./page.html";

const validSid = (s) => typeof s === "string" && /^[a-z0-9]{4,32}$/i.test(s);
const guard = async (req, env, fn) => { if (await limited(req, env)) return json({ error: "Rate limit: try again in a minute." }, 429); try { return json(await fn()); } catch (e) { return json({ error: "Agent error: " + e.message }, 502); } };

export default {
  async fetch(req, env) {
    const url = new URL(req.url), p = url.pathname;
    if (req.method === "GET" && p === "/api/memory") {
      const sid = url.searchParams.get("session");
      if (!validSid(sid)) return json({ error: "session required" }, 400);
      const m = deliver(await load(env.KV, sid), Date.now());
      if (m.fired.length) { delete m.fired; await save(env.KV, sid, m); }
      return json(view(m));
    }
    if (req.method !== "POST" || !p.startsWith("/api/")) return new Response(page, { headers: { "content-type": "text/html;charset=utf-8" } });
    const b = await body(req);
    if (!validSid(b.session)) return json({ error: "session required" }, 400);
    if (p === "/api/forget") { await env.KV.delete(key(b.session)); return json({ forgotten: true }); }
    if (p === "/api/age") { const d = Number(b.days); if (!(d > 0 && d <= 365)) return json({ error: "days 1-365" }, 400); return json(await age(env, b.session, d)); }
    if (p === "/api/chat") { if (!cap(b.message)) return json({ error: "message (<=500 chars) required" }, 400); return guard(req, env, () => turn(env, b.session, b.message)); }
    if (p === "/api/remember") { if (!cap(b.note, 300)) return json({ error: "note (<=300 chars) required" }, 400); return guard(req, env, () => remember(env, b.session, b.note)); }
    if (p === "/api/recall") { if (!cap(b.query, 300)) return json({ error: "query (<=300 chars) required" }, 400); return guard(req, env, () => recall(env, b.session, b.query)); }
    return json({ error: "not found" }, 404);
  },
  async scheduled(_event, env, ctx) { ctx.waitUntil(cron(env)); },
};
