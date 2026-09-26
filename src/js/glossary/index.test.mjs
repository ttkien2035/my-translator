// node --test src/js/glossary/
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { allGlossaryTerms, budgetContext, contextTokens, estimateTokens, isRecognitionTerm } from './index.js';

test('built-in glossaries merge without duplicates', () => {
  const all = allGlossaryTerms();
  assert.ok(all.length >= 470, `got ${all.length}`);
  assert.equal(new Set(all.map(t => t.zh)).size, all.length);
  for (const t of all) {
    assert.ok(t.zh && t.vi, `empty side in ${JSON.stringify(t)}`);
    assert.ok(['core', 'advanced', 'cufe'].includes(t.tier));
  }
  assert.ok(all.some(t => t.zh === '中央财经大学'));
  // The core list wins on a clash, so its translation is the one kept.
  assert.equal(all.filter(t => t.zh === '税盾').length, 1);
});

test('recognition terms are three characters or more', () => {
  assert.ok(isRecognitionTerm('资产负债表'));
  assert.ok(!isRecognitionTerm('资产'));
  assert.ok(!isRecognitionTerm('ESG'));
});

test('token estimate: one per Chinese character, Vietnamese compressed', () => {
  assert.equal(estimateTokens('资产负债表'), 5);
  assert.equal(estimateTokens('bảng cân đối kế toán'), 8); // 20 chars / 2.5
  assert.equal(estimateTokens(''), 0);
});

test('budget keeps long terms and drops short ones first', () => {
  const ctx = {
    general: [{ key: 'domain', value: 'finance' }],
    text: 'bg',
    terms: ['资产负债表', '资产', '贝塔系数', '久期风险'],
    translation_terms: [
      { source: '资产', target: 'tài sản' },
      { source: '贝塔系数', target: 'hệ số beta' },
      { source: '资产负债表', target: 'bảng cân đối kế toán' },
      { source: '', target: 'x' },
    ],
  };
  const full = budgetContext(ctx, 10_000);
  assert.equal(full.dropped, 0);
  assert.deepEqual(full.context.terms, ['资产负债表', '贝塔系数', '久期风险'], 'short term dropped, profile order kept');
  assert.equal(full.context.translation_terms.length, 3, 'empty pair removed');
  assert.deepEqual(full.context.general, ctx.general);
  assert.equal(full.context.text, 'bg');

  // A tight budget: 35 % goes to terms (longest first), pairs fill the rest, longest first.
  const tight = budgetContext(ctx, 40);
  assert.deepEqual(tight.context.terms, ['资产负债表', '贝塔系数'], '14 tokens of terms fit in 35 % of 40');
  assert.equal(tight.context.translation_terms[0].source, '资产负债表');
  assert.ok(tight.dropped > 0);
  assert.ok(tight.tokens <= 40);
  assert.ok(!tight.context.translation_terms.some(p => p.source === '资产'), '2-char pair is the first to go');
});

test('the full built-in glossary is cut to fit the Soniox limit', () => {
  const all = allGlossaryTerms();
  const ctx = {
    terms: all.map(t => t.zh),
    translation_terms: all.map(t => ({ source: t.zh, target: t.vi })),
  };
  assert.ok(contextTokens(ctx) > 8000, 'the whole glossary is over the limit, so budgeting matters');
  const { context, dropped, tokens } = budgetContext(ctx);
  assert.ok(tokens <= 6800);
  assert.ok(dropped > 0);
  assert.equal(context.terms.length, ctx.terms.filter(isRecognitionTerm).length, 'every recognition term (≥ 3 chars) fits in the 35 % share');
  assert.ok(context.translation_terms.length >= 200, `kept ${context.translation_terms.length} pairs`);
  assert.ok(context.translation_terms.some(p => p.source === '加权平均资本成本'));
  assert.ok(contextTokens(context) <= 8000);
});
