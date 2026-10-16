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

    def test_normal_pindahkan_berhasil(self):
        a = Gudang("Lab A")
        b = Gudang("Lab B")
        a.tambah("ROUTER", 5)
        a.pindahkan(b, "ROUTER", 2)
        self.assertEqual(a.jumlah_stok("ROUTER"), 3)
        self.assertEqual(b.jumlah_stok("ROUTER"), 2)


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


class TestB4LeakedInternalState(unittest.TestCase):
    def test_b4_mengubah_daftar_barang_tidak_mengubah_stok(self):
        g = Gudang("Gudang F")
        g.tambah("OSILOSKOP", 2)
        daftar = g.daftar_barang()
        daftar["OSILOSKOP"] = 999
        daftar["SIMSALABIM"] = 1
        self.assertEqual(g.jumlah_stok("OSILOSKOP"), 2)
        self.assertEqual(g.jumlah_stok("SIMSALABIM"), 0)

    def test_b4_daftar_barang_hanya_berisi_stok_tersedia(self):
        g = Gudang("Gudang G")
        g.tambah("MULTIMETER", 1)
        g.tambah("SOLDER", 4)
        g.ambil("MULTIMETER", 1)
        self.assertEqual(g.daftar_barang(), {"SOLDER": 4})


class TestB5NonAtomicTransfer(unittest.TestCase):
    def test_b5_pindahkan_gagal_tidak_mengubah_gudang_tujuan(self):
        a = Gudang("Gudang H")
        b = Gudang("Gudang I")
        a.tambah("LCD", 2)
        with self.assertRaises(ValueError):
            a.pindahkan(b, "LCD", 5)
        self.assertEqual(b.jumlah_stok("LCD"), 0)
        self.assertEqual(a.jumlah_stok("LCD"), 2)


if __name__ == "__main__":
    unittest.main()
