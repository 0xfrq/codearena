import unittest

from solution import LogMixin, Pembayaran, PembayaranKantin, PembayaranUKT, ValidasiMixin


class TestNormalBehavior(unittest.TestCase):
    def test_normal_mro(self):
        self.assertEqual(
            PembayaranUKT.__mro__[:4], (PembayaranUKT, LogMixin, ValidasiMixin, Pembayaran)
        )

    def test_normal_pembayaran_dasar(self):
        dompet = {"saldo": 100}
        self.assertEqual(Pembayaran(dompet, 30).proses(), 70)


class TestB1CooperativeInit(unittest.TestCase):
    def test_b1_semua_init_berjalan(self):
        p = PembayaranUKT({"saldo": 5_000_000}, 1_000)
        self.assertEqual(p.kanal, "aplikasi")
        self.assertEqual(p.batas, 10_000_000)


class TestB2MroChain(unittest.TestCase):
    def test_b2_urutan_log_validasi_debit(self):
        dompet = {"saldo": 5_000_000}
        p = PembayaranUKT(dompet, 2_500_000)
        self.assertEqual(p.proses(), 2_500_000)
        self.assertEqual(p.jejak, ["log", "validasi", "debit"])

    def test_b2_saldo_kurang_ditolak(self):
        dompet = {"saldo": 1_000}
        with self.assertRaises(ValueError):
            PembayaranUKT(dompet, 2_000).proses()
        self.assertEqual(dompet["saldo"], 1_000)

    def test_b2_jumlah_negatif_ditolak(self):
        dompet = {"saldo": 1_000}
        with self.assertRaises(ValueError):
            PembayaranUKT(dompet, -500).proses()
        self.assertEqual(dompet["saldo"], 1_000)


class TestB3NoDoubleCharge(unittest.TestCase):
    def test_b3_kantin_didebit_sekali(self):
        dompet = {"saldo": 50_000}
        p = PembayaranKantin(dompet, 15_000)
        p.proses()
        self.assertEqual(dompet["saldo"], 35_000)
        self.assertEqual(p.jejak.count("debit"), 1)


if __name__ == "__main__":
    unittest.main()
