"""C11 — Mutable Evidence (reference solution)

Perbaikan:
- B1: kategori default dibuat baru per laporan (default_factory) sehingga KATEGORI_DEFAULT tidak ter-alias.
- B2: Transaksi menjadi @dataclass(frozen=True) sehingga bukti tidak dapat diubah.
- B3: salin() menyalin list transaksi dan kategori (bukan berbagi referensi).
- B4: Transaksi memakai __eq__ hasil generate dataclass (eq=True) dan otomatis hashable karena frozen.
- B5: per_metode membuat list baru untuk setiap key (bukan satu list bersama dari dict.fromkeys).
"""

from dataclasses import dataclass, field, replace

KATEGORI_DEFAULT = ["umum"]
METODE_VALID = ("tunai", "qris")


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
        if trx in self.transaksi:
            raise ValueError("transaksi sudah tercatat")
        self.transaksi.append(trx)

    def total(self):
        return sum(t.jumlah for t in self.transaksi)

    def per_metode(self):
        hasil = {m: [] for m in METODE_VALID}
        for t in self.transaksi:
            hasil[t.metode].append(t.kode)
        return hasil

    def salin(self):
        # Transaksi immutable -> cukup salin list-nya.
        return replace(self, transaksi=list(self.transaksi), kategori=list(self.kategori))
