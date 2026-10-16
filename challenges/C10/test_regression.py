import unittest

from solution import QRIS, MetodePembayaran, Transfer, Tunai, proses_semua


class TestNormalBehavior(unittest.TestCase):
    def test_normal_tunai(self):
        self.assertEqual(Tunai("Ani").bayar(50_000), "TUNAI 50000")

    def test_normal_jumlah_nol_ditolak(self):
        with self.assertRaises(ValueError):
            Tunai("Ani").bayar(0)


class TestB1AbstractEnforced(unittest.TestCase):
    def test_b1_kontrak_tidak_bisa_diinstansiasi(self):
        with self.assertRaises(TypeError):
            MetodePembayaran("Ani")

    def test_b1_subclass_tidak_lengkap_ditolak(self):
        class Setengah(MetodePembayaran):
            def bayar(self, jumlah):
                return "?"

        with self.assertRaises(TypeError):
            Setengah("Budi")


class TestB2TransferImplementsContract(unittest.TestCase):
    def test_b2_biaya_admin_transfer(self):
        self.assertEqual(Transfer("Citra").biaya_admin(100_000), 2_500)

    def test_b2_bayar_transfer(self):
        self.assertEqual(Transfer("Citra").bayar(100_000), "TRANSFER 102500")


class TestB3SignatureContract(unittest.TestCase):
    def test_b3_qris_bayar_menerima_jumlah(self):
        self.assertEqual(QRIS("Dodi").bayar(100_000), "QRIS 100700")

    def test_b3_proses_semua_polimorfik(self):
        hasil = proses_semua([Tunai("A"), Transfer("B"), QRIS("C")], 10_000)
        self.assertEqual(hasil, ["TUNAI 10000", "TRANSFER 12500", "QRIS 10070"])


if __name__ == "__main__":
    unittest.main()
