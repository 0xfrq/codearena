/* Team dashboard. Displays server state; every action goes to an authorised endpoint and the
   server decides phase/deadline validity. Statuses change only through explicit actions. */
'use strict';
(function () {
  const boot = JSON.parse(document.getElementById('boot').textContent);
  const $ = (id) => document.getElementById(id);
  let arena = null;
  let waiting = null;
  let dirty = false;
  let lastPhase = null;

  const PHASE_LABEL = {
    BRIEFING: 'Briefing', DEFENSE_PREP: 'Defense Preparation', ATTACK_WINDOW: 'Attack Window',
    DEFENSE_RESPONSE: 'Defense Response', VERIFICATION: 'Verification', RESULTS: 'Round Results', SETUP: 'Persiapan',
  };
  const GUIDE = {
    BRIEFING: 'Baca challenge, spesifikasi, dan rubric. Editor pertahanan dibuka pada fase berikutnya.',
    DEFENSE_PREP: 'Buka tab Defense Prep: perbaiki starter code dan SIMPAN. Versi terakhir yang disimpan akan dikunci sebagai snapshot saat Attack Window dibuka.',
    ATTACK_WINDOW: 'Tab Attack: inspeksi locked snapshot target dan kirim attack report. Tab Defense: pantau laporan masuk dan mulai patch draft.',
    DEFENSE_RESPONSE: 'Laporan sudah dikunci. Selesaikan patch + regression test lalu submit sebelum waktu habis. Ajukan rebuttal jika laporan tidak valid.',
    VERIFICATION: 'Instruktur memvalidasi laporan dan meninjau patch. Hasil review muncul di tab Attack dan Defense.',
    RESULTS: 'Skor ronde dihitung. Tunggu instruktur mengesahkan hasil dan membuka pembahasan.',
  };

  const activateTab = CA.tabs(document.querySelector('[data-tabs="team"]'), 'team-' + boot.team_id);
  CAChat.mount($('chat'));

  document.querySelectorAll('#prep-form, #report-form, #patch-form').forEach((f) => f.addEventListener('input', () => { dirty = true; }));

  function myTeam() {
    return arena && arena.teams.find((t) => t.id === boot.team_id);
  }
  function teamName(id) {
    const t = arena && arena.teams.find((x) => x.id === id);
    return t ? t.name : '-';
  }
  function effectiveRound() {
    const r = arena && arena.round;
    return r && r.state !== 'setup' ? r : null;
  }
  function accepting(r) {
    if (!r || r.state !== 'running' || r.paused) return false;
    return r.deadline == null || CA.serverNow() <= r.deadline;
  }

  /* Reload only when the page identity changed (new round, team change); never discard edits silently. */
  function checkStructure() {
    if (!waiting) return;
    if (!waiting.my_team) { location.href = '/wait'; return; }
    const r = effectiveRound();
    const roundId = r ? r.id : 0;
    const me = myTeam();
    const target = me && me.target_id ? teamName(me.target_id) : '';
    const attacker = me && me.attacker_id ? teamName(me.attacker_id) : '';
    const changed = waiting.my_team.id !== boot.team_id || roundId !== boot.round_id ||
      (roundId && (target !== boot.target || attacker !== boot.attacker)) || waiting.my_team.name !== boot.team_name;
    if (r && !!r.discussion_open !== !!boot.discussion) {
      boot.discussion = !!r.discussion_open;
      refresh(['challenge']);
    }
    if (r && r.state !== boot.round_state) {
      boot.round_state = r.state;
      refresh(['reports_out', 'reports_in', 'patches']);
    }
    if (!changed) return;
    if (!dirty) { location.reload(); return; }
    let b = $('reload-banner');
    if (!b) {
      b = document.createElement('div');
      b.id = 'reload-banner';
      b.className = 'notice warn';
      b.style.margin = '0 0 1rem';
      b.innerHTML = 'Ronde atau kelompok berubah. <button class="btn btn-sm" type="button">Muat ulang halaman</button> <span class="small muted">(simpan dulu perubahan Anda)</span>';
      b.querySelector('button').addEventListener('click', () => location.reload());
      document.querySelector('main').prepend(b);
    }
  }

  function render() {
    renderRound();
    renderStatus();
    renderStats();
    renderScores();
    renderMembers();
  }

  function renderRound() {
    const r = effectiveRound();
    if (!r) {
      $('t-phase').innerHTML = '<span class="ico">○</span><span>Belum ada ronde</span>';
      $('o-phase').textContent = 'Menunggu ronde dimulai';
      $('o-guide').textContent = 'Instruktur akan memulai ronde. Halaman ini diperbarui otomatis.';
      CA.gate('NONE', false, PHASE_LABEL);
      return;
    }
    $('t-round').textContent = 'Ronde ' + r.number + ' · ' + r.challenge.label;
    $('t-phase').className = 'badge tone-' + (r.paused ? 'review' : r.phase_tone);
    $('t-phase').innerHTML = '<span class="ico">' + (r.paused ? '❚❚' : '●') + '</span><span>' + CA.esc(r.phase_label) + (r.paused ? ' · PAUSED' : '') + (r.finalized ? ' · FINAL' : '') + '</span>';
    $('o-phase').textContent = r.phase_label + (r.paused ? ' (di-pause)' : '');
    $('o-phase-desc').textContent = r.phase_description;
    $('o-guide').textContent = r.finalized ? 'Ronde telah difinalisasi. Skor resmi tersimpan.' + (r.discussion_open ? ' Pembahasan dibuka — lihat tab Challenge.' : '') : (GUIDE[r.phase] || '');
    $('o-timeline').innerHTML = r.timeline.map((p) =>
      '<div class="row between small" style="padding:.2rem 0;border-bottom:1px solid var(--border)">' +
      '<span>' + (p.state === 'completed' ? '<span class="tone-success">✓</span>' : p.state === 'active' ? '<span class="tone-analysis">●</span>' : '<span class="muted">○</span>') + ' ' + CA.esc(p.label) + '</span>' +
      '<span class="mono muted">' + (p.started_at ? CA.fmtTime(p.started_at) : '') + (p.ended_at ? ' – ' + CA.fmtTime(p.ended_at) : '') + ' · ' + p.submissions + ' submission</span></div>').join('');
    if (lastPhase !== null && lastPhase !== r.phase) {
      CA.toast('Fase berganti: ' + r.phase_label);
      refresh(['prep', 'snapshots', 'reports_out', 'reports_in', 'patches', 'addressed']);
      if (r.phase === 'DEFENSE_PREP') activateTab('prep');
      if (r.phase === 'ATTACK_WINDOW') activateTab('attack');
    }
    lastPhase = r.phase;
    gate();
  }

  function gate() {
    const r = effectiveRound();
    CA.gate(r ? r.phase : 'NONE', accepting(r), PHASE_LABEL);
    // report form follows its own mode (revisions are allowed after the attack window)
    const form = $('report-form');
    if (form && r) {
      const mode = form.querySelector('[name=mode]').value;
      const allowed = accepting(r) && (r.phase === 'ATTACK_WINDOW' || (mode === 'revise' && ['DEFENSE_RESPONSE', 'VERIFICATION'].includes(r.phase)));
      form.querySelectorAll('input:not([type=hidden]), textarea').forEach((i) => { i.readOnly = !allowed; });
      ['report-save', 'report-submit'].forEach((id) => { const b = $(id); if (b) b.disabled = !allowed; });
      $('report-save').classList.toggle('hidden', mode === 'revise');
      form.querySelector('.phase-hint').textContent = allowed ? '' : 'Laporan baru hanya dapat dibuat selama Attack Window.';
    }
  }

  function renderStatus() {
    const t = myTeam();
    const set = (side, st) => {
      $('t-' + side).outerHTML = st ? CA.badge(st, 'big').replace('<span class="badge', '<span id="t-' + side + '" class="badge') : '<span id="t-' + side + '" class="badge big tone-idle"><span class="ico">○</span><span>Belum dimulai</span></span>';
      $('t-' + side + '-reason').textContent = st ? st.reason : '';
      $('t-' + side + '-at').textContent = st ? 'diperbarui ' + CA.fmtTime(st.at) : '';
    };
    set('attack', t && t.attack);
    set('defense', t && t.defense);
    $('t-target').textContent = t && t.target_id ? teamName(t.target_id) : '-';
    $('t-attacker').textContent = t && t.attacker_id ? teamName(t.attacker_id) : '-';
  }

  function renderStats() {
    const s = arena && arena.stats && arena.stats.teams[String(boot.team_id)];
    if (!s) return;
    $('t-open').textContent = s.open_findings;
    $('t-patch').textContent = s.patch ? 'v' + s.patch.version + ' — ' + s.patch.label : '-';
    $('cnt-out').textContent = s.outgoing.official;
    $('cnt-in').textContent = s.incoming.official;
    $('t-out-summary').textContent = '· ' + s.outgoing.official + ' laporan · ' + s.outgoing.pending + ' pending · ' + s.outgoing.valid + ' valid';
  }

  function renderScores() {
    if (!arena) return;
    const mine = arena.scores.find((s) => s.team_id === boot.team_id);
    if (mine) {
      CA.animateNumber($('t-round-score'), mine.round.total);
      CA.animateNumber($('t-total'), mine.cumulative);
      $('t-rank').textContent = '#' + mine.rank;
    }
    $('o-leaderboard').innerHTML = arena.scores.map((s) =>
      '<div class="lb-row"><span class="rank' + (s.rank === 1 ? ' r1' : '') + '">' + s.rank + '</span>' +
      '<span><b>' + CA.esc(teamName(s.team_id)) + '</b>' + (s.team_id === boot.team_id ? ' <span class="pill">tim Anda</span>' : '') +
      '<div class="split">A ' + s.round.attack + ' · D ' + s.round.defense + ' · R ' + s.round.rebuttal + ' · ronde ' + s.round.total + (s.round_finalized ? ' ✓' : ' ◷') + '</div></span>' +
      '<span class="pts">' + s.cumulative + '</span></div>').join('') || '<span class="muted">Belum ada skor.</span>';
  }

  function renderMembers() {
    const t = myTeam();
    if (!t) return;
    t.members.forEach((m) => {
      const el = document.querySelector('#t-members [data-nim="' + m.nim + '"] .dot');
      if (el) el.classList.toggle('on', m.online);
    });
  }

  function clockLoop() {
    const r = effectiveRound();
    const el = $('t-clock');
    const note = $('t-clock-note');
    if (!r || r.finalized) { el.textContent = '--:--'; el.className = 'clock'; note.textContent = r && r.finalized ? 'ronde final' : ''; return; }
    if (r.paused) {
      el.textContent = r.remaining_ms != null ? CA.fmtDur(r.remaining_ms) : 'PAUSED';
      el.className = 'clock paused';
      note.textContent = 'timer dibekukan (pause)';
    } else if (r.deadline != null) {
      const left = r.deadline - CA.serverNow();
      el.textContent = CA.fmtDur(left);
      el.className = 'clock' + (left < 0 ? ' over' : left < 60000 ? ' warn' : '');
      note.textContent = left < 0 ? 'waktu habis — menunggu server menutup fase' : 'batas waktu dari server';
    } else {
      el.textContent = r.phase_started_at ? CA.fmtDur(CA.serverNow() - r.phase_started_at) : '--:--';
      el.className = 'clock';
      note.textContent = 'fase ditutup manual oleh instruktur (waktu berjalan)';
    }
    gate();
  }
  setInterval(clockLoop, 500);

  const FRAG_URL = (name) => '/team/frag/' + name;
  const pending = new Set();
  const flush = CA.debounce(() => {
    const parts = Array.from(pending);
    pending.clear();
    parts.forEach((p) => CA.refreshFrag($('frag-' + p), FRAG_URL(p)));
  }, 250);
  function refresh(parts) {
    parts.forEach((p) => { if ($('frag-' + p)) pending.add(p); if (p === 'reports_in') pending.add('addressed'); });
    flush();
  }

  CA.connect('team', {
    onSnapshot(data) {
      arena = data.arena;
      waiting = data.waiting;
      checkStructure();
      render();
    },
    onEvent(kind, data) {
      if (!arena) return;
      switch (kind) {
        case 'team_status': {
          const t = arena.teams.find((x) => x.id === data.team_id);
          if (t && arena.round && data.round_id === arena.round.id) {
            t[data.side] = data.status;
            if (data.team_id === boot.team_id) renderStatus();
          }
          break;
        }
        case 'round':
          arena.round = data.round;
          renderRound();
          checkStructure();
          break;
        case 'stats':
          arena.stats = data;
          renderStats();
          break;
        case 'scores':
          arena.scores = data.scores;
          renderScores();
          break;
        case 'team_refresh':
          refresh(data.parts || []);
          if ((data.parts || []).includes('reports_in')) CA.toast('Ada pembaruan laporan masuk untuk tim Anda.');
          break;
        case 'presence':
          arena.teams.forEach((t) => t.members.forEach((m) => { if (m.nim === data.nim) m.online = data.online; }));
          renderMembers();
          break;
        default:
          if (kind.startsWith('chat') || kind === 'lobby_locked') CAChat.onEvent(kind, data);
      }
    },
  });

  /* ------------------------------------------------------------ own-action refreshes */
  document.addEventListener('ca:done', (e) => {
    const url = e.detail.url;
    if (url.startsWith('/api/team/prep')) { refresh(['prep']); dirty = false; }
    if (url.startsWith('/api/team/report')) refresh(['reports_out']);
    if (url.startsWith('/api/team/defense')) refresh(['reports_in']);
    if (url.startsWith('/api/team/patch')) {
      refresh(['patches']);
      dirty = false;
      if (e.detail.result && e.detail.result.version) $('patch-label').textContent = 'Draft v' + e.detail.result.version + ' tersimpan ' + new Date().toLocaleTimeString('id-ID', { hour12: false });
    }
  });

  /* ------------------------------------------------------------ attack report form */
  const reportForm = $('report-form');
  const REPORT_FIELDS = ['title', 'code_location', 'trigger_input', 'expected', 'actual', 'repro_steps', 'root_cause', 'concept', 'evidence'];
  function reportBody() {
    const body = {};
    REPORT_FIELDS.forEach((f) => { body[f] = reportForm.querySelector('[name=' + f + ']').value; });
    const id = reportForm.querySelector('[name=id]').value;
    body.id = id ? Number(id) : null;
    return body;
  }
  function fillReport(data, mode) {
    REPORT_FIELDS.forEach((f) => { reportForm.querySelector('[name=' + f + ']').value = data ? (data[f] || '') : ''; });
    reportForm.querySelector('[name=id]').value = data ? data.id : '';
    reportForm.querySelector('[name=mode]').value = mode;
    reportForm.querySelector('[name=revision_reason]').value = '';
    $('revision-reason-field').classList.toggle('hidden', mode !== 'revise');
    $('report-form-title').textContent = !data ? 'Attack report baru' : mode === 'revise' ? 'Revisi laporan #' + data.id : 'Edit draft #' + data.id;
    $('report-submit').textContent = mode === 'revise' ? 'Kirim revisi →' : 'Submit laporan →';
    dirty = false;
    gate();
  }
  if (reportForm) {
    $('report-new').addEventListener('click', () => fillReport(null, 'draft'));
    $('report-save').addEventListener('click', async () => {
      try {
        const r = await CA.api('/api/team/report/save', reportBody());
        reportForm.querySelector('[name=id]').value = r.id;
        $('report-form-title').textContent = 'Edit draft #' + r.id;
        dirty = false;
        CA.toast('Draft laporan #' + r.id + ' disimpan', 'ok');
        refresh(['reports_out']);
      } catch (e) { CA.toast(e.message, 'error'); }
    });
    $('report-submit').addEventListener('click', async () => {
      const mode = reportForm.querySelector('[name=mode]').value;
      try {
        if (mode === 'revise') {
          if (!confirm('Kirim revisi laporan? Versi sebelumnya tetap tercatat di riwayat revisi.')) return;
          const body = reportBody();
          body.revision_reason = reportForm.querySelector('[name=revision_reason]').value;
          await CA.api('/api/team/report/revise', body);
          CA.toast('Revisi terkirim — menunggu validasi instruktur', 'ok');
        } else {
          if (!confirm('Submit laporan ke target? Setelah dikirim laporan terkunci, tercatat dengan timestamp, dan tidak dapat dihapus. Validitas ditentukan instruktur.')) return;
          const saved = await CA.api('/api/team/report/save', reportBody());
          await CA.api('/api/team/report/submit', { id: saved.id });
          CA.toast('Laporan #' + saved.id + ' disubmit — status: menunggu validasi', 'ok');
        }
        fillReport(null, 'draft');
        refresh(['reports_out']);
      } catch (e) { CA.toast(e.message, 'error'); }
    });
  }

  /* ------------------------------------------------------------ patch submit */
  const patchSubmit = $('patch-submit');
  if (patchSubmit) {
    patchSubmit.addEventListener('click', async () => {
      if (!confirm('Submit patch untuk review? Konten patch yang disubmit dibekukan; perubahan berikutnya menjadi versi baru.')) return;
      try {
        await CA.api('/api/team/patch/save', CA.serializeForm($('patch-form')));
        const r = await CA.api('/api/team/patch/submit', {});
        dirty = false;
        CA.toast('Patch v' + r.version + ' disubmit — menunggu review instruktur', 'ok');
        $('patch-label').textContent = 'Berbasis v' + r.version + ' (submitted) — simpan untuk membuat v' + (r.version + 1);
        refresh(['patches', 'reports_in']);
      } catch (e) { CA.toast(e.message, 'error'); }
    });
  }

  /* ------------------------------------------------------------ delegated buttons in fragments */
  const dlg = $('dlg-defense');
  document.addEventListener('click', (e) => {
    const edit = e.target.closest('[data-edit-report]');
    if (edit) { fillReport(JSON.parse(edit.dataset.editReport), 'draft'); reportForm.scrollIntoView({ behavior: 'smooth' }); return; }
    const rev = e.target.closest('[data-revise-report]');
    if (rev) { fillReport(JSON.parse(rev.dataset.reviseReport), 'revise'); reportForm.scrollIntoView({ behavior: 'smooth' }); return; }
    const d = e.target.closest('[data-defense-dlg]');
    if (d && dlg) {
      const rep = JSON.parse(d.dataset.report);
      const kind = d.dataset.defenseDlg;
      const f = $('dlg-defense-form');
      f.querySelector('[name=report_id]').value = rep.id;
      f.querySelector('[name=kind]').value = kind;
      $('dlg-state-field').classList.toggle('hidden', kind !== 'analysis');
      if (['analyzing', 'fixing', 'testing'].includes(rep.defense_state)) f.querySelector('[name=state]').value = rep.defense_state;
      $('dlg-defense-title').textContent = (kind === 'analysis' ? 'Analisis laporan #' : 'Rebuttal laporan #') + rep.id;
      $('dlg-text-label').textContent = kind === 'analysis' ? 'Catatan analisis root cause' : 'Alasan teknis mengapa laporan tidak valid (min. 20 karakter)';
      $('dlg-note').textContent = kind === 'analysis'
        ? 'Menyimpan catatan ini mengubah status defense tim sesuai progress yang dipilih (tindakan eksplisit).'
        : 'Rebuttal dinilai instruktur. Bonus maks 10 poin hanya jika laporan memang terbukti tidak valid.';
      f.querySelector('[name=text]').value = kind === 'analysis' ? (rep.defense_analysis || '') : (rep.rebuttal || '');
      dlg.showModal();
    }
  });
  if (dlg) {
    $('dlg-defense-save').addEventListener('click', async () => {
      const f = $('dlg-defense-form');
      const id = Number(f.querySelector('[name=report_id]').value);
      const kind = f.querySelector('[name=kind]').value;
      const text = f.querySelector('[name=text]').value;
      try {
        if (kind === 'analysis') {
          await CA.api('/api/team/defense/report', { report_id: id, state: f.querySelector('[name=state]').value, analysis: text });
          CA.toast('Progress laporan #' + id + ' disimpan', 'ok');
        } else {
          if (!confirm('Kirim rebuttal untuk laporan #' + id + '?')) return;
          await CA.api('/api/team/defense/rebuttal', { report_id: id, text });
          CA.toast('Rebuttal terkirim', 'ok');
        }
        dlg.close();
        refresh(['reports_in']);
      } catch (err) { CA.toast(err.message, 'error'); }
    });
  }

  window.addEventListener('beforeunload', (e) => { if (dirty) { e.preventDefault(); e.returnValue = ''; } });
})();
