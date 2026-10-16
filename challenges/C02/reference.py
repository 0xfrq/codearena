"""C02 — Wrong Instance State (reference solution)

Perbaikan:
- B1: default argument mutable diganti None; tiap instance memperoleh list baru.
- B2: counter dinaikkan melalui class (type(self) / Mahasiswa), bukan melalui self.
- B3: dispensasi membuat instance attribute yang menimpa (shadow) class attribute hanya untuk objek ini.
"""


class Mahasiswa:
    jumlah_mahasiswa = 0
    batas_sks = 24

    def __init__(self, nim, nama, krs=None):
        self.nim = nim
        self.nama = nama
        self.krs = list(krs) if krs is not None else []
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

    def beri_dispensasi(self, batas_baru):
        self.batas_sks = batas_baru

    def info(self):
        return f"{self.nim} {self.nama}: {self.total_sks()}/{self.batas_sks} SKS"
