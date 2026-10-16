"""C03 — The Invisible Vault

Rekening tabungan mahasiswa di koperasi kampus.
Saldo adalah data sensitif: hanya boleh berubah melalui setor() dan tarik().
"""


class Tabungan:
    BIAYA_ADMIN = 0

    def __init__(self, pemilik, saldo_awal=0):
        if saldo_awal < 0:
            raise ValueError("saldo awal tidak boleh negatif")
        self._pemilik = pemilik     # protected: diakses lewat property pemilik
        self._riwayat = []          # protected (konvensi): dipakai class turunan
        self.__saldo = saldo_awal   # private: name mangling -> _Tabungan__saldo

    @property
    def pemilik(self):
        return self._pemilik

    @pemilik.setter
    def pemilik(self, nama):
        nama = str(nama).strip()
        if not nama:
            raise ValueError("nama pemilik wajib diisi")
        self._pemilik = nama

    @property
    def saldo(self):
        return self.__saldo

    @saldo.setter
    def saldo(self, nilai):
        self.__saldo = nilai

    def setor(self, jumlah):
        self.__saldo += jumlah
        self._riwayat.append(("setor", jumlah))

    def tarik(self, jumlah):
        if jumlah <= 0:
            raise ValueError("jumlah harus positif")
        total = jumlah + self.BIAYA_ADMIN
        if total > self.__saldo:
            raise ValueError("saldo tidak cukup")
        self.__saldo -= total
        self._riwayat.append(("tarik", jumlah))

    def riwayat(self):
        """Daftar transaksi (jenis, jumlah) rekening ini."""
        return self._riwayat


class TabunganPelajar(Tabungan):
    """Tabungan pelajar: penarikan maksimal 500_000 per transaksi, tanpa biaya admin."""

    BATAS_TARIK = 500_000

    def tarik(self, jumlah):
        if jumlah <= 0:
            raise ValueError("jumlah harus positif")
        if jumlah > self.BATAS_TARIK:
            raise ValueError("melebihi batas tarik pelajar")
        if jumlah > self.saldo:
            raise ValueError("saldo tidak cukup")
        self.__saldo = self.saldo - jumlah
        self._riwayat.append(("tarik", jumlah))
