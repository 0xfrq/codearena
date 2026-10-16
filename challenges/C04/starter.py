"""C04 — Method Dispatch Failure

Modul penggajian kampus. Data pegawai diimpor dari file CSV sederhana
("Nama;gaji_pokok") dan setiap jenis pegawai memiliki tarif lembur sendiri.
"""


class Pegawai:
    tarif_lembur = 50_000   # rupiah per jam (class attribute, dapat di-override turunan)
    jumlah_pegawai = 0      # total object pegawai (semua jenis)

    def __init__(self, nama, gaji_pokok):
        if gaji_pokok <= 0:
            raise ValueError("gaji pokok harus positif")
        self.nama = nama
        self.gaji_pokok = gaji_pokok
        Pegawai.jumlah_pegawai += 1

    @classmethod
    def dari_csv(cls, baris):
        """Factory: membuat object dari class PEMANGGIL (Pegawai atau turunannya)."""
        nama, gaji = baris.split(";")
        pegawai = Pegawai(nama.strip(), int(gaji))
        Pegawai.jumlah_pegawai += 1
        return pegawai

    @staticmethod
    def ubah_tarif_lembur(tarif):
        """Mengubah tarif lembur untuk class yang memanggil method ini."""
        if tarif <= 0:
            raise ValueError("tarif harus positif")
        Pegawai.tarif_lembur = tarif

    @staticmethod
    def pajak(nominal):
        """Pajak 5% dibulatkan ke bawah. Utilitas murni: tidak membutuhkan self/cls."""
        return nominal * 5 // 100

    def gaji_bersih(self, jam_lembur=0):
        kotor = self.gaji_pokok + jam_lembur * self.tarif_lembur
        return kotor - self.pajak(kotor)


class Dosen(Pegawai):
    tarif_lembur = 75_000


class Satpam(Pegawai):
    pass  # memakai tarif lembur Pegawai
