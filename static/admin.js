/* Instructor dashboard. Sections are server-rendered fragments; this script refreshes them when
   the server reports a relevant change and wires dialogs. Every action is authorised server-side. */
'use strict';
(function () {
  const $ = (id) => document.getElementById(id);
  let arena = null;
  let waiting = null;
  let firstSnapshot = true;

  const fragEl = (name) => document.querySelector('[data-admin-frag="' + name + '"]');
  const pending = new Set();
  const flush = CA.debounce(() => {
    const names = Array.from(pending);
    pending.clear();
    names.forEach(async (n) => {
      await CA.refreshFrag(fragEl(n), '/admin/frag/' + n);
      if (n === 'chat') mountChat();
      if (n === 'review') updateReviewCount();
    });
  }, 350);
  function refresh(names) {
    names.split(/[\s,]+/).filter(Boolean).forEach((n) => { if (fragEl(n)) pending.add(n); });
    flush();
  }

  function mountChat() {
    const c = $('admin-chat');
    if (c) CAChat.mount(c);
  }
  mountChat();

  function updateReviewCount() {
    const el = document.querySelector('[data-review-count]');
    const n = el ? Number(el.dataset.reviewCount) : 0;
    const nav = $('nav-review-count');
    if (nav) { nav.textContent = n; nav.classList.toggle('hidden', !n); }
  }
  updateReviewCount();

  function renderHeader() {
    if (!arena) return;
    const r = arena.round;
    $('h-round').textContent = r ? 'Ronde ' + r.number + ' · ' + r.challenge.id + (r.state === 'setup' ? ' (setup)' : r.finalized ? ' (final)' : '') : 'Belum ada ronde';
    const ph = $('h-phase');
    if (r) {
      ph.className = 'badge tone-' + (r.paused ? 'review' : r.phase_tone);
      ph.innerHTML = '<span class="ico">' + (r.paused ? '❚❚' : '●') + '</span>' + CA.esc(r.phase_label) + (r.paused ? ' · PAUSED' : '');
    } else {
      ph.className = 'badge tone-idle';
      ph.innerHTML = '<span class="ico">○</span>-';
    }
    const s = arena.stats.summary;
    $('h-joined').textContent = s.participants;
    $('h-online').textContent = s.online;
    $('h-pending').textContent = s.pending_reports;
    $('h-patches').textContent = Math.max(0, s.submitted_patches - s.verified_patches);
  }

  function clock() {
    const r = arena && arena.round;
    const el = $('h-clock');
    let txt = '--:--', cls = 'clock';
    if (r && r.state === 'running') {
      if (r.paused) { txt = CA.fmtDur(r.remaining_ms); cls += ' paused'; }
      else if (r.deadline != null) {
        const left = r.deadline - CA.serverNow();
        txt = CA.fmtDur(left);
        cls += left < 0 ? ' over' : left < 60000 ? ' warn' : '';
      } else if (r.phase_started_at) { txt = '⏱ ' + CA.fmtDur(CA.serverNow() - r.phase_started_at); }
    }
    el.textContent = txt;
    el.className = cls;
    el.style.fontSize = '1.6rem';
    document.querySelectorAll('[data-admin-clock]').forEach((c) => { c.textContent = txt; });
  }
  setInterval(clock, 500);

  function markPresence(nim, online) {
    document.querySelectorAll('[data-admin-frag] [data-nim="' + nim + '"] .dot').forEach((d) => d.classList.toggle('on', online));
  }

  CA.connect('admin', {
    onSnapshot(data) {
      arena = data.arena;
      waiting = data.waiting;
      renderHeader();
      if (!firstSnapshot) refresh('teams round scores review session');
      firstSnapshot = false;
    },
    onEvent(kind, data) {
      switch (kind) {
        case 'round':
          if (arena) { arena.round = data.round; renderHeader(); }
          refresh('round');
          break;
        case 'stats':
          if (arena) { arena.stats = data; renderHeader(); }
          break;
        case 'scores':
          refresh('scores');
          break;
        case 'review_queue':
          refresh('review');
          break;
        case 'admin_teams':
          refresh('teams');
          break;
        case 'participant':
          refresh('teams roster');
          break;
        case 'presence':
          markPresence(data.nim, data.online);
          if (arena && typeof data.online_count === 'number') { arena.stats.summary.online = data.online_count; renderHeader(); }
          break;
        case 'team_status':
          break;
        default:
          if (kind.startsWith('chat') || kind === 'lobby_locked') CAChat.onEvent(kind, data);
      }
    },
  });

  document.addEventListener('ca:done', (e) => {
    if (e.detail.refresh) refresh(e.detail.refresh);
    const res = e.detail.result || {};
    if (res.preview) CA.toast(res.preview.length + ' kelompok draft dibuat dari snapshot peserta.', 'ok');
    if (res.invalid && res.invalid.length) CA.toast('Baris tidak valid: ' + res.invalid.join(', '), 'error');
    if (res.placed) CA.toast(res.placed.length + ' peserta ditempatkan.', 'ok');
  });

  document.addEventListener('click', (e) => {
    const rf = e.target.closest('[data-refresh-frag]');
    if (rf) { refresh(rf.dataset.refreshFrag); return; }
    const rr = e.target.closest('[data-review-report]');
    if (rr) { openReportReview(JSON.parse(rr.dataset.reviewReport), rr.dataset.title); return; }
    const rp = e.target.closest('[data-review-patch]');
    if (rp) { openPatchReview(JSON.parse(rp.dataset.reviewPatch), rp.dataset.title); }
  });

  /* pairing editor */
  document.addEventListener('submit', async (e) => {
    const form = e.target.closest('form[data-custom="pairing"]');
    if (!form) return;
    e.preventDefault();
    const pairs = Array.from(form.querySelectorAll('select[data-attacker]')).map((s) => ({ attacker: Number(s.dataset.attacker), target: Number(s.value) }));
    try {
      await CA.api('/api/admin/round/pairing', { round_id: Number(form.dataset.round), pairs });
      CA.toast('Pairing disimpan', 'ok');
      refresh('round');
    } catch (err) { CA.toast(err.message, 'error'); }
  });

  /* report review dialog */
  const dr = $('dlg-review-report');
  const fr = $('form-review-report');
  let currentReport = null;
  function syncReportFields() {
    const v = fr.querySelector('[name=verdict]').value;
    $('rr-rubric').classList.toggle('hidden', v !== 'valid');
    $('rr-dup').classList.toggle('hidden', v !== 'duplicate');
    $('rr-bonus').classList.toggle('hidden', !(v === 'invalid' && currentReport && currentReport.rebuttal));
  }
  fr.querySelector('[name=verdict]').addEventListener('change', syncReportFields);
  function openReportReview(r, title) {
    currentReport = r;
    $('rr-title').textContent = 'Review laporan ' + (title || '#' + r.id);
    fr.querySelector('[name=report_id]').value = r.id;
    const reviewed = ['valid', 'invalid', 'duplicate', 'needs_revision'].includes(r.status);
    fr.querySelector('[name=verdict]').value = reviewed ? r.status : (r.possible_duplicate ? 'duplicate' : 'valid');
    fr.querySelector('[name=repro]').value = reviewed ? r.repro : 10;
    fr.querySelector('[name=rootcause]').value = reviewed ? r.rootcause : 10;
    fr.querySelector('[name=evidence]').value = reviewed ? r.evidence_score : 5;
    fr.querySelector('[name=duplicate_of]').value = r.duplicate_of || r.possible_duplicate || '';
    fr.querySelector('[name=rebuttal_bonus]').value = r.rebuttal_bonus || 0;
    fr.querySelector('[name=note]').value = r.review_note || '';
    $('rr-hint').textContent = (r.possible_duplicate ? 'Server mendeteksi kemungkinan duplikat dari #' + r.possible_duplicate + ' (lokasi/judul sama). ' : '') +
      (r.rebuttal ? 'Defender mengajukan rebuttal — bonus maks 10 jika laporan memang tidak valid. ' : '') +
      'Valid = 10/10/5 maks. Hanya 2 temuan valid terbaik per tim per ronde yang dihitung.';
    syncReportFields();
    dr.showModal();
  }
  $('rr-save').addEventListener('click', async () => {
    const body = CA.serializeForm(fr);
    ['repro', 'rootcause', 'evidence', 'rebuttal_bonus'].forEach((k) => { if (body[k] == null) body[k] = 0; });
    try {
      await CA.api('/api/admin/review/report', body);
      dr.close();
      CA.toast('Review laporan #' + body.report_id + ' disimpan', 'ok');
      refresh('review scores');
    } catch (err) { CA.toast(err.message, 'error'); }
  });

  /* patch review dialog */
  const dp = $('dlg-review-patch');
  const fp = $('form-review-patch');
  function openPatchReview(p, title) {
    $('rp-title').textContent = 'Review ' + (title || 'patch #' + p.id);
    fp.querySelector('[name=patch_id]').value = p.id;
    const reviewed = p.status === 'accepted' || p.status === 'rejected';
    fp.querySelector('[name=verdict]').value = reviewed ? p.status : 'accepted';
    fp.querySelector('[name=rootfix]').value = reviewed ? p.rootfix : 20;
    fp.querySelector('[name=regression]').value = reviewed ? p.regression : 15;
    fp.querySelector('[name=edge]').value = reviewed ? p.edge : 10;
    fp.querySelector('[name=explain]').value = reviewed ? p.explain : 5;
    fp.querySelector('[name=note]').value = p.review_note || '';
    dp.showModal();
  }
  $('rp-save').addEventListener('click', async () => {
    const body = CA.serializeForm(fp);
    ['rootfix', 'regression', 'edge', 'explain'].forEach((k) => { if (body[k] == null) body[k] = 0; });
    try {
      await CA.api('/api/admin/review/patch', body);
      dp.close();
      CA.toast('Review patch disimpan', 'ok');
      refresh('review scores');
    } catch (err) { CA.toast(err.message, 'error'); }
  });
})();
