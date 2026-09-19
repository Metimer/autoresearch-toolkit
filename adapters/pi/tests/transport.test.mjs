import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Transport } from "../transport.ts";

test("transport rejects incompatible JSON, missing verdicts, exit mismatches and excessive output", async (t) => {
  const root = mkdtempSync(join(tmpdir(), "autoresearch transport "));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const script = join(root, "engine.mjs");
  const cases = [
    ["console.log('not json')", /JSON|Unexpected/],
    ['console.log(JSON.stringify({schema_version:2,command:"doctor",ok:true}))', /Incompatible/],
    ['console.log(JSON.stringify({schema_version:1,command:"status",ok:true}))', /Incompatible/],
    ['console.log(JSON.stringify({schema_version:1,command:"doctor",ok:true})); process.exitCode=2', /Incompatible/],
    ['console.log(JSON.stringify({schema_version:1,ok:false,error:{code:"test",message:"failure"}})); process.exitCode=4', /Engine test: failure/],
    ["process.stdout.write('x'.repeat(17*1024*1024))", /exceeds 16 MiB/],
  ];
  for (const [source, expected] of cases) {
    writeFileSync(script, source);
    // The Node executable stands in for a malformed CLI; argv[0] is the script.
    await assert.rejects(new Transport(process.execPath, root).run([script]), expected);
  }
  const shell = join(root, "bad-engine");
  writeFileSync(shell, '#!/bin/sh\nprintf \'{"schema_version":1,"command":"evaluate","ok":true}\\n\'\n', { mode: 0o700 });
  await assert.rejects(new Transport(shell, root).run(["evaluate"]), /verdict/);
  const signal = AbortSignal.abort();
  await assert.rejects(new Transport(shell, root).run(["evaluate"], signal), /before launch/);
  assert.throws(() => new Transport("relative", root), /absolute/);
});
