// Real-browser check via the Chrome DevTools Protocol (headless Chrome/Edge).
// Prepares a mid-round game state through the API, then loads each page with the right login
// cookie, records JS exceptions / console errors, performs clicks and saves screenshots.
//
//   node tools/browser-check.mjs <chrome.exe> <screenshot-dir>
//   env: BASE (default http://127.0.0.1:8090), PW (instructor password)
import { spawn } from 'node:child_process';
import { mkdirSync, writeFileSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const CHROME = process.argv[2];
const OUT = process.argv[3] || './screens';
const BASE = process.env.BASE || 'http://127.0.0.1:8090';
const PW = process.env.PW || 'admin-test-pw';
const PORT = 9333;
mkdirSync(OUT, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failures = 0;
const check = (c, m) => { console.log((c ? '  \u2713 ' : '  \u2717 ') + m); if (!c) failures++; };

async function http(method, path, body, cookie) {
  const r = await fetch(BASE + path, { method, headers: { 'content-type': 'application/json', cookie: cookie || '' }, body: body ? JSON.stringify(body) : undefined });
  const set = (r.headers.getSetCookie ? r.headers.getSetCookie() : []).map((c) => c.split(';')[0]);
  let json = null;
  try { json = await r.json(); } catch (_) { /* ignore */ }
  return { status: r.status, json, cookie: set[0] };
}

async function prepareState() {
  const admin = (await http('POST', '/api/admin/login', { password: PW })).cookie;
  const snap = (await http('GET', '/api/snapshot?view=arena', null, admin)).json.data.arena;
  const students = {};
  for (const t of snap.teams) {
    for (const m of t.members) {
      const j = await http('POST', '/api/join', { nim: m.nim });
      if (j.cookie) students[m.nim] = { cookie: j.cookie, team: t.id };
    }
  }
  const round = await http('POST', '/api/admin/round/create', { challenge_id: 'C13', briefing: 0, defense_prep: 10, attack_window: 20, defense_response: 10, verification: 0, auto_advance: false }, admin);
  if (round.status !== 200) throw new Error('create round: ' + JSON.stringify(round.json));
  const rid = round.json.round_id;
  await http('POST', '/api/admin/round/start', { round_id: rid }, admin);
  await http('POST', '/api/admin/round/advance', { round_id: rid, from: 'BRIEFING' }, admin);
  await http('POST', '/api/admin/round/advance', { round_id: rid, from: 'DEFENSE_PREP' }, admin);
  const arena = (await http('GET', '/api/snapshot?view=arena', null, admin)).json.data.arena;
  const first = (teamId) => Object.entries(students).find(([, s]) => s.team === teamId);
  const edges = arena.stats.edges;
  // edge 0: report submitted; edge 1: attacker inspecting; edge 2: defender fixing after report
  const [nimA, a] = first(edges[0].attacker_id);
  const rep = await http('POST', '/api/team/report/save', { title: 'Riwayat dibagi semua akun', code_location: 'Akun._riwayat', trigger_input: 'a.topup(10000); len(b.riwayat)', expected: '0', actual: '1', repro_steps: 'buat dua akun', root_cause: 'class attribute mutable', concept: 'class attribute', evidence: 'def test_b1(self): ...' }, a.cookie);
  await http('POST', '/api/team/report/submit', { id: rep.json.id }, a.cookie);
  const [, b] = first(edges[1].attacker_id);
  await http('POST', '/api/team/status', { side: 'attack', status: 'INSPECTING_TARGET' }, b.cookie);
  const [, c] = first(edges[2].attacker_id);
  const rep2 = await http('POST', '/api/team/report/save', { title: 'Tarik mahasiswa tidak mengurangi saldo', code_location: 'AkunMahasiswa.tarik', trigger_input: 'm.tarik(100000) dua kali', expected: 'ValueError', actual: 'berhasil', repro_steps: '-', root_cause: 'name mangling', concept: 'name mangling + overriding', evidence: '' }, c.cookie);
  await http('POST', '/api/team/report/submit', { id: rep2.json.id }, c.cookie);
  const [, d] = first(edges[2].target_id);
  await http('POST', '/api/team/defense/report', { report_id: rep2.json.id, state: 'fixing', analysis: 'override menulis self.__saldo' }, d.cookie);
  await http('POST', '/api/chat/send', { room: 'broadcast', body: 'Attack window ronde 2 dibuka — semangat!' }, admin);
  return { admin, student: a.cookie, studentNim: nimA, defender: d.cookie };
}

/* ---------------------------------------------------------------- minimal CDP client */
async function cdp() {
  let info = null;
  for (let i = 0; i < 40 && !info; i++) {
    try { info = await (await fetch('http://127.0.0.1:' + PORT + '/json/version')).json(); } catch (_) { await sleep(250); }
  }
  if (!info) throw new Error('browser did not start');
  const ws = new WebSocket(info.webSocketDebuggerUrl);
  await new Promise((r) => { ws.onopen = r; });
  let id = 0;
  const waiting = new Map();
  const listeners = [];
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data);
    if (msg.id && waiting.has(msg.id)) { waiting.get(msg.id)(msg); waiting.delete(msg.id); } else listeners.forEach((l) => l(msg));
  };
  const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
    const mid = ++id;
    waiting.set(mid, (msg) => (msg.error ? reject(new Error(method + ': ' + msg.error.message)) : resolve(msg.result)));
    ws.send(JSON.stringify({ id: mid, method, params, sessionId }));
  });
  return { send, on: (f) => listeners.push(f), close: () => ws.close() };
}

