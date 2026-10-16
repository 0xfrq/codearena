import dataclasses
import unittest

from solution import KATEGORI_DEFAULT, LaporanHarian, Transaksi


class TestNormalBehavior(unittest.TestCase):
    def test_normal_total(self):
        lap = LaporanHarian("2026-10-16")
        lap.tambah(Transaksi("T1", 12_000))
        lap.tambah(Transaksi("T2", 8_000, "qris"))
        self.assertEqual(lap.total(), 20_000)

    def test_normal_dataclass_equality(self):
        self.assertEqual(Transaksi("T1", 5_000), Transaksi("T1", 5_000))

    def test_normal_jumlah_nol_ditolak(self):
        with self.assertRaises(ValueError):
            LaporanHarian("2026-10-16").tambah(Transaksi("T0", 0))


class TestB1SharedDefault(unittest.TestCase):
    def test_b1_kategori_tidak_dibagi(self):
        a = LaporanHarian("2026-10-16")
        b = LaporanHarian("2026-10-17")
        a.kategori.append("kantin")
        self.assertEqual(b.kategori, ["umum"])
        self.assertEqual(KATEGORI_DEFAULT, ["umum"])


class TestB2FrozenEvidence(unittest.TestCase):
    def test_b2_transaksi_tidak_bisa_diubah(self):
        t = Transaksi("T9", 50_000)
        with self.assertRaises(dataclasses.FrozenInstanceError):
            t.jumlah = 1
        self.assertEqual(t.jumlah, 50_000)


class TestB3IndependentCopy(unittest.TestCase):
    def test_b3_arsip_tidak_ikut_berubah(self):
        lap = LaporanHarian("2026-10-16")
        lap.tambah(Transaksi("T1", 10_000))
        arsip = lap.salin()
        lap.tambah(Transaksi("T2", 99_000))
        self.assertEqual(arsip.total(), 10_000)
        self.assertEqual(len(arsip.transaksi), 1)

    def test_b3_kategori_arsip_independen(self):
        lap = LaporanHarian("2026-10-16", kategori=["umum"])
        arsip = lap.salin()
        lap.kategori.append("darurat")
        self.assertEqual(arsip.kategori, ["umum"])


if __name__ == "__main__":
    unittest.main()
