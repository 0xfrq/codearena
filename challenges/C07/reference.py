"""C07 — The Broken Family Tree (reference solution)

Perbaikan:
- B1: Dosen.__init__ memanggil super().__init__(nama, nomor) sehingga attribute & validasi base terpasang.
- B2: kartu() memakai self.prefix sehingga lookup mengikuti class konkret.
- B3: Tendik meneruskan argumen yang benar ke constructor base (nama, bukan unit).
- B4: Mahasiswa.nonaktifkan memanggil super().nonaktifkan() sebelum menambah perilaku khusus.
- B5: hanya_dosen memakai isinstance sehingga turunan Dosen (DosenTamu) ikut terhitung.
"""


class Civitas:
    prefix = "CIV"

    def __init__(self, nama, nomor):
        if nomor <= 0:
            raise ValueError("nomor harus positif")
        self.nama = nama
        self.nomor = nomor
        self.aktif = True

    def kartu(self):
        return f"{self.prefix}-{self.nomor:04d}"

    def nonaktifkan(self):
        self.aktif = False

    def profil(self):
        status = "aktif" if self.aktif else "nonaktif"
        return f"{self.kartu()} {self.nama} ({status})"


class Dosen(Civitas):
    prefix = "DSN"

    def __init__(self, nama, nomor, nidn):
        super().__init__(nama, nomor)
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
        super().nonaktifkan()
        self.status_studi = "cuti"


class Tendik(Civitas):
    prefix = "TDK"

    def __init__(self, nama, nomor, unit):
        super().__init__(nama, nomor)
        self.unit = unit


def hanya_dosen(daftar):
    return [c for c in daftar if isinstance(c, Dosen)]