async function openPage(c, url, cookies, name, actions) {
  const { browserContextId } = await c.send('Target.createBrowserContext', {}); // isolated cookie jar per page
  const { targetId } = await c.send('Target.createTarget', { url: 'about:blank', browserContextId });
  const { sessionId } = await c.send('Target.attachToTarget', { targetId, flatten: true });
  const errors = [];
  c.on((msg) => {
    if (msg.sessionId !== sessionId) return;
    if (msg.method === 'Runtime.exceptionThrown') errors.push('exception: ' + (msg.params.exceptionDetails.exception?.description || msg.params.exceptionDetails.text));
    if (msg.method === 'Runtime.consoleAPICalled' && msg.params.type === 'error') errors.push('console.error: ' + msg.params.args.map((a) => a.value || a.description).join(' '));
    if (msg.method === 'Log.entryAdded' && msg.params.entry.level === 'error' && !/favicon/.test(msg.params.entry.url || '')) errors.push('log: ' + msg.params.entry.text + ' ' + (msg.params.entry.url || ''));
  });
  for (const m of ['Page.enable', 'Runtime.enable', 'Log.enable', 'Network.enable']) await c.send(m, {}, sessionId);
  await c.send('Emulation.setDeviceMetricsOverride', { width: 1600, height: 1000, deviceScaleFactor: 1, mobile: false }, sessionId);
  for (const ck of cookies) {
    const [k, v] = ck.split('=');
    await c.send('Network.setCookie', { name: k, value: v, url: BASE }, sessionId);
  }
  await c.send('Page.navigate', { url: BASE + url }, sessionId);
  await sleep(2500);
  const evaluate = async (expr) => (await c.send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true }, sessionId)).result.value;
  const shot = async (suffix) => {
    const { data } = await c.send('Page.captureScreenshot', { format: 'png' }, sessionId);
    const file = join(OUT, name + (suffix ? '-' + suffix : '') + '.png');
    writeFileSync(file, Buffer.from(data, 'base64'));
    return file;
  };
  console.log('\n== ' + name + ' (' + url + ')');
  await shot();
  if (actions) await actions(evaluate, shot);
  check(errors.length === 0, 'no JS errors' + (errors.length ? ': ' + errors.join(' | ') : ''));
  await c.send('Target.closeTarget', { targetId });
  await c.send('Target.disposeBrowserContext', { browserContextId });
}

