// Onboarding for non-technical users (Commit E): one dialog component with
// "fix it now" buttons, the offline pack download, API-key entry with a live
// check, the microphone-permission guide and the first-run wizard.
//
// Rules (doc_coding/work-plan.md › Commit E): never tell the user to go to
// "Settings › X › Y" — every fixable problem comes with the button that fixes
// it; one task = one button; defaults fit a Vietnamese finance student in
// China; guide on first run, stay quiet afterwards.

import { settingsManager } from './settings.js';
import { allGlossaryTerms, isRecognitionTerm } from './glossary/index.js';

const tauri = () => window.__TAURI__.core;

// ─── Tiny DOM helper (textContent only: nothing here renders HTML) ────────

function h(tag, attrs = {}, ...children) {
    const el = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) {
        if (v == null || v === false) continue;
        if (k === 'class') el.className = v;
        else if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
        else if (k === 'text') el.textContent = v;
        else el.setAttribute(k, v === true ? '' : v);
    }
    for (const c of children.flat()) {
        if (c == null || c === false) continue;
        el.append(c instanceof Node ? c : document.createTextNode(String(c)));
    }
    return el;
}

// ─── Dialog ──────────────────────────────────────────────────────────────

let openDialogCtl = null;

/**
 * Modal sheet. `actions`: [{ label, kind: 'primary'|'secondary'|'link', onClick }]
 * — onClick may return a Promise; the button shows busy while it runs. The
 * dialog stays open unless the handler calls `ctl.close()`.
 */
export function openDialog({ icon = '', title, body, actions = [], dismissible = true, wide = false, onDismiss } = {}) {
    openDialogCtl?.close();
    const titleEl = h('h3', { class: 'dlg-title', id: 'dlg-title' }, title);
    const bodyEl = h('div', { class: 'dlg-body' });
    const footEl = h('div', { class: 'dlg-actions' });
    const card = h('div', { class: `modal-card dlg-card${wide ? ' dlg-wide' : ''}`, role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'dlg-title' },
        h('div', { class: 'dlg-head' }, icon ? h('span', { class: 'dlg-icon', 'aria-hidden': 'true' }, icon) : null, titleEl),
        bodyEl, footEl);
    const overlay = h('div', { class: 'modal-overlay dlg-overlay' }, card);

    const onKey = (e) => {
        if (e.key === 'Escape' && dismissible && !e.isComposing) { e.preventDefault(); e.stopPropagation(); ctl.close(); onDismiss?.(); }
    };
    const ctl = {
        el: card,
        body: bodyEl,
        setTitle(t) { titleEl.textContent = t; },
        setBody(...nodes) { bodyEl.replaceChildren(...nodes.flat().filter(Boolean)); },
        setActions(list) {
            footEl.replaceChildren(...list.map((a) => {
                const btn = h('button', { type: 'button', class: `dlg-btn dlg-${a.kind || 'secondary'}`, disabled: a.disabled }, a.label);
                btn.addEventListener('click', async () => {
                    if (btn.disabled) return;
                    btn.disabled = true;
                    try { await a.onClick?.(ctl, btn); } finally { if (btn.isConnected) btn.disabled = false; }
                });
                return btn;
            }));
            (footEl.querySelector('.dlg-primary') || footEl.querySelector('button'))?.focus({ preventScroll: true });
        },
        close() {
            document.removeEventListener('keydown', onKey, true);
            overlay.remove();
            if (openDialogCtl === ctl) openDialogCtl = null;
        },
        isOpen: () => overlay.isConnected,
    };
    ctl.setBody(...[].concat(body || []));
    ctl.setActions(actions);
    document.addEventListener('keydown', onKey, true);
    document.body.append(overlay);
    openDialogCtl = ctl;
    const firstInput = bodyEl.querySelector('input');
    if (firstInput) setTimeout(() => firstInput.focus(), 30);
    return ctl;
}

const p = (text, cls = '') => h('p', { class: `dlg-text ${cls}`.trim() }, text);

function progressBar() {
    const fill = h('div', { class: 'progress-fill' });
    const pct = h('span', { class: 'progress-pct' }, '0 %');
    const line = h('p', { class: 'dlg-text dlg-muted' }, '');
    const wrap = h('div', { class: 'dlg-progress' }, h('div', { class: 'dlg-progress-row' }, h('div', { class: 'progress-bar' }, fill), pct), line);
    return {
        el: wrap,
        set(frac, text) {
            const v = Math.max(0, Math.min(1, frac));
            fill.style.width = `${(v * 100).toFixed(1)}%`;
            pct.textContent = `${Math.floor(v * 100)} %`;
            if (text != null) line.textContent = text;
        },
    };
}

