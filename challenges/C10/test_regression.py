import unittest

from solution import QRIS, EWallet, MetodePembayaran, Transfer, Tunai, proses_semua


class TestNormalBehavior(unittest.TestCase):
    def test_normal_tunai(self):
        self.assertEqual(Tunai("Ani").bayar(50_000), "TUNAI 50000")

    def test_normal_qris_dibulatkan_ke_bawah(self):
        self.assertEqual(QRIS("Ani").biaya_admin(10_999), 76)


class TestB1AbstractTanpaABC(unittest.TestCase):
    def test_b1_kontrak_tidak_dapat_diinstansiasi(self):
        with self.assertRaises(TypeError):
            MetodePembayaran("Ani")

    def test_b1_subclass_belum_lengkap_tidak_dapat_diinstansiasi(self):
        class Setengah(MetodePembayaran):
            def biaya_admin(self, jumlah):
                return 0

        with self.assertRaises(TypeError):
            Setengah("Budi")


class TestB2TransferBiayaAdmin(unittest.TestCase):
    def test_b2_transfer_biaya_flat(self):
        t = Transfer("Cici")
        self.assertEqual(t.biaya_admin(100_000), 2_500)
        self.assertEqual(t.bayar(100_000), "TRANSFER 102500")


class TestB3SignatureKontrak(unittest.TestCase):
    def test_b3_qris_bayar_menerima_jumlah(self):
        self.assertEqual(QRIS("Dodi").bayar(100_000), "QRIS 100700")

    def test_b3_proses_semua_polimorfik(self):
        hasil = proses_semua([Tunai("Eka"), QRIS("Eka")], 100_000)
        self.assertEqual(hasil, ["TUNAI 100000", "QRIS 100700"])


class TestB4TipeKembalian(unittest.TestCase):
    def test_b4_biaya_admin_ewallet_integer(self):
        e = EWallet("Fani")
        self.assertIsInstance(e.biaya_admin(150_050), int)
        self.assertEqual(e.biaya_admin(150_050), 1_500)
        self.assertEqual(e.bayar(150_050), "EWALLET 151550")


class TestB5InvariantBase(unittest.TestCase):
    def test_b5_tunai_jumlah_tidak_valid_ditolak(self):
        with self.assertRaises(ValueError):
            Tunai("Gina").bayar(0)
        with self.assertRaises(ValueError):
            Tunai("Gina").bayar(-5_000)


if __name__ == "__main__":
    unittest.main()
