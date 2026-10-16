"""C11 — Mutable Evidence

Pencatatan transaksi harian kantin kampus menggunakan dataclass.
Transaksi adalah BUKTI: setelah dicatat, nilainya tidak boleh berubah.
Laporan harian dapat diarsipkan dengan salin(); arsip harus independen dari laporan aktif.
"""

from dataclasses import dataclass, replace

KATEGORI_DEFAULT = ["umum"]


@dataclass
class Transaksi:
    kode: str
    jumlah: int
    metode: str = "tunai"


@dataclass
class LaporanHarian:
    tanggal: str
    transaksi: list = None
    kategori: list = None

    def __post_init__(self):
        if self.transaksi is None:
            self.transaksi = []
        if self.kategori is None:
            self.kategori = KATEGORI_DEFAULT

    def tambah(self, trx):
        if trx.jumlah <= 0:
            raise ValueError("jumlah transaksi harus positif")
        self.transaksi.append(trx)

    def total(self):
        return sum(t.jumlah for t in self.transaksi)

    def salin(self):
        """Membuat arsip yang independen dari laporan aktif."""
        return replace(self)
