"""C08 — Override Gone Wrong (reference solution)

Perbaikan:
- B1: nama method Manajer.jabatan diperbaiki sehingga benar-benar meng-override.
- B2: Manajer memanggil super().hitung_gaji(jam_lembur) sehingga lapisan PegawaiTetap (tunjangan) ikut dihitung.
- B3: PegawaiTetap.hitung_gaji mempertahankan signature parent (jam_lembur=0) dan meneruskannya.
"""

TARIF_LEMBUR = 40_000


class Pegawai:
    def __init__(self, nama, gaji_pokok):
        self.nama = nama
        self.gaji_pokok = gaji_pokok

    def jabatan(self):
        return "Staf"

    def hitung_gaji(self, jam_lembur=0):
        if jam_lembur < 0:
            raise ValueError("jam lembur tidak boleh negatif")
        return self.gaji_pokok + jam_lembur * TARIF_LEMBUR

    def slip(self, jam_lembur=0):
        return f"{self.nama} [{self.jabatan()}] Rp{self.hitung_gaji(jam_lembur):,}"


class PegawaiTetap(Pegawai):
    def __init__(self, nama, gaji_pokok, tunjangan):
        super().__init__(nama, gaji_pokok)
        self.tunjangan = tunjangan

    def jabatan(self):
        return "Pegawai Tetap"

    def hitung_gaji(self, jam_lembur=0):
        return super().hitung_gaji(jam_lembur) + self.tunjangan


class Manajer(PegawaiTetap):
    def __init__(self, nama, gaji_pokok, tunjangan, bonus_jabatan):
        super().__init__(nama, gaji_pokok, tunjangan)
        self.bonus_jabatan = bonus_jabatan

    def jabatan(self):
        return "Manajer"

    def hitung_gaji(self, jam_lembur=0):
        return super().hitung_gaji(jam_lembur) + self.bonus_jabatan


def total_penggajian(daftar_pegawai, jam_lembur=0):
    return sum(p.hitung_gaji(jam_lembur) for p in daftar_pegawai)