async function main() {
  const st = await prepareState();
  const profile = mkdtempSync(join(tmpdir(), 'ca-chrome-'));
  const proc = spawn(CHROME, ['--headless=new', '--remote-debugging-port=' + PORT, '--user-data-dir=' + profile, '--no-first-run', '--disable-gpu', '--window-size=1600,1000', 'about:blank'], { stdio: 'ignore' });
  const c = await cdp();
  try {
    await openPage(c, '/', [], 'landing', async (ev) => {
      check(await ev("document.querySelector('#join-form') !== null"), 'join form present');
    });
    await openPage(c, '/arena', [st.admin], 'arena-instructor', async (ev, shot) => {
      check(await ev("document.getElementById('conn').textContent") === 'LIVE', 'WebSocket LIVE indicator');
      const nodes = await ev("document.querySelectorAll('#a-graph .node').length");
      const arrows = await ev("document.querySelectorAll('#a-graph g.edge path.line').length");
      check(nodes === 5, 'graph shows 5 team nodes (' + nodes + ')');
      check(arrows === 5, 'graph shows 5 ATTACK arrows (' + arrows + ')');
      check(await ev("[...document.querySelectorAll('#a-graph .edge text')].some(t => t.textContent.includes('1 pending'))"), 'arrow label shows pending report');
      check(await ev("document.getElementById('a-phase').textContent.includes('ATTACK WINDOW')"), 'summary shows ATTACK WINDOW');
      check(await ev("document.querySelectorAll('#a-timeline .tl-step.completed').length === 2 && document.querySelectorAll('#a-timeline .tl-step.active').length === 1"), 'timeline: 2 completed, 1 active');
      check(await ev("/\\d\\d:\\d\\d/.test(document.getElementById('a-clock').textContent)"), 'countdown running');
      await ev("document.querySelector('#a-graph .node').dispatchEvent(new MouseEvent('click', {bubbles: true}))");
      await sleep(900);
      check(await ev("document.getElementById('drawer').classList.contains('open') && document.querySelector('#drawer-body .inspector') !== null"), 'click node opens team inspector without leaving arena');
      check(await ev("document.querySelector('#drawer-body').textContent.includes('Histori perubahan skor')"), 'inspector shows score history');
      await shot('inspector');
      await ev("document.getElementById('drawer-close').click()");
      await ev("document.querySelector('#a-graph path.hit').dispatchEvent(new MouseEvent('click', {bubbles: true}))");
      await sleep(900);
      check(await ev("document.querySelector('#drawer-body').textContent.includes('Relasi serangan resmi')"), 'click arrow opens attack detail panel');
      await shot('edge');
      await ev("document.getElementById('drawer-close').click()");
      await ev("document.querySelector('[data-view=board]').click()");
      await sleep(500);
      check(await ev("document.querySelectorAll('#a-board .tcard').length") === 5, 'team board shows 5 cards');
      check(await ev("document.querySelector('#a-board .tcard').textContent.includes('ATTACK') && document.querySelector('#a-board .tcard').textContent.includes('DEFENSE')"), 'cards separate ATTACK and DEFENSE');
      await shot('board');
      await ev("document.querySelector('[data-arena-filter=fixing]').click()");
      await sleep(400);
      const dimmed = await ev("document.querySelectorAll('#a-board .tcard.dim').length");
      check(dimmed === 4, 'filter "bug fixing" highlights only the fixing team (' + (5 - dimmed) + ' shown)');
      await shot('board-filter');
      await ev("document.querySelector('[data-arena-filter=all]').click(); document.querySelector('[data-view=graph]').click()");
      await ev("document.querySelector('[data-list=reports]').click()");
      await sleep(500);
      check(await ev("document.querySelector('#drawer-body table') !== null"), 'summary number opens detail list');
    });
    await openPage(c, '/arena', [st.student], 'arena-student', async (ev) => {
      check(await ev("document.querySelectorAll('#a-graph .node').length") === 5, 'student arena renders graph');
      check(!(await ev("document.body.innerHTML.includes('Riwayat dibagi semua akun')")), 'student arena does not contain report titles');
    });
    await openPage(c, '/team', [st.student], 'team-attacker', async (ev, shot) => {
      check(await ev("document.getElementById('t-attack').textContent.includes('Laporan terkirim')"), 'attack status badge from server');
      check(await ev("document.getElementById('t-defense').textContent.length > 0"), 'defense status badge separate');
      await ev("document.querySelector('[data-tab=attack]').click()");
      await sleep(300);
      check(await ev("document.querySelector('#frag-snapshots pre.code .ln') !== null"), 'locked target snapshot rendered with line numbers');
      check(await ev("document.querySelector('#frag-reports_out').textContent.includes('Menunggu validasi instruktur')"), 'submitted report shows pending (no premature validity claim)');
      await shot('attack-tab');
    });
    await openPage(c, '/team', [st.defender], 'team-defender', async (ev, shot) => {
      await ev("document.querySelector('[data-tab=defense]').click()");
      await sleep(300);
      check(await ev("document.getElementById('t-defense').textContent.includes('Memperbaiki bug')"), 'defender status FIXING_BUG');
      check(await ev("document.querySelector('#frag-reports_in').textContent.includes('Tarik mahasiswa tidak mengurangi saldo')"), 'incoming report visible to target team');
      await shot('defense-tab');
    });
    await openPage(c, '/admin', [st.admin], 'admin', async (ev, shot) => {
      check(await ev("document.querySelectorAll('[data-admin-frag]').length") === 8, 'all 8 dashboard sections');
      await ev("document.querySelector('[data-review-report]').click()");
      await sleep(300);
      check(await ev("document.getElementById('dlg-review-report').open"), 'review dialog opens');
      await shot('review-dialog');
    });
    await openPage(c, '/wait', [st.student], 'waiting', async (ev) => {
      check(await ev("document.querySelectorAll('#w-list .chip').length") >= 20, 'waiting room lists attendees');
    });
  } finally {
    c.close();
    proc.kill();
  }
  console.log('\nRESULT: ' + (failures ? failures + ' failed' : 'all browser checks passed'));
  process.exit(failures ? 1 : 0);
}

main().catch((e) => { console.error(e); process.exit(2); });
