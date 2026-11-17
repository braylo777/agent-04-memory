//! Seven memory modules over a single JSON record per session (serde_json::Value).
use regex::Regex;
use serde_json::{json, Map, Value};

pub const DAY: i64 = 86_400_000;
pub const HALF_LIFE_DAYS: f64 = 14.0;
pub const RECENCY_WEIGHT: f64 = 0.15;
pub const FORGET_BELOW: f64 = 0.05;
pub const MAX_ITEMS: usize = 60;
pub const MAX_TURNS: usize = 8;
pub const KEEP_TURNS: usize = 4;
pub const SUMMARY_CHARS: usize = 800;
pub const MAX_EPISODES: usize = 50;
pub const TTL: u64 = 60 * 60 * 24 * 30;
pub const REM_INDEX: &str = "rem-index";
pub const KEYS: [&str; 18] = ["name", "location", "hometown", "job", "employer", "diet", "allergy", "pet", "family", "favorite_color", "favorite_food", "favorite_music", "language", "hobby", "goal", "vehicle", "birthday", "other"];

pub fn extract_prompt() -> String {
    format!("Extract durable facts about the USER from the message. Reply ONLY with a JSON array of {{\"key\",\"value\"}} objects, or [] if none. key must be one of: {}. For \"other\" use key \"other:<short topic>\". value is a short phrase, e.g. [{{\"key\":\"name\",\"value\":\"Sam\"}},{{\"key\":\"location\",\"value\":\"Austin\"}}].", KEYS.join(", "))
}

/// Key-value store (Workers KV in the Worker, a map in tests).
#[allow(async_fn_in_trait)]
pub trait Kv {
    async fn get(&self, key: &str) -> Option<String>;
    async fn put(&self, key: &str, val: &str, ttl: Option<u64>);
    async fn delete(&self, key: &str);
}

pub fn key(sid: &str) -> String {
    format!("mem:{sid}")
}
pub fn empty() -> Value {
    json!({"working":{"summary":"","turns":[]},"facts":[],"episodes":[],"vectors":[],"reminders":[],"inbox":[]})
}
pub async fn load<K: Kv>(kv: &K, sid: &str) -> Value {
    let mut m = empty();
    if let Some(raw) = kv.get(&key(sid)).await {
        if let Ok(Value::Object(o)) = serde_json::from_str::<Value>(&raw) {
            for (k, v) in o {
                m[k] = v;
            }
        }
    }
    m
}
pub async fn save<K: Kv>(kv: &K, sid: &str, m: &Value) {
    kv.put(&key(sid), &m.to_string(), Some(TTL)).await;
}

fn arr(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}
fn st(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}
fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn take(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

// ---- working ----
pub fn w_context(w: &Value) -> Vec<Value> {
    let mut out = vec![];
    let s = st(w, "summary");
    if !s.is_empty() {
        out.push(json!({"role":"system","content":format!("Summary of earlier conversation: {s}")}));
    }
    out.extend(arr(&w["turns"]));
    out
}
pub fn w_append(w: &Value, user: &str, assistant: &str) -> Value {
    let mut turns = arr(&w["turns"]);
    turns.push(json!({"role":"user","content":user}));
    turns.push(json!({"role":"assistant","content":assistant}));
    json!({"summary": w["summary"], "turns": turns})
}
pub fn w_compact(w: &Value) -> Value {
    let turns = arr(&w["turns"]);
    if turns.len() <= MAX_TURNS {
        return w.clone();
    }
    let old = &turns[..turns.len() - KEEP_TURNS];
    let digest: Vec<String> = old.iter().map(|t| format!("{}: {}", st(t, "role"), take(&collapse(&st(t, "content")), 80))).collect();
    let prev = st(w, "summary");
    let mut summary = format!("{}{}", if prev.is_empty() { String::new() } else { format!("{prev} | ") }, digest.join(" | "));
    let n = summary.chars().count();
    if n > SUMMARY_CHARS {
        summary = format!("…{}", summary.chars().skip(n - SUMMARY_CHARS).collect::<String>());
    }
    json!({"summary": summary, "turns": turns[turns.len() - KEEP_TURNS..]})
}

// ---- semantic ----
fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}
pub fn upsert(facts: &Value, items: &Value, now: i64) -> Value {
    let mut out = arr(facts);
    for it in items.as_array().cloned().unwrap_or_default() {
        let (Some(k), Some(v)) = (it.get("key").and_then(|x| x.as_str()), it.get("value").and_then(|x| x.as_str())) else { continue };
        if v.trim().is_empty() {
            continue;
        }
        let (k, v) = (norm(k), v.trim().to_string());
        let cur = out.iter().position(|f| st(f, "key") == k && f.get("superseded_at").map_or(true, |x| x.is_null()));
        if let Some(i) = cur {
            if norm(&st(&out[i], "value")) == norm(&v) {
                out[i]["confirmed_at"] = json!(now);
                continue;
            }
        }
        let id = format!("f{}", out.len() + 1);
        let mut f = Map::new();
        f.insert("id".into(), json!(id));
        f.insert("key".into(), json!(k));
        f.insert("value".into(), json!(v));
        f.insert("created_at".into(), json!(now));
        if let Some(i) = cur {
            f.insert("supersedes".into(), out[i]["id"].clone());
            out[i]["superseded_at"] = json!(now);
            out[i]["superseded_by"] = json!(id);
        }
        out.push(Value::Object(f));
    }
    Value::Array(out)
}
fn is_active(f: &Value) -> bool {
    f.get("superseded_at").map_or(true, |x| x.is_null() || x.as_i64() == Some(0))
}
pub fn active(facts: &Value) -> Vec<Value> {
    arr(facts).into_iter().filter(is_active).collect()
}
pub fn history(facts: &Value, k: &str) -> Vec<Value> {
    arr(facts).into_iter().filter(|f| st(f, "key") == norm(k)).collect()
}
pub fn sem_render(facts: &Value) -> String {
    let l: Vec<String> = active(facts).iter().map(|f| format!("- {}: {}", st(f, "key"), st(f, "value"))).collect();
    if l.is_empty() { "(none yet)".into() } else { l.join("\n") }
}

