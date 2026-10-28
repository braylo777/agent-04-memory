// Working memory: the live context buffer. Compaction runs only after a turn completes, never mid-turn.
export const MAX_TURNS = 8, KEEP_TURNS = 4, SUMMARY_CHARS = 800;
export const context = (w) => [...(w.summary ? [{ role: "system", content: `Summary of earlier conversation: ${w.summary}` }] : []), ...w.turns];
export function append(w, user, assistant) { return { ...w, turns: [...w.turns, { role: "user", content: user }, { role: "assistant", content: assistant }] }; }
export function compact(w) {
  if (w.turns.length <= MAX_TURNS) return w;
  const old = w.turns.slice(0, w.turns.length - KEEP_TURNS);
  const digest = old.map((t) => `${t.role}: ${t.content.replace(/\s+/g, " ").slice(0, 80)}`).join(" | ");
  let summary = (w.summary ? w.summary + " | " : "") + digest;
  if (summary.length > SUMMARY_CHARS) summary = "…" + summary.slice(-SUMMARY_CHARS);
  return { summary, turns: w.turns.slice(-KEEP_TURNS) };
}
