"""C11 — Mutable Evidence (reference solution)

Perbaikan:
- B1: kategori memakai field(default_factory=...) sehingga tiap laporan memiliki list sendiri
      dan KATEGORI_DEFAULT tidak pernah dimodifikasi.
- B2: Transaksi dibuat frozen=True sehingga bukti tidak dapat diubah setelah dibuat.
- B3: salin() membuat list baru untuk field mutable (transaksi frozen sehingga cukup shallow per elemen).
"""

from dataclasses import dataclass, field, replace

KATEGORI_DEFAULT = ["umum"]


@dataclass(frozen=True)
class Transaksi:
    kode: str
    jumlah: int
    metode: str = "tunai"


@dataclass
class LaporanHarian:
    tanggal: str
    transaksi: list = field(default_factory=list)
    kategori: list = field(default_factory=lambda: list(KATEGORI_DEFAULT))

    def tambah(self, trx):
        if trx.jumlah <= 0:
            raise ValueError("jumlah transaksi harus positif")
        self.transaksi.append(trx)

    def total(self):
        return sum(t.jumlah for t in self.transaksi)

    def salin(self):
        return replace(self, transaksi=list(self.transaksi), kategori=list(self.kategori))
