import unittest

from solution import Mahasiswa


class TestNormalBehavior(unittest.TestCase):
    def test_normal_ambil_mk(self):
        m = Mahasiswa("25032014005", "Ani", [])
        m.ambil_mk("PBO", 4)
        m.ambil_mk("BASDAT", 3)
        self.assertEqual(m.total_sks(), 7)

    def test_normal_duplikat_mk_ditolak(self):
        m = Mahasiswa("25032014006", "Budi", [])
        m.ambil_mk("PBO", 4)
        with self.assertRaises(ValueError):
            m.ambil_mk("PBO", 4)


class TestB1SharedDefault(unittest.TestCase):
    def test_b1_krs_default_tidak_dibagi(self):
        a = Mahasiswa("25032014007", "Citra")
        b = Mahasiswa("25032014018", "Dodi")
        a.ambil_mk("ALPRO", 3)
        self.assertEqual(b.krs, [])
        self.assertIsNot(a.krs, b.krs)


class TestB2Counter(unittest.TestCase):
    def test_b2_counter_class_bertambah(self):
        sebelum = Mahasiswa.jumlah_mahasiswa
        Mahasiswa("25032014021", "Eka", [])
        Mahasiswa("25032014025", "Fajar", [])
        self.assertEqual(Mahasiswa.jumlah_mahasiswa, sebelum + 2)

    def test_b2_counter_tidak_menjadi_instance_attribute(self):
        m = Mahasiswa("25032014029", "Gita", [])
        self.assertNotIn("jumlah_mahasiswa", vars(m))


class TestB3Dispensasi(unittest.TestCase):
    def test_b3_dispensasi_hanya_untuk_satu_mahasiswa(self):
        pintar = Mahasiswa("25032014031", "Hana", [])
        biasa = Mahasiswa("25032014032", "Indra", [])
        pintar.beri_dispensasi(30)
        self.assertEqual(pintar.batas_sks, 30)
        self.assertEqual(biasa.batas_sks, 24)
        self.assertEqual(Mahasiswa.batas_sks, 24)

    def test_b3_dispensasi_mengizinkan_sks_lebih(self):
        m = Mahasiswa("25032014039", "Joko", [])
        m.beri_dispensasi(26)
        m.ambil_mk("A", 13)
        m.ambil_mk("B", 13)
        self.assertEqual(m.total_sks(), 26)


if __name__ == "__main__":
    unittest.main()
