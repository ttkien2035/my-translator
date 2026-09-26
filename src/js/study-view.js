// Study view — the Library's "ôn bài" screen for one saved session.
//
// Reads the session into its own SessionStore (SessionStore.resume), lets the
// student toggle ⭐ ❓ 📝 on any sentence and keep writing notes after class,
// and writes back through the same atomic save_session path as live capture
// (JSON + Markdown). The session currently open in Live is shown read-only so
// two stores never write the same file.
//
// Cost model: rows are built once per open with textContent (no innerHTML,
// no per-row listeners — one delegated handler), rows use CSS
// content-visibility so a 3-hour session (~2 000 sentences) only lays out what
// is on screen, and filtering/search only toggles `hidden`.

import { SessionStore } from './session-store.js';

const MARKS = [
    { mark: '⭐', title: 'Quan trọng' },
    { mark: '❓', title: 'Chưa hiểu' },
    { mark: '📝', title: 'Sẽ thi' },
];
const SAVE_DEBOUNCE_MS = 600;
const NOTES_DEBOUNCE_MS = 800;
const SEARCH_DEBOUNCE_MS = 150;

export class StudyView {
    /**
     * @param {object} deps
     * @param {(msg: string, kind: string) => void} deps.toast
     */
    constructor({ toast }) {
        this.toast = toast;
        this.store = null;       // SessionStore of the open session (null when closed)
        this.readOnly = false;
        this.rows = [];          // [{ el, seg, hay }] in document order
        this.filter = 'all';
        this.query = '';
        this._saveTimer = null;
        this._notesTimer = null;
        this._searchTimer = null;

        this.listEl = document.getElementById('study-list');
        this.notesEl = document.getElementById('study-notes-text');
        this.searchEl = document.getElementById('study-search');
        this.countEl = document.getElementById('study-count');
        this.saveStateEl = document.getElementById('study-save-state');
        this.bannerEl = document.getElementById('study-readonly-note');
        this._bind();
    }

    _bind() {
        // One delegated handler for every row: mark buttons toggle, a click on
        // the text of a filtered row jumps to it in the full transcript.
        this.listEl?.addEventListener('click', (e) => {
            const row = e.target.closest('.study-row');
            if (!row) return;
            const btn = e.target.closest('.study-mark-btn');
            if (btn) {
                this._toggleMark(row, btn.dataset.mark);
                return;
            }
            if (this.filter !== 'all' || this.query) this._jumpTo(row);
        });

        document.querySelectorAll('.study-filter').forEach((b) => {
            b.addEventListener('click', () => this._setFilter(b.dataset.filter));
        });

        this.searchEl?.addEventListener('input', () => {
            clearTimeout(this._searchTimer);
            this._searchTimer = setTimeout(() => {
                this.query = this.searchEl.value.trim().toLowerCase();
                this._applyVisibility();
            }, SEARCH_DEBOUNCE_MS);
        });

        this.notesEl?.addEventListener('input', () => {
            if (this.readOnly || !this.store) return;
            this._setSaveState('pending');
            clearTimeout(this._notesTimer);
            this._notesTimer = setTimeout(() => this._commitNotes(), NOTES_DEBOUNCE_MS);
        });
        this.notesEl?.addEventListener('blur', () => this._commitNotes());
    }

    /**
     * Open a saved session. `liveStore` is the app's live SessionStore: when it
     * holds this very session, show its in-memory state read-only.
     */
    async open(id, liveStore) {
        await this.close();
        const isLive = liveStore && liveStore.id === id;
        this.readOnly = !!isLive;
        this.store = isLive ? liveStore : await SessionStore.resume(id);
        if (!isLive) this.store._lastPersistAt = Date.now(); // freshly loaded = clean

        this.filter = 'all';
        this.query = '';
        if (this.searchEl) this.searchEl.value = '';
        document.querySelectorAll('.study-filter').forEach((b) => {
            b.classList.toggle('active', b.dataset.filter === 'all');
        });
        if (this.bannerEl) this.bannerEl.style.display = this.readOnly ? '' : 'none';
        if (this.notesEl) {
            this.notesEl.value = this.store.notes || '';
            this.notesEl.readOnly = this.readOnly;
        }
        this._render();
        this._setSaveState('clean');
        return this.store;
    }

    /** Write any pending edit and forget the session. Safe to call twice. */
    async close() {
        await this.flush();
        this.store = null;
        this.rows = [];
        if (this.listEl) this.listEl.textContent = '';
    }

    /** Persist pending notes/marks now (before export, on exit, on close). */
    async flush() {
        clearTimeout(this._saveTimer);
        this._saveTimer = null;
        if (!this.store || this.readOnly) return;
        this._commitNotes(false);
        const r = await this.store.persist();
        this._setSaveState(r === 'failed' ? 'failed' : 'clean');
    }

    /** Keep the open store in step with a rename done elsewhere. */
    setTitle(id, title) {
        if (this.store && this.store.id === id) this.store.title = title;
    }

    /** Markdown of the open session (for Copy). */
    markdown() {
        return this.store ? this.store._toMarkdown() : '';
    }

    // ─── Rendering ───────────────────────────────────────────────

