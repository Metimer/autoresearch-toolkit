// Characterization of the archived Pi reader, not desired Rust behavior.
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  parseJsonlEntry,
  reconstructJsonlState,
  hasAutoresearchConfigHeader,
} from '../../originals/pi-autoresearch/extensions/pi-autoresearch/jsonl.ts';

const journal = (...entries) => entries.map(JSON.stringify).join('\n');

test('legacy reader rejects malformed and non-object JSON lines', () => {
  for (const line of ['{', 'null', '[]', '7']) assert.equal(parseJsonlEntry(line), null);
});

test('legacy reader preserves metric direction and secondary metrics', () => {
  const state = reconstructJsonlState(journal(
    { type: 'config', name: 'Synthetic session', metricName: 'throughput', bestDirection: 'higher' },
    { run: 1, metric: 10, status: 'keep', metrics: { duration_ms: 20 } },
  ));
  assert.equal(state.bestDirection, 'higher');
  assert.equal(state.results[0].metric, 10);
  assert.deepEqual(state.secondaryMetrics, [{ name: 'duration_ms', unit: 'ms' }]);
});

test('legacy config changes after results start a new segment', () => {
  const state = reconstructJsonlState(journal(
    { type: 'config', metricName: 'time_ms' },
    { run: 1, metric: 12, status: 'keep' },
    { type: 'config', metricName: 'size_bytes' },
    { run: 2, metric: 100, status: 'discard' },
  ));
  assert.deepEqual(state.results.map(run => run.segment), [0, 1]);
  assert.equal(state.currentSegment, 1);
});

test('known legacy defect: unknown and missing statuses become keep', () => {
  const state = reconstructJsonlState(journal({ run: 1, status: 'unknown' }, { run: 2 }));
  assert.deepEqual(state.results.map(run => run.status), ['keep', 'keep']);
  // The Rust contract deliberately rejects unknown outcomes (contracts.rs).
});

test('known legacy behavior: corrupt middle lines are silently skipped', () => {
  const state = reconstructJsonlState('{"run":1,"status":"keep"}\n{broken\n{"run":2,"status":"discard"}');
  assert.equal(state.results.length, 2);
});

test('legacy header detection does not prove commands or outcomes are valid', () => {
  assert.equal(hasAutoresearchConfigHeader('{"type":"config"}'), true);
  assert.equal(hasAutoresearchConfigHeader('{"run":1}'), false);
});
