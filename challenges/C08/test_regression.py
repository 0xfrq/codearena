import unittest

from solution import Manajer, Pegawai, PegawaiTetap, total_penggajian


class TestNormalBehavior(unittest.TestCase):
    def test_normal_pegawai_lembur(self):
        self.assertEqual(Pegawai("Ani", 3_000_000).hitung_gaji(2), 3_080_000)

    def test_normal_lembur_negatif_ditolak(self):
        with self.assertRaises(ValueError):
            Pegawai("Ani", 3_000_000).hitung_gaji(-1)

    def test_normal_nested_inheritance(self):
        self.assertEqual(Manajer.__mro__[:3], (Manajer, PegawaiTetap, Pegawai))


class TestB1OverrideName(unittest.TestCase):
    def test_b1_jabatan_manajer(self):
        m = Manajer("Bu Sari", 8_000_000, 1_000_000, 2_000_000)
        self.assertEqual(m.jabatan(), "Manajer")
        self.assertIn("[Manajer]", m.slip())

    def test_b1_manajer_meng_override_jabatan(self):
        self.assertIn("jabatan", vars(Manajer))


class TestB2NestedSuper(unittest.TestCase):
    def test_b2_gaji_manajer_termasuk_tunjangan(self):
        m = Manajer("Bu Sari", 8_000_000, 1_000_000, 2_000_000)
        self.assertEqual(m.hitung_gaji(), 11_000_000)


class TestB3OverrideSignature(unittest.TestCase):
    def test_b3_pegawai_tetap_menerima_jam_lembur(self):
        p = PegawaiTetap("Pak Dodi", 5_000_000, 500_000)
        self.assertEqual(p.hitung_gaji(3), 5_620_000)

    def test_b3_total_penggajian_polimorfik(self):
        daftar = [
            Pegawai("Ani", 3_000_000),
            PegawaiTetap("Dodi", 5_000_000, 500_000),
            Manajer("Sari", 8_000_000, 1_000_000, 2_000_000),
        ]
        self.assertEqual(total_penggajian(daftar, 1), 3_040_000 + 5_540_000 + 11_040_000)


if __name__ == "__main__":
    unittest.main()
