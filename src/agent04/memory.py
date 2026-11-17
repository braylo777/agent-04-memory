"""Seven memory modules: working, semantic, episodic, procedural (playbooks), retrieval, prospective, store."""
import json, math, re

DAY = 86400000
HALF_LIFE_DAYS, RECENCY_WEIGHT, FORGET_BELOW, MAX_ITEMS = 14, 0.15, 0.05, 60
MAX_TURNS, KEEP_TURNS, SUMMARY_CHARS = 8, 4, 800
MAX_EPISODES = 50
TTL = 60 * 60 * 24 * 30
REM_INDEX = "rem-index"
KEYS = ["name", "location", "hometown", "job", "employer", "diet", "allergy", "pet", "family", "favorite_color", "favorite_food", "favorite_music", "language", "hobby", "goal", "vehicle", "birthday", "other"]
EXTRACT_PROMPT = ('Extract durable facts about the USER from the message. Reply ONLY with a JSON array of {"key","value"} objects, or [] if none. key must be one of: '
                  + ", ".join(KEYS) + '. For "other" use key "other:<short topic>". value is a short phrase, e.g. [{"key":"name","value":"Sam"},{"key":"location","value":"Austin"}].')

# ---- store ----
key = lambda sid: f"mem:{sid}"
empty = lambda: {"working": {"summary": "", "turns": []}, "facts": [], "episodes": [], "vectors": [], "reminders": [], "inbox": []}


def load(kv, sid):
    raw = kv.get(key(sid))
    return {**empty(), **(json.loads(raw) if raw else {})}


def save(kv, sid, m):
    kv.put(key(sid), json.dumps(m, separators=(",", ":")), TTL)


# ---- working ----
def w_context(w):
    return ([{"role": "system", "content": f"Summary of earlier conversation: {w['summary']}"}] if w["summary"] else []) + w["turns"]


def w_append(w, user, assistant):
    return {**w, "turns": [*w["turns"], {"role": "user", "content": user}, {"role": "assistant", "content": assistant}]}


def w_compact(w):
    if len(w["turns"]) <= MAX_TURNS:
        return w
    old = w["turns"][: len(w["turns"]) - KEEP_TURNS]
    digest = " | ".join(f"{t['role']}: {re.sub(r'\s+', ' ', t['content'])[:80]}" for t in old)
    summary = (w["summary"] + " | " if w["summary"] else "") + digest
    if len(summary) > SUMMARY_CHARS:
        summary = "…" + summary[-SUMMARY_CHARS:]
    return {"summary": summary, "turns": w["turns"][-KEEP_TURNS:]}


# ---- semantic ----
_norm = lambda s: str(s).strip().lower()


def upsert(facts, items, now):
    out = [dict(f) for f in facts]
    for it in items or []:
        if not isinstance(it, dict) or not isinstance(it.get("key"), str) or not isinstance(it.get("value"), str) or not it["value"].strip():
            continue
        k, v = _norm(it["key"]), it["value"].strip()
        cur = next((f for f in out if f["key"] == k and not f.get("superseded_at")), None)
        if cur and _norm(cur["value"]) == _norm(v):
            cur["confirmed_at"] = now
            continue
        fid = f"f{len(out) + 1}"
        if cur:
            cur["superseded_at"] = now
            cur["superseded_by"] = fid
        out.append({"id": fid, "key": k, "value": v, "created_at": now, **({"supersedes": cur["id"]} if cur else {})})
    return out


active = lambda facts: [f for f in facts if not f.get("superseded_at")]
history = lambda facts, k: [f for f in facts if f["key"] == _norm(k)]
sem_render = lambda facts: "\n".join(f"- {f['key']}: {f['value']}" for f in active(facts)) or "(none yet)"


# ---- episodic ----
def ep_log(episodes, task, outcome, now):
    ep = {"id": f"e{now}-{len(episodes)}", "task": task[:300], "outcome": re.sub(r"\s+", " ", outcome)[:300], "at": now}
    return [*episodes, ep][-MAX_EPISODES:], ep


ep_describe = lambda ep: f"Past task: {ep['task']} -> outcome: {ep['outcome']}"


def ep_by_ids(episodes, ids):
    return [e for e in (next((e for e in episodes if e["id"] == i), None) for i in ids) if e]


def _iso_date(ms):
    from datetime import datetime, timezone
    return datetime.fromtimestamp(ms / 1000, timezone.utc).strftime("%Y-%m-%d")


