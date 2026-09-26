// Course glossaries: one merged, de-duplicated list, plus the budgeting that
// keeps a Soniox context inside its 8 000-token limit.
//
// Consumers:
// - Settings › Hồ sơ › "Nạp từ điển tài chính" merges `allGlossaryTerms()`
//   into the active profile (translation pairs + recognition terms).
// - soniox.js calls `budgetContext()` on the profile before every connect.
// - The Local engine gets the pairs unbudgeted: hotwords cost nothing and the
//   LLM prompt only carries the pairs found in each sentence.

import { FINANCE_GLOSSARY } from './finance-zh-vi.js';
import { FINANCE_ADVANCED_GLOSSARY, CUFE_GLOSSARY } from './finance-advanced-zh-vi.js';

/** Every built-in term `{zh, en, vi, tier}`; the first occurrence of a `zh` wins. */
export function allGlossaryTerms() {
  const seen = new Set();
  const out = [];
  for (const [tier, list] of [['core', FINANCE_GLOSSARY], ['advanced', FINANCE_ADVANCED_GLOSSARY], ['cufe', CUFE_GLOSSARY]]) {
    for (const g of list) {
      const zh = g.zh.trim();
      if (!zh || seen.has(zh)) continue;
      seen.add(zh);
      out.push({ zh, en: g.en, vi: g.vi.trim(), tier });
    }
  }
  return out;
}

const CJK = /[㐀-䶿一-鿿]/g;

export function cjkCount(s) {
  return (s.match(CJK) || []).length;
}

/**
 * Worth sending as a recognition hint? Terms of one or two characters are
 * words the recogniser already knows; as X-ASR hotwords they produced false
 * alarms, and Soniox tokens are better spent on longer, rarer terms.
 */
export function isRecognitionTerm(zh) {
  return cjkCount(zh) >= 3;
}

/**
 * Rough Soniox token count. The docs put the context limit at 8 000 tokens
 * ≈ 10 000 characters; Chinese is close to one token per character,
 * Vietnamese with diacritics roughly one per 2.5 characters. Rounded up.
 */
export function estimateTokens(s) {
  const cjk = cjkCount(s);
  return cjk + Math.ceil((s.length - cjk) / 2.5);
}

/** Tokens the whole context object costs, JSON keys included (approximation). */
export function contextTokens(ctx) {
  let n = 0;
  for (const kv of ctx.general || []) n += estimateTokens(`${kv.key}${kv.value}`) + 4;
  if (ctx.text) n += estimateTokens(ctx.text);
  for (const t of ctx.terms || []) n += estimateTokens(t) + 1;
  for (const p of ctx.translation_terms || []) n += estimateTokens(`${p.source}${p.target}`) + 6;
  return n;
}

// Tokens reserved for `general` + `text` (background, carryover); the rest
// of the 8 000 goes to terms and pairs. A margin absorbs estimation error.
export const SONIOX_CONTEXT_TOKENS = 8000;
const GLOSSARY_TOKEN_BUDGET = 6000;

/**
 * Trim `terms` / `translation_terms` to fit the Soniox budget. Priority is
 * the length of the Chinese term (longer = rarer = what recognition and
 * translation get wrong), translation pairs ahead of bare terms of the same
 * length; a term that is already a pair's source is not sent twice.
 * `general` and `text` are passed through. Returns `{context, dropped}`.
 */
export function budgetContext(ctx, budget = GLOSSARY_TOKEN_BUDGET) {
  const pairs = (ctx.translation_terms || []).filter(p => p.source && p.target);
  const pairSources = new Set(pairs.map(p => p.source));
  const terms = (ctx.terms || []).filter(t => t && isRecognitionTerm(t) && !pairSources.has(t));

  const items = [
    ...pairs.map(p => ({ kind: 'pair', item: p, prio: cjkCount(p.source) * 2 + 1, cost: estimateTokens(`${p.source}${p.target}`) + 6 })),
    ...terms.map(t => ({ kind: 'term', item: t, prio: cjkCount(t) * 2, cost: estimateTokens(t) + 1 })),
  ].sort((a, b) => b.prio - a.prio);

  const kept = { pair: [], term: [] };
  let used = 0;
  let dropped = 0;
  for (const it of items) {
    if (used + it.cost > budget) { dropped++; continue; }
    used += it.cost;
    kept[it.kind].push(it.item);
  }
  // Keep the profile's own order for the ones that made it.
  const keptPairs = new Set(kept.pair);
  const keptTerms = new Set(kept.term);
  return {
    context: {
      ...ctx,
      translation_terms: pairs.filter(p => keptPairs.has(p)),
      terms: terms.filter(t => keptTerms.has(t)),
    },
    dropped,
    tokens: used,
  };
}
