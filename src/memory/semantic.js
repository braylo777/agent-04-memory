// Semantic memory: facts about the user as key/value slots. A changed value supersedes the old fact; nothing is deleted.
export const KEYS = ["name", "location", "hometown", "job", "employer", "diet", "allergy", "pet", "family", "favorite_color", "favorite_food", "favorite_music", "language", "hobby", "goal", "vehicle", "birthday", "other"];
export const EXTRACT_PROMPT = `Extract durable facts about the USER from the message. Reply ONLY with a JSON array of {"key","value"} objects, or [] if none. key must be one of: ${KEYS.join(", ")}. For "other" use key "other:<short topic>". value is a short phrase, e.g. [{"key":"name","value":"Sam"},{"key":"location","value":"Austin"}].`;
const norm = (s) => String(s).trim().toLowerCase();
export function upsert(facts, items, now) {
  const out = facts.map((f) => ({ ...f }));
  for (const it of items || []) {
    if (!it || typeof it.key !== "string" || typeof it.value !== "string" || !it.value.trim()) continue;
    const k = norm(it.key), v = it.value.trim();
    const cur = out.find((f) => f.key === k && !f.superseded_at);
    if (cur && norm(cur.value) === norm(v)) { cur.confirmed_at = now; continue; }
    const id = `f${out.length + 1}`;
    if (cur) { cur.superseded_at = now; cur.superseded_by = id; }
    out.push({ id, key: k, value: v, created_at: now, ...(cur ? { supersedes: cur.id } : {}) });
  }
  return out;
}
export const active = (facts) => facts.filter((f) => !f.superseded_at);
export const history = (facts, k) => facts.filter((f) => f.key === norm(k));
export const render = (facts) => active(facts).map((f) => `- ${f.key}: ${f.value}`).join("\n") || "(none yet)";
