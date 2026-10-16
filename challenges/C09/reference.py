"""C09 — Multiple Inheritance Conflict (reference solution)

Perbaikan:
- B1: LogMixin.__init__ cooperative (super().__init__) sehingga ValidasiMixin.__init__ ikut berjalan.
- B2: LogMixin.proses memakai super().proses() sehingga validasi berikutnya di MRO tidak dilewati.
- B3: PembayaranKantin tidak memanggil kedua mixin secara eksplisit; cukup mewarisi rantai MRO.
- B4: urutan base PembayaranParkir diperbaiki menjadi (LogMixin, ValidasiMixin).
- B5: PembayaranHibah menetapkan batas SETELAH super().__init__ agar tidak ditimpa base.
"""


class Pembayaran:
    def __init__(self, dompet, jumlah):
        self.dompet = dompet
        self.jumlah = jumlah
        self.jejak = []

    def proses(self):
        self.dompet["saldo"] -= self.jumlah
        self.jejak.append("debit")
        return self.dompet["saldo"]


class LogMixin(Pembayaran):
    def __init__(self, dompet, jumlah):
        super().__init__(dompet, jumlah)
        self.kanal = "aplikasi"

    def proses(self):
        self.jejak.append("log")
        return super().proses()


class ValidasiMixin(Pembayaran):
    def __init__(self, dompet, jumlah):
        super().__init__(dompet, jumlah)
        self.batas = 10_000_000

    def proses(self):
        if not (0 < self.jumlah <= self.batas):
            raise ValueError("jumlah di luar batas")
        if self.jumlah > self.dompet["saldo"]:
            raise ValueError("saldo tidak cukup")
        self.jejak.append("validasi")
        return super().proses()


class PembayaranUKT(LogMixin, ValidasiMixin):
    pass


class PembayaranKantin(LogMixin, ValidasiMixin):
    pass


class PembayaranParkir(LogMixin, ValidasiMixin):
    pass


class PembayaranHibah(ValidasiMixin, Pembayaran):
    def __init__(self, dompet, jumlah):
        super().__init__(dompet, jumlah)
        self.batas = 50_000_000
