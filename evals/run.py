"""Live eval of the deployed agent (Llama 3.3 + BGE on Workers AI). Every case uses a fresh session that is deleted after.
 recall: tell a fact, then ask; pass if the reply contains it.
 supersede: tell, change, ask; pass only if the new value is active, the old one is marked superseded (kept) and the reply uses the new value.
 recency: store an older (aged 45 days) and a newer conflicting note; pass if the newer ranks first.
 forgetting: store a note, age it 90 days, store a fresh one; pass if the old one was forgotten and the fresh one is returned.
 reminders: schedule via chat, wait past due, read memory; pass if the reminder fired into the inbox.
Usage: BASE=https://... python3 evals/run.py"""
import json, math, os, re, time, urllib.error, urllib.request
from datetime import datetime, timezone
from pathlib import Path

here = Path(__file__).parent
BASE = os.environ.get("BASE", "https://agent-04-memory.braylo777.workers.dev")
GAP = float(os.environ.get("EVAL_GAP_MS", "12500")) / 1000
C = json.loads((here / "cases.json").read_text())
lat = {"chat": [], "recall": []}
R = {k: [] for k in ("recall", "supersede", "recency", "forgetting", "reminders")}
QUOTA = "Workers AI daily free allocation exhausted (shared account); excluded from rates"
state = {"quota": False, "n": 0}


def call(url, body=None, gap=GAP):
    for _ in range(6):
        time.sleep(gap)
        t = time.time()
        req = urllib.request.Request(url, json.dumps(body).encode() if body is not None else None, {"content-type": "application/json"})
        try:
            r = urllib.request.urlopen(req, timeout=60); status = r.status; raw = r.read()
        except urllib.error.HTTPError as e:
            status = e.code; raw = e.read()
        ms = (time.time() - t) * 1000
        if status == 429:
            time.sleep(30); continue
        try:
            return {"status": status, "ms": ms, "body": json.loads(raw)}
        except ValueError:
            return {"status": status, "ms": ms, "body": None}
    raise RuntimeError("rate limited repeatedly")


def pct(xs, p):
    s = sorted(xs)
    return round(s[max(0, math.ceil(p / 100 * len(s)) - 1)]) if s else None


ratio = lambda k, n: {"correct": k, "n": n, "rate": round(k / n, 4) if n else None}
has = lambda s, keys: any(k.lower() in str(s or "").lower() for k in keys)
sid = lambda: "ev" + format(int(time.time() * 1000), "x") + str(state.__setitem__("n", state["n"] + 1) or state["n"])


def post(p, b, gap=GAP):
    r = call(BASE + p, b, gap)
    if re.search(r"4006|neurons", (r["body"] or {}).get("error", "") or ""):
        state["quota"] = True
    return r


def chat(s, message):
    r = post("/api/chat", {"session": s, "message": message})
    if r["status"] == 200:
        lat["chat"].append(r["ms"])
    return r


mem = lambda s: call(BASE + "/api/memory?session=" + s, None, 0.5)["body"]
done = lambda s: post("/api/forget", {"session": s}, 0.5)


def run(kind, fn):
    for c in C[kind]:
        if state["quota"]:
            R[kind].append({**c, "not_run": QUOTA}); continue
        s = sid()
        res = fn(c, s)
        if state["quota"]:
            res["not_run"] = QUOTA
        R[kind].append({**c, **res}); done(s)
        print(kind, res.get("pass"))


def f_recall(c, s):
    chat(s, c["tell"]); b = chat(s, c["ask"])
    return {"reply": (b["body"] or {}).get("reply") or (b["body"] or {}).get("error"), "pass": b["status"] == 200 and has(b["body"]["reply"], c["expect"])}


def f_supersede(c, s):
    chat(s, c["tell"]); chat(s, c["change"]); b = chat(s, c["ask"])
    sem = ((b["body"] or {}).get("modules") or {}).get("semantic") or {}
    act = " | ".join(f["value"] for f in sem.get("active", [])); sup = " | ".join(f["value"] for f in sem.get("superseded", []))
    store_ok = has(act, [c["new"]]) and not has(act, [c["old"]]) and has(sup, [c["old"]])
    reply_ok = has((b["body"] or {}).get("reply"), [c["new"]])
    return {"active": act, "superseded": sup, "reply": (b["body"] or {}).get("reply"), "store_ok": store_ok, "reply_ok": reply_ok, "pass": store_ok and reply_ok}


