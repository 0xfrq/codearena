"""C10 — Abstract Contract Violation (reference solution)

Perbaikan:
- B1: MetodePembayaran mewarisi ABC sehingga @abstractmethod benar-benar ditegakkan.
- B2: Transfer mengimplementasikan biaya_admin (Rp2.500 flat).
- B3: QRIS.bayar mengikuti signature kontrak bayar(self, jumlah).
- B4: EWallet.biaya_admin memakai aritmetika integer (dibulatkan ke bawah).
- B5: Tunai tidak meng-override total_tagihan; validasi di base class tetap berlaku.
"""

from abc import ABC, abstractmethod


class MetodePembayaran(ABC):
    kode = "?"

    def __init__(self, pemilik):
        self.pemilik = pemilik

    @abstractmethod
    def biaya_admin(self, jumlah):
        """Biaya admin (int, rupiah) untuk transaksi sebesar `jumlah`."""

    @abstractmethod
    def bayar(self, jumlah):
        """Memproses pembayaran; mengembalikan '<KODE> <total tagihan>'."""

    def total_tagihan(self, jumlah):
        if jumlah <= 0:
            raise ValueError("jumlah harus positif")
        return jumlah + self.biaya_admin(jumlah)


class Tunai(MetodePembayaran):
    kode = "TUNAI"

    def biaya_admin(self, jumlah):
        return 0

    def bayar(self, jumlah):
        return f"{self.kode} {self.total_tagihan(jumlah)}"


class Transfer(MetodePembayaran):
    kode = "TRANSFER"

    def biaya_admin(self, jumlah):
        return 2_500

    def bayar(self, jumlah):
        return f"{self.kode} {self.total_tagihan(jumlah)}"


class QRIS(MetodePembayaran):
    kode = "QRIS"

    def biaya_admin(self, jumlah):
        return jumlah * 7 // 1000

    def bayar(self, jumlah):
        return f"{self.kode} {self.total_tagihan(jumlah)}"


class EWallet(MetodePembayaran):
    kode = "EWALLET"

    def biaya_admin(self, jumlah):
        return jumlah // 100

    def bayar(self, jumlah):
        return f"{self.kode} {self.total_tagihan(jumlah)}"


def proses_semua(daftar_metode, jumlah):
    return [m.bayar(jumlah) for m in daftar_metode]
