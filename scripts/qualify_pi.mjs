// Explicit package-loader check. Run after npm ci in the extracted Pi adapter.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const root = resolve(process.argv[2] ?? ".");
const { discoverAndLoadExtensions } = await import(pathToFileURL(join(root, "adapters/pi/node_modules/@earendil-works/pi-coding-agent/dist/index.js")));
const temporary = await mkdtemp(join(tmpdir(), "autoresearch-packaged-pi-"));
try {
  const loaded = await discoverAndLoadExtensions([join(root, "adapters/pi/index.ts")], temporary, join(temporary, "agent"));
  assert.deepEqual(loaded.errors, []);
  assert.equal(loaded.extensions.length, 1);
  loaded.runtime.flagValues.set("autoresearch-engine", join(root, "bin/autoresearch"));
  loaded.runtime.flagValues.set("autoresearch-root", temporary);
  loaded.runtime.flagValues.set("autoresearch-session", "not-created");
  const extension = loaded.extensions[0];
  assert.deepEqual([...extension.tools.keys()], ["autoresearch_engine"]);
  assert.equal(extension.handlers.has("agent_end"), false);
  // Negotiation must pass; the subsequent read fails because no session exists.
  await assert.rejects(extension.tools.get("autoresearch_engine").definition.execute(
    "package-check", { action: "status" }, undefined, undefined, { hasUI: false },
  ), /Engine io:/);
  const manifestHash = createHash("sha256").update(await readFile(join(root, "BUNDLE.json"))).digest("hex");
  console.log(JSON.stringify({ schema_version: 1, pi: "0.85.1", packaged_loader: "passed", engine_negotiation: "passed", provider_requests: 0,
    package_manifest_sha256: manifestHash, node: process.version, platform: process.platform, arch: process.arch }));
} finally {
  await rm(temporary, { recursive: true, force: true });
}