ep_render = lambda eps: "\n".join(f'- {_iso_date(e["at"])}: "{e["task"]}" -> {e["outcome"]}' for e in eps)

# ---- procedural ----
PLAYBOOKS = [
    {"name": "plan-a-trip", "triggers": ["trip", "travel", "vacation", "itinerary"], "steps": ["Ask dates and budget if missing", "Use the user's known location as origin", "Respect diet and allergies for food ideas", "Give a day-by-day outline"]},
    {"name": "recommend-food", "triggers": ["dinner", "lunch", "restaurant", "eat", "recipe", "cook"], "steps": ["Check diet and allergy facts first", "Prefer the user's location", "Offer two or three options with one-line reasons"]},
    {"name": "write-email", "triggers": ["email", "message to", "draft"], "steps": ["Use the user's name and role in the sign-off", "Keep it under 120 words", "End with one clear ask"]},
    {"name": "study-plan", "triggers": ["study", "exam", "learn", "certification"], "steps": ["Use the user's stated goal", "Break into weekly milestones", "Suggest one practice method per week"]},
    {"name": "debug-code", "triggers": ["bug", "error", "stack trace", "exception", "debug"], "steps": ["Ask for the exact error and language if missing", "Reproduce, isolate, then fix", "Explain the root cause in one line"]},
]


def select(message, books=PLAYBOOKS):
    m = message.lower()
    scored = [(b, sum(1 for t in b["triggers"] if re.search(rf"\b{re.escape(t)}", m, re.I))) for b in books]
    return [b for b, h in sorted([x for x in scored if x[1]], key=lambda x: -x[1])]  # stable sort, like JS


pb_render = lambda books: "\n".join(f"Playbook {b['name']}: {'; '.join(b['steps'])}" for b in books)

# ---- prospective ----
_UNIT = {"second": 1000, "sec": 1000, "minute": 60000, "min": 60000, "hour": 3600000, "day": 86400000}


def parse_reminder(message, now):
    m = re.search(r"remind me (?:in|after) (\d+)\s*(seconds?|secs?|minutes?|mins?|hours?|days?)\s*(?:to |that |about )?(.+)", message, re.I)
    if not m:
        return None
    unit = _UNIT.get(re.sub(r"s$", "", m.group(2).lower()))
    return {"text": re.sub(r"[.!]+$", "", m.group(3)).strip(), "due_at": now + int(m.group(1)) * unit} if unit else None


def schedule(reminders, r, now):
    return [*reminders, {"id": f"r{now}-{len(reminders)}", **r, "created_at": now, "fired_at": None}]


def fire_due(reminders, now):
    fired, out = [], []
    for r in reminders:
        if not r.get("fired_at") and r["due_at"] <= now:
            fired.append({**r, "fired_at": now})
            out.append({**r, "fired_at": now})
        else:
            out.append(r)
    return out, fired


pending = lambda reminders: [r for r in reminders if not r.get("fired_at")]


# ---- retrieval ----
def cosine(a, b):
    d = x = y = 0.0
    for p, q in zip(a, b):
        d += p * q; x += p * p; y += q * q
    den = math.sqrt(x) * math.sqrt(y)
    return d / (den or 1)


strength = lambda it, now: 0.5 ** ((now - (it["used_at"] if it.get("used_at") is not None else it["at"])) / (HALF_LIFE_DAYS * DAY))
score = lambda sim, it, now: sim + RECENCY_WEIGHT * strength(it, now)


def vadd(items, text, kind, ref, vec, now):
    it = {"id": f"v{now}-{len(items)}", "text": text, "kind": kind, "ref": ref, "vec": [round(x, 4) for x in vec], "at": now}
    return [*items, it][-MAX_ITEMS:]


def vsearch(items, qv, now, k=3, flt=lambda it: True):
    hits = [{"it": it, "sim": (s := cosine(qv, it["vec"])), "recency": strength(it, now), "score": score(s, it, now)} for it in items if flt(it)]
    return sorted(hits, key=lambda h: -h["score"])[:k]


touch = lambda items, ids, now: [{**it, "used_at": now} if it["id"] in ids else it for it in items]


def forget(items, now):
    kept = [it for it in items if strength(it, now) >= FORGET_BELOW]
    return kept, [it["text"] for it in items if it not in kept]
