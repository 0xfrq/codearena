import unittest

from solution import Gudang


class TestNormalBehavior(unittest.TestCase):
    def test_normal_tambah_dan_ambil(self):
        g = Gudang("Lab RPL")
        g.tambah("MOUSE", 5)
        g.ambil("MOUSE", 2)
        self.assertEqual(g.jumlah_stok("MOUSE"), 3)

    def test_normal_ambil_melebihi_stok_ditolak(self):
        g = Gudang("Lab RPL")
        g.tambah("KABEL", 1)
        with self.assertRaises(ValueError):
            g.ambil("KABEL", 2)

    def test_normal_satuan_class_attribute(self):
        self.assertEqual(Gudang.satuan, "unit")
        self.assertEqual(Gudang("X").satuan, "unit")


class TestB1SharedState(unittest.TestCase):
    def test_b1_stok_gudang_tidak_bocor_ke_gudang_lain(self):
        a = Gudang("Gudang A")
        b = Gudang("Gudang B")
        a.tambah("PC", 3)
        self.assertNotIn("PC", b.stok)

    def test_b1_stok_adalah_instance_attribute(self):
        g = Gudang("Gudang C")
        self.assertIn("stok", vars(g))


class TestB2UnknownItem(unittest.TestCase):
    def test_b2_barang_tidak_terdaftar_bernilai_nol(self):
        g = Gudang("Gudang D")
        self.assertEqual(g.jumlah_stok("TIDAK-ADA"), 0)


class TestB3EmptyItems(unittest.TestCase):
    def test_b3_barang_habis_tidak_dihitung_sebagai_jenis(self):
        g = Gudang("Gudang E")
        g.tambah("PROYEKTOR", 2)
        g.tambah("SPIDOL", 10)
        g.ambil("PROYEKTOR", 2)
        self.assertEqual(g.jenis_barang(), 1)
        self.assertEqual(g.ringkasan(), "Gudang E: 1 jenis, 10 unit")


if __name__ == "__main__":
    unittest.main()
