use agent_04_memory::agent::*;
use agent_04_memory::memory::{self as M, Kv, DAY};
use futures::executor::block_on;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Default)]
struct Map(RefCell<HashMap<String, String>>);
impl Kv for Map {
    async fn get(&self, k: &str) -> Option<String> { self.0.borrow().get(k).cloned() }
    async fn put(&self, k: &str, v: &str, _t: Option<u64>) { self.0.borrow_mut().insert(k.into(), v.into()); }
    async fn delete(&self, k: &str) { self.0.borrow_mut().remove(k); }
}

const VOCAB: [&str; 12] = ["gym", "equinox", "planet", "paris", "rome", "trip", "dinner", "spicy", "denver", "lisbon", "dentist", "python"];
fn fake_vec(t: &str) -> Vec<f64> {
    let s = t.to_lowercase();
    let mut v: Vec<f64> = VOCAB.iter().map(|w| if s.contains(w) { 1.0 } else { 0.0 }).collect();
    v.push(0.01);
    v
}
struct Fake {
    extract: Box<dyn Fn(&str) -> String>,
    reply: Box<dyn Fn(&Value) -> String>,
    seen_len: RefCell<usize>,
}
fn fake() -> Fake {
    Fake { extract: Box::new(|_| "[]".into()), reply: Box::new(|_| "ok".into()), seen_len: RefCell::new(0) }
}
impl Ai for Fake {
    async fn run(&self, _m: &str, inp: Value) -> Result<Value, String> {
        if let Some(t) = inp.get("text") {
            return Ok(json!({"data": t.as_array().unwrap().iter().map(|x| fake_vec(x.as_str().unwrap())).collect::<Vec<_>>()}));
        }
        let sys = inp["messages"][0]["content"].as_str().unwrap();
        if sys.starts_with("Extract") {
            return Ok(json!({"response": (self.extract)(inp["messages"][1]["content"].as_str().unwrap())}));
        }
        *self.seen_len.borrow_mut() = inp["messages"].as_array().unwrap().len();
        Ok(json!({"response": (self.reply)(&inp)}))
    }
}
fn ids(v: &Value) -> Vec<String> { v.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect() }

