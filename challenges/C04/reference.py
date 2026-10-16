"""C04 — Method Dispatch Failure (reference solution)

Perbaikan:
- B1: dari_csv memakai cls(...) sehingga Dosen.dari_csv menghasilkan Dosen.
- B2: ubah_tarif_lembur menjadi @classmethod dan meng-assign cls.tarif_lembur.
- B3: counter hanya dinaikkan di __init__ (factory tidak menambah lagi).
- B4: counter dinaikkan SETELAH validasi berhasil.
- B5: gaji_bersih memanggil self.pajak(...) sehingga override pajak di turunan dipakai.
"""


class Pegawai:
    tarif_lembur = 50_000
    jumlah_pegawai = 0

    def __init__(self, nama, gaji_pokok):
        if gaji_pokok <= 0:
            raise ValueError("gaji pokok harus positif")
        self.nama = nama
        self.gaji_pokok = gaji_pokok
        Pegawai.jumlah_pegawai += 1

    @classmethod
    def dari_csv(cls, baris):
        nama, gaji = baris.split(";")
        return cls(nama.strip(), int(gaji))

    @classmethod
    def ubah_tarif_lembur(cls, tarif):
        if tarif <= 0:
            raise ValueError("tarif harus positif")
        cls.tarif_lembur = tarif

    @staticmethod
    def pajak(nominal):
        return nominal * 5 // 100

    def gaji_bersih(self, jam_lembur=0):
        kotor = self.gaji_pokok + jam_lembur * self.tarif_lembur
        return kotor - self.pajak(kotor)


class Dosen(Pegawai):
    tarif_lembur = 75_000


class Satpam(Pegawai):
    @staticmethod
    def pajak(nominal):
        return nominal * 2 // 100
