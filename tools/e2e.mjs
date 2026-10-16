// End-to-end scenario against a RUNNING Code Arena server (HTTP + WebSocket), simulating the
// instructor and 20 students. WARNING: it archives the current session (session reset) first.
//
//   node tools/e2e.mjs                 (BASE=http://127.0.0.1:8090 PW=<instructor password>)
//
// Requires Node >= 22 (global fetch + WebSocket).
import { readFileSync } from 'node:fs';

const BASE = process.env.BASE || 'http://127.0.0.1:8090';
const PW = process.env.PW || 'admin-test-pw';
const WSBASE = BASE.replace(/^http/, 'ws');
const NIMS = ['25032014005', '25032014006', '25032014007', '25032014018', '25032014021', '25032014025', '25032014029', '25032014031', '25032014032', '25032014039', '25032014043', '25032014045', '25032014050', '25032014051', '25032014053', '25032014058', '25032014062', '25032014071', '25032014075', '25032014090'];
const C03_REFERENCE = readFileSync(new URL('../challenges/C03/reference.py', import.meta.url), 'utf8');
const C03_TESTS = readFileSync(new URL('../challenges/C03/test_regression.py', import.meta.url), 'utf8');

let passed = 0;
let failed = 0;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function check(cond, msg) {
  if (cond) { passed++; console.log('  ✓ ' + msg); } else { failed++; console.log('  ✗ ' + msg); }
}
function section(t) { console.log('\n== ' + t); }

class Client {
  constructor(name) { this.name = name; this.cookie = ''; this.msgs = []; this.ws = null; this.snapshots = 0; }
  async req(method, path, body) {
    const r = await fetch(BASE + path, {
      method,
      headers: { 'content-type': 'application/json', cookie: this.cookie },
      body: body === undefined ? undefined : JSON.stringify(body),
      redirect: 'manual',
    });
    const sc = r.headers.getSetCookie ? r.headers.getSetCookie() : [];
    for (const c of sc) {
      const kv = c.split(';')[0];
      const [k] = kv.split('=');
      const jar = Object.fromEntries(this.cookie.split('; ').filter(Boolean).map((x) => [x.split('=')[0], x]));
      jar[k] = kv;
      this.cookie = Object.values(jar).join('; ');
    }
    const text = await r.text();
    let json = null;
    try { json = JSON.parse(text); } catch (_) { /* html */ }
    return { status: r.status, json, text, location: r.headers.get('location') };
  }
  post(path, body) { return this.req('POST', path, body || {}); }
  get(path) { return this.req('GET', path); }
  connect(view) {
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(WSBASE + '/ws?view=' + view, { headers: { cookie: this.cookie } });
      this.ws.onmessage = (m) => {
        const msg = JSON.parse(m.data);
        this.msgs.push(msg);
        if (msg.t === 'snapshot') { this.snapshots++; this.last = msg; resolve(msg); }
      };
      this.ws.onerror = () => reject(new Error(this.name + ' ws error'));
      setTimeout(() => reject(new Error(this.name + ' ws timeout')), 5000);
    });
  }
  close() { if (this.ws) this.ws.close(); }
  mark() { return this.msgs.length; }
  async waitFor(pred, from = 0, ms = 4000) {
    const t0 = Date.now();
    while (Date.now() - t0 < ms) {
      const hit = this.msgs.slice(from).find(pred);
      if (hit) return hit;
      await sleep(40);
    }
    return null;
  }
  events(from = 0) { return this.msgs.slice(from).filter((m) => m.t === 'ev'); }
}

