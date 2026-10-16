"""C03 — The Invisible Vault (reference solution)

Perbaikan:
- B1: property saldo tidak memiliki setter -> read-only (assignment memicu AttributeError).
- B2: setor() memvalidasi jumlah > 0 sebelum mengubah saldo.
- B3: TabunganPelajar tidak menulis self.__saldo (yang ter-mangle menjadi _TabunganPelajar__saldo);
      validasi khusus pelajar lalu delegasi ke super().tarik().
- B4: riwayat() mengembalikan salinan list, bukan list internal.
- B5: __init__ meng-assign melalui property (self.pemilik = ...) sehingga validasi juga berlaku saat object dibuat.
"""


class Tabungan:
    BIAYA_ADMIN = 0

    def __init__(self, pemilik, saldo_awal=0):
        if saldo_awal < 0:
            raise ValueError("saldo awal tidak boleh negatif")
        self.pemilik = pemilik      # lewat setter -> tervalidasi
        self._riwayat = []
        self.__saldo = saldo_awal

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

    def setor(self, jumlah):
        if jumlah <= 0:
            raise ValueError("jumlah harus positif")
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
        return list(self._riwayat)


class TabunganPelajar(Tabungan):
    BATAS_TARIK = 500_000

    def tarik(self, jumlah):
        if jumlah > self.BATAS_TARIK:
            raise ValueError("melebihi batas tarik pelajar")
        super().tarik(jumlah)
