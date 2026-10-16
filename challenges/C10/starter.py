"""C10 — Abstract Contract Violation

Metode pembayaran di loket administrasi kampus.
MetodePembayaran adalah KONTRAK: tidak boleh dibuat langsung, dan setiap metode konkret
wajib mengimplementasikan biaya_admin(jumlah) dan bayar(jumlah).
"""

from abc import abstractmethod


class MetodePembayaran:
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

    def total_tagihan(self, jumlah):
        """Tunai tidak dikenai biaya admin."""
        return jumlah

    def bayar(self, jumlah):
        return f"{self.kode} {self.total_tagihan(jumlah)}"


class Transfer(MetodePembayaran):
    """Transfer bank: biaya admin flat Rp2.500."""

    kode = "TRANSFER"

    def bayar(self, jumlah):
        return f"{self.kode} {self.total_tagihan(jumlah)}"


class QRIS(MetodePembayaran):
    """QRIS: biaya admin 0,7% (dibulatkan ke bawah)."""

    kode = "QRIS"

    def biaya_admin(self, jumlah):
        return jumlah * 7 // 1000

    def bayar(self):
        return f"{self.kode} {self.total_tagihan(self.jumlah)}"


class EWallet(MetodePembayaran):
    """E-wallet kampus: biaya admin 1% (dibulatkan ke bawah)."""

    kode = "EWALLET"

    def biaya_admin(self, jumlah):
        return jumlah * 0.01

    def bayar(self, jumlah):
        return f"{self.kode} {self.total_tagihan(jumlah)}"


def proses_semua(daftar_metode, jumlah):
    """Memproses pembayaran yang sama dengan semua metode (polimorfik)."""
    return [m.bayar(jumlah) for m in daftar_metode]
