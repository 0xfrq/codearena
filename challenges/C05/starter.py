"""C05 — The Decorator Trap

Kasir koperasi mahasiswa. Setiap operasi kas dibungkus decorator buatan sendiri:
- @nominal_positif  : menolak nominal <= 0 sebelum operasi dijalankan.
- @catat_transaksi  : mencatat NAMA operasi ke AUDIT_LOG setiap kali operasi BERHASIL dipanggil.
"""

AUDIT_LOG = []


def catat_transaksi(func):
    AUDIT_LOG.append(func.__name__)

    def wrapper(*args, **kwargs):
        return func(*args, **kwargs)

    return wrapper


def nominal_positif(func):
    def wrapper(self, nominal, *args, **kwargs):
        if nominal <= 0:
            raise ValueError("nominal harus positif")
        func(self, nominal, *args, **kwargs)

    return wrapper


class KasirKoperasi:
    def __init__(self, saldo_kas=0):
        self.saldo_kas = saldo_kas

    @catat_transaksi
    @nominal_positif
    def terima_pembayaran(self, nominal, keterangan=""):
        """Menerima pembayaran dan mengembalikan saldo kas terbaru."""
        self.saldo_kas += nominal
        return self.saldo_kas

    @catat_transaksi
    @nominal_positif
    def beri_kembalian(self, nominal):
        """Mengeluarkan uang kembalian dan mengembalikan saldo kas terbaru."""
        if nominal > self.saldo_kas:
            raise ValueError("kas tidak cukup")
        self.saldo_kas -= nominal
        return self.saldo_kas
