// node --test src/js/  (no DOM needed for what is tested here)
import { test } from 'node:test';
import assert from 'node:assert/strict';

// settings.js reads window.__TAURI__ at import time.
globalThis.window = { __TAURI__: { core: { invoke: async () => ({}), Channel: class {} } } };
globalThis.performance ??= { now: () => Date.now() };
const { MicSilenceWatch, financeProfile } = await import('./onboarding.js');

const zeros = (n) => new Int16Array(n).buffer;

test('denied microphone: seconds of exact zeros fire once', () => {
  let t = 0;
  const realNow = performance.now;
  performance.now = () => t;
  try {
    let fired = 0;
    const w = new MicSilenceWatch(() => fired++, 4000);
    for (; t <= 3900; t += 200) w.feed(zeros(3200));
    assert.equal(fired, 0, 'not before 4 s');
    for (; t <= 6000; t += 200) w.feed(zeros(3200));
    assert.equal(fired, 1, 'fires once, then stays quiet');
  } finally {
    performance.now = realNow;
  }
});

test('a real microphone (any non-zero sample) never fires', () => {
  let t = 0;
  const realNow = performance.now;
  performance.now = () => t;
  try {
    let fired = 0;
    const w = new MicSilenceWatch(() => fired++, 4000);
    const quietRoom = new Int16Array(3200);
    quietRoom[1234] = 1; // noise floor of a quiet room: tiny but not zero
    w.feed(quietRoom.buffer);
    for (t = 0; t <= 10000; t += 200) w.feed(zeros(3200));
    assert.equal(fired, 0);
  } finally {
    performance.now = realNow;
  }
});

test('default finance profile carries the whole glossary', () => {
  const p = financeProfile();
  assert.equal(p.name, 'Tài chính – kinh tế');
  assert.ok(p.context.translation_terms.length >= 470);
  assert.ok(p.context.terms.every((t) => (t.match(/[㐀-鿿]/g) || []).length >= 3), 'recognition terms are ≥ 3 characters');
  assert.ok(p.context.translation_terms.some((t) => t.source === '中央财经大学'));
});
