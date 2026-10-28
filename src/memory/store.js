// One KV record per session holds every memory type. Short TTL keeps the free tier clean.
export const TTL = 60 * 60 * 24 * 30;
export const key = (sid) => `mem:${sid}`;
export const REM_INDEX = "rem-index"; // sessions with pending reminders, read by the cron trigger
export const empty = () => ({ working: { summary: "", turns: [] }, facts: [], episodes: [], vectors: [], reminders: [], inbox: [] });
export async function load(kv, sid) { return { ...empty(), ...((await kv.get(key(sid), "json")) || {}) }; }
export async function save(kv, sid, m) { await kv.put(key(sid), JSON.stringify(m), { expirationTtl: TTL }); }
