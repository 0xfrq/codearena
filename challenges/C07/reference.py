"""C07 — The Broken Family Tree (reference solution)

Perbaikan:
- B1: Dosen.__init__ memanggil super().__init__(nama, nomor) sehingga attribute base terbentuk.
- B2: kartu() memakai self.prefix (lookup lewat instance -> class turunan), bukan Civitas.prefix.
- B3: Mahasiswa meneruskan argumen ke constructor parent dengan urutan yang benar.
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


class Mahasiswa(Civitas):
    prefix = "MHS"

    def __init__(self, nama, nomor, prodi):
        super().__init__(nama, nomor)
        self.prodi = prodi


class Tendik(Civitas):
    prefix = "TDK"

    def __init__(self, nama, nomor, unit):
        super().__init__(nama, nomor)
        self.unit = unit
