// Bundles src/ into one ES module (dist/worker.js) for API upload. wrangler users don't need this.
import { execSync } from "node:child_process";
execSync("npx -y esbuild@0.28.2 src/index.js --bundle --format=esm --loader:.html=text --outfile=dist/worker.js --log-level=warning", { stdio: "inherit" });
