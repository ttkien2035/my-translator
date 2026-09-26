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

// Calibrated against Soniox's own count on 2026-09-26: two contexts this
// estimate put at 11 526 and 9 225 tokens were rejected as 9 958 and 8 061
// ("Context is too long … the maximum is 8000"), so real ≈ 0.87 × estimate.
// 8 000 estimated ≈ 6 960 real, plus `general` (a domain line) and `text`
// (500 characters of carryover ≈ 170 real) ≈ 7 150 — an 8 025 estimate was
// accepted live. The margin covers a user-added glossary with a different mix.
export const SONIOX_CONTEXT_TOKENS = 8000;
const GLOSSARY_TOKEN_BUDGET = 8000;

// Share of the glossary budget reserved for recognition terms. A term costs
// ~5 tokens, a translation pair ~20, and a recognition error cannot be
// repaired downstream, so the terms come first: 35 % holds every term of the
// built-in glossary (≥ 3 characters); pairs fill the rest, longest first.
const TERMS_SHARE = 0.35;

/**
 * Trim `terms` / `translation_terms` to fit the Soniox budget. Within each
 * list the priority is the length of the Chinese term (longer = rarer = what
 * recognition and translation get wrong). `general` and `text` are passed
 * through. Returns `{context, dropped, tokens}`.
 */
export function budgetContext(ctx, budget = GLOSSARY_TOKEN_BUDGET) {
  const pairs = (ctx.translation_terms || []).filter(p => p.source && p.target);
  const terms = [...new Set((ctx.terms || []).filter(t => t && isRecognitionTerm(t)))];
  const byLength = (a, b) => cjkCount(b) - cjkCount(a);

  let used = 0;
  let dropped = 0;
  const take = (list, cost, limit) => {
    const kept = new Set();
    for (const item of list) {
      const c = cost(item);
      if (used + c > limit) { dropped++; continue; }
      used += c;
      kept.add(item);
    }
    return kept;
  };
  const keptTerms = take([...terms].sort(byLength), t => estimateTokens(t) + 1, budget * TERMS_SHARE);
  const keptPairs = take([...pairs].sort((a, b) => byLength(a.source, b.source)), p => estimateTokens(`${p.source}${p.target}`) + 6, budget);

  // Profile order for the survivors.
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
