import json, re
from agent04 import agent as A
from agent04 import memory as M
from helpers import KV, fake_ai

DAY = 864e5


def test_turn_writes_semantic_and_supersedes():
    kv = KV()
    ai = fake_ai(lambda m: '[{"key":"location","value":"Denver"}]' if "Denver" in m else '[{"key":"location","value":"Lisbon"}]' if "Lisbon" in m else "[]")
    A.turn(ai, kv, "abcd", "I live in Denver", 1)
    r = A.turn(ai, kv, "abcd", "I moved to Lisbon", 2)
    assert [f["value"] for f in r["modules"]["semantic"]["active"]] == ["Lisbon"]
    assert [f["value"] for f in r["modules"]["semantic"]["superseded"]] == ["Denver"]


def test_episodic_similar_task_recalls_outcome():
    kv = KV()
    A.turn(fake_ai(reply=lambda i: "Booked a 3-day Rome trip"), kv, "abcd", "Plan my Rome trip", 1)
    def reply(inp):
        assert re.search(r"Episodic memory[\s\S]*Rome trip[\s\S]*Booked a 3-day Rome trip", inp["messages"][0]["content"])
        return "ok"
    r = A.turn(fake_ai(reply=reply), kv, "abcd", "Another Rome trip please", 2)
    assert len(r["used"]["episodes"]) == 1


def test_procedural_playbook_injected():
    def reply(inp):
        assert "Playbook recommend-food" in inp["messages"][0]["content"]
        return "ok"
    assert A.turn(fake_ai(reply=reply), KV(), "abcd", "Ideas for dinner?", 1)["used"]["playbooks"] == ["recommend-food"]


def test_working_memory_compacted_after_turn_never_mid_turn():
    seen = []
    kv = KV()
    ai = fake_ai(reply=lambda inp: (seen.append(len(inp["messages"])), "ok")[1])
    for i in range(5):
        A.turn(ai, kv, "abcd", f"msg {i}", i)
    assert seen[-1] == 1 + M.MAX_TURNS + 1
    assert len(M.load(kv, "abcd")["working"]["turns"]) == 4


def test_prospective_reminder_fires_via_cron_once():
    kv, ai = KV(), fake_ai()
    A.turn(ai, kv, "abcd", "Remind me in 1 minute to stretch", 0)
    assert json.loads(kv.get(M.REM_INDEX)) == ["abcd"]
    assert A.cron(kv, 30e3)["fired"] == 0 and A.cron(kv, 61e3)["fired"] == 1
    m = M.load(kv, "abcd")
    assert m["inbox"][0]["text"] == "stretch" and json.loads(kv.get(M.REM_INDEX)) == []


def test_retrieval_recency_and_forgetting_after_aging():
    kv, ai, now = KV(), fake_ai(), 1000 * 864000 * 100
    now = int(1000 * DAY)
    A.remember(ai, kv, "abcd", "My gym is Planet", now)
    A.age(kv, "abcd", 60, now)
    A.remember(ai, kv, "abcd", "My gym is Equinox", now)
    assert "Equinox" in A.recall(ai, kv, "abcd", "which gym", 3, now)["results"][0]["text"]
    A.age(kv, "abcd", 90, now)
    r = A.recall(ai, kv, "abcd", "which gym", 3, now)
    assert len(r["results"]) == 0 and len(r["forgotten"]) == 2


def test_http_validation_and_memory_view():
    kv = KV()
    post = lambda p, b: A.handle(None, kv, "POST", p, body=b)[0]
    assert post("/api/chat", {"session": "abcd"}) == 400
    assert post("/api/age", {"session": "abcd", "days": 0}) == 400
    assert A.handle(None, kv, "GET", "/api/memory", "no")[0] == 400
    s, v = A.handle(None, kv, "GET", "/api/memory", "abcd")
    assert s == 200 and list(v) == ["working", "semantic", "episodic", "procedural", "retrieval", "prospective"]
