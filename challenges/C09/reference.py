"""C09 — Multiple Inheritance Conflict (reference solution)

Perbaikan:
- B1: LogMixin.__init__ memakai super().__init__ (cooperative) sehingga ValidasiMixin.__init__ ikut berjalan.
- B2: LogMixin.proses memakai super().proses() sehingga mengikuti MRO: LogMixin -> ValidasiMixin -> Pembayaran.
- B3: PembayaranKantin tidak memanggil kedua mixin secara eksplisit (yang menyebabkan debit ganda);
      cukup mewarisi rantai MRO yang sudah benar.
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
    """Pembayaran UKT: dicatat lalu divalidasi sebelum didebit."""


class PembayaranKantin(LogMixin, ValidasiMixin):
    """Pembayaran kantin: rantai MRO sudah menjalankan kedua mixin tepat sekali."""
