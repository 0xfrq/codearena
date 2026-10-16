# Code Arena

*find the flaw. prove the bug. defend your code.*

Code Arena adalah platform recap praktikum **Pemrograman Berorientasi Objek (PBO) Python** dengan mekanik kompetisi **attack & defense** ala CTF. Semua kelompok bertanding secara simultan: setiap tim menyerang implementasi satu tim lawan (berdasarkan *locked snapshot*) sambil mempertahankan dan memperbaiki implementasinya sendiri. Instruktur memantau semuanya secara real-time di **Live Arena**.

Satu binary Rust (Axum + Tokio + Askama + SQLx/SQLite + WebSocket) melayani seluruh kelas di jaringan lokal. Frontend hanya HTML/CSS/vanilla JS — tanpa CDN, sehingga tetap jalan di lab tanpa internet.

---

## Daftar isi

1. [Fitur](#fitur)
2. [Menjalankan (development)](#menjalankan-development)
3. [Deployment di lab (LAN)](#deployment-di-lab-lan)
4. [Konfigurasi](#konfigurasi)
5. [Alur kelas untuk instruktur](#alur-kelas-untuk-instruktur)
6. [Aturan permainan](#aturan-permainan)
7. [Challenge bank](#challenge-bank)
8. [Arsitektur](#arsitektur)
9. [Keamanan & integritas data](#keamanan--integritas-data)
10. [Pengujian](#pengujian)
11. [Keterbatasan yang diketahui](#keterbatasan-yang-diketahui)
12. [Struktur proyek](#struktur-proyek)

---

## Fitur

| Area | Isi |
|---|---|
| Join | Mahasiswa join dengan NIM (divalidasi terhadap roster, 20 NIM awal sudah di-seed). Satu NIM = satu perangkat aktif; reconnect memulihkan identitas & kelompok. |
| Waiting room | Jumlah hadir, daftar kehadiran + status online, status pembentukan kelompok — real-time. |
| Assignment | **Random** (snapshot peserta yang menunggu, dibagi seimbang, preview → konfirmasi) dan **manual** (buat/namai kelompok, tambah/pindah/keluarkan anggota). Peserta terlambat tetap menunggu; dapat ditempatkan eksplisit ke kelompok terkecil. Kelompok dikunci saat ronde pertama dimulai; perubahan setelahnya wajib beralasan dan tercatat. |
| Round engine | Fase resmi: Briefing → Defense Preparation → Attack Window → Defense Response → Verification → Round Results. Timer bersumber dari server, auto-advance opsional, pause/resume (timer dibekukan), tambah/kurangi waktu. |
| Fairness | Pairing ring (setiap tim menyerang tepat satu & diserang tepat satu), dikunci saat ronde dimulai. **Locked defense snapshot v1** (SHA-256) dibuat saat Attack Window dibuka; patch defender masuk v2, v3, … dan tidak pernah mengubah snapshot yang diuji penyerang. |
| Attack | Workspace dengan snapshot target, form laporan lengkap (lokasi, pemicu, expected/actual, reproduksi, root cause, konsep PBO, bukti), draft → konfirmasi submit → terkunci. Revisi hanya jika instruktur meminta (tercatat di `report_revisions`). |
| Defense | Incoming reports dengan status independen per laporan, catatan analisis, rebuttal, patch editor + regression test + penjelasan, riwayat patch & feedback review. |
| Status | **Attack status** (10 kode) dan **defense status** (12 kode) terpisah, masing-masing dengan kode, label, ikon, warna, alasan, dan timestamp. Hanya berubah karena event nyata (submit, simpan patch, review, pilihan eksplisit tim) — tidak pernah karena halaman dibuka. |
| Live Arena | Network view (attack graph SVG: node tim + panah ATTACK berarah, label jumlah laporan/pending/valid), Team board (kartu ATTACK/DEFENSE/ROUND STATUS terpisah), global summary yang dapat diklik, round timeline (completed/active/upcoming + paused), filter tampilan, leaderboard, feed aktivitas, team inspector & detail serangan di drawer, zoom. |
| Scoring | Rubric default: attack 10/10/5 per temuan valid (maks 2 temuan dihitung), defense 20/15/10/5 (patch terbaik, maks 50), rebuttal ≤ 10. Duplikat = 0. Skor dihitung ulang dari data review (idempotent), difinalisasi ke tabel immutable, koreksi hanya via adjustment beralasan + audit log. |
| Chat | Team chat (privat), cross-team lobby, instructor broadcast. WebSocket, histori persisten, rate limit (5 pesan/10 detik), maks 500 karakter, moderasi (hapus pesan, mute, tutup lobby). |
| Instruktur | Roster, waiting room, assignment, ronde & pairing, kontrol fase, review laporan & patch (dialog rubric), koreksi skor, broadcast, moderasi chat, buka pembahasan (answer key + semua laporan), finalisasi, ekspor CSV/JSON, reset sesi (arsip), audit log, halaman challenge bank + peta cakupan materi. |

---

## Menjalankan (development)

### Prasyarat

* **Rust 1.80+** (diuji dengan 1.96.1).
* Toolchain linker:
  * **Windows (MSVC)** — pasang *Visual Studio Build Tools* dengan workload "Desktop development with C++" (menyediakan `link.exe`), **atau**
  * **Windows (GNU)** — toolchain `x86_64-pc-windows-gnu` + MinGW-w64 `gcc` di `PATH` (SQLite di-compile dari source oleh `libsqlite3-sys`).
  * **Linux/macOS** — `cc`/`gcc`/`clang` standar.
* **Python 3.10+** *opsional*, hanya untuk `tools/verify_challenges.py` (server **tidak** menjalankan Python).
* **Node 22+** *opsional*, hanya untuk skrip uji `tools/e2e.mjs` dan `tools/browser-check.mjs`.

### Jalankan

```bash
cargo run --release
# Code Arena listening on http://0.0.0.0:8080
```

Saat pertama kali dijalankan tanpa `CODEARENA_ADMIN_PASSWORD`, server membuat password instruktur acak dan menuliskannya ke `data/admin-password.txt` (hanya hash yang disimpan di database).

* Mahasiswa: `http://localhost:8080/`
* Instruktur: `http://localhost:8080/admin`
* Live Arena (proyektor): `http://localhost:8080/arena`

> **Catatan Windows tanpa Visual Studio** (kondisi mesin pengembangan proyek ini): pasang toolchain GNU terpisah tanpa mengubah instalasi Rust yang ada, lalu jalankan cargo dengannya:
>
> ```powershell
> $env:RUSTUP_HOME='D:\rust-gnu\rustup'; $env:CARGO_HOME='D:\rust-gnu\cargo'
> .\rustup-init.exe -y --default-host x86_64-pc-windows-gnu --profile minimal --no-modify-path
> $env:PATH="D:\rust-gnu\cargo\bin;C:\MinGW\bin;$env:PATH"   # C:\MinGW\bin = MinGW-w64 gcc
> cargo build --release
> ```

---

## Deployment di lab (LAN)

1. Build sekali: `cargo build --release` → binary tunggal `target/release/codearena(.exe)`. Template, CSS, JS, migrasi, dan challenge bank sudah tertanam di binary.
2. Salin binary ke komputer server (boleh laptop instruktur). Buat folder kerja; database dibuat otomatis di `./data/codearena.db`.
3. Jalankan dengan password instruktur sendiri:
   ```powershell
   $env:CODEARENA_ADMIN_PASSWORD = 'ganti-password-ini'
   .\codearena.exe
   ```
   ```bash
   CODEARENA_ADMIN_PASSWORD='ganti-password-ini' ./codearena
   ```
4. Izinkan port di firewall (Windows: *Allow an app through firewall* atau `New-NetFirewallRule -DisplayName "Code Arena" -Direction Inbound -Protocol TCP -LocalPort 8080 -Action Allow`).
5. Cari IP server (`ipconfig` / `ip addr`), lalu bagikan `http://<IP-server>:8080/` ke mahasiswa. Buka `/arena` di proyektor dan `/admin` di laptop instruktur.
6. **Gunakan browser/profil terpisah** untuk akun instruktur dan akun mahasiswa uji coba (cookie login dipisah per peran; halaman otomatis memakai identitas yang sesuai, tetapi pemisahan profil menghindari kebingungan).

Kebutuhan sumber daya kecil: satu proses, SQLite (WAL), ~20–40 koneksi WebSocket untuk satu kelas.

**Backup**: hentikan server lalu salin folder `data/` (atau gunakan `sqlite3 data/codearena.db ".backup backup.db"` saat berjalan). Restart server **tidak** mengulang ronde: fase, deadline, snapshot, laporan, dan skor dipulihkan dari database.

---

## Konfigurasi

| Variabel | Default | Keterangan |
|---|---|---|
| `CODEARENA_BIND` | `0.0.0.0:8080` | Alamat & port HTTP/WebSocket. |
| `CODEARENA_DATA` | `data` | Folder data (database, `admin-password.txt`). |
| `CODEARENA_DB` | `<DATA>/codearena.db` | Lokasi file SQLite. |
| `CODEARENA_ADMIN_PASSWORD` | – | Password instruktur. Jika kosong, dibuat otomatis (lihat `data/admin-password.txt`). |
| `RUST_LOG` | `info,sqlx=warn` | Level log. |

---

## Alur kelas untuk instruktur

1. **Login** di `/admin`. Bagian *Roster*: periksa/tambah/nonaktifkan NIM.
2. Mahasiswa membuka `/` dan join dengan NIM → masuk **waiting room**.
3. **Waiting Room & Kelompok**:
   * *Randomize teams* (target 4, default hanya peserta online) → preview draft → periksa/ubah manual → **Konfirmasi kelompok**; atau buat kelompok manual.
   * Peserta terlambat: *Tempatkan peserta terlambat* atau tambah manual.
4. **Ronde & Fase** → *Buat ronde*: pilih challenge (C01–C13), durasi per fase (menit), auto-advance. Server membuat pairing ring; periksa/ubah/acak ulang lalu **Mulai ronde & kunci pairing**.
5. Kendalikan fase: *Buka fase berikutnya*, *Pause/Lanjutkan*, *+1/+5/−1 menit*. Defense Prep, Attack Window, dan Defense Response dapat ditutup otomatis oleh server saat waktu habis.
6. **Review** (bisa dimulai sejak laporan masuk): beri verdict laporan (valid + rubric 10/10/5, invalid + bonus rebuttal, duplicate, needs revision) dan review patch (accepted/rejected + rubric 20/15/10/5).
7. Di **Round Results**: *Finalisasi & sahkan skor* (finalisasi paksa tersedia jika ada yang belum direview — nilainya 0). Lalu *Buka pembahasan* untuk menampilkan answer key, reference solution, regression test resmi, dan semua laporan kepada mahasiswa.
8. **Skor & Koreksi**: koreksi skor wajib beralasan dan tercatat. **Sesi & Ekspor**: unduh CSV leaderboard / JSON lengkap; *Reset sesi* mengarsipkan sesi (data tetap ada) dan membuka sesi baru.

---

## Aturan permainan

### Fase ronde

| Fase | Yang terjadi | Ditutup oleh |
|---|---|---|
| Briefing | Challenge & rubric dibaca. | Instruktur |
| Defense Preparation | Tim memperbaiki starter code dan **menyimpan** draft pertahanan. | Timer (opsional) / instruktur |
| Attack Window | Server mengunci **snapshot v1** setiap tim (draft tersimpan terakhir, atau starter code). Tim menyerang snapshot target dan mengirim laporan; defender sudah boleh menganalisis & membuat patch draft. | Timer / instruktur |
| Defense Response | Laporan dikunci (`pending_review`). Defender menyelesaikan & submit patch + regression test, atau mengajukan rebuttal. | Timer / instruktur |
| Verification | Instruktur memvalidasi laporan & patch. | Instruktur |
| Round Results | Skor dihitung; instruktur memfinalisasi. | Instruktur (finalisasi) |

Semua aksi gameplay ditolak server ketika ronde di-pause atau deadline fase sudah lewat.

### Status tim (terpisah)

* **Attack**: `NOT_STARTED`, `TARGET_ASSIGNED`, `INSPECTING_TARGET`, `PREPARING_REPORT`, `REPORT_SUBMITTED`, `AWAITING_VALIDATION`, `VALID_FINDING`, `INVALID_FINDING`, `DUPLICATE_FINDING`, `ATTACK_COMPLETED`.
* **Defense**: `NOT_STARTED`, `DEFENSE_PREPARATION`, `AWAITING_ATTACK`, `ATTACK_RECEIVED`, `ANALYZING_BUG`, `FIXING_BUG`, `WRITING_REGRESSION_TEST`, `PATCH_SUBMITTED`, `AWAITING_REVIEW`, `PATCH_ACCEPTED`, `PATCH_REJECTED`, `DEFENSE_COMPLETED`.

Pemicu: pembukaan fase (event resmi), penyimpanan draft laporan, submit laporan (penyerang `REPORT_SUBMITTED`, target `ATTACK_RECEIVED`), penyimpanan patch (kode berubah → `FIXING_BUG`, hanya test berubah → `WRITING_REGRESSION_TEST`, hanya analisis → `ANALYZING_BUG`), tombol status eksplisit, review instruktur, dan finalisasi. Status koneksi (online/offline) ditampilkan terpisah dan tidak pernah memengaruhi status gameplay.

### Skor

| Komponen | Poin |
|---|---|
| Attack per temuan valid: reproduksi / root cause & konsep / bukti & test | 10 / 10 / 5 (maks 25) |
| Attack per ronde | 2 temuan valid terbaik (maks 50) |
| Defense per ronde (patch terbaik yang direview; versi kumulatif) | root cause 20 · regression 15 · normal & edge 10 · penjelasan 5 (maks 50) |
| Rebuttal (laporan invalid yang dibantah dengan benar) | maks 10 per ronde |

Tidak ada poin otomatis untuk mengirim laporan; duplikat dan invalid bernilai 0. Skor live dihitung ulang dari data review; setelah finalisasi tersimpan di `round_scores` (dikunci trigger database). Perubahan setelahnya hanya lewat `score_adjustments` beralasan.

---

## Challenge bank

13 challenge (12 materi + 1 capstone), masing-masing di `challenges/Cxx/`:
`meta.json` (tujuan pembelajaran, spesifikasi, skenario bug, prosedur attack, catatan rubric, peta konsep), `starter.py` (berisi bug), `reference.py` (answer key), `test_regression.py` (unittest; test `test_bN_*` memetakan bug `BN`).

| ID | Judul | Materi | Fokus |
|---|---|---|---|
| C01 | Shared Inventory | M1 | class, object, struktur class, class vs instance attribute |
| C02 | Wrong Instance State | M1 | instance/class attribute, default mutable, shadowing, method |
| C03 | The Invisible Vault | M2 | visibility, encapsulation, name mangling, property validation |
| C04 | Method Dispatch Failure | M3 | classmethod factory, staticmethod vs classmethod |
| C05 | The Decorator Trap | M3 | custom decorator, waktu eksekusi, return value, `functools.wraps` |
| C06 | Broken Function Wrapper | M3 | chaining decorator, `*args`/`**kwargs`, decorator berparameter |
| C07 | The Broken Family Tree | M4 | inheritance, base/derived class, `super().__init__` |
| C08 | Override Gone Wrong | M4 | overriding, nested inheritance, signature override |
| C09 | Multiple Inheritance Conflict | M4 | MRO, cooperative `super()`, diamond |
| C10 | Abstract Contract Violation | M5 | ABC & abstract method, kontrak signature |
| C11 | Mutable Evidence | M5 | dataclass, frozen, default_factory, aliasing |
| C12 | The Missing Behavior | M5 | duck typing, `typing.Protocol`, `runtime_checkable` |
| C13 | The Broken E-Wallet | M1–M5 | capstone modular: akun abstrak, name mangling, decorator, dataclass, Protocol |

Verifikasi bank (menjalankan kode challenge milik platform, bukan kode mahasiswa):

```bash
python tools/verify_challenges.py
```

Memeriksa: semua reference lulus seluruh regression test, setiap bug starter tertangkap minimal satu test, metadata lengkap, dan 34/34 subtopik resmi tercakup. Peta cakupan juga tersedia di `/admin/challenges`.

---

## Arsitektur

```
Browser (HTML dari Askama + vanilla JS)          Rust (Axum + Tokio)                     SQLite (SQLx, WAL)
 ├─ fetch POST /api/...  ──────────────────▶  handlers ─▶ engine/assignment/chat ─┬─▶ transaksi (write lock)
 │                                                                               └─▶ Outbox → EventBus (setelah commit)
 └─ WebSocket /ws?view=… ◀── snapshot per-viewer ◀── views::snapshot_for ◀── query
                         ◀── event incremental (difilter Audience) ◀── EventBus (broadcast, versi monoton)
```

* **Server = sumber kebenaran.** Browser hanya merender snapshot/event dan mengirim aksi eksplisit.
* **Snapshot + incremental**: saat connect (dan setelah perubahan struktural / lag / reconnect) server mengirim snapshot khusus viewer; selanjutnya event kecil (`team_status`, `stats`, `scores`, `round`, `presence`, `activity`, `team_refresh`, `chat`, …). Setiap event membawa versi `v`; client mengabaikan event dengan `v` ≤ snapshot. Event membawa **state absolut** (bukan delta), sehingga event ganda/terlewat tidak dapat menggandakan skor.
* **Audience**: `All`, `Instructors`, `Students`, `Team(id)`, `TeamAndInstructors(id)`, `Nim`. Konten laporan, draft, dan kode tidak pernah dikirim lewat event publik — tim mengambil fragmen HTML yang aksesnya diperiksa server.
* **Write lock global** sebelum setiap transaksi tulis: menghindari `SQLITE_BUSY`, dan transisi fase memakai compare-and-set (`expected` phase) sehingga klik ganda / timer tidak dapat melompati fase.
* **Timer**: tugas latar setiap 1 detik memajukan fase yang deadline-nya lewat (jika auto-advance & tidak di-pause).

Modul utama: `engine.rs` (ronde, pairing, snapshot, laporan, patch, review, finalisasi), `assignment.rs`, `scoring.rs`, `views.rs` (snapshot JSON), `events.rs`, `ws.rs`, `chat.rs`, `auth.rs`, `web/` (halaman Askama, fragmen, API).

---

## Keamanan & integritas data

* **Identitas**: token login acak 256-bit, disimpan hanya sebagai SHA-256; cookie `HttpOnly`, `SameSite=Lax`. Mahasiswa: NIM roster (bukan password) + satu perangkat aktif per NIM; instruktur dapat *lepas perangkat*. Instruktur: password terpisah (hash + salt, perbandingan constant-time, rate limit login).
* **Otorisasi server-side** di setiap endpoint: tim selalu diambil dari sesi login (tidak pernah dari body request), kepemilikan laporan/patch diperiksa, fase & deadline & pause ditegakkan di server.
* **Tanpa eksekusi kode mahasiswa.** Server tidak pernah menjalankan Python; validitas serangan dan kebenaran patch diputuskan lewat review instruktur. Jumlah regression test hanya dihitung statis (dan diberi label demikian di UI).
* **Trigger database**: snapshot defense immutable, pairing terkunci tidak bisa diubah/dihapus, laporan resmi tidak bisa dihapus, konten patch yang disubmit beku, skor final immutable.
* **Audit**: `audit_log` (review, koreksi skor, perubahan fase, finalisasi, reset, perubahan kelompok terkunci, moderasi chat), `team_history`, `report_revisions`, `activity`.
* **Input**: panjang field dibatasi, Askama meng-escape HTML, JSON di `<script>` di-escape, chat dibatasi 500 karakter + rate limit.
* Permintaan API memakai JSON (`Content-Type: application/json`) + cookie `SameSite=Lax`, sehingga form lintas situs tidak dapat memicu aksi.

---

## Pengujian

Ringkasan (detail lengkap di [`TESTING.md`](TESTING.md)):

```bash
cargo test                          # 29 unit test + 6 integration test (engine nyata + SQLite sementara)
python tools/verify_challenges.py   # 13 challenge: reference lulus, setiap bug tertangkap, 34/34 topik
node tools/e2e.mjs                  # 107 pemeriksaan HTTP + WebSocket (server berjalan; me-reset sesi!)
node tools/browser-check.mjs <chrome.exe> <folder-screenshot>   # Chrome headless via DevTools Protocol
```

---

## Keterbatasan yang diketahui

* **Tidak ada eksekusi otomatis kode/test** (sesuai prioritas keamanan versi awal). Reproduksi bug dan kebenaran patch dinilai instruktur; mahasiswa menguji secara lokal. Menambahkan runner otomatis memerlukan sandbox terisolasi (batas waktu, memori, filesystem, jaringan) — belum diimplementasikan.
* **NIM bukan autentikasi kuat.** Mahasiswa yang mengetahui NIM teman yang sedang offline dapat join sebagai teman tersebut. Mitigasi: satu perangkat aktif per NIM, tombol *lepas perangkat*, audit log join, dan pengawasan kelas.
* **Satu akun instruktur bersama**; reviewer tercatat sebagai `instructor` (belum ada akun per asisten).
* **Satu instance server, satu sesi aktif.** Presence, rate limiter, dan event bus berada di memori proses; tidak untuk deployment multi-server. Restart memutus koneksi sejenak (client reconnect otomatis dan memuat snapshot).
* **Tanpa HTTPS bawaan** (ditujukan untuk LAN). Untuk jaringan tidak tepercaya, tempatkan di belakang reverse proxy TLS.
* **Auto-advance** berjalan dengan granularitas ~1 detik. Jika server mati melewati deadline, fase tersebut ditutup sekali saat server menyala lagi (fase berikutnya mendapat durasi penuh).
* **Layout graph**: dirancang untuk 2–10 kelompok; lebih banyak kelompok tetap tampil tetapi lebih kecil (gunakan zoom/Team board).
* Snapshot dan patch berupa **satu file modul Python**; challenge multi-file tidak didukung.
* Kelompok tidak dapat dihapus/ditambah saat ronde berjalan (perpindahan anggota beralasan tetap bisa). Reset sesi ketika ronde berjalan mengarsipkan ronde tanpa finalisasi.
* UI memakai fitur browser modern (`<dialog>`, CSS `color-mix`): Chrome/Edge 111+, Firefox 113+, Safari 16.2+.

---

## Struktur proyek

```
Cargo.toml
migrations/0001_init.sql        skema + trigger integritas
challenges/C01..C13/            meta.json, starter.py, reference.py, test_regression.py
src/
  main.rs, lib.rs               startup, timer
  state.rs, events.rs, ws.rs    state, event bus + audience, WebSocket
  auth.rs, assignment.rs        identitas/join, waiting room & kelompok
  engine.rs, scoring.rs         ronde/fase/pairing/snapshot/laporan/patch/review, rubric
  views.rs, chat.rs, models.rs  snapshot JSON, chat, kode status
  web/                          halaman & fragmen Askama, API JSON, admin
templates/                      Askama (*.html)
static/                         app.css, common.js, arena.js, team.js, wait.js, admin.js, chat.js
tests/flow.rs                   integration test engine
tools/                          verify_challenges.py, e2e.mjs, browser-check.mjs
```
