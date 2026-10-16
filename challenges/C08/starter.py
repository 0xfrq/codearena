"""C08 — Override Gone Wrong

Slip gaji bertingkat: Pegawai -> PegawaiTetap -> Manajer (nested inheritance).
  - Pegawai      : gaji = gaji_pokok + jam_lembur * 40.000
  - PegawaiTetap : gaji Pegawai + tunjangan
  - Manajer      : gaji PegawaiTetap + bonus_jabatan; slip manajer diberi penanda ' *'
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

    def hitung_gaji(self):
        return super().hitung_gaji() + self.tunjangan


class Manajer(PegawaiTetap):
    def __init__(self, nama, gaji_pokok, tunjangan, bonus_jabatan):
        super().__init__(nama, gaji_pokok, tunjangan)
        self.bonus_jabatan = bonus_jabatan

    def jabtan(self):
        return "Manajer"

    def hitung_gaji(self, jam_lembur=0):
        return Pegawai.hitung_gaji(self, jam_lembur) + self.bonus_jabatan

    def slip(self, jam_lembur=0):
        return super().slip() + " *"


def total_penggajian(daftar_pegawai, jam_lembur=0):
    """Total gaji semua pegawai (polimorfik) untuk jam lembur yang sama."""
    total = 0
    for p in daftar_pegawai:
        total += p.hitung_gaji(jam_lembur)
        if isinstance(p, PegawaiTetap):
            total += p.tunjangan  # tunjangan pegawai tetap ikut direkap
    return total
