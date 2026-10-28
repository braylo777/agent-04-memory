// Prospective memory: things to do in the future. Reminders fire when due, from the cron trigger or on the next request.
const UNIT = { second: 1e3, sec: 1e3, minute: 6e4, min: 6e4, hour: 36e5, day: 864e5 };
export function parse(message, now) {
  const m = message.match(/remind me (?:in|after) (\d+)\s*(seconds?|secs?|minutes?|mins?|hours?|days?)\s*(?:to |that |about )?(.+)/i);
  if (!m) return null;
  const unit = UNIT[m[2].toLowerCase().replace(/s$/, "")];
  return unit ? { text: m[3].replace(/[.!]+$/, "").trim(), due_at: now + Number(m[1]) * unit } : null;
}
export function schedule(reminders, r, now) { return [...reminders, { id: `r${now}-${reminders.length}`, ...r, created_at: now, fired_at: null }]; }
export function fireDue(reminders, now) {
  const fired = [];
  const out = reminders.map((r) => (!r.fired_at && r.due_at <= now ? (fired.push({ ...r, fired_at: now }), { ...r, fired_at: now }) : r));
  return { reminders: out, fired };
}
export const pending = (reminders) => reminders.filter((r) => !r.fired_at);
