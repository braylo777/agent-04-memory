// Retrieval memory: a small vector store (Workers AI BGE embeddings + cosine in KV) with recency-aware scoring and decay-based forgetting.
export const DAY = 86400000, HALF_LIFE_DAYS = 14, RECENCY_WEIGHT = 0.15, FORGET_BELOW = 0.05, MAX_ITEMS = 60;
export function cosine(a, b) { let d = 0, x = 0, y = 0; for (let i = 0; i < a.length; i++) { d += a[i] * b[i]; x += a[i] * a[i]; y += b[i] * b[i]; } return d / (Math.sqrt(x) * Math.sqrt(y) || 1); }
// Exponential decay since last use: 1.0 when fresh, 0.5 after one half-life. Accessing an item refreshes it.
export const strength = (it, now) => Math.pow(0.5, (now - (it.used_at ?? it.at)) / (HALF_LIFE_DAYS * DAY));
export const score = (sim, it, now) => sim + RECENCY_WEIGHT * strength(it, now);
export function add(items, { text, kind, ref, vec, now }) {
  const v = vec.map((x) => +x.toFixed(4));
  return [...items, { id: `v${now}-${items.length}`, text, kind, ref, vec: v, at: now }].slice(-MAX_ITEMS);
}
export function search(items, qv, now, k = 3, filter = () => true) {
  return items.filter(filter).map((it) => { const sim = cosine(qv, it.vec); return { it, sim, recency: strength(it, now), score: score(sim, it, now) }; })
    .sort((a, b) => b.score - a.score).slice(0, k);
}
export const touch = (items, ids, now) => items.map((it) => (ids.includes(it.id) ? { ...it, used_at: now } : it));
// Forgetting: drop items whose strength has decayed below the threshold.
export function forget(items, now) { const kept = items.filter((it) => strength(it, now) >= FORGET_BELOW); return { items: kept, forgotten: items.filter((it) => !kept.includes(it)).map((it) => it.text) }; }
export async function embed(env, texts) { return (await env.AI.run("@cf/baai/bge-base-en-v1.5", { text: texts })).data; }