async function main() {
  const admin = new Client('admin');
  section('instructor authentication & session reset');
  check((await admin.post('/api/admin/login', { password: 'wrong' })).status === 403, 'wrong instructor password rejected');
  check((await admin.post('/api/admin/login', { password: PW })).status === 200, 'instructor login');
  const reset = await admin.post('/api/admin/session/reset', { confirm: 'RESET', name: 'E2E Recap PBO' });
  check(reset.status === 200, 'session reset archives previous session');
  check((await admin.post('/api/admin/session/reset', { confirm: 'nope' })).status === 400, 'reset requires typed confirmation');

  section('join with NIM');
  const anon = new Client('anon');
  check((await anon.post('/api/join', { nim: '99999999999' })).status === 403, 'unknown NIM rejected');
  check((await anon.post('/api/join', { nim: "1' OR 1=1" })).status === 400, 'malformed NIM rejected');
  check((await anon.post('/api/admin/teams/random', {})).status === 401, 'anonymous cannot call instructor API');
  check((await anon.get('/team')).status === 303, 'anonymous /team redirects');

  const students = NIMS.map((n) => new Client(n));
  for (const s of students.slice(0, 18)) {
    const r = await s.post('/api/join', { nim: s.name });
    if (r.status !== 200) console.log(r);
  }
  check(students.slice(0, 18).every((s) => s.cookie.includes('ca_student=')), '18 students joined and got a session cookie');
  await Promise.all(students.slice(0, 18).map((s) => s.connect('wait')));
  await admin.connect('admin');
  const dev2 = new Client('device2');
  check((await dev2.post('/api/join', { nim: NIMS[0] })).status === 409, 'same NIM on a second device is refused while the first is online');
  const reuse = await students[0].post('/api/join', { nim: NIMS[0] });
  check(reuse.status === 200 && reuse.json.redirect === '/wait', 'same browser re-join restores identity');
  check((await students[1].post('/api/admin/teams/random', {})).status === 401, 'student cannot call instructor API');

  section('random assignment (preview -> confirm) and late arrivals');
  const prev = await admin.post('/api/admin/teams/random', { target_size: 4, include_offline: false });
  const sizes = (prev.json.preview || []).map((t) => t.members.length).sort((a, b) => b - a);
  check(JSON.stringify(sizes) === '[4,4,4,3,3]', '18 present -> 4+4+4+3+3 (' + sizes.join('+') + ')');
  const allMembers = prev.json.preview.flatMap((t) => t.members);
  check(new Set(allMembers).size === 18 && allMembers.length === 18, 'no participant lost or duplicated');
  await students[0].post('/api/join', { nim: NIMS[0] });
  const w0 = await students[0].get('/api/snapshot?view=wait');
  check(w0.json.data.waiting.assignment === 'preview' && w0.json.data.waiting.my_team === null, 'draft teams are invisible to students');
  // two latecomers join after the snapshot
  for (const s of students.slice(18)) await s.post('/api/join', { nim: s.name });
  await Promise.all(students.slice(18).map((s) => s.connect('wait')));
  const m0 = students[0].mark();
  check((await admin.post('/api/admin/teams/confirm', {})).status === 200, 'instructor confirms teams');
  const snapAfter = await students[0].waitFor((m) => m.t === 'snapshot', m0);
  check(snapAfter && snapAfter.data.waiting.my_team && snapAfter.data.waiting.my_team.members.length >= 3, 'student receives own team over WebSocket');
  const late = await students[19].get('/api/snapshot?view=wait');
  check(late.json.data.waiting.my_team === null, 'latecomer after snapshot keeps waiting');
  const placed = await admin.post('/api/admin/teams/place-late', {});
  check(placed.status === 200 && placed.json.placed.length === 2, 'instructor places latecomers into smallest teams');
  const arenaA = await admin.get('/api/snapshot?view=arena');
  const teamSizes = arenaA.json.data.arena.teams.map((t) => t.members.length).sort();
  check(JSON.stringify(teamSizes) === '[4,4,4,4,4]', '20 participants -> 5 teams of 4');

  // resolve team membership for every student
  const teams = arenaA.json.data.arena.teams;
  const teamOf = (nim) => teams.find((t) => t.members.some((m) => m.nim === nim)).id;
  const byTeam = {};
  students.forEach((s) => { (byTeam[teamOf(s.name)] = byTeam[teamOf(s.name)] || []).push(s); });

  section('round setup, pairing lock and attack graph');
  const cr = await admin.post('/api/admin/round/create', { challenge_id: 'C03', briefing: 0, defense_prep: 10, attack_window: 15, defense_response: 10, verification: 0, auto_advance: false });
  check(cr.status === 200, 'round created with C03');
  const roundId = cr.json.round_id;
  const pubSetup = await students[0].get('/api/snapshot?view=arena');
  check(pubSetup.json.data.arena.stats.edges.length === 0, 'students cannot see the unlocked (proposed) pairing');
  const adminSetup = await admin.get('/api/snapshot?view=arena');
  check(adminSetup.json.data.arena.stats.edges.length === 5 && adminSetup.json.data.arena.stats.edges.every((e) => e.proposed), 'instructor sees 5 proposed attack relations');
  check((await admin.post('/api/admin/round/start', { round_id: roundId })).status === 200, 'round started (pairing + teams locked)');
  check((await admin.post('/api/admin/round/shuffle', { round_id: roundId })).status === 409, 'locked pairing cannot be reshuffled');
  const arenaB = (await students[0].get('/api/snapshot?view=arena')).json.data.arena;
  const edges = arenaB.stats.edges;
  check(edges.length === 5 && edges.every((e) => e.locked), 'public graph shows 5 locked ATTACK arrows');
  check(new Set(edges.map((e) => e.attacker_id)).size === 5 && new Set(edges.map((e) => e.target_id)).size === 5, 'every team attacks exactly one and is attacked by exactly one');
  check(edges.every((e) => e.attacker_id !== e.target_id), 'no team attacks itself');
  check(arenaB.teams.every((t) => t.attack.code === 'TARGET_ASSIGNED' && t.defense.code === 'NOT_STARTED'), 'separate attack/defense statuses initialised from real events');
  const reArena = (await students[0].get('/api/snapshot?view=arena')).json.data.arena.stats.edges;
  check(JSON.stringify(reArena.map((e) => [e.attacker_id, e.target_id])) === JSON.stringify(edges.map((e) => [e.attacker_id, e.target_id])), 'arrow directions identical after refresh');

  const X = edges[0].attacker_id;             // attacker team
  const Y = edges[0].target_id;               // its target
  const Z = edges.find((e) => e.attacker_id !== X && e.target_id !== X && e.attacker_id !== Y && e.target_id !== Y)?.attacker_id
    || teams.map((t) => t.id).find((id) => id !== X && id !== Y);
  const x0 = byTeam[X][0], y0 = byTeam[Y][0], z0 = byTeam[Z][0];
  const nameOf = (id) => teams.find((t) => t.id === id).name;

  section('defense preparation and locked snapshots');
  check((await x0.post('/api/team/prep/save', { code: 'x', notes: '' })).status === 409, 'prep save refused during briefing');
  await admin.post('/api/admin/round/advance', { round_id: roundId, from: 'BRIEFING' });
  check((await admin.post('/api/admin/round/advance', { round_id: roundId, from: 'BRIEFING' })).status === 409, 'stale advance (double click) refused');
  check((await y0.post('/api/team/prep/save', { code: C03_REFERENCE.replace('class Tabungan', 'class  Tabungan'), notes: 'hardening' })).status === 200, 'defender saves prep draft');
  check((await x0.post('/api/team/report/save', { title: 't' })).status === 409, 'attack refused before attack window');
  await admin.post('/api/admin/round/advance', { round_id: roundId, from: 'DEFENSE_PREP' });
  const snapFragX = await x0.get('/team/frag/snapshots');
  check(snapFragX.status === 200 && snapFragX.text.includes('Target resmi: <b>' + nameOf(Y) + '</b>'), 'attacker sees ONLY its official target snapshot');
  check(snapFragX.text.includes('class  Tabungan'), 'attacker sees the defender\'s locked (hardened) snapshot');
  const snapFragZ = await z0.get('/team/frag/snapshots');
  check(!snapFragZ.text.includes('class  Tabungan') || Z === Y, 'other teams do not receive that snapshot');
  const arenaC = (await admin.get('/api/snapshot?view=arena')).json.data.arena;
  check(arenaC.teams.every((t) => t.defense.code === 'AWAITING_ATTACK'), 'all defenders AWAITING_ATTACK after snapshot lock');
  check(Object.values(arenaC.stats.teams).every((s) => s.snapshot && s.snapshot.sha.length === 12), 'every team has a locked snapshot hash');

  section('attack report flow and privacy');
  const markY = y0.mark(), markZ = z0.mark(), markX = x0.mark();
  check((await x0.post('/api/team/status', { side: 'attack', status: 'INSPECTING_TARGET' })).status === 200, 'explicit INSPECTING_TARGET');
  check((await x0.post('/api/team/status', { side: 'attack', status: 'VALID_FINDING' })).status === 409, 'verdict status cannot be self-assigned');
  const secret = 'E2E-SECRET saldo dapat ditimpa';
  const draft = await x0.post('/api/team/report/save', {
    title: secret, code_location: 'Tabungan.saldo setter', trigger_input: 't.saldo = 10**9', expected: 'AttributeError', actual: 'saldo berubah',
    repro_steps: 'buat Tabungan lalu assign', root_cause: 'setter tanpa validasi', concept: 'encapsulation / property', evidence: 'def test_x(self): ...',
  });
  check(draft.status === 200 && draft.json.id > 0, 'draft report saved');
  const sub = await x0.post('/api/team/report/submit', { id: draft.json.id });
  check(sub.status === 200, 'report submitted');
  const yRefresh = await y0.waitFor((m) => m.t === 'ev' && m.kind === 'team_refresh' && m.data.parts.includes('reports_in'), markY);
  check(!!yRefresh, 'defender notified in real time');
  const yStatus = await y0.waitFor((m) => m.t === 'ev' && m.kind === 'team_status' && m.data.team_id === Y && m.data.status.code === 'ATTACK_RECEIVED', markY);
  check(!!yStatus, 'defender status ATTACK_RECEIVED from a real submission');
  await sleep(300);
  check(!z0.events(markZ).some((m) => m.kind === 'team_refresh'), 'uninvolved team receives no private refresh');
  const leaked = [x0, y0, z0, ...students].some((s) => s.events(0).some((m) => JSON.stringify(m.data).includes('E2E-SECRET')));
  check(!leaked, 'report title never sent over WebSocket to students');
  check((await y0.get('/team/frag/reports_in')).text.includes('E2E-SECRET'), 'target team can read the report');
  check(!(await z0.get('/team/frag/reports_in')).text.includes('E2E-SECRET') || Z === Y, 'other team cannot read it');
  check(!(await z0.get('/arena/frag/edge/' + X)).text.includes('E2E-SECRET'), 'public edge panel hides report content');
  check((await admin.get('/arena/frag/edge/' + X)).text.includes('E2E-SECRET'), 'instructor edge panel shows report content');
  const dupe = await x0.post('/api/team/report/save', { title: 'dupe', code_location: 'Tabungan.saldo setter', trigger_input: 't.saldo = 10**9', expected: 'a', actual: 'b', repro_steps: 'c', root_cause: 'd', concept: 'e' });
  check((await x0.post('/api/team/report/submit', { id: dupe.json.id })).status === 409, 'identical resubmission refused');
  await x0.post('/api/team/report/delete', { id: dupe.json.id });
  const xStats = (await admin.get('/api/snapshot?view=arena')).json.data.arena.stats.edges.find((e) => e.attacker_id === X);
  check(xStats.counts.official === 1 && xStats.counts.pending === 1, 'arrow shows 1 report, 1 pending');
  check(!!(await x0.waitFor((m) => m.t === 'ev' && m.kind === 'team_status' && m.data.status.code === 'REPORT_SUBMITTED', markX)), 'attacker status REPORT_SUBMITTED');

  section('pause freezes the round');
  check((await admin.post('/api/admin/round/pause', { round_id: roundId })).status === 200, 'pause');
  const paused = (await x0.get('/api/snapshot?view=arena')).json.data.arena.round;
  check(paused.paused === true && paused.timeline.find((p) => p.phase === 'ATTACK_WINDOW').state === 'active', 'timeline shows paused active phase');
  const r1 = paused.remaining_ms;
  await sleep(1200);
  const r2 = (await x0.get('/api/snapshot?view=arena')).json.data.arena.round.remaining_ms;
  check(r1 === r2, 'remaining time does not decrease while paused');
  check((await x0.post('/api/team/report/save', { title: 'p' })).status === 409, 'actions refused while paused');
  await admin.post('/api/admin/round/resume', { round_id: roundId });

  section('defense: patch + regression test (snapshot untouched)');
  const markY2 = y0.mark();
  check((await y0.post('/api/team/patch/save', { code: C03_REFERENCE, root_cause: 'setter', regression_tests: '', explanation: '', addressed: [draft.json.id] })).status === 200, 'patch draft saved');
  check(!!(await y0.waitFor((m) => m.t === 'ev' && m.kind === 'team_status' && m.data.team_id === Y && m.data.status.code === 'FIXING_BUG', markY2)), 'FIXING_BUG only after real code change');
  check((await y0.post('/api/team/patch/submit', {})).status === 400, 'patch without regression test refused');
  await y0.post('/api/team/patch/save', { code: C03_REFERENCE, root_cause: 'setter', regression_tests: C03_TESTS, explanation: 'property read-only', addressed: [draft.json.id] });
  check((await y0.post('/api/team/patch/submit', {})).status === 200, 'patch submitted');
  const snapAgain = await x0.get('/team/frag/snapshots');
  check(snapAgain.text.includes('class  Tabungan'), 'attacker still sees the locked snapshot after defender patched');

  section('chat rooms');
  const markChat = y0.mark();
  const mz = z0.mark();
  check((await x0.post('/api/chat/send', { room: 'team:' + X, body: 'rahasia tim X' })).status === 200, 'team chat message');
  check((await x0.post('/api/chat/send', { room: 'team:' + Y, body: 'mengintip' })).status === 403, 'cannot post into another team');
  check((await x0.post('/api/chat/send', { room: 'broadcast', body: 'hi' })).status === 403, 'students cannot broadcast');
  await admin.post('/api/chat/send', { room: 'broadcast', body: 'Pengumuman E2E' });
  await sleep(400);
  check(!y0.events(markChat).some((m) => m.kind === 'chat' && m.data.body === 'rahasia tim X'), 'team chat not delivered to other teams');
  check(byTeam[X][1].events(0).some((m) => m.kind === 'chat' && m.data.body === 'rahasia tim X'), 'team chat delivered to teammates');
  check(z0.events(mz).some((m) => m.kind === 'chat' && m.data.room === 'broadcast' && m.data.body === 'Pengumuman E2E'), 'broadcast delivered to everyone');
  const hist = await y0.get('/api/chat/history?room=team:' + X);
  check(hist.status === 403, 'history of another team is forbidden');

  section('verification, scoring and finalization');
  await admin.post('/api/admin/round/advance', { round_id: roundId, from: 'ATTACK_WINDOW' });
  check((await x0.post('/api/team/report/save', { title: 'late' })).status === 409, 'reports locked after attack window');
  const markS = x0.mark();
  check((await admin.post('/api/admin/review/report', { report_id: draft.json.id, verdict: 'valid', repro: 10, rootcause: 10, evidence: 5, note: 'tepat' })).status === 200, 'instructor validates report');
  const scoreEv = await x0.waitFor((m) => m.t === 'ev' && m.kind === 'scores', markS);
  const xScore = scoreEv && scoreEv.data.scores.find((s) => s.team_id === X);
  check(xScore && xScore.round.attack === 25, 'attack score 25 pushed live');
  await admin.post('/api/admin/round/advance', { round_id: roundId, from: 'DEFENSE_RESPONSE' });
  const yStatus2 = (await admin.get('/api/snapshot?view=arena')).json.data.arena.teams.find((t) => t.id === Y).defense.code;
  check(yStatus2 === 'AWAITING_REVIEW', 'defender AWAITING_REVIEW in verification');
  const reviewFrag = await admin.get('/admin/frag/review');
  const pid = Number((reviewFrag.text.match(/data-review-patch="[^"]*?&quot;id&quot;:(\d+)/) || [])[1]);
  check(pid > 0, 'patch visible in review queue');
  check((await admin.post('/api/admin/review/patch', { patch_id: pid, verdict: 'accepted', rootfix: 20, regression: 15, edge: 10, explain: 5, note: '' })).status === 200, 'patch accepted');
  await admin.post('/api/admin/round/advance', { round_id: roundId, from: 'VERIFICATION' });
  const fin = await admin.post('/api/admin/round/finalize', { round_id: roundId, force: false });
  check(fin.status === 200, 'round finalized');
  check(fin.json.scores[String(X)].attack === 25 && fin.json.scores[String(Y)].defense === 50, 'final scores follow rubric');
  check((await admin.post('/api/admin/round/finalize', { round_id: roundId })).status === 409, 'cannot finalize twice');
  check((await admin.post('/api/admin/review/report', { report_id: draft.json.id, verdict: 'invalid', note: 'x' })).status === 409, 'finalized results cannot be re-reviewed');
  const s1 = (await y0.get('/api/snapshot?view=arena')).json.data.arena.scores;
  y0.close();
  await sleep(200);
  const snapRe = await y0.connect('team');
  const s2 = snapRe.data.arena.scores;
  check(JSON.stringify(s1) === JSON.stringify(s2), 'reconnect restores identical official scores (no double counting)');
  check(s2.find((s) => s.team_id === Y).cumulative === 50 && s2.find((s) => s.team_id === Y).rank === 1, 'leaderboard ranks from server');
  check((await admin.post('/api/admin/score/adjust', { team_id: X, round_id: roundId, points: 3, reason: '' })).status === 400, 'score correction requires a reason');
  check((await admin.post('/api/admin/score/adjust', { team_id: X, round_id: roundId, points: 3, reason: 'koreksi e2e' })).status === 200, 'reasoned score correction');
  check((await admin.post('/api/admin/round/discussion', { round_id: roundId, open: true })).status === 200, 'discussion opened');
  check((await z0.get('/team/frag/challenge')).text.includes('Reference solution'), 'students see answer key after discussion opens');

  section('pages render');
  for (const [c, path, marker] of [[x0, '/team', 'Patch editor'], [x0, '/arena', 'Network view'], [x0, '/wait', 'Waiting room'], [admin, '/admin', 'Ronde &amp; Fase'], [admin, '/admin/challenges', 'Cakupan materi'], [admin, '/admin/challenges/C13', 'The Broken E-Wallet'], [admin, '/arena/frag/team/' + X, 'Histori perubahan skor'], [anon, '/', 'Join Arena'], [anon, '/admin/login', 'Login instruktur']]) {
    const r = await c.get(path);
    check(r.status === 200 && r.text.includes(marker), 'GET ' + path + ' renders');
  }
  for (const f of ['round', 'teams', 'review', 'scores', 'chat', 'roster', 'session', 'audit']) {
    const r = await admin.get('/admin/frag/' + f);
    check(r.status === 200, 'admin fragment ' + f);
  }
  const csv = await admin.get('/admin/export/scores.csv');
  check(csv.status === 200 && csv.text.startsWith('rank,team'), 'CSV export');
  const js = await admin.get('/admin/export/session.json');
  check(js.status === 200 && js.json.rounds.length === 1 && js.json.audit_log.length > 10, 'JSON export with audit log');

  section('device release');
  const markR = students[5].mark();
  await admin.post('/api/admin/participant/release', { nim: students[5].name });
  check(!!(await students[5].waitFor((m) => m.t === 'ev' && m.kind === 'force_logout', markR)), 'released student receives force_logout');
  check((await students[5].get('/api/snapshot?view=wait')).status === 401, 'released token no longer valid');

  [admin, ...students].forEach((s) => s.close());
  console.log('\nRESULT: ' + passed + ' passed, ' + failed + ' failed');
  process.exit(failed ? 1 : 0);
}

main().catch((e) => { console.error(e); process.exit(2); });