// Decimal units, as Finder shows sizes on macOS.
const gb = (bytes) => `${(bytes / 1e9).toFixed(1).replace('.', ',')} GB`;
const mb = (bytes) => `${Math.round(bytes / 1e6)} MB`;

// ─── Offline pack (X-ASR + Hy-MT2 + Silero VAD) ──────────────────────────

export async function offlinePackStatus() {
    const list = await tauri().invoke('local_models_status');
    const missing = list.filter((m) => !m.installed);
    return { ready: missing.length === 0, missingBytes: missing.reduce((a, m) => a + m.size, 0), items: list };
}

let packJob = null; // one download at a time, shared by every screen that shows it

/**
 * Start (or join) the offline-pack download. `onProgress({ frac, received,
 * total, text })`. Resolves when everything is installed; rejects with a
 * user-facing message.
 */
export function downloadOfflinePack(onProgress) {
    if (packJob) {
        if (onProgress) packJob.listeners.add(onProgress);
        if (packJob.last && onProgress) onProgress(packJob.last);
        return packJob.promise;
    }
    const listeners = new Set(onProgress ? [onProgress] : []);
    const job = { listeners, last: null };
    job.promise = (async () => {
        const status = await offlinePackStatus();
        if (status.ready) return;
        const total = status.missingBytes;
        const got = {};
        const names = { 'silero-vad': 'ngắt câu', 'x-asr-zh-en-punct-int8': 'nhận dạng giọng nói', 'hy-mt2-1.8b-q6': 'dịch' };
        const ch = new (tauri().Channel)();
        ch.onmessage = (m) => {
            got[m.id] = m.phase === 'done' || m.phase === 'extracting' ? m.total : m.received;
            const received = Object.values(got).reduce((a, b) => a + b, 0);
            const part = names[m.id] || m.id;
            const text = m.phase === 'extracting' ? `Đang giải nén phần ${part}…`
                : m.phase === 'error' ? `Lỗi khi tải phần ${part}`
                : `Đang tải phần ${part} · ${mb(received)} / ${mb(total)}`;
            job.last = { frac: total ? received / total : 1, received, total, text };
            job.listeners.forEach((l) => l(job.last));
        };
        try {
            await tauri().invoke('local_models_download', { onProgress: ch });
        } catch (err) {
            throw new Error(friendlyDownloadError(err));
        }
        job.last = { frac: 1, received: total, total, text: 'Đã tải xong' };
        job.listeners.forEach((l) => l(job.last));
        document.dispatchEvent(new CustomEvent('offline-pack-changed'));
    })().finally(() => { packJob = null; updateDownloadChip(null); });
    packJob = job;
    job.listeners.add(updateDownloadChip);
    return job.promise;
}

function friendlyDownloadError(err) {
    const s = String(err?.message || err);
    if (/in progress|đang tải/i.test(s)) return 'Gói offline đang được tải ở nơi khác, chờ một chút rồi thử lại.';
    if (/sha|checksum/i.test(s)) return 'File tải về bị lỗi. Bấm Thử lại để tải lại.';
    if (/space|disk|No space/i.test(s)) return 'Máy không đủ dung lượng trống (cần khoảng 2 GB).';
    return 'Không tải được. Kiểm tra Wi-Fi rồi bấm Thử lại.';
}

/** Small corner chip so a download started in a dialog stays visible after it closes. */
function updateDownloadChip(state) {
    let chip = document.getElementById('download-chip');
    if (!state || state.frac >= 1) { chip?.remove(); return; }
    if (!chip) {
        chip = h('div', { id: 'download-chip', class: 'download-chip', role: 'status' });
        document.body.append(chip);
    }
    chip.textContent = `📦 Gói offline ${Math.floor(state.frac * 100)} %`;
}

/**
 * Make sure the offline pack is installed, asking first. Resolves true when
 * it is (already, or after downloading here), false if the user declines.
 */
