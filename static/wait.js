/* Waiting room: live attendance and assignment status (all data from the server snapshot). */
'use strict';
(function () {
  let state = null;
  const me = document.getElementById('chat').dataset.me;
  CAChat.mount(document.getElementById('chat'));

  function render() {
    const w = state;
    if (!w) return;
    CA.animateNumber(document.getElementById('w-joined'), w.counts.joined);
    document.getElementById('w-online').textContent = w.counts.online;
    document.getElementById('w-assigned').textContent = w.counts.assigned;
    const list = document.getElementById('w-list');
    list.innerHTML = w.participants.filter((p) => p.joined).map((p) =>
      '<span class="chip' + (p.nim === me ? ' me' : '') + (p.assigned ? ' assigned' : '') + '" data-nim="' + CA.esc(p.nim) + '">' +
      '<span class="dot' + (p.online ? ' on' : '') + '" title="' + (p.online ? 'online' : 'offline') + '"></span>' + CA.esc(p.nim) +
      (p.team ? ' <span class="small muted">· ' + CA.esc(p.team) + '</span>' : '') + '</span>').join('') || '<span class="muted">Belum ada peserta.</span>';
    const status = document.getElementById('w-status');
    const msg = document.getElementById('w-message');
    const teamBox = document.getElementById('w-team');
    if (w.my_team) {
      status.className = 'badge big tone-success';
      status.innerHTML = '<span class="ico">✓</span><span>Kelompok terkonfirmasi</span>';
      msg.className = 'notice ok';
      msg.textContent = 'Kelompok Anda sudah ditetapkan instruktur.';
      teamBox.classList.remove('hidden');
      document.getElementById('w-team-name').textContent = w.my_team.name;
      document.getElementById('w-team-members').innerHTML = w.my_team.members.map((m) =>
        '<span class="chip' + (m.nim === me ? ' me' : '') + '"><span class="dot' + (m.online ? ' on' : '') + '"></span>' + CA.esc(m.nim) + '</span>').join('');
      document.getElementById('team-link').classList.remove('hidden');
    } else {
      teamBox.classList.add('hidden');
      if (w.assignment === 'preview') {
        status.className = 'badge big tone-review';
        status.innerHTML = '<span class="ico">◷</span><span>Instruktur menyusun kelompok</span>';
        msg.className = 'notice';
        msg.textContent = 'Instruktur sedang meninjau pembagian kelompok. Hasil akan muncul setelah dikonfirmasi.';
      } else if (w.assignment === 'confirmed') {
        status.className = 'badge big tone-investigate';
        status.innerHTML = '<span class="ico">◎</span><span>Menunggu penempatan</span>';
        msg.className = 'notice warn';
        msg.textContent = 'Kelompok sudah dibentuk, tetapi Anda belum ditempatkan (mis. datang setelah pembagian). Tunggu instruktur menempatkan Anda.';
      } else {
        status.className = 'badge big tone-idle';
        status.innerHTML = '<span class="ico">○</span><span>Menunggu instruktur</span>';
        msg.className = 'notice';
        msg.textContent = 'Kelompok belum dibentuk. Tunggu instruktur menyusun kelompok — halaman ini diperbarui otomatis.';
      }
    }
  }

  const conn = CA.connect('wait', {
    onSnapshot(data) { state = data.waiting; render(); },
    onEvent(kind, data) {
      if (kind === 'presence') {
        if (!state) return;
        state.participants.forEach((p) => { if (p.nim === data.nim) p.online = data.online; });
        if (state.my_team) state.my_team.members.forEach((m) => { if (m.nim === data.nim) m.online = data.online; });
        state.counts.online = data.online_count;
        render();
      } else if (kind === 'participant') {
        conn.resync();
      } else if (kind.startsWith('chat') || kind === 'lobby_locked') {
        CAChat.onEvent(kind, data);
      }
    },
  });
})();
