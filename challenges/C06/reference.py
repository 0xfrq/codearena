"""C06 — Broken Function Wrapper (reference solution)

Perbaikan:
- B1: catat_panggilan menerima dan meneruskan *args DAN **kwargs.
- B2: statistik dinaikkan SETELAH func (validasi di lapisan dalam) berhasil.
- B3: batas_durasi membaca durasi dari posisi maupun keyword dengan inspect.signature.bind.
- B4: durasi tidak di-pop dari kwargs; argumen diteruskan apa adanya.
- B5: catat_panggilan mencatat SEBELUM memanggil func sehingga pemanggilan gagal juga tercatat.
"""

import functools
import inspect

LOG_PANGGILAN = []


def catat_panggilan(func):
    @functools.wraps(func)
    def wrapper(self, *args, **kwargs):
        LOG_PANGGILAN.append((func.__name__, args, kwargs))
        return func(self, *args, **kwargs)

    return wrapper


def hitung_peminjaman(func):
    @functools.wraps(func)
    def wrapper(self, *args, **kwargs):
        hasil = func(self, *args, **kwargs)
        self.statistik += 1
        return hasil

    return wrapper


def wajib_anggota(func):
    @functools.wraps(func)
    def wrapper(self, member, *args, **kwargs):
        if member not in self.anggota:
            raise PermissionError(f"{member} bukan anggota")
        return func(self, member, *args, **kwargs)

    return wrapper


def batas_durasi(maks):
    def decorator(func):
        sig = inspect.signature(func)

        @functools.wraps(func)
        def wrapper(self, *args, **kwargs):
            bound = sig.bind(self, *args, **kwargs)
            bound.apply_defaults()
            durasi = bound.arguments["durasi"]
            if durasi > maks:
                raise ValueError(f"durasi maksimal {maks} hari")
            return func(self, *args, **kwargs)

        return wrapper

    return decorator


class Perpustakaan:
    def __init__(self, anggota):
        self.anggota = set(anggota)
        self.peminjaman = []
        self.statistik = 0

    @catat_panggilan
    @hitung_peminjaman
    @wajib_anggota
    @batas_durasi(14)
    def pinjam(self, member, judul, durasi=7):
        self.peminjaman.append((member, judul, durasi))
        return len(self.peminjaman)
