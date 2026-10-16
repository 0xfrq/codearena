"""C09 — Multiple Inheritance Conflict

Pembayaran kampus (UKT, kantin, parkir, hibah riset) yang menggabungkan beberapa mixin
melalui multiple inheritance. Dompet direpresentasikan sebagai dict: {"saldo": <int>}.

Rantai yang diharapkan untuk setiap pembayaran ber-log: log -> validasi -> debit (tepat satu kali).
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
        Pembayaran.__init__(self, dompet, jumlah)
        self.kanal = "aplikasi"

    def proses(self):
        self.jejak.append("log")
        return Pembayaran.proses(self)


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
    """Pembayaran kantin: pengembang ingin 'memastikan' kedua mixin dijalankan."""

    def proses(self):
        LogMixin.proses(self)
        return ValidasiMixin.proses(self)


class PembayaranParkir(ValidasiMixin, LogMixin):
    """Pembayaran parkir: alurnya sama dengan pembayaran ber-log lainnya."""


class PembayaranHibah(ValidasiMixin, Pembayaran):
    """Pencairan hibah riset: tanpa log, tetapi batas transaksinya 50.000.000."""

    def __init__(self, dompet, jumlah):
        self.batas = 50_000_000
        super().__init__(dompet, jumlah)
