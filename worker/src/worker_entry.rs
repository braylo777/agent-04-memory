//! Cloudflare Worker entry (wasm32 only). Same routes and JSON as the old src/index.js, plus the cron trigger.
use crate::agent::{cron, handle, needs_limit, Ai};
use crate::memory::Kv;
use crate::page::PAGE;
use serde_json::{json, Value};
use wasm_bindgen::JsCast;
use worker::*;

struct CfAi(worker::Ai);
impl Ai for CfAi {
    async fn run(&self, model: &str, input: Value) -> std::result::Result<Value, String> {
        self.0.run::<Value, Value>(model, input).await.map_err(|e| e.to_string())
    }
}

struct CfKv(kv::KvStore);
impl Kv for CfKv {
    async fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).text().await.ok().flatten()
    }
    async fn put(&self, key: &str, val: &str, ttl: Option<u64>) {
        if let Ok(mut b) = self.0.put(key, val) {
            if let Some(t) = ttl {
                b = b.expiration_ttl(t);
            }
            let _ = b.execute().await;
        }
    }
    async fn delete(&self, key: &str) {
        let _ = self.0.delete(key).await;
    }
}

async fn rate_ok(env: &Env, ip: &str) -> bool {
    let rl = match js_sys::Reflect::get(env.as_ref(), &"RATE_LIMITER".into()) {
        Ok(v) if !v.is_undefined() && !v.is_null() => v,
        _ => return true,
    };
    let opts = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&opts, &"key".into(), &ip.into());
    let Ok(f) = js_sys::Reflect::get(&rl, &"limit".into()).and_then(|f| f.dyn_into::<js_sys::Function>().map_err(|e| e)) else { return true };
    let Ok(p) = f.call1(&rl, &opts) else { return true };
    match wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(p)).await {
        Ok(r) => js_sys::Reflect::get(&r, &"success".into()).ok().and_then(|v| v.as_bool()).unwrap_or(true),
        Err(_) => true,
    }
}

#[event(fetch)]
async fn fetch(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    let (path, method) = (url.path().to_string(), req.method());
    let session = url.query_pairs().find(|(k, _)| k == "session").map(|(_, v)| v.to_string());
    let body: Value = if method == Method::Post { req.json().await.unwrap_or(json!({})) } else { json!({}) };
    let kv = CfKv(env.kv("KV")?);
    let ai = CfAi(env.ai("AI")?);
    if method == Method::Post && needs_limit(&path, &body) {
        let ip = req.headers().get("cf-connecting-ip")?.unwrap_or_else(|| "anon".into());
        if !rate_ok(&env, &ip).await {
            return Ok(Response::from_json(&json!({"error":"Rate limit: try again in a minute."}))?.with_status(429));
        }
    }
    let (status, v) = handle(&ai, &kv, method.as_ref(), &path, session.as_deref(), &body, js_sys::Date::now() as i64).await;
    if status == 0 {
        let h = Headers::new();
        h.set("content-type", "text/html;charset=utf-8")?;
        return Ok(Response::ok(PAGE)?.with_headers(h));
    }
    Ok(Response::from_json(&v)?.with_status(status))
}

#[event(scheduled)]
async fn scheduled(_e: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    if let Ok(k) = env.kv("KV") {
        cron(&CfKv(k), js_sys::Date::now() as i64).await;
    }
}
