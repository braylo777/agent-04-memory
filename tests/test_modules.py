import pytest
from agent04 import memory as M

DAY, now = M.DAY, 1000 * M.DAY


def test_episodic_log_and_caps():
    eps, ep = M.ep_log([], "Plan Rome trip", "Made a 3-day plan", 1)
    assert len(eps) == 1 and "Rome" in M.ep_describe(ep) and "3-day" in M.ep_describe(ep)
    e = []
    for i in range(M.MAX_EPISODES + 5):
        e, _ = M.ep_log(e, f"t{i}", "o", i)
    assert len(e) == M.MAX_EPISODES
    assert [x["task"] for x in M.ep_by_ids(e, [e[0]["id"], "nope"])] == ["t5"] and '"t5"' in M.ep_render([e[0]])


def test_playbooks():
    assert M.select("Plan a trip and travel itinerary")[0]["name"] == "plan-a-trip"
    assert M.select("where for dinner?")[0]["name"] == "recommend-food"
    assert M.select("hello there") == [] and M.select("the debtor") == []
    assert all(b["triggers"] and b["steps"] for b in M.PLAYBOOKS)
    assert "Playbook plan-a-trip: Ask dates" in M.pb_render([M.PLAYBOOKS[0]])


def test_reminders():
    assert M.parse_reminder("Remind me in 10 minutes to call Mom.", 0) == {"text": "call Mom", "due_at": 600000}
    assert M.parse_reminder("remind me after 2 days about the dentist", 0)["due_at"] == 2 * 86400000
    assert M.parse_reminder("hello", 0) is None
    r = M.schedule([], {"text": "a", "due_at": 10}, 0)
    r = M.schedule(r, {"text": "b", "due_at": 100}, 0)
    rem, fired = M.fire_due(r, 50)
    assert [f["text"] for f in fired] == ["a"] and [f["text"] for f in M.pending(rem)] == ["b"]
    rem, fired = M.fire_due(rem, 60)
    assert fired == []


def test_strength_halves():
    assert M.strength({"at": now}, now) == 1
    assert abs(M.strength({"at": now - M.HALF_LIFE_DAYS * DAY}, now) - 0.5) < 1e-9


def test_recency_breaks_ties_and_similarity_wins():
    s = M.vadd([], "old", None, None, [1, 0], now - 60 * DAY)
    s = M.vadd(s, "new", None, None, [1, 0], now)
    assert M.vsearch(s, [1, 0], now, 2)[0]["it"]["text"] == "new"
    s = M.vadd([], "relevant-old", None, None, [1, 0], now - 30 * DAY)
    s = M.vadd(s, "irrelevant-new", None, None, [0, 1], now)
    assert M.vsearch(s, [1, 0], now, 1)[0]["it"]["text"] == "relevant-old"


def test_forgetting_and_touch():
    s = M.vadd([], "ancient", None, None, [1], now - 90 * DAY)
    s = M.vadd(s, "fresh", None, None, [1], now)
    kept, gone = M.forget(s, now)
    assert [i["text"] for i in kept] == ["fresh"] and gone == ["ancient"]
    assert M.strength({"at": now - 90 * DAY}, now) < M.FORGET_BELOW
    t = M.touch(M.vadd([], "used", None, None, [1], now - 90 * DAY), [f"v{now - 90 * DAY}-0"], now)
    assert len(M.forget(t, now)[0]) == 1


def test_semantic():
    assert [f["value"] for f in M.active(M.upsert([], [{"key": "name", "value": "Sam"}], 1))] == ["Sam"]
    f = M.upsert([], [{"key": "location", "value": "Denver"}], 1)
    f = M.upsert(f, [{"key": "Location", "value": "Lisbon"}], 2)
    assert len(f) == 2
    old, cur = M.history(f, "location")
    assert old["superseded_at"] == 2 and old["superseded_by"] == cur["id"] and cur["supersedes"] == old["id"]
    assert [x["value"] for x in M.active(f)] == ["Lisbon"] and "Denver" not in M.sem_render(f)
    f = M.upsert([], [{"key": "diet", "value": "vegetarian"}], 1)
    f = M.upsert(f, [{"key": "diet", "value": "Vegetarian "}, None, {"key": 3}, {"key": "x", "value": ""}], 5)
    assert len(f) == 1 and f[0]["confirmed_at"] == 5
    f = []
    for i, v in enumerate("ABC"):
        f = M.upsert(f, [{"key": "job", "value": v}], i + 1)
    assert [x["value"] for x in M.active(f)] == ["C"] and len([x for x in f if x.get("superseded_at")]) == 2


def test_working():
    w0 = {"summary": "", "turns": []}
    def fill(n):
        w = w0
        for i in range(n):
            w = M.w_append(w, f"u{i}", f"a{i}")
        return w
    w = fill(M.MAX_TURNS // 2)
    assert M.w_compact(w) is w
    w = M.w_compact(fill(5))
    assert len(w["turns"]) == M.KEEP_TURNS and w["turns"][-1]["content"] == "a4" and "user: u0" in w["summary"]
    w = w0
    for _ in range(200):
        w = M.w_compact(M.w_append(w, "x" * 100, "y"))
    assert len(w["summary"]) <= M.SUMMARY_CHARS + 1 and M.w_context(w)[0]["role"] == "system"
