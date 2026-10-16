import unittest

from solution import Tabungan, TabunganPelajar


class TestNormalBehavior(unittest.TestCase):
    def test_normal_setor_tarik(self):
        t = Tabungan("Ani", 100_000)
        t.setor(50_000)
        t.tarik(30_000)
        self.assertEqual(t.saldo, 120_000)
        self.assertEqual(t.riwayat(), [("setor", 50_000), ("tarik", 30_000)])

    def test_normal_saldo_privat_ter_mangling(self):
        t = Tabungan("Budi", 10)
        self.assertFalse(hasattr(t, "__saldo"))
        self.assertIn("_Tabungan__saldo", vars(t))

    def test_normal_pelajar_batas_tarik(self):
        p = TabunganPelajar("Citra", 1_000_000)
        with self.assertRaises(ValueError):
            p.tarik(600_000)


class TestB1ReadOnlySaldo(unittest.TestCase):
    def test_b1_saldo_tidak_bisa_diassign_langsung(self):
        t = Tabungan("Dodi", 1_000)
        with self.assertRaises(AttributeError):
            t.saldo = 999_999_999
        self.assertEqual(t.saldo, 1_000)


class TestB2SetorValidation(unittest.TestCase):
    def test_b2_setor_negatif_ditolak(self):
        t = Tabungan("Eka", 100_000)
        with self.assertRaises(ValueError):
            t.setor(-50_000)
        self.assertEqual(t.saldo, 100_000)

    def test_b2_setor_nol_ditolak(self):
        t = Tabungan("Fajar", 0)
        with self.assertRaises(ValueError):
            t.setor(0)


class TestB3NameMangling(unittest.TestCase):
    def test_b3_tarik_pelajar_mengurangi_saldo(self):
        p = TabunganPelajar("Gita", 300_000)
        p.tarik(100_000)
        self.assertEqual(p.saldo, 200_000)

    def test_b3_tidak_ada_saldo_bayangan(self):
        p = TabunganPelajar("Hana", 300_000)
        p.tarik(100_000)
        self.assertNotIn("_TabunganPelajar__saldo", vars(p))

    def test_b3_tidak_bisa_tarik_berulang_melebihi_saldo(self):
        p = TabunganPelajar("Indra", 150_000)
        p.tarik(100_000)
        with self.assertRaises(ValueError):
            p.tarik(100_000)


if __name__ == "__main__":
    unittest.main()
