#!/usr/bin/env node
/**
 * Run the Tauri CLI with an optional bundle-identifier override from `.env`.
 *
 * If the repo-root `.env` defines `APP_IDENTIFIER`, it is injected via
 * `--config {"identifier": "..."}` so the dev build can use a distinct id
 * (e.g. `com.personal.translator.dev`). This gives the dev build its OWN macOS
 * Screen-Recording / Microphone permission entry, leaving the installed stable
 * app's permissions untouched. When `.env` has no `APP_IDENTIFIER`, the default
 * identifier from `tauri.conf.json` is used unchanged.
 *
 * Usage: node scripts/tauri-with-env.mjs <dev|build> [extra tauri args...]
 */
import { run } from '@tauri-apps/cli';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

/** Read a single key from repo-root `.env`; returns undefined if absent. */
function readEnvValue(key) {
  try {
    const content = readFileSync(join(repoRoot, '.env'), 'utf8');
    for (const line of content.split('\n')) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith('#')) continue;
      const eq = trimmed.indexOf('=');
      if (eq === -1) continue;
      if (trimmed.slice(0, eq).trim() !== key) continue;
      return trimmed
        .slice(eq + 1)
        .trim()
        .replace(/^["']|["']$/g, '');
    }
  } catch {
    // no .env — fall through to default
  }
  return undefined;
}

const args = process.argv.slice(2);
const identifier = process.env.APP_IDENTIFIER || readEnvValue('APP_IDENTIFIER');

/** Read a top-level string field from src-tauri/tauri.conf.json. */
function readConfValue(key) {
  try {
    const conf = JSON.parse(readFileSync(join(repoRoot, 'src-tauri', 'tauri.conf.json'), 'utf8'));
    return conf[key];
  } catch {
    return undefined;
  }
}

// All --config overrides are merged into ONE object (passed once).
const override = {};
const merge = (target, src) => {
  for (const [k, v] of Object.entries(src)) {
    if (v && typeof v === 'object' && !Array.isArray(v)) merge((target[k] ??= {}), v);
    else target[k] = v;
  }
};

if (identifier) {
  // Dev override: distinct id + " Dev" product name + ad-hoc signing ("-"). Ad-hoc lets a
  // local dev build run without any certificate, and the " Dev" name makes the app distinct
  // in Finder and in the macOS permission lists.
  const baseName = readConfValue('productName') || 'MeowLaoshi';
  const devName = baseName.endsWith(' Dev') ? baseName : `${baseName} Dev`;
  // Ad-hoc changes on every rebuild, so macOS Screen-Recording permission does NOT persist
  // across rebuilds. Set APP_SIGNING_IDENTITY in .env to a STABLE cert name (e.g. a
  // self-signed "MeowLaoshi Dev" cert) to make the permission stick.
  const signingIdentity =
    process.env.APP_SIGNING_IDENTITY || readEnvValue('APP_SIGNING_IDENTITY') || '-';
  merge(override, { identifier, productName: devName, bundle: { macOS: { signingIdentity } } });
  // Enable the WebView inspector (DevTools) so JS/console errors are visible in the dev app.
  if (!args.includes('--features')) {
    args.push('--features', 'devtools');
  }
  // A dev build only needs the runnable .app — skip the .dmg.
  if (args[0] === 'build' && !args.includes('--bundles')) {
    args.push('--bundles', 'app');
  }
  const signLabel = signingIdentity === '-' ? 'ad-hoc ("-")' : `"${signingIdentity}"`;
  console.log(`[tauri-with-env] dev override: identifier=${identifier}, productName="${devName}", signing=${signLabel}, devtools=on`);
} else if (args[0] === 'build') {
  // Local distributable build (`npm run build:local`) — the free DMG for friends:
  // - macOS: ad-hoc signature ("-") unless APPLE_SIGNING_IDENTITY is set. Required for
  //   Apple Silicon and avoids the "app is damaged" error; no Apple Developer account.
  // - Updater artifacts need the updater's private key. Use it from the environment or
  //   .env when present; otherwise skip them so the build still succeeds (the app is
  //   fine — only the auto-update .tar.gz/.sig files aren't produced).
  if (!process.env.APPLE_SIGNING_IDENTITY) merge(override, { bundle: { macOS: { signingIdentity: '-' } } });
  const key = process.env.TAURI_SIGNING_PRIVATE_KEY || readEnvValue('TAURI_SIGNING_PRIVATE_KEY');
  if (key) {
    process.env.TAURI_SIGNING_PRIVATE_KEY = key;
    process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ??= readEnvValue('TAURI_SIGNING_PRIVATE_KEY_PASSWORD') || '';
  } else if (!process.env.TAURI_SIGNING_PRIVATE_KEY_PATH) {
    merge(override, { bundle: { createUpdaterArtifacts: false } });
    console.log('[tauri-with-env] no TAURI_SIGNING_PRIVATE_KEY — building without auto-update artifacts');
  }
  console.log('[tauri-with-env] local build: macOS ad-hoc signed');
} else {
  console.log('[tauri-with-env] no APP_IDENTIFIER — using default identifier + name from tauri.conf.json');
}

if (Object.keys(override).length > 0) args.push('--config', JSON.stringify(override));

run(args, 'tauri').catch((err) => {
  console.error(err);
  process.exit(1);
});
