import unittest

from solution import Manajer, Pegawai, PegawaiTetap, total_penggajian


class TestNormalBehavior(unittest.TestCase):
    def test_normal_gaji_staf(self):
        p = Pegawai("Andi", 5_000_000)
        self.assertEqual(p.hitung_gaji(2), 5_080_000)
        self.assertEqual(p.slip(), "Andi [Staf] Rp5,000,000")

    def test_normal_lembur_negatif_ditolak_staf(self):
        with self.assertRaises(ValueError):
            Pegawai("Andi", 5_000_000).hitung_gaji(-1)


class TestB1TypoOverride(unittest.TestCase):
    def test_b1_jabatan_manajer(self):
        m = Manajer("Maya", 10_000_000, 2_000_000, 3_000_000)
        self.assertEqual(m.jabatan(), "Manajer")
        self.assertIn("[Manajer]", m.slip())


class TestB2LapisanTerlewat(unittest.TestCase):
    def test_b2_gaji_manajer_termasuk_tunjangan(self):
        m = Manajer("Maya", 10_000_000, 2_000_000, 3_000_000)
        self.assertEqual(m.hitung_gaji(), 15_000_000)


class TestB3SignatureOverride(unittest.TestCase):
    def test_b3_pegawai_tetap_menerima_jam_lembur(self):
        t = PegawaiTetap("Tono", 6_000_000, 1_000_000)
        self.assertEqual(t.hitung_gaji(2), 7_080_000)

    def test_b3_lembur_negatif_ditolak_di_semua_level(self):
        with self.assertRaises(ValueError):
            PegawaiTetap("Tono", 6_000_000, 1_000_000).hitung_gaji(-1)
        with self.assertRaises(ValueError):
            Manajer("Maya", 10_000_000, 2_000_000, 3_000_000).hitung_gaji(-1)


class TestB4SlipManajer(unittest.TestCase):
    def test_b4_slip_manajer_memuat_lembur(self):
        m = Manajer("Maya", 10_000_000, 2_000_000, 3_000_000)
        self.assertEqual(m.slip(2), "Maya [Manajer] Rp15,080,000 *")


class TestB5RekapPolimorfik(unittest.TestCase):
    def test_b5_total_sama_dengan_jumlah_gaji_individu(self):
        daftar = [
            Pegawai("Andi", 5_000_000),
            PegawaiTetap("Tono", 6_000_000, 1_000_000),
            Manajer("Maya", 10_000_000, 2_000_000, 3_000_000),
        ]
        # 5.040.000 + 7.040.000 + 15.040.000
        self.assertEqual(total_penggajian(daftar, 1), 27_120_000)


if __name__ == "__main__":
    unittest.main()