#[test]
fn turn_writes_semantic_and_supersedes() {
    let kv = Map::default();
    let mut ai = fake();
    ai.extract = Box::new(|m| if m.contains("Denver") { r#"[{"key":"location","value":"Denver"}]"#.into() } else if m.contains("Lisbon") { r#"[{"key":"location","value":"Lisbon"}]"#.into() } else { "[]".into() });
    block_on(turn(&ai, &kv, "abcd", "I live in Denver", 1)).unwrap();
    let r = block_on(turn(&ai, &kv, "abcd", "I moved to Lisbon", 2)).unwrap();
    assert_eq!(r["modules"]["semantic"]["active"][0]["value"], "Lisbon");
    assert_eq!(r["modules"]["semantic"]["superseded"][0]["value"], "Denver");
}
#[test]
fn episodic_similar_task_recalls_outcome() {
    let kv = Map::default();
    let mut a = fake();
    a.reply = Box::new(|_| "Booked a 3-day Rome trip".into());
    block_on(turn(&a, &kv, "abcd", "Plan my Rome trip", 1)).unwrap();
    let mut b = fake();
    b.reply = Box::new(|inp| {
        let s = inp["messages"][0]["content"].as_str().unwrap();
        assert!(regex::Regex::new(r"Episodic memory[\s\S]*Rome trip[\s\S]*Booked a 3-day Rome trip").unwrap().is_match(s));
        "ok".into()
    });
    let r = block_on(turn(&b, &kv, "abcd", "Another Rome trip please", 2)).unwrap();
    assert_eq!(r["used"]["episodes"].as_array().unwrap().len(), 1);
}
#[test]
fn procedural_playbook_injected() {
    let mut a = fake();
    a.reply = Box::new(|inp| { assert!(inp["messages"][0]["content"].as_str().unwrap().contains("Playbook recommend-food")); "ok".into() });
    let r = block_on(turn(&a, &Map::default(), "abcd", "Ideas for dinner?", 1)).unwrap();
    assert_eq!(r["used"]["playbooks"], json!(["recommend-food"]));
}
#[test]
fn working_memory_compacted_after_turn_never_mid_turn() {
    let kv = Map::default();
    let ai = fake();
    for i in 0..5 { block_on(turn(&ai, &kv, "abcd", &format!("msg {i}"), i)).unwrap(); }
    assert_eq!(*ai.seen_len.borrow(), 1 + M::MAX_TURNS + 1);
    assert_eq!(block_on(M::load(&kv, "abcd"))["working"]["turns"].as_array().unwrap().len(), 4);
}
#[test]
fn reminder_fires_via_cron_once() {
    let kv = Map::default();
    let ai = fake();
    block_on(turn(&ai, &kv, "abcd", "Remind me in 1 minute to stretch", 0)).unwrap();
    assert_eq!(ids(&serde_json::from_str::<Value>(&block_on(kv.get(M::REM_INDEX)).unwrap()).unwrap()), ["abcd"]);
    assert_eq!(block_on(cron(&kv, 30_000))["fired"], 0);
    assert_eq!(block_on(cron(&kv, 61_000))["fired"], 1);
    let m = block_on(M::load(&kv, "abcd"));
    assert_eq!(m["inbox"][0]["text"], "stretch");
    assert_eq!(block_on(kv.get(M::REM_INDEX)).unwrap(), "[]");
}
#[test]
fn retrieval_recency_and_forgetting_after_aging() {
    let kv = Map::default();
    let ai = fake();
    let now = 1000 * DAY;
    block_on(remember(&ai, &kv, "abcd", "My gym is Planet", now)).unwrap();
    block_on(age(&kv, "abcd", 60.0, now));
    block_on(remember(&ai, &kv, "abcd", "My gym is Equinox", now)).unwrap();
    assert!(block_on(recall(&ai, &kv, "abcd", "which gym", 3, now)).unwrap()["results"][0]["text"].as_str().unwrap().contains("Equinox"));
    block_on(age(&kv, "abcd", 90.0, now));
    let r = block_on(recall(&ai, &kv, "abcd", "which gym", 3, now)).unwrap();
    assert_eq!(r["results"].as_array().unwrap().len(), 0);
    assert_eq!(r["forgotten"].as_array().unwrap().len(), 2);
}
#[test]
fn http_validation_and_memory_view() {
    let kv = Map::default();
    let ai = fake();
    let post = |p: &str, b: Value| block_on(handle(&ai, &kv, "POST", p, None, &b, 0)).0;
    assert_eq!(post("/api/chat", json!({"session":"abcd"})), 400);
    assert_eq!(post("/api/age", json!({"session":"abcd","days":0})), 400);
    assert_eq!(block_on(handle(&ai, &kv, "GET", "/api/memory", Some("no"), &json!({}), 0)).0, 400);
    let (s, v) = block_on(handle(&ai, &kv, "GET", "/api/memory", Some("abcd"), &json!({}), 0));
    assert_eq!(s, 200);
    let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["working", "semantic", "episodic", "procedural", "retrieval", "prospective"]);
}
#[test]
fn episodic_playbooks_reminders_modules() {
    let (eps, ep) = M::ep_log(&json!([]), "Plan Rome trip", "Made a 3-day plan", 1);
    assert_eq!(eps.as_array().unwrap().len(), 1);
    assert!(M::ep_describe(&ep).contains("Rome") && M::ep_describe(&ep).contains("3-day"));
    let mut e = json!([]);
    for i in 0..(M::MAX_EPISODES + 5) { e = M::ep_log(&e, &format!("t{i}"), "o", i as i64).0; }
    assert_eq!(e.as_array().unwrap().len(), M::MAX_EPISODES);
    let id0 = e[0]["id"].as_str().unwrap().to_string();
    assert_eq!(M::ep_by_ids(&e, &[id0, "nope".into()]).iter().map(|x| x["task"].as_str().unwrap().to_string()).collect::<Vec<_>>(), ["t5"]);
    assert_eq!(M::select("Plan a trip and travel itinerary")[0].name, "plan-a-trip");
    assert_eq!(M::select("where for dinner?")[0].name, "recommend-food");
    assert!(M::select("hello there").is_empty() && M::select("the debtor").is_empty());
    assert!(M::pb_render(&[&M::PLAYBOOKS[0]]).starts_with("Playbook plan-a-trip: Ask dates"));
    assert_eq!(M::parse_reminder("Remind me in 10 minutes to call Mom.", 0), Some(json!({"text":"call Mom","due_at":600000})));
    assert_eq!(M::parse_reminder("remind me after 2 days about the dentist", 0).unwrap()["due_at"], 2 * 86_400_000);
    assert_eq!(M::parse_reminder("hello", 0), None);
    let r = M::schedule(&json!([]), &json!({"text":"a","due_at":10}), 0);
    let r = M::schedule(&r, &json!({"text":"b","due_at":100}), 0);
    let (rem, fired) = M::fire_due(&r, 50);
    assert_eq!(fired.iter().map(|f| f["text"].clone()).collect::<Vec<_>>(), [json!("a")]);
    assert_eq!(M::pending(&rem)[0]["text"], "b");
    assert!(M::fire_due(&rem, 60).1.is_empty());
}
#[test]
fn retrieval_module() {
    let now = 1000 * DAY;
    assert_eq!(M::strength(&json!({"at": now}), now), 1.0);
    assert!((M::strength(&json!({"at": now - 14 * DAY}), now) - 0.5).abs() < 1e-9);
    let s = M::vadd(&json!([]), "old", "x", None, &[1.0, 0.0], now - 60 * DAY);
    let s = M::vadd(&s, "new", "x", None, &[1.0, 0.0], now);
    assert_eq!(M::vsearch(&s, &[1.0, 0.0], now, 2, &|_| true)[0].it["text"], "new");
    let s = M::vadd(&json!([]), "relevant-old", "x", None, &[1.0, 0.0], now - 30 * DAY);
    let s = M::vadd(&s, "irrelevant-new", "x", None, &[0.0, 1.0], now);
    assert_eq!(M::vsearch(&s, &[1.0, 0.0], now, 1, &|_| true)[0].it["text"], "relevant-old");
    let s = M::vadd(&json!([]), "ancient", "x", None, &[1.0], now - 90 * DAY);
    let s = M::vadd(&s, "fresh", "x", None, &[1.0], now);
    let (kept, gone) = M::forget(&s, now);
    assert_eq!(kept[0]["text"], "fresh");
    assert_eq!(gone, ["ancient"]);
    let t = M::touch(&M::vadd(&json!([]), "used", "x", None, &[1.0], now - 90 * DAY), &[format!("v{}-0", now - 90 * DAY)], now);
    assert_eq!(M::forget(&t, now).0.as_array().unwrap().len(), 1);
}
#[test]
fn semantic_module() {
    let f = M::upsert(&json!([]), &json!([{"key":"location","value":"Denver"}]), 1);
    let f = M::upsert(&f, &json!([{"key":"Location","value":"Lisbon"}]), 2);
    assert_eq!(f.as_array().unwrap().len(), 2);
    let h = M::history(&f, "location");
    assert_eq!(h[0]["superseded_at"], 2);
    assert_eq!(h[0]["superseded_by"], h[1]["id"]);
    assert_eq!(h[1]["supersedes"], h[0]["id"]);
    assert!(!M::sem_render(&f).contains("Denver"));
    let f = M::upsert(&json!([]), &json!([{"key":"diet","value":"vegetarian"}]), 1);
    let f = M::upsert(&f, &json!([{"key":"diet","value":"Vegetarian "}, null, {"key":3}, {"key":"x","value":""}]), 5);
    assert_eq!(f.as_array().unwrap().len(), 1);
    assert_eq!(f[0]["confirmed_at"], 5);
    let mut f = json!([]);
    for (i, v) in ["A", "B", "C"].iter().enumerate() { f = M::upsert(&f, &json!([{"key":"job","value":v}]), i as i64 + 1); }
    assert_eq!(M::active(&f)[0]["value"], "C");
    assert_eq!(f.as_array().unwrap().iter().filter(|x| x.get("superseded_at").is_some()).count(), 2);
}
#[test]
fn working_module() {
    let w0 = json!({"summary":"","turns":[]});
    let fill = |n: usize| { let mut w = w0.clone(); for i in 0..n { w = M::w_append(&w, &format!("u{i}"), &format!("a{i}")); } w };
    let w = fill(M::MAX_TURNS / 2);
    assert_eq!(M::w_compact(&w), w);
    let w = M::w_compact(&fill(5));
    assert_eq!(w["turns"].as_array().unwrap().len(), M::KEEP_TURNS);
    assert_eq!(w["turns"][3]["content"], "a4");
    assert!(w["summary"].as_str().unwrap().contains("user: u0"));
    let mut w = w0.clone();
    for _ in 0..200 { w = M::w_compact(&M::w_append(&w, &"x".repeat(100), "y")); }
    assert!(w["summary"].as_str().unwrap().chars().count() <= M::SUMMARY_CHARS + 1);
    assert_eq!(M::w_context(&w)[0]["role"], "system");
}
