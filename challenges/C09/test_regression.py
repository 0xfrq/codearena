import unittest

from solution import (
    LogMixin,
    Pembayaran,
    PembayaranHibah,
    PembayaranKantin,
    PembayaranParkir,
    PembayaranUKT,
    ValidasiMixin,
)


class TestNormalBehavior(unittest.TestCase):
    def test_normal_mro_ukt(self):
        self.assertEqual(PembayaranUKT.__mro__, (PembayaranUKT, LogMixin, ValidasiMixin, Pembayaran, object))

    def test_normal_ukt_valid(self):
        d = {"saldo": 5_000_000}
        self.assertEqual(PembayaranUKT(d, 3_000_000).proses(), 2_000_000)


class TestB1CooperativeInit(unittest.TestCase):
    def test_b1_attribute_kedua_mixin_terpasang(self):
        p = PembayaranUKT({"saldo": 1_000}, 100)
        self.assertEqual(p.kanal, "aplikasi")
        self.assertEqual(p.batas, 10_000_000)


class TestB2ValidasiTerlewat(unittest.TestCase):
    def test_b2_saldo_kurang_ditolak_tanpa_mengubah_saldo(self):
        d = {"saldo": 100_000}
        with self.assertRaises(ValueError):
            PembayaranUKT(d, 500_000).proses()
        self.assertEqual(d["saldo"], 100_000)

    def test_b2_urutan_jejak_ukt(self):
        p = PembayaranUKT({"saldo": 1_000_000}, 250_000)
        p.proses()
        self.assertEqual(p.jejak, ["log", "validasi", "debit"])


class TestB3DebitGanda(unittest.TestCase):
    def test_b3_kantin_mendebit_tepat_sekali(self):
        d = {"saldo": 100_000}
        p = PembayaranKantin(d, 20_000)
        self.assertEqual(p.proses(), 80_000)
        self.assertEqual(d["saldo"], 80_000)
        self.assertEqual(p.jejak.count("debit"), 1)


class TestB4UrutanMro(unittest.TestCase):
    def test_b4_mro_parkir(self):
        self.assertEqual(PembayaranParkir.__mro__[1:3], (LogMixin, ValidasiMixin))

    def test_b4_jejak_parkir_log_dulu(self):
        p = PembayaranParkir({"saldo": 50_000}, 5_000)
        p.proses()
        self.assertEqual(p.jejak, ["log", "validasi", "debit"])


class TestB5BatasHibah(unittest.TestCase):
    def test_b5_batas_hibah_tidak_tertimpa(self):
        d = {"saldo": 60_000_000}
        p = PembayaranHibah(d, 20_000_000)
        self.assertEqual(p.batas, 50_000_000)
        self.assertEqual(p.proses(), 40_000_000)

    def test_b5_hibah_di_atas_batas_ditolak(self):
        d = {"saldo": 90_000_000}
        with self.assertRaises(ValueError):
            PembayaranHibah(d, 60_000_000).proses()
        self.assertEqual(d["saldo"], 90_000_000)


if __name__ == "__main__":
    unittest.main()