export async function ensureOfflinePack() {
    let status;
    try { status = await offlinePackStatus(); } catch { status = { ready: false, missingBytes: 0 }; }
    if (status.ready) return true;
    return new Promise((resolve) => {
        const bar = progressBar();
        const intro = p(`Dịch offline cần tải một lần khoảng ${gb(status.missingBytes)}: phần nhận dạng giọng nói, phần dịch và phần ngắt câu. Nên dùng Wi-Fi ổn định; tải xong app tự bắt đầu.`);
        let settled = false;
        const finish = (v) => { if (!settled) { settled = true; resolve(v); } };
        const start = async (ctl) => {
            ctl.setBody(intro, bar.el);
            ctl.setActions([{ label: 'Tải tiếp trong nền', kind: 'secondary', onClick: (c) => { c.close(); finish(false); } }]);
            try {
                await downloadOfflinePack((s) => bar.set(s.frac, s.text));
                ctl.close();
                finish(true);
            } catch (err) {
                ctl.setBody(intro, p(err.message, 'dlg-error'));
                ctl.setActions([
                    { label: 'Để sau', kind: 'secondary', onClick: (c) => { c.close(); finish(false); } },
                    { label: 'Thử lại', kind: 'primary', onClick: (c) => start(c) },
                ]);
            }
        };
        const ctl = openDialog({
            icon: '📦',
            title: 'Cần tải gói offline',
            body: [intro],
            onDismiss: () => finish(false),
            actions: [
                { label: 'Để sau', kind: 'secondary', onClick: (c) => { c.close(); finish(false); } },
                { label: 'Tải ngay', kind: 'primary', onClick: (c) => start(c) },
            ],
        });
        if (packJob) start(ctl); // already downloading (e.g. started in the wizard): show it
    });
}

// ─── API keys ────────────────────────────────────────────────────────────

const ENGINES = {
    soniox: { name: 'Soniox', setting: 'soniox_api_key', hint: 'Chưa có key? Hỏi người đã gửi app cho bạn.' },
    openai: { name: 'OpenAI', setting: 'openai_api_key', hint: 'Key tạo tại platform.openai.com › API keys.' },
    qwen: { name: 'Qwen (DashScope)', setting: 'qwen_api_key', hint: 'Key tạo tại bailian.console.alibabacloud.com (region Singapore).' },
};

/**
 * Ask for an engine's API key, check it live when possible, save it.
 * Resolves 'saved' | 'offline' (user chose offline instead) | null.
 * `ping(key) → Promise<boolean>` is the app's live check (optional).
 */
export function askApiKey(engine, { reason, ping, offerOffline = true } = {}) {
    const e = ENGINES[engine];
    return new Promise((resolve) => {
        const input = h('input', { type: 'password', class: 'dlg-input', placeholder: `Dán API key ${e.name} vào đây`, autocomplete: 'off', spellcheck: 'false' });
        const status = p('', 'dlg-muted');
        const body = [
            reason ? p(reason, 'dlg-error') : null,
            p(engine === 'soniox' ? 'Soniox là cách dịch qua mạng, chính xác nhất. Dán key một lần, app sẽ nhớ.' : `Dán key ${e.name} một lần, app sẽ nhớ.`),
            input, status, p(e.hint, 'dlg-muted'),
        ];
        let settled = false;
        const finish = (v) => { if (!settled) { settled = true; resolve(v); } };
        const save = async (ctl) => {
            const key = input.value.trim();
            if (!key) { status.textContent = 'Chưa có key nào được dán.'; input.focus(); return; }
            status.className = 'dlg-text dlg-muted';
            status.textContent = 'Đang kiểm tra key…';
            const ok = ping ? await ping(key).catch(() => false) : true;
            if (!ok) {
                status.className = 'dlg-text dlg-error';
                status.textContent = navigator.onLine === false
                    ? 'Máy đang mất mạng nên chưa kiểm tra được. Kết nối Wi-Fi (hoặc VPN) rồi thử lại.'
                    : 'Key không dùng được. Kiểm tra đã dán đủ, không thừa dấu cách; vẫn lỗi thì hỏi người cấp key.';
                return;
            }
            await settingsManager.save({ [e.setting]: key });
            const formInput = document.getElementById(engine === 'soniox' ? 'input-api-key' : engine === 'openai' ? 'input-openai-key' : 'input-qwen-key');
            if (formInput) formInput.value = key;
            ctl.close();
            finish('saved');
        };
        input.addEventListener('keydown', (ev) => { if (ev.key === 'Enter' && !ev.isComposing) { ev.preventDefault(); ctl.el.querySelector('.dlg-primary')?.click(); } });
        const actions = [];
        if (offerOffline) actions.push({ label: 'Dùng offline thay thế', kind: 'secondary', onClick: (c) => { c.close(); finish('offline'); } });
        else actions.push({ label: 'Huỷ', kind: 'secondary', onClick: (c) => { c.close(); finish(null); } });
        actions.push({ label: 'Kiểm tra & lưu', kind: 'primary', onClick: save });
        const ctl = openDialog({ icon: '🔑', title: `Cần API key ${e.name}`, body, actions, onDismiss: () => finish(null) });
    });
}

