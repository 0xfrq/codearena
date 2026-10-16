import unittest

from solution import KartuPerpus, LaporanNilai, StrukKantin, bisa_diekspor, ekspor_semua


class TestNormalBehavior(unittest.TestCase):
    def test_normal_laporan_nilai(self):
        self.assertEqual(ekspor_semua([LaporanNilai("25032014005", "A")]), ["NILAI;25032014005;A"])

    def test_normal_kartu_bukan_turunan_laporan(self):
        from solution import LaporanBase

        self.assertFalse(isinstance(KartuPerpus(1), LaporanBase))


class TestB1DuckTyping(unittest.TestCase):
    def test_b1_objek_duck_typed_diekspor(self):
        hasil = ekspor_semua([LaporanNilai("25032014006", "B"), KartuPerpus(42)])
        self.assertEqual(hasil, ["NILAI;25032014006;B", "PERPUS;42"])

    def test_b1_objek_tanpa_perilaku_ditolak(self):
        with self.assertRaises(TypeError):
            ekspor_semua([object()])


class TestB2StructuralCheck(unittest.TestCase):
    def test_b2_cek_protocol_tidak_error(self):
        self.assertTrue(bisa_diekspor(KartuPerpus(1)))
        self.assertFalse(bisa_diekspor(42))

    def test_b2_export_bukan_callable_ditolak(self):
        class Palsu:
            export = "bukan method"

        self.assertFalse(bisa_diekspor(Palsu()))


class TestB3StrukBehavior(unittest.TestCase):
    def test_b3_struk_kantin_punya_method_export(self):
        self.assertEqual(StrukKantin(15_000).export(), "KANTIN;15000")

    def test_b3_struk_ikut_ekspor_semua(self):
        self.assertEqual(ekspor_semua([StrukKantin(5_000)]), ["KANTIN;5000"])


if __name__ == "__main__":
    unittest.main()
