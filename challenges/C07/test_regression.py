import unittest

from solution import Civitas, Dosen, Mahasiswa, Tendik


class TestNormalBehavior(unittest.TestCase):
    def test_normal_base_class(self):
        c = Civitas("Umum", 7)
        self.assertEqual(c.kartu(), "CIV-0007")
        self.assertTrue(c.aktif)

    def test_normal_hierarki(self):
        self.assertTrue(issubclass(Dosen, Civitas))
        self.assertTrue(issubclass(Mahasiswa, Civitas))
        self.assertTrue(issubclass(Tendik, Civitas))

    def test_normal_tendik_nonaktif(self):
        t = Tendik("Pak Joko", 3, "BAAK")
        t.nonaktifkan()
        self.assertFalse(t.aktif)


class TestB1SuperInit(unittest.TestCase):
    def test_b1_dosen_punya_attribute_base(self):
        d = Dosen("Bu Rina", 12, "0011223344")
        self.assertEqual(d.nama, "Bu Rina")
        self.assertEqual(d.nomor, 12)
        self.assertTrue(d.aktif)
        self.assertEqual(d.nidn, "0011223344")

    def test_b1_dosen_validasi_base_berjalan(self):
        with self.assertRaises(ValueError):
            Dosen("Bu Rina", 0, "0011223344")


class TestB2PrefixLookup(unittest.TestCase):
    def test_b2_kartu_tendik_memakai_prefix_turunan(self):
        self.assertEqual(Tendik("Pak Joko", 3, "BAAK").kartu(), "TDK-0003")

    def test_b2_profil_dosen(self):
        d = Dosen("Bu Rina", 12, "0011223344")
        self.assertEqual(d.profil(), "DSN-0012 Bu Rina (aktif)")


class TestB3ArgumentOrder(unittest.TestCase):
    def test_b3_mahasiswa_nama_dan_nomor_benar(self):
        m = Mahasiswa("Ani", 5, "Informatika")
        self.assertEqual(m.nama, "Ani")
        self.assertEqual(m.nomor, 5)
        self.assertEqual(m.kartu(), "MHS-0005")


if __name__ == "__main__":
    unittest.main()
