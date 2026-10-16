import unittest

from solution import LOG_PANGGILAN, Perpustakaan


def perpus():
    return Perpustakaan(["25032014005", "25032014006"])


class TestNormalBehavior(unittest.TestCase):
    def setUp(self):
        LOG_PANGGILAN.clear()

    def test_normal_pinjam_posisi(self):
        p = perpus()
        self.assertEqual(p.pinjam("25032014005", "Clean Code"), 1)
        self.assertEqual(p.peminjaman, [("25032014005", "Clean Code", 7)])
        self.assertEqual(p.statistik, 1)

    def test_normal_bukan_anggota_ditolak(self):
        p = perpus()
        with self.assertRaises(PermissionError):
            p.pinjam("99999999999", "SICP")

    def test_normal_nama_method_terjaga(self):
        self.assertEqual(Perpustakaan.pinjam.__name__, "pinjam")


class TestB1Kwargs(unittest.TestCase):
    def test_b1_pinjam_dengan_keyword(self):
        p = perpus()
        p.pinjam("25032014006", "Design Patterns", durasi=10)
        self.assertEqual(p.peminjaman[-1], ("25032014006", "Design Patterns", 10))

    def test_b1_semua_argumen_keyword(self):
        p = perpus()
        p.pinjam(member="25032014005", judul="Refactoring", durasi=3)
        self.assertEqual(p.peminjaman[-1][2], 3)


class TestB2ChainingOrder(unittest.TestCase):
    def test_b2_penolakan_anggota_tidak_dihitung(self):
        p = perpus()
        with self.assertRaises(PermissionError):
            p.pinjam("00000000000", "SICP")
        self.assertEqual(p.statistik, 0)

    def test_b2_penolakan_durasi_tidak_dihitung(self):
        p = perpus()
        with self.assertRaises(ValueError):
            p.pinjam("25032014005", "SICP", 30)
        self.assertEqual(p.statistik, 0)


class TestB3PositionalArgs(unittest.TestCase):
    def test_b3_durasi_posisi_tetap_dibatasi(self):
        p = perpus()
        with self.assertRaises(ValueError):
            p.pinjam("25032014005", "TAOCP", 30)
        self.assertEqual(p.peminjaman, [])

    def test_b3_durasi_posisi_valid_diterima(self):
        p = perpus()
        p.pinjam("25032014005", "TAOCP", 14)
        self.assertEqual(p.peminjaman[-1][2], 14)


if __name__ == "__main__":
    unittest.main()
