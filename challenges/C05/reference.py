"""C05 — The Decorator Trap (reference solution)

Perbaikan:
- B1: wrapper nominal_positif mengembalikan hasil func.
- B2: AUDIT_LOG ditulis di dalam wrapper (saat dipanggil), bukan saat decorator diterapkan,
      dan hanya setelah func selesai tanpa exception.
- B3: semua wrapper memakai functools.wraps sehingga __name__/__doc__ terjaga.
- B4: kuota disimpan per object (di instance kasir), bukan di closure yang dibagi semua kasir.
- B5: kuota hanya bertambah SETELAH operasi berhasil.
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


def batasi_harian(maks):
    def decorator(func):
        @functools.wraps(func)
        def wrapper(self, *args, **kwargs):
            kuota = vars(self).setdefault("_kuota_terpakai", {})
            if kuota.get(func.__name__, 0) >= maks:
                raise PermissionError("kuota harian habis")
            hasil = func(self, *args, **kwargs)
            kuota[func.__name__] = kuota.get(func.__name__, 0) + 1
            return hasil

        return wrapper

    return decorator


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
    @batasi_harian(3)
    @nominal_positif
    def beri_kembalian(self, nominal):
        """Mengeluarkan uang kembalian dan mengembalikan saldo kas terbaru."""
        if nominal > self.saldo_kas:
            raise ValueError("kas tidak cukup")
        self.saldo_kas -= nominal
        return self.saldo_kas
