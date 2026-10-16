import unittest

from solution import Tabungan, TabunganPelajar


class TestNormalBehavior(unittest.TestCase):
    def test_normal_setor_dan_tarik(self):
        t = Tabungan("Ani", 100_000)
        t.setor(50_000)
        t.tarik(30_000)
        self.assertEqual(t.saldo, 120_000)

    def test_normal_saldo_awal_negatif_ditolak(self):
        with self.assertRaises(ValueError):
            Tabungan("Budi", -1)

    def test_normal_tarik_melebihi_saldo_ditolak(self):
        t = Tabungan("Cici", 10_000)
        with self.assertRaises(ValueError):
            t.tarik(20_000)
        self.assertEqual(t.saldo, 10_000)

    def test_normal_ganti_pemilik_kosong_ditolak(self):
        t = Tabungan("Dodi", 0)
        with self.assertRaises(ValueError):
            t.pemilik = "   "
        self.assertEqual(t.pemilik, "Dodi")


class TestB1ReadOnlySaldo(unittest.TestCase):
    def test_b1_saldo_tidak_dapat_ditimpa(self):
        t = Tabungan("Eka", 1_000)
        with self.assertRaises(AttributeError):
            t.saldo = 10**9
        self.assertEqual(t.saldo, 1_000)


class TestB2ValidasiSetor(unittest.TestCase):
    def test_b2_setor_negatif_ditolak(self):
        t = Tabungan("Fani", 50_000)
        with self.assertRaises(ValueError):
            t.setor(-40_000)
        self.assertEqual(t.saldo, 50_000)

    def test_b2_setor_nol_ditolak(self):
        t = Tabungan("Gina", 0)
        with self.assertRaises(ValueError):
            t.setor(0)
        self.assertEqual(t.riwayat(), [])


class TestB3NameMangling(unittest.TestCase):
    def test_b3_tarik_pelajar_mengurangi_saldo(self):
        t = TabunganPelajar("Hana", 300_000)
        t.tarik(100_000)
        self.assertEqual(t.saldo, 200_000)

    def test_b3_saldo_tidak_bisa_ditarik_dua_kali(self):
        t = TabunganPelajar("Irfan", 100_000)
        t.tarik(80_000)
        with self.assertRaises(ValueError):
            t.tarik(80_000)

    def test_b3_tidak_ada_vault_tersembunyi(self):
        t = TabunganPelajar("Jihan", 100_000)
        t.tarik(10_000)
        self.assertNotIn("_TabunganPelajar__saldo", vars(t))

    def test_normal_batas_tarik_pelajar(self):
        t = TabunganPelajar("Kevin", 2_000_000)
        with self.assertRaises(ValueError):
            t.tarik(600_000)


class TestB4RiwayatBocor(unittest.TestCase):
    def test_b4_riwayat_tidak_dapat_dimanipulasi_dari_luar(self):
        t = Tabungan("Lala", 100_000)
        t.tarik(25_000)
        r = t.riwayat()
        r.clear()
        r.append(("setor", 10**9))
        self.assertEqual(t.riwayat(), [("tarik", 25_000)])


class TestB5ValidasiConstructor(unittest.TestCase):
    def test_b5_pemilik_kosong_ditolak_saat_dibuat(self):
        with self.assertRaises(ValueError):
            Tabungan("   ", 10_000)

    def test_b5_nama_pemilik_dirapikan_saat_dibuat(self):
        t = Tabungan("  Maya  ", 0)
        self.assertEqual(t.pemilik, "Maya")


if __name__ == "__main__":
    unittest.main()