// ---- episodic ----
pub fn ep_log(episodes: &Value, task: &str, outcome: &str, now: i64) -> (Value, Value) {
    let mut e = arr(episodes);
    let ep = json!({"id": format!("e{}-{}", now, e.len()), "task": take(task, 300), "outcome": take(&collapse(outcome), 300), "at": now});
    e.push(ep.clone());
    let start = e.len().saturating_sub(MAX_EPISODES);
    (Value::Array(e[start..].to_vec()), ep)
}
pub fn ep_describe(ep: &Value) -> String {
    format!("Past task: {} -> outcome: {}", st(ep, "task"), st(ep, "outcome"))
}
pub fn ep_by_ids(episodes: &Value, ids: &[String]) -> Vec<Value> {
    let e = arr(episodes);
    ids.iter().filter_map(|id| e.iter().find(|x| st(x, "id") == *id).cloned()).collect()
}
fn iso_date(ms: i64) -> String {
    let days = ms.div_euclid(DAY);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}
pub fn iso_ms(ms: i64) -> String {
    let rem = ms.rem_euclid(DAY);
    format!("{}T{:02}:{:02}:{:02}.{:03}Z", iso_date(ms), rem / 3_600_000, rem % 3_600_000 / 60_000, rem % 60_000 / 1000, rem % 1000)
}
pub fn ep_render(eps: &[Value]) -> String {
    eps.iter().map(|e| format!("- {}: \"{}\" -> {}", iso_date(e["at"].as_i64().unwrap_or(0)), st(e, "task"), st(e, "outcome"))).collect::<Vec<_>>().join("\n")
}

// ---- procedural ----
pub struct Playbook {
    pub name: &'static str,
    pub triggers: &'static [&'static str],
    pub steps: &'static [&'static str],
}
pub const PLAYBOOKS: [Playbook; 5] = [
    Playbook { name: "plan-a-trip", triggers: &["trip", "travel", "vacation", "itinerary"], steps: &["Ask dates and budget if missing", "Use the user's known location as origin", "Respect diet and allergies for food ideas", "Give a day-by-day outline"] },
    Playbook { name: "recommend-food", triggers: &["dinner", "lunch", "restaurant", "eat", "recipe", "cook"], steps: &["Check diet and allergy facts first", "Prefer the user's location", "Offer two or three options with one-line reasons"] },
    Playbook { name: "write-email", triggers: &["email", "message to", "draft"], steps: &["Use the user's name and role in the sign-off", "Keep it under 120 words", "End with one clear ask"] },
    Playbook { name: "study-plan", triggers: &["study", "exam", "learn", "certification"], steps: &["Use the user's stated goal", "Break into weekly milestones", "Suggest one practice method per week"] },
    Playbook { name: "debug-code", triggers: &["bug", "error", "stack trace", "exception", "debug"], steps: &["Ask for the exact error and language if missing", "Reproduce, isolate, then fix", "Explain the root cause in one line"] },
];
pub fn select(message: &str) -> Vec<&'static Playbook> {
    let m = message.to_lowercase();
    let mut v: Vec<(&Playbook, usize)> = PLAYBOOKS
        .iter()
        .map(|b| (b, b.triggers.iter().filter(|t| Regex::new(&format!(r"(?i)\b{}", regex::escape(t))).unwrap().is_match(&m)).count()))
        .filter(|x| x.1 > 0)
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1)); // stable
    v.into_iter().map(|x| x.0).collect()
}
pub fn pb_render(books: &[&Playbook]) -> String {
    books.iter().map(|b| format!("Playbook {}: {}", b.name, b.steps.join("; "))).collect::<Vec<_>>().join("\n")
}

