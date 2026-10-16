import unittest

from solution import Mahasiswa


class TestNormalBehavior(unittest.TestCase):
    def test_normal_ambil_mk(self):
        m = Mahasiswa("2501", "Ani", krs=[])
        m.ambil_mk("PBO", 3)
        m.ambil_mk("BASDAT", 4)
        self.assertEqual(m.total_sks(), 7)

    def test_normal_duplikat_dan_sks_nol_ditolak(self):
        m = Mahasiswa("2502", "Budi", krs=[])
        m.ambil_mk("PBO", 3)
        with self.assertRaises(ValueError):
            m.ambil_mk("PBO", 3)
        with self.assertRaises(ValueError):
            m.ambil_mk("ALPRO", 0)

    def test_normal_drop_kode_tidak_ada_ditolak(self):
        m = Mahasiswa("2503", "Cici", krs=[])
        with self.assertRaises(ValueError):
            m.drop_mk("TIDAK-ADA")


class TestB1MutableDefault(unittest.TestCase):
    def test_b1_krs_tidak_dibagi_antar_mahasiswa(self):
        a = Mahasiswa("2511", "Dina")
        b = Mahasiswa("2512", "Eko")
        a.ambil_mk("KALKULUS", 3)
        self.assertEqual(b.krs, [])
        self.assertIsNot(a.krs, b.krs)


class TestB2ClassCounter(unittest.TestCase):
    def test_b2_counter_class_bertambah(self):
        sebelum = Mahasiswa.jumlah_mahasiswa
        Mahasiswa("2521", "Fajar", krs=[])
        Mahasiswa("2522", "Gita", krs=[])
        self.assertEqual(Mahasiswa.jumlah_mahasiswa, sebelum + 2)

    def test_b2_counter_tidak_menjadi_instance_attribute(self):
        m = Mahasiswa("2523", "Hadi", krs=[])
        self.assertNotIn("jumlah_mahasiswa", vars(m))


class TestB3Dispensasi(unittest.TestCase):
    def test_b3_dispensasi_hanya_untuk_satu_mahasiswa(self):
        a = Mahasiswa("2531", "Indah", krs=[])
        b = Mahasiswa("2532", "Joko", krs=[])
        a.beri_dispensasi(30)
        self.assertEqual(a.batas_sks, 30)
        self.assertEqual(b.batas_sks, 24)
        self.assertEqual(Mahasiswa.batas_sks, 24)
        self.assertEqual(a.info(), "2531 Indah: 0/30 SKS")


class TestB4BatasInklusif(unittest.TestCase):
    def test_b4_tepat_24_sks_diperbolehkan(self):
        m = Mahasiswa("2541", "Kiki", krs=[])
        m.ambil_mk("MK1", 8)
        m.ambil_mk("MK2", 8)
        m.ambil_mk("MK3", 8)
        self.assertEqual(m.total_sks(), 24)
        with self.assertRaises(ValueError):
            m.ambil_mk("MK4", 1)


class TestB5DropMk(unittest.TestCase):
    def test_b5_drop_benar_benar_menghapus_mk(self):
        m = Mahasiswa("2551", "Lina", krs=[])
        m.ambil_mk("PBO", 3)
        m.ambil_mk("JARKOM", 3)
        m.drop_mk("PBO")
        self.assertEqual(m.krs, [("JARKOM", 3)])
        self.assertEqual(m.total_sks(), 3)
        m.ambil_mk("PBO", 3)  # boleh diambil ulang setelah di-drop


if __name__ == "__main__":
    unittest.main()
