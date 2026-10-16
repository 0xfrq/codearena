"""C03 — The Invisible Vault (reference solution)

Perbaikan:
- B1: property `saldo` read-only (setter dihapus) -> saldo hanya berubah lewat setor/tarik.
- B2: setor() memvalidasi jumlah > 0.
- B3: TabunganPelajar tidak menulis self.__saldo (yang ter-mangling menjadi _TabunganPelajar__saldo);
      ia memakai helper protected milik parent untuk mengubah saldo.
"""


class Tabungan:
    BIAYA_ADMIN = 0

    def __init__(self, pemilik, saldo_awal=0):
        if saldo_awal < 0:
            raise ValueError("saldo awal tidak boleh negatif")
        self.pemilik = pemilik
        self._riwayat = []
        self.__saldo = saldo_awal

    @property
    def saldo(self):
        return self.__saldo

    def _ubah_saldo(self, delta):
        """Satu-satunya jalur (protected) untuk mengubah saldo privat."""
        baru = self.__saldo + delta
        if baru < 0:
            raise ValueError("saldo tidak boleh negatif")
        self.__saldo = baru

    def setor(self, jumlah):
        if jumlah <= 0:
            raise ValueError("jumlah harus positif")
        self._ubah_saldo(jumlah)
        self._riwayat.append(("setor", jumlah))

    def tarik(self, jumlah):
        if jumlah <= 0:
            raise ValueError("jumlah harus positif")
        total = jumlah + self.BIAYA_ADMIN
        if total > self.__saldo:
            raise ValueError("saldo tidak cukup")
        self._ubah_saldo(-total)
        self._riwayat.append(("tarik", jumlah))

    def riwayat(self):
        return list(self._riwayat)


class TabunganPelajar(Tabungan):
    BATAS_TARIK = 500_000

    def tarik(self, jumlah):
        if jumlah <= 0:
            raise ValueError("jumlah harus positif")
        if jumlah > self.BATAS_TARIK:
            raise ValueError("melebihi batas tarik pelajar")
        if jumlah > self.saldo:
            raise ValueError("saldo tidak cukup")
        self._ubah_saldo(-jumlah)
        self._riwayat.append(("tarik", jumlah))
