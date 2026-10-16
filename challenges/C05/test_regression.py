import unittest

from solution import AUDIT_LOG, KasirKoperasi


class TestNormalBehavior(unittest.TestCase):
    def setUp(self):
        AUDIT_LOG.clear()

    def test_normal_saldo_bertambah(self):
        k = KasirKoperasi(1_000)
        k.terima_pembayaran(500)
        self.assertEqual(k.saldo_kas, 1_500)

    def test_normal_nominal_negatif_ditolak_dan_tidak_dicatat(self):
        k = KasirKoperasi()
        with self.assertRaises(ValueError):
            k.terima_pembayaran(-10)
        self.assertEqual(k.saldo_kas, 0)
        self.assertEqual(AUDIT_LOG, [])


class TestB1ReturnValue(unittest.TestCase):
    def test_b1_terima_pembayaran_mengembalikan_saldo(self):
        k = KasirKoperasi(2_000)
        self.assertEqual(k.terima_pembayaran(3_000), 5_000)

    def test_b1_beri_kembalian_mengembalikan_saldo(self):
        k = KasirKoperasi(5_000)
        self.assertEqual(k.beri_kembalian(1_500), 3_500)


class TestB2LogAtCallTime(unittest.TestCase):
    def setUp(self):
        AUDIT_LOG.clear()

    def test_b2_setiap_pemanggilan_dicatat(self):
        k = KasirKoperasi()
        k.terima_pembayaran(5_000)
        k.terima_pembayaran(2_000)
        k.beri_kembalian(1_000)
        self.assertEqual(len(AUDIT_LOG), 3)

    def test_b2_kembalian_gagal_tidak_dicatat(self):
        k = KasirKoperasi(100)
        with self.assertRaises(ValueError):
            k.beri_kembalian(1_000)
        self.assertEqual(AUDIT_LOG, [])


class TestB3Metadata(unittest.TestCase):
    def setUp(self):
        AUDIT_LOG.clear()

    def test_b3_nama_method_terjaga(self):
        self.assertEqual(KasirKoperasi.terima_pembayaran.__name__, "terima_pembayaran")
        self.assertIn("saldo kas", KasirKoperasi.beri_kembalian.__doc__)

    def test_b3_log_memakai_nama_operasi(self):
        k = KasirKoperasi()
        k.terima_pembayaran(1_000)
        self.assertEqual(AUDIT_LOG, ["terima_pembayaran"])


if __name__ == "__main__":
    unittest.main()
