"""C02 — Wrong Instance State (reference solution)

Perbaikan:
- B1: default argument mutable diganti None; setiap mahasiswa mendapat list KRS baru.
- B2: counter dinaikkan melalui class (Mahasiswa.jumlah_mahasiswa), bukan self.
- B3: dispensasi meng-assign self.batas_sks (instance attribute) sehingga class attribute tetap 24.
- B4: batas SKS inklusif: total == batas masih diperbolehkan (pakai >, bukan >=).
- B5: drop_mk meng-assign hasil filter ke self.krs, bukan ke variabel lokal.
"""


class Mahasiswa:
    jumlah_mahasiswa = 0
    batas_sks = 24

    def __init__(self, nim, nama, krs=None):
        self.nim = nim
        self.nama = nama
        self.krs = [] if krs is None else krs
        Mahasiswa.jumlah_mahasiswa += 1

    def total_sks(self):
        return sum(sks for _, sks in self.krs)

    def ambil_mk(self, kode, sks):
        if sks <= 0:
            raise ValueError("sks harus positif")
        if any(k == kode for k, _ in self.krs):
            raise ValueError("mata kuliah sudah diambil")
        if self.total_sks() + sks > self.batas_sks:
            raise ValueError("melebihi batas sks")
        self.krs.append((kode, sks))

    def drop_mk(self, kode):
        if not any(k == kode for k, _ in self.krs):
            raise ValueError("mata kuliah tidak ada di KRS")
        self.krs = [(k, sks) for k, sks in self.krs if k != kode]

    def beri_dispensasi(self, batas_baru):
        if batas_baru <= 0:
            raise ValueError("batas harus positif")
        self.batas_sks = batas_baru  # instance attribute: hanya mahasiswa ini

    def info(self):
        return f"{self.nim} {self.nama}: {self.total_sks()}/{self.batas_sks} SKS"
