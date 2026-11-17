//! Agent loop over the memory modules, plus a pure router the Worker adapts HTTP to.
use crate::memory::{self as M, Kv};
use regex::Regex;
use serde_json::{json, Value};

pub const MODEL: &str = "@cf/meta/llama-3.3-70b-instruct-fp8-fast";
pub const EMBED: &str = "@cf/baai/bge-base-en-v1.5";

#[allow(async_fn_in_trait)]
pub trait Ai {
    async fn run(&self, model: &str, input: Value) -> Result<Value, String>;
}

pub fn text(r: &Value) -> String {
    match r.get("response") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => r.to_string(),
        Some(v) => v.to_string(),
    }
}
pub fn parse_json(s: &str) -> Option<Value> {
    Regex::new(r"[\[{][\s\S]*[\]}]").unwrap().find(s).and_then(|m| serde_json::from_str(m.as_str()).ok())
}
pub fn cap(s: Option<&str>, n: usize) -> bool {
    matches!(s, Some(s) if !s.trim().is_empty() && s.chars().count() <= n)
}
async fn chat<A: Ai>(ai: &A, messages: Vec<Value>, max: u32) -> Result<String, String> {
    Ok(text(&ai.run(MODEL, json!({"messages": messages, "max_tokens": max})).await?))
}
async fn ask<A: Ai>(ai: &A, system: &str, user: &str, max: u32) -> Result<String, String> {
    chat(ai, vec![json!({"role":"system","content":system}), json!({"role":"user","content":user})], max).await
}
async fn embed<A: Ai>(ai: &A, t: &str) -> Result<Vec<f64>, String> {
    let r = ai.run(EMBED, json!({"text":[t]})).await?;
    Ok(r["data"][0].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default())
}
fn arr(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}
fn st(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

pub fn deliver(m: &Value, now: i64) -> (Value, Vec<Value>) {
    let (reminders, fired) = M::fire_due(&m["reminders"], now);
    let mut inbox = arr(&m["inbox"]);
    inbox.extend(fired.iter().map(|r| json!({"text": r["text"], "due_at": r["due_at"], "fired_at": r["fired_at"]})));
    let start = inbox.len().saturating_sub(20);
    let mut out = m.clone();
    out["reminders"] = reminders;
    out["inbox"] = Value::Array(inbox[start..].to_vec());
    (out, fired)
}

pub async fn track_reminder_session<K: Kv>(kv: &K, sid: &str, on: bool) {
    let idx: Vec<String> = kv.get(M::REM_INDEX).await.and_then(|r| serde_json::from_str(&r).ok()).unwrap_or_default();
    let next: Vec<String> = if on {
        let mut n = idx.clone();
        if !n.contains(&sid.to_string()) { n.push(sid.to_string()); }
        n
    } else {
        idx.iter().filter(|s| *s != sid).cloned().collect()
    };
    if next.len() != idx.len() {
        kv.put(M::REM_INDEX, &json!(next).to_string(), None).await;
    }
}

fn r3(x: f64) -> f64 { (x * 1e3).round() / 1e3 }
fn r4(x: f64) -> f64 { (x * 1e4).round() / 1e4 }

pub fn view(m: &Value, now: i64) -> Value {
    let facts = &m["facts"];
    let eps = arr(&m["episodes"]);
    let vecs = arr(&m["vectors"]);
    let inbox = arr(&m["inbox"]);
    let last = |v: &Vec<Value>, n: usize| -> Vec<Value> { v[v.len().saturating_sub(n)..].iter().rev().cloned().collect() };
    json!({
        "working": {"summary": m["working"]["summary"], "turns": arr(&m["working"]["turns"]).len()},
        "semantic": {"active": M::active(facts), "superseded": arr(facts).into_iter().filter(|f| f.get("superseded_at").map_or(false, |x| !x.is_null())).collect::<Vec<_>>()},
        "episodic": last(&eps, 5).iter().map(|e| json!({"id": e["id"], "task": e["task"], "outcome": e["outcome"], "at": e["at"]})).collect::<Vec<_>>(),
        "procedural": M::PLAYBOOKS.iter().map(|b| b.name).collect::<Vec<_>>(),
        "retrieval": last(&vecs, 8).iter().map(|it| json!({"kind": it["kind"], "text": it["text"], "strength": r3(M::strength(it, now))})).collect::<Vec<_>>(),
        "prospective": {"pending": M::pending(&m["reminders"]), "inbox": last(&inbox, 5)},
    })
}

pub async fn turn<A: Ai, K: Kv>(ai: &A, kv: &K, sid: &str, message: &str, now: i64) -> Result<Value, String> {
    let (mut m, _) = deliver(&M::load(kv, sid).await, now);
    m["vectors"] = M::forget(&m["vectors"], now).0;
    let qv = embed(ai, message).await?;
    let hits = M::vsearch(&m["vectors"], &qv, now, 3, &|_| true);
    m["vectors"] = M::touch(&m["vectors"], &hits.iter().map(|h| st(&h.it, "id")).collect::<Vec<_>>(), now);
    let ep_ids: Vec<String> = hits.iter().filter(|h| st(&h.it, "kind") == "episode" && h.sim > 0.6).map(|h| st(&h.it, "ref")).collect();
    let past = M::ep_by_ids(&m["episodes"], &ep_ids);
    let notes: Vec<String> = hits.iter().filter(|h| st(&h.it, "kind") == "note" && h.sim > 0.6).map(|h| st(&h.it, "text")).collect();
    let books = M::select(message);
    let reminder = M::parse_reminder(message, now);
    let inbox = arr(&m["inbox"]);
    let mut parts: Vec<String> = vec![
        "You are a friendly assistant with several kinds of memory. Use them naturally; never invent facts.".into(),
        format!("Semantic memory (current facts about the user):\n{}", M::sem_render(&m["facts"])),
    ];
    if !past.is_empty() { parts.push(format!("Episodic memory (relevant past tasks and outcomes):\n{}", M::ep_render(&past))); }
    if !notes.is_empty() { parts.push(format!("Retrieved notes:\n{}", notes.iter().map(|n| format!("- {n}")).collect::<Vec<_>>().join("\n"))); }
    if !books.is_empty() { parts.push(format!("Procedural memory (follow these playbooks):\n{}", M::pb_render(&books))); }
    if let Some(r) = &reminder { parts.push(format!("A reminder was just scheduled for {}: \"{}\". Confirm it briefly.", M::iso_ms(r["due_at"].as_i64().unwrap()), st(r, "text"))); }
    if !inbox.is_empty() { parts.push(format!("Reminders that came due: {}", inbox[inbox.len().saturating_sub(3)..].iter().map(|r| st(r, "text")).collect::<Vec<_>>().join("; "))); }
    let mut msgs = vec![json!({"role":"system","content":parts.join("\n\n")})];
    msgs.extend(M::w_context(&m["working"]));
    msgs.push(json!({"role":"user","content":message}));
    let reply = chat(ai, msgs, 400).await?;
    let extracted = parse_json(&ask(ai, &M::extract_prompt(), message, 200).await?).unwrap_or(json!([]));
    let extracted = if extracted.is_array() { extracted } else { json!([]) };
    m["facts"] = M::upsert(&m["facts"], &extracted, now);
    let (eps, episode) = M::ep_log(&m["episodes"], message, &reply, now);
    m["episodes"] = eps;
    let ev = embed(ai, &M::ep_describe(&episode)).await?;
    m["vectors"] = M::vadd(&m["vectors"], &M::ep_describe(&episode), "episode", Some(&st(&episode, "id")), &ev, now);
    if let Some(r) = &reminder {
        m["reminders"] = M::schedule(&m["reminders"], r, now);
        track_reminder_session(kv, sid, true).await;
    }
    m["working"] = M::w_compact(&M::w_append(&m["working"], message, &reply));
    M::save(kv, sid, &m).await;
    Ok(json!({"reply": reply, "modules": view(&m, now), "used": {"episodes": past.iter().map(|e| e["id"].clone()).collect::<Vec<_>>(), "playbooks": books.iter().map(|b| b.name).collect::<Vec<_>>(), "notes": notes, "reminder_scheduled": reminder.is_some()}}))
}

pub async fn remember<A: Ai, K: Kv>(ai: &A, kv: &K, sid: &str, note: &str, now: i64) -> Result<Value, String> {
    let mut m = M::load(kv, sid).await;
    let v = embed(ai, note).await?;
    m["vectors"] = M::vadd(&m["vectors"], note, "note", None, &v, now);
    M::save(kv, sid, &m).await;
    Ok(json!({"stored": true, "items": arr(&m["vectors"]).len()}))
}

pub async fn recall<A: Ai, K: Kv>(ai: &A, kv: &K, sid: &str, query: &str, k: usize, now: i64) -> Result<Value, String> {
    let mut m = M::load(kv, sid).await;
    let (items, forgotten) = M::forget(&m["vectors"], now);
    let qv = embed(ai, query).await?;
    let hits = M::vsearch(&items, &qv, now, k, &|it| st(it, "kind") == "note");
    m["vectors"] = M::touch(&items, &hits.iter().map(|h| st(&h.it, "id")).collect::<Vec<_>>(), now);
    M::save(kv, sid, &m).await;
    Ok(json!({"results": hits.iter().map(|h| json!({"text": h.it["text"], "similarity": r4(h.sim), "recency": r4(h.recency), "score": r4(h.score)})).collect::<Vec<_>>(), "forgotten": forgotten}))
}

/// Demo/eval helper: shift every timestamp in ONE session back by `days`.
pub async fn age<K: Kv>(kv: &K, sid: &str, days: f64, _now: i64) -> Value {
    let mut m = M::load(kv, sid).await;
    let d = (days * M::DAY as f64) as i64;
    for list in ["vectors", "episodes"] {
        if let Some(a) = m[list].as_array_mut() {
            for it in a {
                for k in ["at", "used_at"] {
                    if let Some(t) = it.get(k).and_then(|x| x.as_i64()) { it[k] = json!(t - d); }
                }
            }
        }
    }
    M::save(kv, sid, &m).await;
    json!({"aged_days": days})
}

pub async fn cron<K: Kv>(kv: &K, now: i64) -> Value {
    let idx: Vec<String> = kv.get(M::REM_INDEX).await.and_then(|r| serde_json::from_str(&r).ok()).unwrap_or_default();
    let mut fired = 0;
    for sid in &idx {
        let (m, f) = deliver(&M::load(kv, sid).await, now);
        if !f.is_empty() {
            fired += f.len();
            M::save(kv, sid, &m).await;
        }
        if M::pending(&m["reminders"]).is_empty() {
            track_reminder_session(kv, sid, false).await;
        }
    }
    json!({"sessions": idx.len(), "fired": fired})
}

fn valid_sid(s: Option<&str>) -> bool {
    matches!(s, Some(s) if Regex::new(r"^[a-zA-Z0-9]{4,32}$").unwrap().is_match(s))
}

/// Pure router: (status, json). status 0 = not an API call (serve the page).
pub async fn handle<A: Ai, K: Kv>(ai: &A, kv: &K, method: &str, path: &str, query_session: Option<&str>, body: &Value, now: i64) -> (u16, Value) {
    let err = |s: u16, m: &str| (s, json!({"error": m}));
    if method == "GET" && path == "/api/memory" {
        if !valid_sid(query_session) { return err(400, "session required"); }
        let sid = query_session.unwrap();
        let (m, fired) = deliver(&M::load(kv, sid).await, now);
        if !fired.is_empty() { M::save(kv, sid, &m).await; }
        return (200, view(&m, now));
    }
    if method != "POST" || !path.starts_with("/api/") { return (0, Value::Null); }
    if !valid_sid(body.get("session").and_then(|v| v.as_str())) { return err(400, "session required"); }
    let sid = body["session"].as_str().unwrap();
    let res = |r: Result<Value, String>| match r { Ok(v) => (200, v), Err(e) => (502, json!({"error": format!("Agent error: {e}")})) };
    match path {
        "/api/forget" => { kv.delete(&M::key(sid)).await; (200, json!({"forgotten": true})) }
        "/api/age" => {
            let d = body.get("days").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
            if !(d > 0.0 && d <= 365.0) { return err(400, "days 1-365"); }
            (200, age(kv, sid, d, now).await)
        }
        "/api/chat" => { let m = body.get("message").and_then(|v| v.as_str()); if !cap(m, 500) { return err(400, "message (<=500 chars) required"); } res(turn(ai, kv, sid, m.unwrap(), now).await) }
        "/api/remember" => { let n = body.get("note").and_then(|v| v.as_str()); if !cap(n, 300) { return err(400, "note (<=300 chars) required"); } res(remember(ai, kv, sid, n.unwrap(), now).await) }
        "/api/recall" => { let q = body.get("query").and_then(|v| v.as_str()); if !cap(q, 300) { return err(400, "query (<=300 chars) required"); } res(recall(ai, kv, sid, q.unwrap(), 3, now).await) }
        _ => err(404, "not found"),
    }
}

/// True when the request passed validation and will call the model (the old Worker rate-limited only those).
pub fn needs_limit(path: &str, body: &Value) -> bool {
    if !valid_sid(body.get("session").and_then(|v| v.as_str())) { return false; }
    let f = |k: &str, n: usize| cap(body.get(k).and_then(|v| v.as_str()), n);
    match path { "/api/chat" => f("message", 500), "/api/remember" => f("note", 300), "/api/recall" => f("query", 300), _ => false }
}
