import unittest

from solution import LOG_PANGGILAN, Perpustakaan


class Base(unittest.TestCase):
    def setUp(self):
        LOG_PANGGILAN.clear()
        self.p = Perpustakaan(["A01", "A02"])


class TestNormalBehavior(Base):
    def test_normal_pinjam_posisi(self):
        self.assertEqual(self.p.pinjam("A01", "Clean Code"), 1)
        self.assertEqual(self.p.peminjaman, [("A01", "Clean Code", 7)])

    def test_normal_nama_method_terjaga(self):
        self.assertEqual(Perpustakaan.pinjam.__name__, "pinjam")

    def test_normal_non_anggota_ditolak(self):
        with self.assertRaises(PermissionError):
            self.p.pinjam("X99", "Refactoring")


class TestB1Kwargs(Base):
    def test_b1_pinjam_dengan_keyword(self):
        self.assertEqual(self.p.pinjam("A01", judul="SICP"), 1)

    def test_b1_kwargs_tercatat_di_log(self):
        self.p.pinjam("A02", "SICP", durasi=3)
        self.assertEqual(LOG_PANGGILAN, [("pinjam", ("A02", "SICP"), {"durasi": 3})])


class TestB2StatistikSetelahValidasi(Base):
    def test_b2_non_anggota_tidak_menambah_statistik(self):
        with self.assertRaises(PermissionError):
            self.p.pinjam("X99", "Refactoring")
        self.assertEqual(self.p.statistik, 0)

    def test_b2_durasi_berlebih_tidak_menambah_statistik(self):
        with self.assertRaises(ValueError):
            self.p.pinjam("A01", "Refactoring", durasi=30)
        self.assertEqual(self.p.statistik, 0)
        self.p.pinjam("A01", "Refactoring")
        self.assertEqual(self.p.statistik, 1)


class TestB3DurasiPosisi(Base):
    def test_b3_durasi_posisi_juga_dibatasi(self):
        with self.assertRaises(ValueError):
            self.p.pinjam("A01", "Design Patterns", 30)
        self.assertEqual(self.p.peminjaman, [])


class TestB4DurasiKeywordHilang(Base):
    def test_b4_durasi_keyword_tersimpan(self):
        self.p.pinjam("A01", "Pragmatic Programmer", durasi=10)
        self.assertEqual(self.p.peminjaman, [("A01", "Pragmatic Programmer", 10)])


class TestB5LogPanggilanGagal(Base):
    def test_b5_pemanggilan_gagal_tetap_tercatat(self):
        with self.assertRaises(PermissionError):
            self.p.pinjam("X99", "Refactoring")
        self.p.pinjam("A01", "Refactoring")
        self.assertEqual(len(LOG_PANGGILAN), 2)
        self.assertEqual(LOG_PANGGILAN[0][1], ("X99", "Refactoring"))


if __name__ == "__main__":
    unittest.main()
