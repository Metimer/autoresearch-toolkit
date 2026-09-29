// Called by package_pi.py against an extracted npm tarball and an independent Pi host.
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const [root, host, engine] = process.argv.slice(2).map((path) => resolve(path));
const { DefaultResourceLoader, SettingsManager } = await import(pathToFileURL(join(host, "dist/index.js")));
const hostPackage = JSON.parse(await readFile(join(host, "package.json"), "utf8"));
const bundle = JSON.parse(await readFile(join(root, "BUNDLE.json"), "utf8"));
assert.equal(hostPackage.version, bundle.tested_pi, "qualify with the pinned Pi host");
const temporary = await mkdtemp(join(tmpdir(), "ultramarine-host-"));
try {
  const loader = new DefaultResourceLoader({
    cwd: temporary, agentDir: join(temporary, "agent"),
    settingsManager: SettingsManager.inMemory(),
    additionalExtensionPaths: [root],
    noExtensions: true, noSkills: true,
    noContextFiles: true, noPromptTemplates: true, noThemes: true,
  });
  await loader.reload();
  const loaded = loader.getExtensions();
  assert.deepEqual(loaded.errors, []);
  assert.equal(loaded.extensions.length, 1);
  const skills = loader.getSkills();
  assert.deepEqual(skills.diagnostics, []);
  assert.deepEqual(skills.skills.map((skill) => skill.name).sort(), ["autoresearch-run", "autoresearch-scout"]);
  const extension = loaded.extensions[0];
  assert.deepEqual([...extension.tools.keys()], ["autoresearch_engine"]);
  assert.ok(extension.commands.has("autoresearch-stop"));
  assert.equal(extension.handlers.has("agent_end"), false);
  const tool = extension.tools.get("autoresearch_engine").definition;
  // Loading without configuration succeeds; using the tool explains what's missing.
  await assert.rejects(tool.execute("missing-config", { action: "status" }, undefined, undefined, { hasUI: false }),
    /Set --autoresearch-engine/);
  loaded.runtime.flagValues.set("autoresearch-engine", engine);
  loaded.runtime.flagValues.set("autoresearch-root", temporary);
  loaded.runtime.flagValues.set("autoresearch-session", "not-created");
  await assert.rejects(tool.execute("negotiate", { action: "status" }, undefined, undefined, { hasUI: false }), /Engine io:/);
  console.log(JSON.stringify({ pi: hostPackage.version, pi_loader: "passed", skills_loader: "passed",
    engine_negotiation: "passed", provider_requests: 0, node: process.version,
    platform: process.platform, arch: process.arch }));
} finally {
  await rm(temporary, { recursive: true, force: true });
}
