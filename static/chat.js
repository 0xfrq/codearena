/* Chat widget: team chat, cross-team lobby, instructor broadcast.
   Sender identity is assigned by the server; this widget only renders and posts text. */
'use strict';
(function () {
  const widgets = [];
  const LABELS = { lobby: 'Lobby', broadcast: 'Broadcast instruktur' };

  function roomLabel(room, custom) {
    if (custom) return custom;
    if (LABELS[room]) return LABELS[room];
    if (room.startsWith('team:')) return 'Chat tim';
    return room;
  }

  function mount(container) {
    if (!container || container.dataset.mounted) return;
    container.dataset.mounted = '1';
    const rooms = (container.dataset.rooms || 'lobby,broadcast').split(',').filter(Boolean);
    const labels = (container.dataset.labels || '').split('|');
    const instructor = !!container.dataset.instructor;
    const w = { container, rooms, instructor, active: rooms[0], logs: {}, unread: {}, loaded: {}, lobbyLocked: false };
    container.innerHTML =
      '<div class="chat">' +
      '<div class="chat-tabs">' + rooms.map((r, i) => '<button type="button" data-room="' + CA.esc(r) + '">' + CA.esc(roomLabel(r, labels[i])) + '<span class="unread hidden">0</span></button>').join('') + '</div>' +
      '<div class="chat-log" aria-live="polite"></div>' +
      '<form class="chat-form"><input type="text" maxlength="500" placeholder="Tulis pesan… (maks 500 karakter)" autocomplete="off"><button class="btn btn-primary" type="submit">Kirim</button></form>' +
      '<div class="small muted chat-note"></div>' +
      '</div>';
    w.log = container.querySelector('.chat-log');
    w.form = container.querySelector('.chat-form');
    w.input = w.form.querySelector('input');
    w.note = container.querySelector('.chat-note');
    container.querySelectorAll('[data-room]').forEach((b) => b.addEventListener('click', () => select(w, b.dataset.room)));
    w.form.addEventListener('submit', async (e) => {
      e.preventDefault();
      const body = w.input.value.trim();
      if (!body) return;
      w.input.disabled = true;
      try {
        await CA.api('/api/chat/send', { room: w.active, body, as: w.instructor ? 'instructor' : 'student' });
        w.input.value = '';
      } catch (err) {
        CA.toast(err.message, 'error');
      } finally {
        w.input.disabled = false;
        w.input.focus();
      }
    });
    w.log.addEventListener('click', async (e) => {
      const del = e.target.closest('[data-del]');
      if (!del) return;
      if (!confirm('Hapus pesan ini?')) return;
      try { await CA.api('/api/admin/chat/delete', { message_id: Number(del.dataset.del) }); } catch (err) { CA.toast(err.message, 'error'); }
    });
    widgets.push(w);
    rooms.forEach((r) => load(w, r));
    select(w, w.active);
  }

  async function load(w, room) {
    try {
      const r = await fetch('/api/chat/history?room=' + encodeURIComponent(room) + '&as=' + (w.instructor ? 'instructor' : 'student'), { credentials: 'same-origin' });
      const data = await r.json();
      if (!r.ok) throw new Error(data.error || 'gagal');
      w.logs[room] = data.messages.map((m) => ({ id: m.id, room: m.room, sender_kind: m.sender_kind, sender_label: m.sender_label, body: m.body, at: m.created_at, deleted: m.deleted === 1 }));
      w.loaded[room] = true;
      if (room === w.active) render(w);
    } catch (e) {
      w.logs[room] = [];
    }
  }

  function select(w, room) {
    w.active = room;
    w.unread[room] = 0;
    w.container.querySelectorAll('[data-room]').forEach((b) => {
      b.classList.toggle('active', b.dataset.room === room);
      const u = b.querySelector('.unread');
      const n = w.unread[b.dataset.room] || 0;
      u.textContent = n;
      u.classList.toggle('hidden', n === 0);
    });
    const canPost = room === 'broadcast' ? w.instructor : room === 'lobby' ? (w.instructor || !w.lobbyLocked) : true;
    w.form.classList.toggle('hidden', !canPost);
    w.note.textContent = room === 'broadcast' ? (w.instructor ? 'Broadcast terlihat oleh semua peserta.' : 'Pengumuman resmi instruktur (hanya-baca).')
      : room === 'lobby' ? (w.lobbyLocked && !w.instructor ? 'Lobby sedang ditutup oleh instruktur.' : 'Lobby terlihat oleh semua kelompok.')
      : 'Chat privat anggota tim (instruktur dapat memoderasi).';
    render(w);
    updateBadge();
  }

  function render(w) {
    const list = w.logs[w.active] || [];
    const atBottom = w.log.scrollHeight - w.log.scrollTop - w.log.clientHeight < 40;
    w.log.innerHTML = list.length === 0 ? '<div class="muted small">Belum ada pesan.</div>' : list.map((m) =>
      '<div class="msg ' + CA.esc(m.sender_kind) + (m.deleted ? ' deleted' : '') + '">' +
      '<span class="meta">' + CA.fmtTime(m.at) + '</span> <span class="from">' + CA.esc(m.sender_label) + '</span>: ' +
      '<span class="body">' + CA.esc(m.deleted && !w.instructor ? '[pesan dihapus oleh moderator]' : m.body) + '</span>' +
      (m.deleted && w.instructor ? ' <span class="small muted">(dihapus)</span>' : '') +
      (w.instructor && !m.deleted && m.sender_kind !== 'system' ? ' <button class="btn btn-sm btn-ghost" type="button" data-del="' + m.id + '" title="Hapus pesan">✕</button>' : '') +
      '</div>').join('');
    if (atBottom || list.length < 20) w.log.scrollTop = w.log.scrollHeight;
  }

  function updateBadge() {
    const el = document.getElementById('cnt-chat');
    if (!el) return;
    const total = widgets.reduce((s, w) => s + Object.values(w.unread).reduce((a, b) => a + b, 0), 0);
    el.textContent = total;
    el.classList.toggle('hidden', total === 0);
  }

  function onEvent(kind, data) {
    widgets.forEach((w) => {
      if (kind === 'chat') {
        if (!w.rooms.includes(data.room)) return;
        const list = w.logs[data.room] || (w.logs[data.room] = []);
        if (list.some((m) => m.id === data.id)) return;
        list.push({ id: data.id, room: data.room, sender_kind: data.sender_kind, sender_label: data.sender_label, body: data.body, at: data.at, deleted: false });
        if (list.length > 200) list.shift();
        if (data.room === w.active && !document.hidden) render(w);
        else {
          w.unread[data.room] = (w.unread[data.room] || 0) + 1;
          const b = w.container.querySelector('[data-room="' + data.room + '"] .unread');
          if (b) { b.textContent = w.unread[data.room]; b.classList.remove('hidden'); }
          if (data.room === w.active) render(w);
        }
        if (data.room === 'broadcast' && !w.instructor && data.sender_kind !== 'student') {
          CA.toast('📢 ' + data.body);
        }
        updateBadge();
      } else if (kind === 'chat_deleted') {
        const list = w.logs[data.room] || [];
        list.forEach((m) => { if (m.id === data.id) m.deleted = true; });
        if (data.room === w.active) render(w);
      } else if (kind === 'lobby_locked') {
        w.lobbyLocked = !!data.locked;
        select(w, w.active);
      } else if (kind === 'chat_muted') {
        CA.toast(data.until ? 'Anda di-mute oleh instruktur sampai ' + CA.fmtTime(data.until) : 'Mute dicabut.', data.until ? 'error' : 'ok');
      }
    });
  }

  window.CAChat = { mount, onEvent, widgets };
})();
