"""C06 — Broken Function Wrapper

Layanan peminjaman buku perpustakaan kampus.
Method pinjam() dibungkus beberapa decorator yang dirangkai (chaining):

    @catat_panggilan      -> mencatat (nama_fungsi, args, kwargs) ke LOG_PANGGILAN
    @hitung_peminjaman    -> menambah statistik HANYA untuk peminjaman yang BERHASIL
    @wajib_anggota        -> menolak (PermissionError) jika member bukan anggota
    @batas_durasi(14)     -> menolak (ValueError) durasi > 14 hari
"""

LOG_PANGGILAN = []


def catat_panggilan(func):
    def wrapper(self, *args):
        LOG_PANGGILAN.append((func.__name__, args))
        return func(self, *args)

    wrapper.__name__ = func.__name__
    return wrapper


def hitung_peminjaman(func):
    def wrapper(self, *args, **kwargs):
        self.statistik += 1
        return func(self, *args, **kwargs)

    wrapper.__name__ = func.__name__
    return wrapper


def wajib_anggota(func):
    def wrapper(self, member, *args, **kwargs):
        if member not in self.anggota:
            raise PermissionError(f"{member} bukan anggota")
        return func(self, member, *args, **kwargs)

    wrapper.__name__ = func.__name__
    return wrapper


def batas_durasi(maks):
    def decorator(func):
        def wrapper(self, *args, **kwargs):
            durasi = kwargs.get("durasi", 7)
            if durasi > maks:
                raise ValueError(f"durasi maksimal {maks} hari")
            return func(self, *args, **kwargs)

        wrapper.__name__ = func.__name__
        return wrapper

    return decorator


class Perpustakaan:
    def __init__(self, anggota):
        self.anggota = set(anggota)
        self.peminjaman = []
        self.statistik = 0  # jumlah peminjaman yang berhasil

    @catat_panggilan
    @hitung_peminjaman
    @wajib_anggota
    @batas_durasi(14)
    def pinjam(self, member, judul, durasi=7):
        self.peminjaman.append((member, judul, durasi))
        return len(self.peminjaman)
