/* Live Arena. Everything drawn here comes from the server snapshot / server events:
   nodes = confirmed teams, arrows = official pairings (attacker -> target), counts and scores =
   server-computed aggregates. Nothing is invented for animation's sake. */
'use strict';
(function () {
  const boot = JSON.parse(document.getElementById('boot').textContent);
  const $ = (id) => document.getElementById(id);
  const SVGNS = 'http://www.w3.org/2000/svg';
  const NODE_W = 236;
  const NODE_H = 124;
  const TONES = ['idle', 'investigate', 'threat', 'analysis', 'review', 'success', 'fail'];
  const TONE_COLOR = {};

  let snap = null;
  let view = CA.store.get('arena-view', 'graph');
  let filter = 'all';
  let zoom = Number(CA.store.get('arena-zoom', '1')) || 1;
  let feed = [];
  let drawer = null; // { type: 'team'|'edge'|'list', id }

  (function readToneColors() {
    const cs = getComputedStyle(document.documentElement);
    TONES.forEach((t) => { TONE_COLOR[t] = cs.getPropertyValue('--' + t).trim() || '#888'; });
  })();

  /* ------------------------------------------------------------------ helpers */
  const teamById = (id) => snap && snap.teams.find((t) => t.id === id);
  const teamName = (id) => { const t = teamById(id); return t ? t.name : '?'; };
  const tstats = (id) => (snap && snap.stats && snap.stats.teams[String(id)]) || null;
  const scoreOf = (id) => snap && snap.scores.find((s) => s.team_id === id);
  const trunc = (s, n) => (s && s.length > n ? s.slice(0, n - 1) + '…' : s || '');
  function el(tag, attrs, parent) {
    const e = document.createElementNS(SVGNS, tag);
    Object.entries(attrs || {}).forEach(([k, v]) => e.setAttribute(k, v));
    if (parent) parent.appendChild(e);
    return e;
  }

  const FILTERS = [
    ['all', 'Semua kelompok', () => true],
    ['attack', 'Attack aktif', (t) => ['INSPECTING_TARGET', 'PREPARING_REPORT', 'REPORT_SUBMITTED'].includes(t.attack && t.attack.code)],
    ['fixing', 'Bug fixing', (t) => ['ANALYZING_BUG', 'FIXING_BUG', 'WRITING_REGRESSION_TEST'].includes(t.defense && t.defense.code)],
    ['review', 'Menunggu review', (t) => {
      const s = tstats(t.id);
      return (t.attack && t.attack.code === 'AWAITING_VALIDATION') || ['PATCH_SUBMITTED', 'AWAITING_REVIEW'].includes(t.defense && t.defense.code) ||
        (s && (s.outgoing.pending > 0 || (s.patch && s.patch.status === 'submitted')));
    }],
    ['accepted', 'Patch accepted', (t) => { const s = tstats(t.id); return (s && s.patches_accepted > 0) || (t.defense && t.defense.code === 'PATCH_ACCEPTED'); }],
    ['dispute', 'Invalid / dispute', (t) => { const s = tstats(t.id); return s && (s.outgoing.invalid + s.outgoing.duplicate > 0 || s.disputes > 0); }],
  ];
  const matches = (t) => (FILTERS.find((f) => f[0] === filter) || FILTERS[0])[2](t);

  /* ------------------------------------------------------------------ summary + timeline */
  function renderSummary() {
    const r = snap.round;
    $('a-session').textContent = snap.session.name;
    if (r) {
      $('a-challenge').textContent = r.challenge.label + ' · ' + r.challenge.materi;
      $('a-round').textContent = 'RONDE ' + String(r.number).padStart(2, '0') + (r.state === 'setup' ? ' · SETUP' : r.finalized ? ' · FINAL' : '');
      const tone = r.paused ? 'review' : r.phase_tone;
      $('a-phase').className = 'badge big tone-' + tone;
      $('a-phase').innerHTML = '<span class="ico">' + (r.paused ? '❚❚' : '●') + '</span>' + CA.esc(r.phase_label.toUpperCase()) + (r.paused ? ' · PAUSED' : '');
    } else {
      $('a-challenge').textContent = 'Belum ada ronde';
      $('a-round').textContent = '-';
      $('a-phase').className = 'badge big tone-idle';
      $('a-phase').innerHTML = '<span class="ico">○</span>MENUNGGU';
    }
    const s = snap.stats.summary;
    CA.animateNumber($('s-teams'), s.teams);
    CA.animateNumber($('s-online'), s.online);
    CA.animateNumber($('s-reports'), s.reports);
    CA.animateNumber($('s-valid'), s.valid);
    CA.animateNumber($('s-patches'), s.verified_patches);
    CA.animateNumber($('s-awaiting'), s.awaiting_review_teams);
  }

  function renderTimeline() {
    const r = snap.round;
    const box = $('a-timeline');
    if (!r) { box.innerHTML = ''; return; }
    box.innerHTML = r.timeline.map((p) => {
      const cls = 'tl-step ' + p.state + (p.state === 'active' && r.paused ? ' paused' : '');
      const planned = p.planned_secs ? CA.fmtDur(p.planned_secs * 1000) : 'manual';
      let meta;
      if (p.state === 'completed') meta = CA.fmtTime(p.started_at) + '–' + CA.fmtTime(p.ended_at) + ' · ' + CA.fmtDur(p.actual_ms);
      else if (p.state === 'active') meta = 'mulai ' + CA.fmtTime(p.started_at) + ' · rencana ' + planned + (r.paused ? ' · PAUSED' : '');
      else meta = 'rencana ' + planned + (p.auto ? ' · auto' : '');
      return '<div class="' + cls + '" title="' + CA.esc(p.description) + '"><div class="t">' + CA.esc(p.label) + '</div><div class="m">' + CA.esc(meta) + '</div><div class="m">' + p.submissions + ' submission</div></div>';
    }).join('');
  }

  function clockLoop() {
    const r = snap && snap.round;
    const c = $('a-clock');
    if (!r || r.state !== 'running') { c.textContent = r && r.finalized ? 'FINAL' : '--:--'; c.className = 'clock'; return; }
    if (r.paused) { c.textContent = r.remaining_ms != null ? CA.fmtDur(r.remaining_ms) : 'PAUSED'; c.className = 'clock paused'; return; }
    if (r.deadline != null) {
      const left = r.deadline - CA.serverNow();
      c.textContent = CA.fmtDur(left);
      c.className = 'clock' + (left < 0 ? ' over' : left < 60000 ? ' warn' : '');
    } else {
      c.textContent = r.phase_started_at ? '⏱ ' + CA.fmtDur(CA.serverNow() - r.phase_started_at) : '--:--';
      c.className = 'clock';
    }
  }
  setInterval(clockLoop, 500);

  /* ------------------------------------------------------------------ graph layout */
  // Order nodes along attack cycles so that most arrows connect neighbours (few crossings).
  function cycles(teams, edges) {
    const next = new Map(edges.map((e) => [e.attacker_id, e.target_id]));
    const ids = teams.map((t) => t.id);
    const seen = new Set();
    const out = [];
    ids.forEach((start) => {
      if (seen.has(start)) return;
      const cyc = [];
      let cur = start;
      while (cur != null && !seen.has(cur) && ids.includes(cur)) { seen.add(cur); cyc.push(cur); cur = next.get(cur); }
      out.push(cyc);
    });
    return out;
  }

  function layout(teams, edges) {
    const pos = new Map();
    if (!edges.length) {
      const cols = Math.max(1, Math.min(4, Math.ceil(Math.sqrt(teams.length))));
      teams.forEach((t, i) => pos.set(t.id, { x: (i % cols) * (NODE_W + 60) + NODE_W / 2 + 40, y: Math.floor(i / cols) * (NODE_H + 70) + NODE_H / 2 + 40 }));
      return pos;
    }
    const cyc = cycles(teams, edges).sort((a, b) => b.length - a.length);
    // chord between neighbours leaves room for the ATTACK label between two node cards
    const radiusFor = (n) => (n <= 1 ? 0 : n === 2 ? 150 : Math.max(210, (NODE_W + 170) / (2 * Math.sin(Math.PI / n))));
    let cursorX = 0;
    let maxH = 0;
    cyc.forEach((c) => {
      const R = radiusFor(c.length);
      const cx = cursorX + R + NODE_W / 2 + 40;
      const cy = R + NODE_H / 2 + 60;
      c.forEach((id, i) => {
        const a = -Math.PI / 2 + (2 * Math.PI * i) / c.length;
        pos.set(id, { x: cx + R * Math.cos(a), y: cy + R * Math.sin(a) });
      });
      cursorX = cx + R + NODE_W / 2 + 40;
      maxH = Math.max(maxH, cy + R + NODE_H / 2 + 60);
    });
    return pos;
  }

  function clip(cx, cy, tx, ty, pad) {
    const dx = tx - cx, dy = ty - cy;
    if (dx === 0 && dy === 0) return { x: cx, y: cy };
    const sx = (NODE_W / 2 + pad) / Math.abs(dx || 1e-9);
    const sy = (NODE_H / 2 + pad) / Math.abs(dy || 1e-9);
    const s = Math.min(sx, sy);
    return { x: cx + dx * s, y: cy + dy * s };
  }

  function edgeStyle(e) {
    const r = snap.round;
    const c = e.counts;
    if (e.proposed) return { tone: 'idle', dash: '8 7', text: 'ATTACK (usulan)' };
    let tone = 'idle';
    if (c.pending > 0) tone = 'threat';
    else if (r && r.phase === 'ATTACK_WINDOW' && r.state === 'running') tone = 'threat';
    else if (c.valid > 0) tone = 'success';
    else if (c.needs_revision > 0) tone = 'investigate';
    else if (c.official > 0) tone = 'review';
    let text = 'ATTACK · ' + c.official + ' laporan';
    if (c.pending) text += ' · ' + c.pending + ' pending';
    if (c.valid) text += ' · ' + c.valid + ' valid';
    if (c.invalid + c.duplicate) text += ' · ' + (c.invalid + c.duplicate) + ' ditolak';
    return { tone, dash: null, text };
  }

  function renderGraph() {
    const host = $('a-graph');
    host.innerHTML = '';
    const teams = snap.teams;
    if (!teams.length) {
      host.innerHTML = '<div class="empty-stage"><div><h2>Belum ada kelompok terkonfirmasi</h2><p>Arena menampilkan kelompok setelah instruktur mengonfirmasi pembagian kelompok.</p></div></div>';
      return;
    }
    const edges = snap.stats.edges.filter((e) => teamById(e.attacker_id) && teamById(e.target_id));
    const pos = layout(teams, edges);
    let maxX = 0, maxY = 0;
    pos.forEach((p) => { maxX = Math.max(maxX, p.x + NODE_W / 2 + 40); maxY = Math.max(maxY, p.y + NODE_H / 2 + 40); });
    const svg = el('svg', { viewBox: '0 0 ' + Math.ceil(maxX) + ' ' + Math.ceil(maxY), preserveAspectRatio: 'xMidYMid meet', role: 'img', 'aria-label': 'Attack graph: ' + edges.length + ' relasi serangan' });
    // zoom 100% = whole graph fits the stage (projector); larger zoom scrolls
    svg.style.width = Math.round(100 * zoom) + '%';
    svg.style.height = Math.round(100 * zoom) + '%';
    svg.style.maxWidth = zoom <= 1 ? Math.ceil(maxX * 1.4) + 'px' : 'none';
    const defs = el('defs', {}, svg);
    TONES.forEach((t) => {
      const m = el('marker', { id: 'arrow-' + t, viewBox: '0 0 12 12', refX: '10', refY: '6', markerWidth: '9', markerHeight: '9', orient: 'auto-start-reverse' }, defs);
      el('path', { d: 'M1,1 L11,6 L1,11 z', fill: TONE_COLOR[t] }, m);
    });

    const edgeLayer = el('g', {}, svg);
    const nodeLayer = el('g', {}, svg);
    const labelLayer = el('g', {}, svg);

    edges.forEach((e) => {
      const a = pos.get(e.attacker_id), b = pos.get(e.target_id);
      if (!a || !b) return;
      const st = edgeStyle(e);
      const dx = b.x - a.x, dy = b.y - a.y;
      const len = Math.hypot(dx, dy) || 1;
      // bow to the left of the travel direction: A->B and B->A never overlap
      const bow = Math.min(90, len * 0.16) + (edges.length === 2 ? 40 : 0);
      const ctrl = { x: (a.x + b.x) / 2 + (dy / len) * bow, y: (a.y + b.y) / 2 - (dx / len) * bow };
      const p0 = clip(a.x, a.y, ctrl.x, ctrl.y, 4);
      const p2 = clip(b.x, b.y, ctrl.x, ctrl.y, 10);
      const d = 'M' + p0.x.toFixed(1) + ',' + p0.y.toFixed(1) + ' Q' + ctrl.x.toFixed(1) + ',' + ctrl.y.toFixed(1) + ' ' + p2.x.toFixed(1) + ',' + p2.y.toFixed(1);
      const dim = filter !== 'all' && !(matches(teamById(e.attacker_id)) || matches(teamById(e.target_id)));
      const g = el('g', { class: 'edge' + (dim ? ' dim' : ''), 'data-edge': e.attacker_id }, edgeLayer);
      const line = el('path', { class: 'line', d, stroke: TONE_COLOR[st.tone], 'marker-end': 'url(#arrow-' + st.tone + ')' }, g);
      if (st.dash) line.setAttribute('stroke-dasharray', st.dash);
      el('path', { class: 'hit', d }, g);
      const title = el('title', {}, g);
      title.textContent = teamName(e.attacker_id) + ' menyerang ' + teamName(e.target_id) + ' — ronde ' + e.round + ' — ' + st.text + (e.locked ? ' (pairing dikunci)' : ' (usulan, belum dikunci)');
      // label at curve midpoint
      const mx = 0.25 * p0.x + 0.5 * ctrl.x + 0.25 * p2.x;
      const my = 0.25 * p0.y + 0.5 * ctrl.y + 0.25 * p2.y;
      const lg = el('g', { class: 'edge tag' + (dim ? ' dim' : ''), 'data-edge': e.attacker_id, transform: 'translate(' + mx.toFixed(1) + ',' + my.toFixed(1) + ')', style: 'cursor:pointer' }, labelLayer);
      const w = st.text.length * 6.9 + 18;
      el('rect', { x: -w / 2, y: -12, width: w, height: 24, rx: 12, stroke: TONE_COLOR[st.tone], fill: '#0e1624' }, lg);
      const tx = el('text', { 'text-anchor': 'middle', y: '4', fill: TONE_COLOR[st.tone], style: 'font:700 12px var(--sans);letter-spacing:.04em' }, lg);
      tx.textContent = st.text;
    });

    teams.forEach((t) => {
      const p = pos.get(t.id);
      if (!p) return;
      const s = tstats(t.id);
      const sc = scoreOf(t.id);
      const g = el('g', { class: 'node' + (matches(t) ? '' : ' dim'), transform: 'translate(' + (p.x - NODE_W / 2).toFixed(1) + ',' + (p.y - NODE_H / 2).toFixed(1) + ')', 'data-team': t.id, style: 'cursor:pointer', tabindex: '0', role: 'button' }, nodeLayer);
      const attackTone = (t.attack && t.attack.tone) || 'idle';
      const defenseTone = (t.defense && t.defense.tone) || 'idle';
      el('rect', { class: 'frame', width: NODE_W, height: NODE_H, rx: 12 }, g);
      el('rect', { x: 0, y: 0, width: 6, height: NODE_H / 2, rx: 3, fill: TONE_COLOR[attackTone] }, g);
      el('rect', { x: 0, y: NODE_H / 2, width: 6, height: NODE_H / 2, rx: 3, fill: TONE_COLOR[defenseTone] }, g);
      const name = el('text', { class: 'name', x: 16, y: 26 }, g);
      name.textContent = trunc(t.name.toUpperCase(), 14);
      const pts = el('text', { class: 'score', x: NODE_W - 12, y: 26, 'text-anchor': 'end' }, g);
      pts.textContent = sc ? sc.cumulative : 0;
      const row = (y, lbl, st, tone) => {
        const l = el('text', { class: 'lbl', x: 16, y }, g);
        l.textContent = lbl;
        const v = el('text', { class: 'st', x: 54, y, fill: TONE_COLOR[tone] }, g);
        v.textContent = st ? (st.icon + ' ' + trunc(st.label, 23)) : '○ —';
      };
      row(52, 'ATK', t.attack, attackTone);
      row(74, 'DEF', t.defense, defenseTone);
      const meta = el('text', { class: 'meta', x: 16, y: 100 }, g);
      const bits = ['● ' + t.online + '/' + t.members.length + ' online'];
      if (s) {
        bits.push(s.open_findings + ' temuan terbuka');
        if (s.patch) bits.push('patch v' + s.patch.version + ' ' + s.patch.status);
      }
      meta.textContent = trunc(bits.join(' · '), 38);
      const meta2 = el('text', { class: 'meta', x: 16, y: 116 }, g);
      meta2.textContent = sc ? ('ronde ' + sc.round.total + ' · A' + sc.round.attack + ' D' + sc.round.defense + ' R' + sc.round.rebuttal) : '';
      const title = el('title', {}, g);
      title.textContent = t.name + '\nATTACK: ' + (t.attack ? t.attack.label + ' — ' + t.attack.reason : '-') + '\nDEFENSE: ' + (t.defense ? t.defense.label + ' — ' + t.defense.reason : '-');
    });
    host.appendChild(svg);
  }

  /* ------------------------------------------------------------------ team board */
  function renderBoard() {
    const host = $('a-board');
    if (!snap.teams.length) { host.innerHTML = '<div class="empty-stage">Belum ada kelompok terkonfirmasi.</div>'; return; }
    const r = snap.round;
    const timeLeft = () => {
      if (!r || r.state !== 'running') return r && r.finalized ? 'final' : '-';
      if (r.paused) return 'PAUSED (' + CA.fmtDur(r.remaining_ms) + ')';
      return r.deadline != null ? '<span data-left></span>' : 'manual';
    };
    host.innerHTML = snap.teams.map((t) => {
      const s = tstats(t.id) || { outgoing: {}, incoming: {}, open_findings: 0 };
      const sc = scoreOf(t.id) || { cumulative: 0, round: { total: 0, attack: 0, defense: 0, rebuttal: 0 }, valid_findings_total: 0, accepted_patches_total: 0, round_finalized: false };
      return '<article class="tcard' + (matches(t) ? '' : ' dim') + '" data-team="' + t.id + '">' +
        '<div class="head"><span class="n">' + CA.esc(t.name.toUpperCase()) + '</span><span class="p">' + sc.cumulative + ' <span class="small muted">POINTS</span></span></div>' +
        '<div class="sec"><div class="sec-t">ATTACK — sebagai penyerang</div><dl>' +
        '<dt>Target</dt><dd>' + (t.target_id ? CA.esc(teamName(t.target_id)) : '-') + '</dd>' +
        '<dt>Challenge</dt><dd>' + (r ? CA.esc(r.challenge.label) : '-') + '</dd>' +
        '<dt>Status</dt><dd>' + CA.badge(t.attack) + '<div class="reason">' + CA.esc(t.attack ? t.attack.reason : '') + '</div></dd>' +
        '<dt>Reports</dt><dd>' + (s.outgoing.official || 0) + ' submitted, ' + (s.outgoing.pending || 0) + ' pending review, ' + (s.outgoing.valid || 0) + ' valid' + ((s.outgoing.invalid || 0) + (s.outgoing.duplicate || 0) ? ', ' + ((s.outgoing.invalid || 0) + (s.outgoing.duplicate || 0)) + ' ditolak' : '') + '</dd></dl></div>' +
        '<div class="sec"><div class="sec-t">DEFENSE — sebagai defender</div><dl>' +
        '<dt>Incoming attack</dt><dd>' + (t.attacker_id ? 'dari ' + CA.esc(teamName(t.attacker_id)) : '-') + '</dd>' +
        '<dt>Status</dt><dd>' + CA.badge(t.defense) + '<div class="reason">' + CA.esc(t.defense ? t.defense.reason : '') + '</div></dd>' +
        '<dt>Open findings</dt><dd>' + s.open_findings + ' (masuk ' + (s.incoming.official || 0) + ')</dd>' +
        '<dt>Patch version</dt><dd>' + (s.patch ? 'v' + s.patch.version + ' — ' + CA.esc(s.patch.label) + ' · ' + s.patch.tests + ' test' : '-') + '</dd>' +
        '<dt>Snapshot</dt><dd class="mono small">' + (s.snapshot ? s.snapshot.sha : (s.prep ? 'draft disimpan ' + s.prep.saves + '×' + (s.prep.ready ? ' · siap' : '') : '-')) + '</dd></dl></div>' +
        '<div class="sec"><div class="sec-t">ROUND STATUS &amp; SKOR</div><dl>' +
        '<dt>Time remaining</dt><dd class="mono">' + timeLeft() + '</dd>' +
        '<dt>Skor ronde</dt><dd>' + sc.round.total + ' ' + (sc.round_finalized ? '<span class="badge tone-success"><span class="ico">✓</span>final</span>' : '<span class="badge tone-review"><span class="ico">◷</span>berjalan</span>') + '</dd></dl>' +
        '<div class="scoreline"><span>attack ' + sc.round.attack + '</span><span>defense ' + sc.round.defense + '</span><span>rebuttal ' + sc.round.rebuttal + '</span><span>valid ' + sc.valid_findings_total + '</span><span>patch ✓ ' + sc.accepted_patches_total + '</span></div></div>' +
        '<div class="small muted" style="margin-top:.4rem">● ' + t.online + '/' + t.members.length + ' anggota online</div>' +
        '</article>';
    }).join('');
    updateBoardClocks();
  }
  function updateBoardClocks() {
    const r = snap && snap.round;
    if (!r || r.deadline == null || r.paused) return;
    const txt = CA.fmtDur(r.deadline - CA.serverNow());
    document.querySelectorAll('#a-board [data-left]').forEach((e) => { e.textContent = txt; });
  }
  setInterval(updateBoardClocks, 1000);

  /* ------------------------------------------------------------------ side panels */
  function renderLeaderboard() {
    const box = $('a-leaderboard');
    if (!snap.scores.length) { box.innerHTML = '<span class="muted small">Belum ada skor.</span>'; return; }
    const prev = {};
    box.querySelectorAll('[data-score]').forEach((e) => { prev[e.dataset.score] = e.dataset.v || e.textContent; });
    box.innerHTML = snap.scores.map((s) =>
      '<div class="lb-row"><span class="rank' + (s.rank === 1 ? ' r1' : '') + '">' + s.rank + '</span>' +
      '<span><b>' + CA.esc(teamName(s.team_id)) + '</b><div class="split">A ' + s.round.attack + ' · D ' + s.round.defense + ' · R ' + s.round.rebuttal + ' · ronde ' + s.round.total + (s.round_finalized ? ' ✓' : '') + (s.adjustments ? ' · koreksi ' + s.adjustments : '') + '</div></span>' +
      '<span class="pts" data-score="' + s.team_id + '" data-v="' + (prev[s.team_id] || s.cumulative) + '">' + (prev[s.team_id] || s.cumulative) + '</span></div>').join('');
    snap.scores.forEach((s) => CA.animateNumber(box.querySelector('[data-score="' + s.team_id + '"]'), s.cumulative));
    $('a-lb-note').textContent = 'resmi dari server';
  }

  function pushFeed(item) {
    if (feed.some((f) => f.key === item.key)) return;
    feed.unshift(item);
    feed = feed.slice(0, 80);
  }
  function renderFeed() {
    $('a-feed').innerHTML = feed.map((f) =>
      '<li><time>' + CA.fmtTime(f.at) + '</time><span>' + (f.tone ? '<span class="tone-' + f.tone + '">' + CA.esc(f.icon || '•') + '</span> ' : '') +
      (f.team_id ? '<span class="who">' + CA.esc(teamName(f.team_id)) + '</span> ' : '') + (f.side ? '<span class="small muted upper">' + f.side + '</span> ' : '') + CA.esc(f.message) + '</span></li>').join('') ||
      '<li class="muted">Belum ada aktivitas pada ronde ini.</li>';
  }

  function renderFilters() {
    $('a-filters').innerHTML = FILTERS.map(([k, label, fn]) =>
      '<button class="filter-chip' + (k === filter ? ' active' : '') + '" type="button" data-arena-filter="' + k + '">' + CA.esc(label) +
      '<span class="n">' + (k === 'all' ? snap.teams.length : snap.teams.filter(fn).length) + '</span></button>').join('');
  }

  function renderLegend() {
    const st = snap.statuses;
    $('a-legend').innerHTML = '<div class="section-label">Attack status</div><div class="chips">' + st.attack.map((s) => CA.badge(s)).join('') + '</div>' +
      '<div class="section-label" style="margin-top:.6rem">Defense status</div><div class="chips">' + st.defense.map((s) => CA.badge(s)).join('') + '</div>' +
      '<p class="small muted" style="margin-top:.6rem">Panah = pairing resmi (penyerang → target). Garis putus-putus = usulan pairing yang belum dikunci. Warna panah: merah = serangan berlangsung/pending, hijau = ada temuan valid, ungu = sudah direview, abu-abu = belum aktif. Setiap status juga memiliki ikon dan teks.</p>';
  }

  let renderQueued = false;
  function render() {
    if (renderQueued) return;
    renderQueued = true;
    requestAnimationFrame(() => {
      renderQueued = false;
      if (!snap) return;
      renderSummary();
      renderTimeline();
      renderFilters();
      if (view === 'graph') renderGraph(); else renderBoard();
      $('a-graph').classList.toggle('hidden', view !== 'graph');
      $('a-board').classList.toggle('hidden', view !== 'board');
      document.querySelectorAll('[data-view]').forEach((b) => b.classList.toggle('active', b.dataset.view === view));
      renderLeaderboard();
      renderFeed();
      clockLoop();
    });
  }

  /* ------------------------------------------------------------------ drawer */
  async function openDrawer(type, id) {
    drawer = { type, id };
    const d = $('drawer');
    d.classList.add('open');
    d.setAttribute('aria-hidden', 'false');
    await refreshDrawer();
  }
  function closeDrawer() {
    drawer = null;
    $('drawer').classList.remove('open');
    $('drawer').setAttribute('aria-hidden', 'true');
  }
  async function refreshDrawer() {
    if (!drawer) return;
    const body = $('drawer-body');
    if (drawer.type === 'team') {
      $('drawer-title').textContent = 'Team inspector — ' + teamName(drawer.id);
      await CA.refreshFrag(body, '/arena/frag/team/' + drawer.id);
    } else if (drawer.type === 'edge') {
      const e = snap.stats.edges.find((x) => x.attacker_id === drawer.id);
      $('drawer-title').textContent = 'Detail serangan' + (e ? ' — ' + teamName(e.attacker_id) + ' → ' + teamName(e.target_id) : '');
      await CA.refreshFrag(body, '/arena/frag/edge/' + drawer.id);
    } else {
      body.innerHTML = listHtml(drawer.id);
    }
  }
  const refreshDrawerSoon = CA.debounce(refreshDrawer, 700);

  function listHtml(kind) {
    const s = snap.stats;
    const rows = (cells) => '<table><tbody>' + cells.join('') + '</tbody></table>';
    const teamRow = (t, extra) => '<tr data-team="' + t.id + '" style="cursor:pointer"><td><b>' + CA.esc(t.name) + '</b></td><td>' + extra + '</td></tr>';
    const titles = { teams: 'Kelompok', online: 'Peserta online', reports: 'Attack report per relasi', valid: 'Temuan valid', patches: 'Patch', awaiting: 'Menunggu review' };
    $('drawer-title').textContent = titles[kind] || 'Detail';
    switch (kind) {
      case 'teams':
        return rows(snap.teams.map((t) => teamRow(t, t.members.length + ' anggota · menyerang ' + (t.target_id ? CA.esc(teamName(t.target_id)) : '-'))));
      case 'online':
        return '<p class="muted small">' + s.summary.online + ' dari ' + s.summary.participants + ' peserta yang hadir sedang online (koneksi real-time aktif).</p>' +
          rows(snap.teams.map((t) => teamRow(t, t.members.map((m) => '<span class="member"><span class="dot' + (m.online ? ' on' : '') + '"></span>' + CA.esc(m.nim) + '</span>').join(''))));
      case 'reports':
        return rows(s.edges.map((e) => '<tr data-edge-row="' + e.attacker_id + '" style="cursor:pointer"><td>' + CA.esc(teamName(e.attacker_id)) + ' → ' + CA.esc(teamName(e.target_id)) + '</td><td>' + e.counts.official + ' laporan · ' + e.counts.pending + ' pending · ' + e.counts.valid + ' valid · ' + (e.counts.invalid + e.counts.duplicate) + ' ditolak</td></tr>'));
      case 'valid':
        return rows(snap.teams.map((t) => { const x = tstats(t.id); return teamRow(t, (x ? x.outgoing.valid : 0) + ' temuan valid ronde ini'); }));
      case 'patches':
        return rows(snap.teams.map((t) => { const x = tstats(t.id); return teamRow(t, x && x.patch ? 'v' + x.patch.version + ' — ' + CA.esc(x.patch.label) + ' · ' + x.patches_accepted + ' accepted' : 'belum ada patch'); }));
      case 'awaiting':
        return rows(snap.teams.filter(FILTERS[3][2]).map((t) => teamRow(t, CA.badge(t.attack) + ' ' + CA.badge(t.defense)))) || '<p class="muted">Tidak ada kelompok yang menunggu review.</p>';
      default:
        return '';
    }
  }

  /* ------------------------------------------------------------------ interactions */
  document.addEventListener('click', (e) => {
    const v = e.target.closest('[data-view]');
    if (v) { view = v.dataset.view; CA.store.set('arena-view', view); render(); return; }
    const f = e.target.closest('[data-arena-filter]');
    if (f) { filter = f.dataset.arenaFilter; render(); return; }
    const z = e.target.closest('[data-zoom]');
    if (z) {
      const dir = Number(z.dataset.zoom);
      zoom = dir === 0 ? 1 : Math.max(0.5, Math.min(2.5, zoom + dir * 0.2));
      CA.store.set('arena-zoom', String(zoom));
      render();
      return;
    }
    const list = e.target.closest('[data-list]');
    if (list) { openDrawer('list', list.dataset.list); return; }
    if (e.target.closest('#drawer-close')) { closeDrawer(); return; }
    const edgeRow = e.target.closest('[data-edge-row]');
    if (edgeRow) { openDrawer('edge', Number(edgeRow.dataset.edgeRow)); return; }
    const edge = e.target.closest('[data-edge]');
    if (edge && edge.closest('#a-graph')) { openDrawer('edge', Number(edge.dataset.edge)); return; }
    const team = e.target.closest('[data-team]');
    if (team && (team.closest('#a-graph') || team.closest('#a-board') || (team.tagName === 'TR' && team.closest('#drawer-body')))) { openDrawer('team', Number(team.dataset.team)); }
  });
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') closeDrawer();
    if ((e.key === 'Enter' || e.key === ' ') && e.target.closest && e.target.closest('.node')) {
      e.preventDefault();
      openDrawer('team', Number(e.target.closest('.node').dataset.team));
    }
  });

  /* ------------------------------------------------------------------ live data */
  CA.connect('arena', {
    onSnapshot(data) {
      snap = data.arena;
      feed = [];
      (snap.feed || []).slice().reverse().forEach((a) => {
        const st = a.side && a.status ? (snap.statuses[a.side] || []).find((x) => x.code === a.status) : null;
        pushFeed({ key: 'a' + a.id, at: a.at, team_id: a.team_id, side: a.side, message: a.message, tone: st ? st.tone : null, icon: st ? st.icon : null });
      });
      renderLegend();
      render();
      if (drawer) refreshDrawerSoon();
    },
    onEvent(kind, data) {
      if (!snap) return;
      switch (kind) {
        case 'team_status': {
          const t = teamById(data.team_id);
          if (t && snap.round && data.round_id === snap.round.id) t[data.side] = data.status;
          pushFeed({ key: 's' + data.team_id + data.side + data.status.at + data.status.code, at: data.status.at, team_id: data.team_id, side: data.side, message: data.status.label + ' — ' + data.status.reason, tone: data.status.tone, icon: data.status.icon });
          render();
          if (drawer && drawer.id === data.team_id) refreshDrawerSoon();
          break;
        }
        case 'activity':
          pushFeed({ key: 'a' + data.id, at: data.at, team_id: data.team_id, side: null, message: data.message, tone: data.kind === 'report_submitted' ? 'threat' : 'idle', icon: data.kind === 'report_submitted' ? '▲' : '•' });
          render();
          break;
        case 'stats':
          snap.stats = data;
          render();
          if (drawer) refreshDrawerSoon();
          break;
        case 'scores':
          snap.scores = data.scores;
          render();
          break;
        case 'round':
          snap.round = data.round;
          render();
          break;
        case 'presence':
          snap.teams.forEach((t) => {
            let changed = false;
            t.members.forEach((m) => { if (m.nim === data.nim) { m.online = data.online; changed = true; } });
            if (changed) t.online = t.members.filter((m) => m.online).length;
          });
          if (typeof data.online_count === 'number') snap.stats.summary.online = data.online_count;
          render();
          break;
        default:
          break;
      }
    },
  });
})();
