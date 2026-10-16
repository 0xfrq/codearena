"""C02 — Wrong Instance State

Sistem KRS (Kartu Rencana Studi) sederhana.
Setiap Mahasiswa mengambil mata kuliah dengan batas SKS tertentu.
"""


class Mahasiswa:
    jumlah_mahasiswa = 0   # class attribute: total objek Mahasiswa yang pernah dibuat
    batas_sks = 24         # class attribute: batas SKS default kampus

    def __init__(self, nim, nama, krs=[]):
        self.nim = nim
        self.nama = nama
        self.krs = krs
        self.jumlah_mahasiswa += 1

    def total_sks(self):
        return sum(sks for _, sks in self.krs)

    def ambil_mk(self, kode, sks):
        """Mengambil mata kuliah. Ditolak jika melebihi batas SKS mahasiswa ini."""
        if sks <= 0:
            raise ValueError("sks harus positif")
        if any(k == kode for k, _ in self.krs):
            raise ValueError("mata kuliah sudah diambil")
        if self.total_sks() + sks > self.batas_sks:
            raise ValueError("melebihi batas sks")
        self.krs.append((kode, sks))

    def beri_dispensasi(self, batas_baru):
        """Dispensasi batas SKS khusus untuk mahasiswa INI (mis. IPK tinggi)."""
        Mahasiswa.batas_sks = batas_baru

    def info(self):
        return f"{self.nim} {self.nama}: {self.total_sks()}/{self.batas_sks} SKS"