def f_recency(c, s):
    post("/api/remember", {"session": s, "note": c["old"]}); post("/api/age", {"session": s, "days": 45}, 0.5)
    post("/api/remember", {"session": s, "note": c["new"]}); r = post("/api/recall", {"session": s, "query": c["q"]})
    if r["status"] == 200:
        lat["recall"].append(r["ms"])
    res = (r["body"] or {}).get("results", [])
    cos = max(res, key=lambda x: x["similarity"], default={}).get("text")
    return {"results": res, "pass": has(res[0]["text"] if res else None, [c["expect"]]), "cosine_only_would_pass": has(cos, [c["expect"]])}


def f_forget(c, s):
    post("/api/remember", {"session": s, "note": c["old"]}); post("/api/age", {"session": s, "days": 90}, 0.5)
    post("/api/remember", {"session": s, "note": c["fresh"]}); r = post("/api/recall", {"session": s, "query": c["q"]})
    if r["status"] == 200:
        lat["recall"].append(r["ms"])
    texts = [x["text"] for x in (r["body"] or {}).get("results", [])]; gone = (r["body"] or {}).get("forgotten", [])
    return {"results": texts, "forgotten": gone, "pass": c["old"] in gone and c["old"] not in texts and c["fresh"] in texts}


def f_remind(c, s):
    r = chat(s, c["msg"])
    pend = ((((r["body"] or {}).get("modules") or {}).get("prospective") or {}).get("pending") or [{}])
    due = pend[0].get("due_at")
    if due:
        time.sleep(max(0, due - time.time() * 1000) / 1000 + 3)
    m = mem(s) or {}
    pr = m.get("prospective") or {}
    inbox = pr.get("inbox", [])
    fired = any(has(x["text"], [c["expect"]]) for x in inbox)
    return {"scheduled": bool(due), "inbox": inbox, "pending_after": len(pr.get("pending", [])), "pass": bool(due) and fired and not pr.get("pending")}


run("recall", f_recall); run("supersede", f_supersede); run("recency", f_recency); run("forgetting", f_forget); run("reminders", f_remind)
ran = lambda k: [x for x in R[k] if "not_run" not in x]
m = lambda k: {**ratio(sum(1 for x in ran(k) if x["pass"]), len(ran(k))), "not_run": len(R[k]) - len(ran(k))}
out = {"project": "agent-04-memory", "base": BASE, "models": {"llm": "@cf/meta/llama-3.3-70b-instruct-fp8-fast", "embed": "@cf/baai/bge-base-en-v1.5"},
       "run_at": datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z"), "n_cases": sum(len(v) for v in R.values()),
       "metrics": {"recall_accuracy": m("recall"), "supersede_correctness": m("supersede"),
                   "supersede_store_only": ratio(sum(1 for x in ran("supersede") if x["store_ok"]), len(ran("supersede"))),
                   "recency_ranking_accuracy": m("recency"), "recency_cosine_only_baseline": ratio(sum(1 for x in ran("recency") if x["cosine_only_would_pass"]), len(ran("recency"))),
                   "forgetting_accuracy": m("forgetting"), "reminder_firing": m("reminders"),
                   "latency_ms": {"chat": {"n": len(lat["chat"]), "p50": pct(lat["chat"], 50), "p95": pct(lat["chat"], 95)}, "recall": {"n": len(lat["recall"]), "p50": pct(lat["recall"], 50), "p95": pct(lat["recall"], 95)}, "note": "end-to-end from the eval client"}},
       "notes": "Recency and forgetting use /api/age to shift one session's timestamps back (no waiting weeks). Reminders were delivered by the on-request check; the */5 cron path is covered by unit tests.", "cases": R}
(here / "results.json").write_text(json.dumps(out, indent=2))
print(json.dumps(out["metrics"], indent=2))
