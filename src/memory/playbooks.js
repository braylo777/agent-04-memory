// Procedural memory: the skills/playbooks file. Each playbook is how-to knowledge the agent follows when triggered.
export const PLAYBOOKS = [
  { name: "plan-a-trip", triggers: ["trip", "travel", "vacation", "itinerary"], steps: ["Ask dates and budget if missing", "Use the user's known location as origin", "Respect diet and allergies for food ideas", "Give a day-by-day outline"] },
  { name: "recommend-food", triggers: ["dinner", "lunch", "restaurant", "eat", "recipe", "cook"], steps: ["Check diet and allergy facts first", "Prefer the user's location", "Offer two or three options with one-line reasons"] },
  { name: "write-email", triggers: ["email", "message to", "draft"], steps: ["Use the user's name and role in the sign-off", "Keep it under 120 words", "End with one clear ask"] },
  { name: "study-plan", triggers: ["study", "exam", "learn", "certification"], steps: ["Use the user's stated goal", "Break into weekly milestones", "Suggest one practice method per week"] },
  { name: "debug-code", triggers: ["bug", "error", "stack trace", "exception", "debug"], steps: ["Ask for the exact error and language if missing", "Reproduce, isolate, then fix", "Explain the root cause in one line"] },
];
export function select(message, books = PLAYBOOKS) {
  const m = message.toLowerCase();
  return books.map((b) => ({ b, hits: b.triggers.filter((t) => new RegExp(`\\b${t}`, "i").test(m)).length })).filter((x) => x.hits).sort((a, b) => b.hits - a.hits).map((x) => x.b);
}
export const render = (books) => books.map((b) => `Playbook ${b.name}: ${b.steps.join("; ")}`).join("\n");
