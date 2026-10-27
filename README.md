# agent-04-memory

A conversational AI agent with **short-term and long-term memory**, persisted in Cloudflare KV. It runs serverless on Cloudflare Workers with the free Workers AI tier. Project 4 of a 10-part agentic AI portfolio.

**Live demo:** https://agent-04-memory.braylo777.workers.dev

## What it is
Chat with it and it remembers you: the recent turns of the conversation, plus durable facts it pulls from what you say (name, location, preferences, goals). Memory is per anonymous browser session, viewable at `/api/memory`, erasable at `/api/forget`, and expires after 7 days.

## How it works
```
message ─▶ load memory (KV) ─▶ system prompt + facts + last 10 turns ─▶ LLM ─▶ reply
                                   │
          memory writer LLM: extract facts as a JSON array ─▶ dedupe/merge ─▶ save (KV, 7-day TTL)
```
- `src/memory.js`: memory model, the fact-extraction "memory writer" and merging (max 20 facts, last 10 turns).
- Two LLM calls per turn: one replies, one curates long-term memory.
- Privacy: random session id, user-controlled delete, automatic expiry. Guardrails: 5 requests/min per IP, 500-char cap.

## Run it
```bash
npm install
npm test            # offline unit tests (Node 20+), LLM is mocked
npx wrangler dev    # local; Workers AI runs via your Cloudflare login
npx wrangler deploy # free tier, no paid API keys
```
`wrangler.toml` needs your KV namespace id first: `npx wrangler kv namespace create KV`.

## Stack
Cloudflare Workers · Workers AI (`@cf/meta/llama-3.3-70b-instruct-fp8-fast`, Workers KV) · vanilla JS · node:test

## License
MIT
