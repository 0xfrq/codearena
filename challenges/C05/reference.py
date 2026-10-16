"""C05 — The Decorator Trap (reference solution)

Perbaikan:
- B1: wrapper nominal_positif mengembalikan hasil fungsi asli.
- B2: pencatatan dipindah ke dalam wrapper (terjadi saat PEMANGGILAN, setelah operasi berhasil),
      bukan saat decorator diterapkan (definisi class).
- B3: functools.wraps menjaga __name__ / __doc__ fungsi asli.
"""

import functools

AUDIT_LOG = []


def catat_transaksi(func):
    @functools.wraps(func)
    def wrapper(*args, **kwargs):
        hasil = func(*args, **kwargs)
        AUDIT_LOG.append(func.__name__)
        return hasil

    return wrapper


def nominal_positif(func):
    @functools.wraps(func)
    def wrapper(self, nominal, *args, **kwargs):
        if nominal <= 0:
            raise ValueError("nominal harus positif")
        return func(self, nominal, *args, **kwargs)

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
