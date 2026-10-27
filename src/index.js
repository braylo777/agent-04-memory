import { json, limited, body, cap } from "./core.js";
import { chatWithMemory, load, key } from "./memory.js";
import page from "./page.html";

const validSid = (s) => typeof s === "string" && /^[a-z0-9]{4,32}$/i.test(s);

export default {
  async fetch(req, env) {
    const url = new URL(req.url);
    const sid = url.searchParams.get("session");
    if (req.method === "GET" && url.pathname === "/api/memory") {
      if (!validSid(sid)) return json({ error: "session required" }, 400);
      return json(await load(env.KV, sid));
    }
    if (req.method === "POST" && url.pathname === "/api/forget") {
      const b = await body(req); if (!validSid(b.session)) return json({ error: "session required" }, 400);
      await env.KV.delete(key(b.session)); return json({ forgotten: true });
    }
    if (req.method === "POST" && url.pathname === "/api/chat") {
      if (await limited(req, env)) return json({ error: "Rate limit: try again in a minute." }, 429);
      const b = await body(req);
      if (!validSid(b.session) || !cap(b.message)) return json({ error: "session and message (<=500 chars) required" }, 400);
      try { return json(await chatWithMemory(env, b.session, b.message)); } catch (e) { return json({ error: "Agent error: " + e.message }, 502); }
    }
    return new Response(page, { headers: { "content-type": "text/html;charset=utf-8" } });
  },
};