    _render() {
        if (!this.listEl) return;
        const frag = document.createDocumentFragment();
        this.rows = [];
        const chunks = this.store._allChunks();
        chunks.forEach((chunk, ci) => {
            if (ci > 0 && chunk.started_at) {
                const sep = document.createElement('div');
                sep.className = 'study-chunk-sep';
                sep.textContent = `Tiếp tục lúc ${this.store._formatDateTime(chunk.started_at).slice(11)}`;
                frag.appendChild(sep);
            }
            chunk.segments.forEach((seg) => {
                const el = this._row(seg);
                frag.appendChild(el);
                this.rows.push({ el, seg, hay: `${seg.tgt || ''}\n${seg.src || ''}`.toLowerCase() });
            });
        });
        if (this.rows.length === 0) {
            const empty = document.createElement('p');
            empty.className = 'study-empty';
            empty.textContent = 'Buổi này chưa có câu nào.';
            frag.appendChild(empty);
        }
        this.listEl.textContent = '';
        this.listEl.appendChild(frag);
        this.listEl.classList.toggle('read-only', this.readOnly);
        this.listEl.scrollTop = 0;
        this._applyVisibility();
    }

    _row(seg) {
        const row = document.createElement('div');
        row.className = 'study-row';
        if (seg.mark) row.dataset.mark = seg.mark;

        const ts = document.createElement('span');
        ts.className = 'study-ts';
        ts.textContent = seg.ts || '';

        const text = document.createElement('div');
        text.className = 'study-text';
        const tgt = document.createElement('div');
        tgt.className = 'study-tgt';
        tgt.textContent = seg.tgt || '';
        text.appendChild(tgt);
        if (seg.src) {
            const src = document.createElement('div');
            src.className = 'study-src';
            src.lang = 'zh-Hans'; // PingFang SC for Chinese source text
            src.textContent = seg.src;
            text.appendChild(src);
        }

        const marks = document.createElement('div');
        marks.className = 'study-marks';
        for (const m of MARKS) {
            const b = document.createElement('button');
            b.type = 'button';
            b.className = 'study-mark-btn';
            b.dataset.mark = m.mark;
            b.title = m.title;
            b.textContent = m.mark;
            b.disabled = this.readOnly;
            marks.appendChild(b);
        }

        row.append(ts, text, marks);
        return row;
    }

    _applyVisibility() {
        let shown = 0;
        for (const r of this.rows) {
            const okFilter = this.filter === 'all' || r.seg.mark === this.filter;
            const okQuery = !this.query || r.hay.includes(this.query);
            const visible = okFilter && okQuery;
            if (r.el.hidden === visible) r.el.hidden = !visible;
            if (visible) shown++;
        }
        const filtering = this.filter !== 'all' || !!this.query;
        this.listEl?.querySelectorAll('.study-chunk-sep').forEach((s) => { s.hidden = filtering; });
        if (this.countEl) {
            this.countEl.textContent = filtering
                ? `${shown}/${this.rows.length} câu`
                : `${this.rows.length} câu`;
        }
    }

    _setFilter(f) {
        this.filter = f;
        document.querySelectorAll('.study-filter').forEach((b) => {
            b.classList.toggle('active', b.dataset.filter === f);
        });
        this._applyVisibility();
    }

    /** Back to the full transcript, centred on `row`, briefly highlighted. */
    _jumpTo(row) {
        this.query = '';
        if (this.searchEl) this.searchEl.value = '';
        this._setFilter('all');
        row.scrollIntoView({ block: 'center' });
        row.classList.add('flash');
        setTimeout(() => row.classList.remove('flash'), 900);
    }

    // ─── Edits ───────────────────────────────────────────────────

    _toggleMark(row, mark) {
        if (this.readOnly || !this.store) return;
        const r = this.rows.find((x) => x.el === row);
        if (!r) return;
        if (r.seg.mark === mark) delete r.seg.mark; else r.seg.mark = mark;
        if (r.seg.mark) row.dataset.mark = r.seg.mark; else delete row.dataset.mark;
        this.store._mutations++;
        if (this.filter !== 'all') this._applyVisibility();
        this._scheduleSave();
    }

    _commitNotes(schedule = true) {
        if (this.readOnly || !this.store || !this.notesEl) return;
        clearTimeout(this._notesTimer);
        this._notesTimer = null;
        const v = this.notesEl.value;
        if (v === (this.store.notes || '')) return;
        this.store.notes = v;
        this.store._mutations++;
        if (schedule) this._scheduleSave();
    }

    _scheduleSave() {
        this._setSaveState('pending');
        clearTimeout(this._saveTimer);
        this._saveTimer = setTimeout(async () => {
            this._saveTimer = null;
            if (!this.store) return;
            const r = await this.store.persist();
            if (r === 'failed') {
                this._setSaveState('failed');
                this.toast('Không lưu được — thay đổi vẫn giữ, sẽ thử lại', 'error');
            } else {
                this._setSaveState('clean');
            }
        }, SAVE_DEBOUNCE_MS);
    }

    _setSaveState(state) {
        if (!this.saveStateEl) return;
        this.saveStateEl.textContent = this.readOnly ? 'chỉ xem'
            : state === 'pending' ? 'đang lưu…'
            : state === 'failed' ? 'lỗi lưu'
            : 'đã lưu';
        this.saveStateEl.dataset.state = this.readOnly ? 'readonly' : state;
    }
}
