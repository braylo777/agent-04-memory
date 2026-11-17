VOCAB = ["gym", "equinox", "planet", "paris", "rome", "trip", "dinner", "spicy", "denver", "lisbon", "dentist", "python"]


class KV:
    def __init__(self):
        self.m = {}
    def get(self, k):
        return self.m.get(k)
    def put(self, k, v, ttl=None):
        self.m[k] = v
    def delete(self, k):
        self.m.pop(k, None)


def fake_vec(t):
    s = t.lower()
    return [1 if w in s else 0 for w in VOCAB] + [0.01]


def fake_ai(extract=lambda m: "[]", reply=lambda inp: "ok"):
    def ai(model, inp):
        if "text" in inp:
            return {"data": [fake_vec(t) for t in inp["text"]]}
        sys = inp["messages"][0]["content"]
        return {"response": extract(inp["messages"][1]["content"]) if sys.startswith("Extract") else reply(inp)}
    return ai
