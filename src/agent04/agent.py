"""Agent loop over the memory modules. ai(model, input) -> dict and kv are injected so tests run offline."""
import json, re, time
from . import memory as M

MODEL = "@cf/meta/llama-3.3-70b-instruct-fp8-fast"
EMBED = "@cf/baai/bge-base-en-v1.5"


def text(r):
    v = r.get("response") if isinstance(r, dict) else None
    return v if isinstance(v, str) else json.dumps(v if v is not None else r, separators=(",", ":"), ensure_ascii=False)


def parse_json(s, fallback=None):
    m = re.search(r"[\[{][\s\S]*[\]}]", str(s))
    try:
        return json.loads(m.group(0)) if m else fallback
    except ValueError:
        return fallback


def cap(s, n=500):
    return isinstance(s, str) and bool(s.strip()) and len(s) <= n


def chat(ai, messages, max_tokens=400):
    return ai(MODEL, {"messages": messages, "max_tokens": max_tokens})


def ask(ai, system, user, max_tokens=400):
    return text(chat(ai, [{"role": "system", "content": system}, {"role": "user", "content": user}], max_tokens))


def embed(ai, texts):
    return ai(EMBED, {"text": texts})["data"]


now_ms = lambda: int(time.time() * 1000)


def deliver(m, now):
    reminders, fired = M.fire_due(m["reminders"], now)
    inbox = [*m["inbox"], *({"text": r["text"], "due_at": r["due_at"], "fired_at": r["fired_at"]} for r in fired)][-20:]
    return {**m, "reminders": reminders, "inbox": inbox, "fired": fired}


def track_reminder_session(kv, sid, on):
    raw = kv.get(M.REM_INDEX)
    idx = json.loads(raw) if raw else []
    nxt = list(dict.fromkeys([*idx, sid])) if on else [s for s in idx if s != sid]
    if len(nxt) != len(idx):
        kv.put(M.REM_INDEX, json.dumps(nxt), None)


def view(m, now=None):
    now = now_ms() if now is None else now
    return {
        "working": {"summary": m["working"]["summary"], "turns": len(m["working"]["turns"])},
        "semantic": {"active": M.active(m["facts"]), "superseded": [f for f in m["facts"] if f.get("superseded_at")]},
        "episodic": [{k: e[k] for k in ("id", "task", "outcome", "at")} for e in reversed(m["episodes"][-5:])],
        "procedural": [b["name"] for b in M.PLAYBOOKS],
        "retrieval": [{"kind": it["kind"], "text": it["text"], "strength": round(M.strength(it, now), 3)} for it in reversed(m["vectors"][-8:])],
        "prospective": {"pending": M.pending(m["reminders"]), "inbox": list(reversed(m["inbox"][-5:]))},
    }


def turn(ai, kv, sid, message, now=None):
    now = now_ms() if now is None else now
    m = deliver(M.load(kv, sid), now)
    m["vectors"], _ = M.forget(m["vectors"], now)
    qv = embed(ai, [message])[0]
    hits = M.vsearch(m["vectors"], qv, now, 3)
    m["vectors"] = M.touch(m["vectors"], [h["it"]["id"] for h in hits], now)
    past = M.ep_by_ids(m["episodes"], [h["it"]["ref"] for h in hits if h["it"]["kind"] == "episode" and h["sim"] > 0.6])
    notes = [h["it"]["text"] for h in hits if h["it"]["kind"] == "note" and h["sim"] > 0.6]
    books = M.select(message)
    reminder = M.parse_reminder(message, now)
    system = "\n\n".join(p for p in [
        "You are a friendly assistant with several kinds of memory. Use them naturally; never invent facts.",
        f"Semantic memory (current facts about the user):\n{M.sem_render(m['facts'])}",
        f"Episodic memory (relevant past tasks and outcomes):\n{M.ep_render(past)}" if past else "",
        "Retrieved notes:\n" + "\n".join("- " + n for n in notes) if notes else "",
        f"Procedural memory (follow these playbooks):\n{M.pb_render(books)}" if books else "",
        f'A reminder was just scheduled for {_iso(reminder["due_at"])}: "{reminder["text"]}". Confirm it briefly.' if reminder else "",
        "Reminders that came due: " + "; ".join(r["text"] for r in m["inbox"][-3:]) if m["inbox"] else "",
    ] if p)
    reply = text(chat(ai, [{"role": "system", "content": system}, *M.w_context(m["working"]), {"role": "user", "content": message}]))
    extracted = parse_json(ask(ai, M.EXTRACT_PROMPT, message, 200), [])
    m["facts"] = M.upsert(m["facts"], extracted if isinstance(extracted, list) else [], now)
    m["episodes"], episode = M.ep_log(m["episodes"], message, reply, now)
    ev = embed(ai, [M.ep_describe(episode)])[0]
    m["vectors"] = M.vadd(m["vectors"], M.ep_describe(episode), "episode", episode["id"], ev, now)
    if reminder:
        m["reminders"] = M.schedule(m["reminders"], reminder, now)
        track_reminder_session(kv, sid, True)
    m["working"] = M.w_compact(M.w_append(m["working"], message, reply))
    m.pop("fired", None)
    M.save(kv, sid, m)
    return {"reply": reply, "modules": view(m, now), "used": {"episodes": [e["id"] for e in past], "playbooks": [b["name"] for b in books], "notes": notes, "reminder_scheduled": bool(reminder)}}


