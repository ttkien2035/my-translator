// Theme (Light / Dark / follow system). Light is the default.
//
// The resolved theme ('light' | 'dark') goes on <html data-theme> — CSS
// tokens in main.css do the rest — and is cached in localStorage so the
// inline script in index.html can apply it before first paint (no flash).
// The native window appearance follows too (traffic-light area, scrollbars,
// form controls). Settings (`theme`) remain the source of truth.

const CACHE_KEY = 'theme_resolved';
const media = window.matchMedia('(prefers-color-scheme: dark)');
let preference = 'light';

function resolve(pref) {
    if (pref === 'system') return media.matches ? 'dark' : 'light';
    return pref === 'dark' ? 'dark' : 'light';
}

function paint() {
    const resolved = resolve(preference);
    if (document.documentElement.dataset.theme !== resolved) {
        document.documentElement.dataset.theme = resolved;
    }
    try { localStorage.setItem(CACHE_KEY, resolved); } catch { /* private mode etc. */ }
    document.dispatchEvent(new CustomEvent('theme-changed', { detail: { theme: resolved, preference } }));
}

// With 'system', repaint when macOS switches appearance.
media.addEventListener('change', () => {
    if (preference === 'system') paint();
});

/** Apply a preference: 'light' | 'dark' | 'system'. */
export async function applyTheme(pref) {
    preference = ['light', 'dark', 'system'].includes(pref) ? pref : 'light';
    // Native appearance first: for 'system' hand control back to macOS (null),
    // so prefers-color-scheme reports the real system setting.
    try {
        await window.__TAURI__?.window?.getCurrentWindow?.().setTheme(preference === 'system' ? null : preference);
    } catch { /* older runtime / no permission: CSS still switches */ }
    paint();
}

export function getThemePreference() {
    return preference;
}

export function getResolvedTheme() {
    return resolve(preference);
}
