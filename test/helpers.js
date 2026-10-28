export const kv = () => { const m = new Map(); return { m, get: async (k, t) => (m.has(k) ? (t === "json" ? JSON.parse(m.get(k)) : m.get(k)) : null), put: async (k, v) => m.set(k, v), delete: async (k) => m.delete(k) }; };
// Fake Workers AI: embeddings are bag-of-keywords vectors; chat replies are scripted.
const VOCAB = ["gym", "equinox", "planet", "paris", "rome", "trip", "dinner", "spicy", "denver", "lisbon", "dentist", "python"];
export const fakeVec = (t) => { const s = t.toLowerCase(); const v = VOCAB.map((w) => (s.includes(w) ? 1 : 0)); v.push(0.01); return v; };
export const fakeAI = (extract = () => "[]", reply = () => "ok") => ({
  calls: [],
  async run(model, inp) {
    this.calls.push({ model, inp });
    if (inp.text) return { data: inp.text.map(fakeVec) };
    const sys = inp.messages[0].content;
    return { response: sys.startsWith("Extract") ? extract(inp.messages[1].content) : reply(inp) };
  },
});