def _iso(ms):
    from datetime import datetime, timezone
    return datetime.fromtimestamp(ms / 1000, timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def remember(ai, kv, sid, note, now=None):
    now = now_ms() if now is None else now
    m = M.load(kv, sid)
    v = embed(ai, [note])[0]
    m["vectors"] = M.vadd(m["vectors"], note, "note", None, v, now)
    M.save(kv, sid, m)
    return {"stored": True, "items": len(m["vectors"])}


def recall(ai, kv, sid, query, k=3, now=None):
    now = now_ms() if now is None else now
    m = M.load(kv, sid)
    items, forgotten = M.forget(m["vectors"], now)
    qv = embed(ai, [query])[0]
    hits = M.vsearch(items, qv, now, k, lambda it: it["kind"] == "note")
    m["vectors"] = M.touch(items, [h["it"]["id"] for h in hits], now)
    M.save(kv, sid, m)
    r = lambda x: round(x, 4)
    return {"results": [{"text": h["it"]["text"], "similarity": r(h["sim"]), "recency": r(h["recency"]), "score": r(h["score"])} for h in hits], "forgotten": forgotten}


def age(kv, sid, days, now=None):
    """Demo/eval helper: shift every timestamp in ONE session back by `days`."""
    m = M.load(kv, sid)
    d = days * M.DAY
    for it in m["vectors"]:
        for k in ("at", "used_at"):
            if isinstance(it.get(k), (int, float)):
                it[k] -= d
    for e in m["episodes"]:
        if isinstance(e.get("at"), (int, float)):
            e["at"] -= d
    M.save(kv, sid, m)
    return {"aged_days": days}


def cron(kv, now=None):
    now = now_ms() if now is None else now
    raw = kv.get(M.REM_INDEX)
    idx = json.loads(raw) if raw else []
    fired = 0
    for sid in idx:
        m = deliver(M.load(kv, sid), now)
        n = len(m.pop("fired"))
        if n:
            fired += n
            M.save(kv, sid, m)
        if not M.pending(m["reminders"]):
            track_reminder_session(kv, sid, False)
    return {"sessions": len(idx), "fired": fired}


_valid_sid = lambda s: isinstance(s, str) and re.fullmatch(r"[a-z0-9]{4,32}", s, re.I) is not None


def handle(ai, kv, method, path, query_session=None, body=None, now=None):
    """Pure router: returns (status, json). The Worker adapts HTTP to this."""
    body = body if isinstance(body, dict) else {}
    if method == "GET" and path == "/api/memory":
        if not _valid_sid(query_session):
            return 400, {"error": "session required"}
        m = deliver(M.load(kv, query_session), now_ms() if now is None else now)
        if m["fired"]:
            m.pop("fired")
            M.save(kv, query_session, m)
        return 200, view(m, now)
    if method != "POST" or not path.startswith("/api/"):
        return 0, None
    sid = body.get("session")
    if not _valid_sid(sid):
        return 400, {"error": "session required"}
    if path == "/api/forget":
        kv.delete(M.key(sid))
        return 200, {"forgotten": True}
    if path == "/api/age":
        try:
            d = float(body.get("days"))
        except (TypeError, ValueError):
            d = 0
        if not (0 < d <= 365):
            return 400, {"error": "days 1-365"}
        return 200, age(kv, sid, body["days"] if isinstance(body["days"], (int, float)) else d, now)
    if path == "/api/chat":
        return (400, {"error": "message (<=500 chars) required"}) if not cap(body.get("message")) else (200, turn(ai, kv, sid, body["message"], now))
    if path == "/api/remember":
        return (400, {"error": "note (<=300 chars) required"}) if not cap(body.get("note"), 300) else (200, remember(ai, kv, sid, body["note"], now))
    if path == "/api/recall":
        return (400, {"error": "query (<=300 chars) required"}) if not cap(body.get("query"), 300) else (200, recall(ai, kv, sid, body["query"], 3, now))
    return 404, {"error": "not found"}