// ─── Microphone ──────────────────────────────────────────────────────────

/** Open the OS microphone-privacy page. */
export function openMicPrivacy() {
    return tauri().invoke('open_privacy_settings', { pane: 'microphone' }).catch(() => {});
}

/** Resolves 'retry' when the user says the permission is now on, null otherwise. */
export function showMicPermissionDialog() {
    return new Promise((resolve) => {
        let settled = false;
        const finish = (v) => { if (!settled) { settled = true; resolve(v); } };
        openDialog({
            icon: '🎤',
            title: 'App chưa nghe được micro',
            body: [
                p('macOS đang chặn micro cho MeowLaoshi, nên app chỉ nhận được im lặng.'),
                p('Bấm nút dưới đây, bật công tắc cạnh MeowLaoshi trong danh sách Micrô, rồi quay lại bấm "Đã bật, thử lại".'),
            ],
            onDismiss: () => finish(null),
            actions: [
                { label: 'Đã bật, thử lại', kind: 'secondary', onClick: (c) => { c.close(); finish('retry'); } },
                { label: 'Mở cài đặt quyền Micrô', kind: 'primary', onClick: () => openMicPrivacy() },
            ],
        });
    });
}

/**
 * Detects a denied microphone: macOS then delivers pure digital silence
 * (every sample exactly 0) instead of an error. Feed every s16le batch;
 * `onDenied` fires once after `ms` of nothing but zeros. A real microphone
 * never yields exact zeros for seconds, even in a quiet room.
 */
export class MicSilenceWatch {
    constructor(onDenied, ms = 4000) {
        this.onDenied = onDenied;
        this.ms = ms;
        this.started = 0;
        this.heard = false;
        this.fired = false;
    }
    feed(buf) {
        if (this.heard || this.fired) return;
        // Byte-wise: a non-zero byte means a non-zero sample, and this works
        // for ArrayBuffer, typed arrays and plain arrays of any length.
        const bytes = buf instanceof Uint8Array ? buf : new Uint8Array(buf instanceof ArrayBuffer ? buf : buf?.buffer ?? buf ?? []);
        for (let i = 0; i < bytes.length; i++) {
            if (bytes[i] !== 0) { this.heard = true; return; }
        }
        const now = performance.now();
        if (!this.started) this.started = now;
        if (now - this.started >= this.ms) { this.fired = true; this.onDenied?.(); }
    }
}

// ─── Course profile with the finance glossary ────────────────────────────

export function financeProfile() {
    const all = allGlossaryTerms();
    return {
        id: 'p-finance',
        name: 'Tài chính – kinh tế',
        context: {
            general: [{ key: 'domain', value: 'finance, economics, accounting — university lecture' }],
            terms: all.map((t) => t.zh).filter(isRecognitionTerm),
            text: null,
            translation_terms: all.map((t) => ({ source: t.zh, target: t.vi })),
        },
    };
}

function generalProfile() {
    return { id: 'p-general', name: 'Môn học', context: { general: [{ key: 'domain', value: 'university lecture' }], terms: [], text: null, translation_terms: [] } };
}

/** Fresh installs get a profile (the finance one unless told otherwise). */
export async function ensureDefaultProfile(kind = 'finance') {
    const s = settingsManager.get();
    if (Array.isArray(s.profiles) && s.profiles.length > 0) return false;
    const prof = kind === 'finance' ? financeProfile() : generalProfile();
    await settingsManager.save({ profiles: [prof], active_profile: prof.id });
    return true;
}

// ─── First-run wizard ────────────────────────────────────────────────────

/**
 * Four short steps replacing the old "choose an engine" box: Soniox key
 * (checked live) → offline pack → subject → microphone test. Every step can
 * be skipped; skipping everything still leaves sensible defaults (finance
 * profile, microphone source). Resolves { mode } — 'soniox' | 'local'.
 */
