// Episodic memory: a log of past tasks and their outcomes, recalled when a new task looks similar.
export const MAX_EPISODES = 50;
export function log(episodes, { task, outcome, now }) {
  const ep = { id: `e${now}-${episodes.length}`, task: task.slice(0, 300), outcome: outcome.replace(/\s+/g, " ").slice(0, 300), at: now };
  return { episodes: [...episodes, ep].slice(-MAX_EPISODES), episode: ep };
}
export const describe = (ep) => `Past task: ${ep.task} -> outcome: ${ep.outcome}`;
export const byIds = (episodes, ids) => ids.map((id) => episodes.find((e) => e.id === id)).filter(Boolean);
export const render = (eps) => eps.map((e) => `- ${new Date(e.at).toISOString().slice(0, 10)}: "${e.task}" -> ${e.outcome}`).join("\n");
