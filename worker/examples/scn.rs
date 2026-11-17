// Parity helper: runs the same scripted scenario as the legacy JS and prints the JSON results.
use agent_04_memory::agent::*;
use agent_04_memory::memory::{self as M, Kv};
use futures::executor::block_on;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashMap};

#[derive(Default)]
struct Map(RefCell<HashMap<String, String>>);
impl Kv for Map {
    async fn get(&self, k: &str) -> Option<String> { self.0.borrow().get(k).cloned() }
    async fn put(&self, k: &str, v: &str, _t: Option<u64>) { self.0.borrow_mut().insert(k.into(), v.into()); }
    async fn delete(&self, k: &str) { self.0.borrow_mut().remove(k); }
}
const VOCAB: [&str; 12] = ["gym", "equinox", "planet", "paris", "rome", "trip", "dinner", "spicy", "denver", "lisbon", "dentist", "python"];
struct Fake;
impl Ai for Fake {
    async fn run(&self, _m: &str, inp: Value) -> Result<Value, String> {
        if let Some(t) = inp.get("text") {
            let f = |s: &str| { let l = s.to_lowercase(); let mut v: Vec<f64> = VOCAB.iter().map(|w| if l.contains(w) { 1.0 } else { 0.0 }).collect(); v.push(0.01); v };
            return Ok(json!({"data": t.as_array().unwrap().iter().map(|x| f(x.as_str().unwrap())).collect::<Vec<_>>()}));
        }
        let sys = inp["messages"][0]["content"].as_str().unwrap();
        let last = inp["messages"].as_array().unwrap().last().unwrap()["content"].as_str().unwrap().to_string();
        if sys.starts_with("Extract") {
            let u = inp["messages"][1]["content"].as_str().unwrap();
            return Ok(json!({"response": if u.contains("Denver") { r#"[{"key":"location","value":"Denver"},{"key":"diet","value":"spicy"}]"# } else if u.contains("Lisbon") { r#"[{"key":"location","value":"Lisbon"}]"# } else { "[]" }}));
        }
        Ok(json!({"response": format!("reply to: {last}")}))
    }
}
fn main() {
    let (kv, ai) = (Map::default(), Fake);
    let mut out: Vec<Value> = vec![];
    let mut t: i64 = 1000 * M::DAY;
    for (m, dt) in [("I live in Denver", 0), ("Plan my Rome trip", 1000), ("Ideas for dinner? spicy", 2000), ("Remind me in 2 hours to call the dentist", 3000), ("I moved to Lisbon", 4000), ("Another Rome trip please", 5000)] {
        t += dt;
        out.push(block_on(turn(&ai, &kv, "sess1", m, t)).unwrap());
    }
    block_on(remember(&ai, &kv, "sess1", "My gym is Planet", t)).unwrap();
    block_on(age(&kv, "sess1", 20.0, t));
    block_on(remember(&ai, &kv, "sess1", "My gym is Equinox", t)).unwrap();
    out.push(block_on(recall(&ai, &kv, "sess1", "which gym", 3, t)).unwrap());
    out.push(block_on(cron(&kv, t + 3 * 3_600_000)));
    block_on(age(&kv, "sess1", 100.0, t));
    out.push(block_on(recall(&ai, &kv, "sess1", "gym", 3, t)).unwrap());
    out.push(block_on(M::load(&kv, "sess1")));
    println!("{}", serde_json::to_string(&out).unwrap());
}
