/* Code Arena — shared browser runtime.
   The browser never decides game state: it renders server snapshots and server events,
   and sends explicit user actions to authorised endpoints. */
'use strict';
(function () {
  const CA = {};
  CA.offset = 0; // server clock - client clock (ms)
  CA.serverNow = () => Date.now() + CA.offset;

  const ESC = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
  CA.esc = (s) => String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ESC[c]);

  CA.fmtTime = (ms) => (ms ? new Date(ms).toLocaleTimeString('id-ID', { hour12: false }) : '-');
  CA.fmtDur = (ms) => {
    if (ms == null || Number.isNaN(ms)) return '--:--';
    const neg = ms < 0;
    const s = Math.ceil(Math.abs(ms) / 1000);
    const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), sec = s % 60;
    const body = (h > 0 ? h + ':' + String(m).padStart(2, '0') : String(m).padStart(2, '0')) + ':' + String(sec).padStart(2, '0');
    return (neg ? '+' : '') + body;
  };

  CA.api = async (url, body) => {
    const r = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body == null ? {} : body),
      credentials: 'same-origin',
    });
    let data = null;
    try { data = await r.json(); } catch (_) { /* non-JSON */ }
    if (!r.ok || (data && data.ok === false)) {
      if (r.status === 401) throw new Error((data && data.error) || 'Sesi login berakhir. Silakan login ulang.');
      throw new Error((data && data.error) || 'Permintaan gagal (HTTP ' + r.status + ')');
    }
    return data || {};
  };

  CA.html = async (url) => {
    const r = await fetch(url, { credentials: 'same-origin' });
    if (!r.ok) {
      let msg = 'HTTP ' + r.status;
      try { const j = await r.json(); msg = j.error || msg; } catch (_) { /* ignore */ }
      throw new Error(msg);
    }
    return r.text();
  };

  CA.toast = (msg, kind) => {
    const box = document.getElementById('toasts');
    if (!box) return;
    const el = document.createElement('div');
    el.className = 'toast ' + (kind || '');
    el.textContent = msg;
    box.appendChild(el);
    setTimeout(() => el.remove(), kind === 'error' ? 7000 : 3500);
  };

  CA.badge = (st, extra) => {
    if (!st) return '<span class="badge tone-idle"><span class="ico">○</span>-</span>';
    return '<span class="badge tone-' + CA.esc(st.tone) + ' ' + (extra || '') + '"><span class="ico">' + CA.esc(st.icon) + '</span>' + CA.esc(st.label) + '</span>';
  };

  /* Turn <pre class="code" data-ln> into numbered lines (text is already HTML-escaped by the server). */
  CA.lineNumbers = (root) => {
    (root || document).querySelectorAll('pre.code[data-ln]:not([data-done])').forEach((pre) => {
      const lines = pre.textContent.replace(/\n$/, '').split('\n');
      pre.innerHTML = lines.map((l) => '<span class="ln">' + CA.esc(l) + '</span>').join('');
      pre.setAttribute('data-done', '1');
    });
  };

  CA.store = {
    get(k, d) { try { const v = localStorage.getItem(k); return v == null ? d : v; } catch (_) { return d; } },
    set(k, v) { try { localStorage.setItem(k, v); } catch (_) { /* storage unavailable */ } },
  };

  CA.tabs = (root, key, onChange) => {
    const buttons = root.querySelectorAll('[data-tab]');
    const panels = document.querySelectorAll('[data-panel]');
    const activate = (name) => {
      let found = false;
      buttons.forEach((b) => { const on = b.dataset.tab === name; b.classList.toggle('active', on); if (on) found = true; });
      if (!found) return;
      panels.forEach((p) => p.classList.toggle('active', p.dataset.panel === name));
      CA.store.set('tab:' + key, name);
      if (onChange) onChange(name);
    };
    buttons.forEach((b) => b.addEventListener('click', () => activate(b.dataset.tab)));
    const saved = location.hash ? location.hash.slice(1) : CA.store.get('tab:' + key, null);
    if (saved) activate(saved);
    return activate;
  };

  /* Purely cosmetic count-up; the final value is always the exact server number. */
  CA.animateNumber = (el, to) => {
    if (!el) return;
    const target = Number(to) || 0;
    const from = Number(el.dataset.v != null ? el.dataset.v : el.textContent) || 0;
    el.dataset.v = String(target);
    if (from === target || window.matchMedia('(prefers-reduced-motion: reduce)').matches) { el.textContent = String(target); return; }
    const start = performance.now();
    const dur = 550;
    const step = (now) => {
      const p = Math.min(1, (now - start) / dur);
      if (el.dataset.v !== String(target)) return; // superseded by a newer value
      el.textContent = String(Math.round(from + (target - from) * (1 - Math.pow(1 - p, 3))));
      if (p < 1) requestAnimationFrame(step); else el.textContent = String(target);
    };
    requestAnimationFrame(step);
  };

  CA.serializeForm = (form) => {
    const out = {};
    form.querySelectorAll('input[name], select[name], textarea[name]').forEach((el) => {
      const type = el.dataset.type;
      const name = el.name;
      if (el.type === 'checkbox') {
        if (type === 'intlist') {
          if (!Array.isArray(out[name])) out[name] = [];
          if (el.checked) out[name].push(parseInt(el.value, 10));
        } else {
          out[name] = el.checked;
        }
        return;
      }
      if (type === 'int') {
        const v = el.value.trim();
        out[name] = v === '' ? null : parseInt(v, 10);
        return;
      }
      out[name] = el.value;
    });
    return out;
  };

  CA.applyFilters = (root) => {
    (root || document).querySelectorAll('.filterbar[data-filter-group]').forEach((bar) => {
      const group = bar.dataset.filterGroup;
      const active = bar.querySelector('.filter-chip.active');
      const f = active ? active.dataset.f : 'all';
      document.querySelectorAll('[data-filter-list="' + group + '"]').forEach((list) => {
        list.querySelectorAll('[data-filter]').forEach((item) => {
          item.classList.toggle('hidden', !(f === 'all' || item.dataset.filter === f));
        });
      });
    });
  };

  /* Replace a server-rendered fragment while keeping checkbox state and open <details>. */
  CA.refreshFrag = async (el, url) => {
    if (!el) return;
    const checked = new Map();
    el.querySelectorAll('input[type=checkbox]').forEach((i) => checked.set(i.name + '=' + i.value, i.checked));
    const opened = [];
    el.querySelectorAll('details').forEach((d, i) => { if (d.open) opened.push(i); });
    const filters = {};
    el.querySelectorAll('.filterbar[data-filter-group] .filter-chip.active').forEach((c) => { filters[c.closest('.filterbar').dataset.filterGroup] = c.dataset.f; });
    try {
      const html = await CA.html(url);
      el.innerHTML = html;
    } catch (e) {
      CA.toast('Gagal memuat ulang: ' + e.message, 'error');
      return;
    }
    el.querySelectorAll('input[type=checkbox]').forEach((i) => {
      const k = i.name + '=' + i.value;
      if (checked.has(k)) i.checked = checked.get(k);
    });
    const ds = el.querySelectorAll('details');
    opened.forEach((i) => { if (ds[i]) ds[i].open = true; });
    Object.keys(filters).forEach((g) => {
      const bar = el.querySelector('.filterbar[data-filter-group="' + g + '"]');
      if (!bar) return;
      bar.querySelectorAll('.filter-chip').forEach((c) => c.classList.toggle('active', c.dataset.f === filters[g]));
    });
    CA.lineNumbers(el);
    CA.applyFilters(el);
    CA.formatTimestamps(el);
    document.dispatchEvent(new CustomEvent('ca:frag', { detail: { el } }));
  };

  CA.formatTimestamps = (root) => {
    (root || document).querySelectorAll('[data-ts]').forEach((el) => {
      const ts = Number(el.dataset.ts);
      if (ts) el.textContent = CA.fmtTime(ts);
    });
  };

  /* Enable/disable phase-gated controls. The server enforces the same rules; this is only UX. */
  CA.gate = (phase, accepting, label) => {
    document.querySelectorAll('[data-phases]').forEach((box) => {
      const allowed = box.dataset.phases.split(' ').includes(phase) && accepting;
      box.querySelectorAll('input, textarea, select, button').forEach((c) => {
        if (c.dataset.nogate) return;
        if (c.type === 'hidden') return;
        if (c.tagName === 'TEXTAREA' || c.tagName === 'INPUT') c.readOnly = !allowed; else c.disabled = !allowed;
      });
      box.classList.toggle('phase-gate', !allowed);
      box.querySelectorAll('.phase-hint').forEach((h) => {
        h.textContent = allowed ? '' : 'Tersedia pada fase: ' + box.dataset.phases.split(' ').map((p) => (label && label[p]) || p).join(', ') + (accepting ? '' : ' (ronde tidak menerima aksi saat ini)');
      });
    });
  };

  async function runAction(el, url, body) {
    if (el.dataset.confirm && !window.confirm(el.dataset.confirm)) return;
    if (el.dataset.prompt) {
      const [field, ...rest] = el.dataset.prompt.split(':');
      const v = window.prompt(rest.join(':') || field);
      if (v == null) return;
      body[field] = v;
    }
    const buttons = el.tagName === 'FORM' ? el.querySelectorAll('button') : [el];
    buttons.forEach((b) => { b.dataset.wasDisabled = b.disabled ? '1' : ''; b.disabled = true; });
    try {
      const result = await CA.api(url, body);
      CA.toast(el.dataset.ok || 'Berhasil.', 'ok');
      if (el.dataset.reset && el.tagName === 'FORM') el.reset();
      if (el.dataset.reload) { setTimeout(() => location.reload(), 400); }
      document.dispatchEvent(new CustomEvent('ca:done', { detail: { el, url, body, result, refresh: el.dataset.refresh || '' } }));
    } catch (e) {
      CA.toast(e.message, 'error');
    } finally {
      buttons.forEach((b) => { b.disabled = b.dataset.wasDisabled === '1'; });
    }
  }

  document.addEventListener('submit', (e) => {
    const form = e.target.closest('form[data-api]');
    if (!form) return;
    e.preventDefault();
    runAction(form, form.dataset.api, CA.serializeForm(form));
  });

  document.addEventListener('click', (e) => {
    const btn = e.target.closest('button[data-api]');
    if (btn) {
      e.preventDefault();
      let body = {};
      try { body = JSON.parse(btn.dataset.body || '{}'); } catch (_) { /* keep empty */ }
      runAction(btn, btn.dataset.api, body);
      return;
    }
    const chip = e.target.closest('.filterbar .filter-chip');
    if (chip) {
      chip.parentElement.querySelectorAll('.filter-chip').forEach((c) => c.classList.toggle('active', c === chip));
      CA.applyFilters(chip.closest('.filterbar').parentElement || document);
      CA.applyFilters(document);
    }
    if (e.target.closest('#logout-btn')) {
      CA.api('/api/logout', {}).finally(() => { location.href = '/'; });
    }
  });

  /* ---------------------------------------------------------------- WebSocket */
  function setConn(state) {
    const el = document.getElementById('conn');
    if (el) {
      el.className = 'conn ' + (state === 'ok' ? 'ok' : state === 'bad' ? 'bad' : '');
      el.textContent = state === 'ok' ? 'LIVE' : state === 'bad' ? 'RECONNECTING' : 'connecting';
    }
    let banner = document.getElementById('reconnect-banner');
    if (state === 'bad') {
      if (!banner) {
        banner = document.createElement('div');
        banner.id = 'reconnect-banner';
        banner.className = 'banner-reconnect';
        banner.setAttribute('role', 'status');
        banner.textContent = 'RECONNECTING — data akan dimuat ulang dari server setelah tersambung';
        document.body.appendChild(banner);
      }
    } else if (banner) {
      banner.remove();
    }
  }

  /* Opens /ws?view=..., applies the snapshot then incremental events with v > snapshot.v.
     On reconnect the server sends a fresh snapshot, so missed events never need replaying. */
  CA.connect = (view, handlers) => {
    let ws = null;
    let lastV = 0;
    let retry = 0;
    let everOpen = false;
    let snapshots = 0;
    const api = {
      resync() { if (ws && ws.readyState === 1) ws.send('resync'); },
      get lastV() { return lastV; },
    };
    function open() {
      setConn(everOpen ? 'bad' : 'connecting');
      const proto = location.protocol === 'https:' ? 'wss' : 'ws';
      ws = new WebSocket(proto + '://' + location.host + '/ws?view=' + encodeURIComponent(view));
      ws.onopen = () => { retry = 0; everOpen = true; };
      ws.onmessage = (m) => {
        let msg;
        try { msg = JSON.parse(m.data); } catch (_) { return; }
        if (msg.now) CA.offset = msg.now - Date.now();
        if (msg.t === 'snapshot') {
          lastV = msg.v;
          setConn('ok');
          snapshots += 1;
          try { handlers.onSnapshot(msg.data, msg); } catch (e) { console.error(e); }
          if (snapshots > 1 && window.CAChat) window.CAChat.reload();
        } else if (msg.t === 'ev') {
          if (msg.v <= lastV) return; // already reflected in the snapshot
          lastV = msg.v;
          if (msg.kind === 'force_logout') {
            CA.toast((msg.data && msg.data.reason) || 'Sesi berakhir.', 'error');
            setTimeout(() => { location.href = '/'; }, 1800);
            return;
          }
          try { handlers.onEvent && handlers.onEvent(msg.kind, msg.data, msg); } catch (e) { console.error(e); }
        } else if (msg.t === 'error') {
          CA.toast(msg.message || 'Kesalahan server', 'error');
        }
      };
      ws.onclose = () => {
        setConn('bad');
        retry += 1;
        setTimeout(open, Math.min(8000, 400 * Math.pow(2, Math.min(retry, 5))));
      };
      ws.onerror = () => { /* onclose handles retry */ };
    }
    open();
    return api;
  };

  CA.debounce = (fn, ms) => {
    let t = null;
    return (...args) => { clearTimeout(t); t = setTimeout(() => fn(...args), ms); };
  };

  document.addEventListener('DOMContentLoaded', () => {
    CA.lineNumbers(document);
    CA.applyFilters(document);
    CA.formatTimestamps(document);
  });

  window.CA = CA;
})();
