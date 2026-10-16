import dataclasses
import unittest

import solution
from solution import LaporanHarian, Transaksi


class TestNormalBehavior(unittest.TestCase):
    def test_normal_total(self):
        lap = LaporanHarian("2026-10-16")
        lap.tambah(Transaksi("T1", 12_000))
        lap.tambah(Transaksi("T2", 8_000, "qris"))
        self.assertEqual(lap.total(), 20_000)

    def test_normal_jumlah_tidak_positif_ditolak(self):
        lap = LaporanHarian("2026-10-16")
        with self.assertRaises(ValueError):
            lap.tambah(Transaksi("T0", 0))


class TestB1AliasingKategori(unittest.TestCase):
    def test_b1_kategori_laporan_tidak_mengubah_default(self):
        lap = LaporanHarian("2026-10-16")
        lap.kategori.append("promo")
        self.assertEqual(solution.KATEGORI_DEFAULT, ["umum"])
        self.assertEqual(LaporanHarian("2026-10-17").kategori, ["umum"])


class TestB2BuktiImmutable(unittest.TestCase):
    def test_b2_transaksi_tidak_dapat_diubah(self):
        t = Transaksi("T9", 15_000)
        with self.assertRaises(dataclasses.FrozenInstanceError):
            t.jumlah = 1
        self.assertEqual(t.jumlah, 15_000)


class TestB3ArsipIndependen(unittest.TestCase):
    def test_b3_arsip_tidak_ikut_berubah(self):
        lap = LaporanHarian("2026-10-16")
        lap.tambah(Transaksi("A1", 5_000))
        arsip = lap.salin()
        lap.tambah(Transaksi("A2", 7_000))
        lap.kategori.append("malam")
        self.assertEqual(len(arsip.transaksi), 1)
        self.assertEqual(arsip.total(), 5_000)
        self.assertNotIn("malam", arsip.kategori)


class TestB4KesetaraanNilai(unittest.TestCase):
    def test_b4_transaksi_sama_dianggap_sama(self):
        self.assertEqual(Transaksi("D1", 3_000), Transaksi("D1", 3_000))
        self.assertEqual(len({Transaksi("D1", 3_000), Transaksi("D1", 3_000)}), 1)

    def test_b4_transaksi_duplikat_ditolak(self):
        lap = LaporanHarian("2026-10-16")
        lap.tambah(Transaksi("D2", 4_000, "qris"))
        with self.assertRaises(ValueError):
            lap.tambah(Transaksi("D2", 4_000, "qris"))
        self.assertEqual(lap.total(), 4_000)


class TestB5ListBersama(unittest.TestCase):
    def test_b5_per_metode_terpisah(self):
        lap = LaporanHarian("2026-10-16")
        lap.tambah(Transaksi("M1", 10_000, "tunai"))
        lap.tambah(Transaksi("M2", 6_000, "qris"))
        lap.tambah(Transaksi("M3", 2_000, "tunai"))
        self.assertEqual(lap.per_metode(), {"tunai": ["M1", "M3"], "qris": ["M2"]})


if __name__ == "__main__":
    unittest.main()