export function runFirstRunWizard({ pingSoniox }) {
    return new Promise((resolve) => {
        const state = { keyOk: !!(settingsManager.get().soniox_api_key || '').trim(), subject: 'finance' };
        let ctl;
        let micStop = null;
        const steps = [stepKey, stepPack, stepSubject, stepMic];
        let index = 0;

        const dots = () => h('div', { class: 'wiz-dots', 'aria-label': `Bước ${index + 1}/${steps.length}` },
            steps.map((_, i) => h('span', { class: `wiz-dot${i === index ? ' on' : i < index ? ' done' : ''}` })));

        const show = (i) => {
            index = i;
            micStop?.(); micStop = null;
            steps[i]();
        };
        const next = () => (index + 1 < steps.length ? show(index + 1) : finish());
        const skipLink = { label: 'Bỏ qua', kind: 'link', onClick: () => finish() };

        async function finish() {
            micStop?.(); micStop = null;
            await ensureDefaultProfile(state.subject);
            const mode = state.keyOk ? 'soniox' : 'local';
            await settingsManager.save({ translation_mode: mode, audio_source: 'microphone', engine_picker_done: true });
            ctl.close();
            resolve({ mode });
        }

        function stepKey() {
            const input = h('input', { type: 'password', class: 'dlg-input', placeholder: 'Dán API key Soniox vào đây', autocomplete: 'off', spellcheck: 'false', value: settingsManager.get().soniox_api_key || '' });
            const status = p(state.keyOk ? '✓ Key đã lưu.' : '', state.keyOk ? 'dlg-ok' : 'dlg-muted');
            const check = async () => {
                const key = input.value.trim();
                if (!key) { status.className = 'dlg-text dlg-muted'; status.textContent = 'Chưa có key thì bấm "Chưa có key": bạn vẫn dùng được chế độ offline.'; return; }
                status.className = 'dlg-text dlg-muted'; status.textContent = 'Đang kiểm tra key…';
                const ok = await pingSoniox(key).catch(() => false);
                if (ok) {
                    await settingsManager.save({ soniox_api_key: key });
                    const f = document.getElementById('input-api-key'); if (f) f.value = key;
                    state.keyOk = true;
                    status.className = 'dlg-text dlg-ok'; status.textContent = '✓ Key hoạt động.';
                    setTimeout(next, 600);
                } else {
                    status.className = 'dlg-text dlg-error';
                    status.textContent = navigator.onLine === false
                        ? 'Máy đang mất mạng nên chưa kiểm tra được. Kết nối Wi-Fi (hoặc VPN) rồi thử lại.'
                        : 'Key không dùng được. Kiểm tra đã dán đủ; vẫn lỗi thì hỏi người đã gửi app cho bạn.';
                }
            };
            input.addEventListener('keydown', (ev) => { if (ev.key === 'Enter' && !ev.isComposing) { ev.preventDefault(); check(); } });
            ctl.setTitle('Chào bạn! Bắt đầu với key Soniox');
            ctl.setBody(dots(),
                p('MeowLaoshi nghe giảng viên nói tiếng Trung và dịch sang tiếng Việt. Cách chính xác nhất là dịch qua mạng bằng Soniox.'),
                input, status,
                p('Chưa có key? Hỏi người đã gửi app cho bạn, hoặc bấm "Chưa có key" để dùng chế độ offline.', 'dlg-muted'));
            const saved = () => (settingsManager.get().soniox_api_key || '').trim();
            ctl.setActions([skipLink,
                { label: 'Chưa có key', kind: 'secondary', onClick: () => { if (!saved()) state.keyOk = false; next(); } },
                { label: state.keyOk ? 'Tiếp' : 'Kiểm tra key', kind: 'primary',
                  onClick: () => (state.keyOk && input.value.trim() === saved() ? next() : check()) }]);
        }

        async function stepPack() {
            let status;
            try { status = await offlinePackStatus(); } catch { status = { ready: false, missingBytes: 1.6e9 }; }
            ctl.setTitle('Gói offline');
            if (status.ready && !packJob) {
                ctl.setBody(dots(), p('✓ Gói offline đã có sẵn: bạn dịch được cả khi mất mạng.', 'dlg-ok'));
                ctl.setActions([skipLink, { label: 'Tiếp', kind: 'primary', onClick: next }]);
                return;
            }
            const bar = progressBar();
            const why = state.keyOk
                ? `Dùng khi mất mạng hoặc không có VPN. Tải một lần khoảng ${gb(status.missingBytes)}; tải trong nền, bạn cứ làm tiếp.`
                : `Bạn chưa có key Soniox nên cần gói này để dịch. Tải một lần khoảng ${gb(status.missingBytes)}; tải trong nền, bạn cứ làm tiếp.`;
            const startDl = () => {
                ctl.setBody(dots(), p(why), bar.el);
                ctl.setActions([skipLink, { label: 'Tiếp', kind: 'primary', onClick: next }]);
                downloadOfflinePack((s) => bar.set(s.frac, s.text)).catch((err) => {
                    if (!ctl.isOpen()) return;
                    bar.set(0, err.message);
                });
            };
            if (packJob) { startDl(); return; }
            ctl.setBody(dots(), p(why));
            ctl.setActions([skipLink,
                { label: 'Để sau', kind: 'secondary', onClick: next },
                { label: state.keyOk ? 'Tải ngay (khuyên dùng)' : 'Tải ngay', kind: 'primary', onClick: startDl }]);
        }

        function stepSubject() {
            const pick = (subject) => { state.subject = subject; next(); };
            ctl.setTitle('Bạn học môn gì?');
            ctl.setBody(dots(),
                p('App nạp sẵn thuật ngữ của môn để nhận đúng và dịch đúng từ chuyên ngành. Đổi hoặc thêm môn sau trong Cài đặt.'),
                h('div', { class: 'wiz-choices' },
                    h('button', { type: 'button', class: 'wiz-choice', onclick: () => pick('finance') },
                        h('strong', {}, '📈 Tài chính – kinh tế'), h('span', {}, 'Nạp sẵn khoảng 480 thuật ngữ, gồm tên các học viện của CUFE')),
                    h('button', { type: 'button', class: 'wiz-choice', onclick: () => pick('general') },
                        h('strong', {}, '📚 Môn khác'), h('span', {}, 'Bắt đầu với từ điển trống'))));
            ctl.setActions([skipLink]);
        }

        function stepMic() {
            const fill = h('div', { class: 'level-fill' });
            const meter = h('div', { class: 'level-meter', 'aria-hidden': 'true' }, fill);
            const status = p('Nói thử vài câu: thanh màu sẽ nhảy theo giọng bạn.', 'dlg-muted');
            const retry = { label: 'Thử lại', kind: 'secondary', onClick: () => startMic() };
            const done = { label: 'Xong, vào app', kind: 'primary', onClick: () => finish() };
            ctl.setTitle('Thử micro');
            ctl.setBody(dots(), p('Lần đầu, macOS sẽ hỏi quyền dùng micro: bấm Cho phép.'), meter, status);
            ctl.setActions([done]);
            const startMic = async () => {
                micStop?.();
                let peak = 0;
                const watch = new MicSilenceWatch(() => {
                    status.className = 'dlg-text dlg-error';
                    status.textContent = 'Chưa nghe thấy gì: có thể macOS đang chặn micro. Bấm "Mở cài đặt quyền Micrô", bật MeowLaoshi, rồi bấm Thử lại.';
                    ctl.setActions([{ label: 'Mở cài đặt quyền Micrô', kind: 'secondary', onClick: () => openMicPrivacy() }, retry, done]);
                }, 5000);
                const ch = new (tauri().Channel)();
                ch.onmessage = (buf) => {
                    const u8 = buf instanceof ArrayBuffer ? new Uint8Array(buf) : new Uint8Array(buf);
                    watch.feed(u8);
                    const s = new Int16Array(u8.buffer, u8.byteOffset, u8.byteLength >> 1);
                    let sum = 0;
                    for (let i = 0; i < s.length; i++) sum += s[i] * s[i];
                    const rms = Math.sqrt(sum / Math.max(1, s.length)) / 32768;
                    const level = Math.min(1, Math.max(0, (20 * Math.log10(rms + 1e-6) + 60) / 50));
                    fill.style.width = `${(level * 100).toFixed(0)}%`;
                    if (level > 0.45 && ++peak > 3 && status.dataset.ok !== '1') {
                        status.dataset.ok = '1';
                        status.className = 'dlg-text dlg-ok';
                        status.textContent = '✓ Micro hoạt động.';
                    }
                };
                try {
                    await tauri().invoke('start_capture', { source: 'microphone', channel: ch });
                    micStop = () => { tauri().invoke('stop_capture').catch(() => {}); micStop = null; };
                } catch (err) {
                    status.className = 'dlg-text dlg-error';
                    status.textContent = 'Không mở được micro. Kiểm tra quyền Micrô rồi thử lại.';
                    ctl.setActions([{ label: 'Mở cài đặt quyền Micrô', kind: 'secondary', onClick: () => openMicPrivacy() }, retry, done]);
                }
            };
            startMic();
        }

        ctl = openDialog({ icon: '🐱', title: '', body: [], wide: true, dismissible: false });
        show(0);
    });
}
