import unittest

from solution import KartuPerpus, LaporanNilai, StrukKantin, bisa_diekspor, ekspor_semua


class Palsu:
    export = "bukan fungsi"


class TestNormalBehavior(unittest.TestCase):
    def test_normal_laporan_nilai(self):
        self.assertEqual(ekspor_semua([LaporanNilai("2501", 88)]), ["NILAI;2501;88"])

    def test_normal_daftar_kosong(self):
        self.assertEqual(ekspor_semua([]), [])


class TestB1PemeriksaanNominal(unittest.TestCase):
    def test_b1_objek_duck_typed_ikut_diekspor(self):
        hasil = ekspor_semua([LaporanNilai("2501", 88), KartuPerpus(7)])
        self.assertEqual(hasil, ["NILAI;2501;88", "PERPUS;7"])

    def test_b1_objek_tanpa_export_ditolak(self):
        with self.assertRaises(TypeError):
            ekspor_semua([LaporanNilai("2501", 88), object()])


class TestB2RuntimeCheckable(unittest.TestCase):
    def test_b2_bisa_diekspor_tanpa_error(self):
        self.assertTrue(bisa_diekspor(KartuPerpus(1)))
        self.assertFalse(bisa_diekspor(42))
        self.assertFalse(bisa_diekspor(None))


class TestB3ExportSebagaiPerilaku(unittest.TestCase):
    def test_b3_struk_kantin_dapat_diekspor(self):
        self.assertEqual(StrukKantin(15_000).export(), "KANTIN;15000")

    def test_b3_struk_kantin_dalam_ekspor_semua(self):
        self.assertEqual(ekspor_semua([StrukKantin(9_000)]), ["KANTIN;9000"])


class TestB4CallableCheck(unittest.TestCase):
    def test_b4_attribute_export_non_callable_bukan_exportable(self):
        self.assertFalse(bisa_diekspor(Palsu()))

    def test_b4_ekspor_semua_menolak_export_non_callable(self):
        with self.assertRaises(TypeError):
            ekspor_semua([Palsu()])


class TestB5IterableApaPun(unittest.TestCase):
    def test_b5_tuple_dan_generator(self):
        self.assertEqual(ekspor_semua((KartuPerpus(3),)), ["PERPUS;3"])
        dokumen = (LaporanNilai(nim, 80) for nim in ["2511", "2512"])
        self.assertEqual(ekspor_semua(dokumen), ["NILAI;2511;80", "NILAI;2512;80"])


if __name__ == "__main__":
    unittest.main()
