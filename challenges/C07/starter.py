"""C07 — The Broken Family Tree

Sistem kartu identitas civitas akademika.
Semua civitas (dosen, mahasiswa, tenaga kependidikan) mewarisi class Civitas.
"""


class Civitas:
    """Base class untuk seluruh civitas akademika."""

    prefix = "CIV"

    def __init__(self, nama, nomor):
        if nomor <= 0:
            raise ValueError("nomor harus positif")
        self.nama = nama
        self.nomor = nomor
        self.aktif = True

    def kartu(self):
        """Nomor kartu: '<PREFIX>-<nomor 4 digit>'. PREFIX mengikuti jenis civitas."""
        return f"{Civitas.prefix}-{self.nomor:04d}"

    def nonaktifkan(self):
        self.aktif = False

    def profil(self):
        status = "aktif" if self.aktif else "nonaktif"
        return f"{self.kartu()} {self.nama} ({status})"


class Dosen(Civitas):
    prefix = "DSN"

    def __init__(self, nama, nomor, nidn):
        self.nidn = nidn


class DosenTamu(Dosen):
    prefix = "DTM"

    def __init__(self, nama, nomor, nidn, asal_kampus):
        super().__init__(nama, nomor, nidn)
        self.asal_kampus = asal_kampus


class Mahasiswa(Civitas):
    prefix = "MHS"

    def __init__(self, nama, nomor, prodi):
        super().__init__(nama, nomor)
        self.prodi = prodi
        self.status_studi = "aktif"

    def nonaktifkan(self):
        """Mahasiswa yang dinonaktifkan berstatus studi 'cuti'."""
        self.status_studi = "cuti"


class Tendik(Civitas):
    prefix = "TDK"

    def __init__(self, nama, nomor, unit):
        super().__init__(unit, nomor)
        self.unit = unit


def hanya_dosen(daftar):
    """Semua civitas yang merupakan dosen (termasuk jenis dosen turunan), urutan dipertahankan."""
    return [c for c in daftar if type(c) is Dosen]
