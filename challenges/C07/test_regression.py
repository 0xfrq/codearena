import unittest

from solution import Civitas, Dosen, DosenTamu, Mahasiswa, Tendik, hanya_dosen


class TestNormalBehavior(unittest.TestCase):
    def test_normal_civitas_dasar(self):
        c = Civitas("Umum", 12)
        self.assertEqual(c.profil(), "CIV-0012 Umum (aktif)")

    def test_normal_nomor_tidak_valid_ditolak(self):
        with self.assertRaises(ValueError):
            Civitas("X", 0)
        with self.assertRaises(ValueError):
            Mahasiswa("Y", -3, "TI")

    def test_normal_mahasiswa_dibuat_lengkap(self):
        m = Mahasiswa("Ani", 42, "TI")
        self.assertEqual((m.nama, m.nomor, m.prodi, m.aktif), ("Ani", 42, "TI", True))


class TestB1ConstructorDosen(unittest.TestCase):
    def test_b1_dosen_memiliki_attribute_base(self):
        d = Dosen("Budi", 7, "0011223344")
        self.assertEqual((d.nama, d.nomor, d.aktif, d.nidn), ("Budi", 7, True, "0011223344"))

    def test_b1_validasi_base_berlaku_untuk_dosen(self):
        with self.assertRaises(ValueError):
            Dosen("Citra", 0, "0099")


class TestB2PrefixKartu(unittest.TestCase):
    def test_b2_prefix_mengikuti_class_konkret(self):
        self.assertEqual(Mahasiswa("Ani", 42, "TI").kartu(), "MHS-0042")
        self.assertEqual(Tendik("Sari", 5, "BAAK").kartu(), "TDK-0005")

    def test_b2_prefix_dosen_tamu(self):
        self.assertEqual(DosenTamu("Dedi", 3, "0077", "ITB").kartu(), "DTM-0003")


class TestB3ArgumenTendik(unittest.TestCase):
    def test_b3_nama_tendik_tidak_tertukar_dengan_unit(self):
        t = Tendik("Sari", 5, "BAAK")
        self.assertEqual(t.nama, "Sari")
        self.assertEqual(t.unit, "BAAK")


class TestB4OverrideTanpaSuper(unittest.TestCase):
    def test_b4_mahasiswa_nonaktif_benar_benar_nonaktif(self):
        m = Mahasiswa("Eka", 9, "SI")
        m.nonaktifkan()
        self.assertFalse(m.aktif)
        self.assertEqual(m.status_studi, "cuti")
        self.assertTrue(m.profil().endswith("(nonaktif)"))


class TestB5IsInstance(unittest.TestCase):
    def test_b5_dosen_tamu_termasuk_dosen(self):
        d = Dosen("Fajar", 1, "001")
        dt = DosenTamu("Gita", 2, "002", "UGM")
        m = Mahasiswa("Hadi", 3, "TI")
        self.assertEqual(hanya_dosen([m, d, dt]), [d, dt])


if __name__ == "__main__":
    unittest.main()