// ---- prospective ----
pub fn parse_reminder(message: &str, now: i64) -> Option<Value> {
    let re = Regex::new(r"(?i)remind me (?:in|after) (\d+)\s*(seconds?|secs?|minutes?|mins?|hours?|days?)\s*(?:to |that |about )?(.+)").unwrap();
    let c = re.captures(message)?;
    let u = c[2].to_lowercase();
    let unit: i64 = match u.trim_end_matches('s') {
        "second" | "sec" => 1000,
        "minute" | "min" => 60_000,
        "hour" => 3_600_000,
        "day" => 86_400_000,
        _ => return None,
    };
    let text = c[3].trim_end_matches(|ch| ch == '.' || ch == '!').trim().to_string();
    Some(json!({"text": text, "due_at": now + c[1].parse::<i64>().ok()? * unit}))
}
pub fn schedule(reminders: &Value, r: &Value, now: i64) -> Value {
    let mut v = arr(reminders);
    let mut o = Map::new();
    o.insert("id".into(), json!(format!("r{}-{}", now, v.len())));
    for (k, x) in r.as_object().unwrap() {
        o.insert(k.clone(), x.clone());
    }
    o.insert("created_at".into(), json!(now));
    o.insert("fired_at".into(), Value::Null);
    v.push(Value::Object(o));
    Value::Array(v)
}
fn unfired(r: &Value) -> bool {
    r.get("fired_at").map_or(true, |x| x.is_null() || x.as_i64() == Some(0))
}
pub fn fire_due(reminders: &Value, now: i64) -> (Value, Vec<Value>) {
    let (mut out, mut fired) = (vec![], vec![]);
    for r in arr(reminders) {
        if unfired(&r) && r["due_at"].as_i64().unwrap_or(i64::MAX) <= now {
            let mut f = r.clone();
            f["fired_at"] = json!(now);
            fired.push(f.clone());
            out.push(f);
        } else {
            out.push(r);
        }
    }
    (Value::Array(out), fired)
}
pub fn pending(reminders: &Value) -> Vec<Value> {
    arr(reminders).into_iter().filter(unfired).collect()
}

// ---- retrieval ----
pub fn cosine(a: &[f64], b: &[f64]) -> f64 {
    let (mut d, mut x, mut y) = (0.0, 0.0, 0.0);
    for (p, q) in a.iter().zip(b) {
        d += p * q;
        x += p * p;
        y += q * q;
    }
    let den = x.sqrt() * y.sqrt();
    d / if den == 0.0 { 1.0 } else { den }
}
pub fn strength(it: &Value, now: i64) -> f64 {
    let t = it.get("used_at").filter(|v| !v.is_null()).or_else(|| it.get("at")).and_then(|v| v.as_i64()).unwrap_or(0);
    0.5f64.powf((now - t) as f64 / (HALF_LIFE_DAYS * DAY as f64))
}
pub fn vadd(items: &Value, text: &str, kind: &str, rf: Option<&str>, vec: &[f64], now: i64) -> Value {
    let mut v = arr(items);
    let rounded: Vec<f64> = vec.iter().map(|x| (x * 1e4).round() / 1e4).collect();
    v.push(json!({"id": format!("v{}-{}", now, v.len()), "text": text, "kind": kind, "ref": rf, "vec": rounded, "at": now}));
    let start = v.len().saturating_sub(MAX_ITEMS);
    Value::Array(v[start..].to_vec())
}
pub struct Hit {
    pub it: Value,
    pub sim: f64,
    pub recency: f64,
    pub score: f64,
}
pub fn vsearch(items: &Value, qv: &[f64], now: i64, k: usize, flt: &dyn Fn(&Value) -> bool) -> Vec<Hit> {
    let mut hits: Vec<Hit> = arr(items)
        .into_iter()
        .filter(|it| flt(it))
        .map(|it| {
            let v: Vec<f64> = arr(&it["vec"]).iter().map(|x| x.as_f64().unwrap_or(0.0)).collect();
            let sim = cosine(qv, &v);
            let rec = strength(&it, now);
            Hit { it, sim, recency: rec, score: sim + RECENCY_WEIGHT * rec }
        })
        .collect();
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(k);
    hits
}
pub fn touch(items: &Value, ids: &[String], now: i64) -> Value {
    Value::Array(arr(items).into_iter().map(|mut it| { if ids.contains(&st(&it, "id")) { it["used_at"] = json!(now); } it }).collect())
}
pub fn forget(items: &Value, now: i64) -> (Value, Vec<String>) {
    let (mut kept, mut gone) = (vec![], vec![]);
    for it in arr(items) {
        if strength(&it, now) >= FORGET_BELOW { kept.push(it) } else { gone.push(st(&it, "text")) }
    }
    (Value::Array(kept), gone)
}
