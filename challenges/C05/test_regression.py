import unittest

from solution import AUDIT_LOG, KasirKoperasi


class Base(unittest.TestCase):
    def setUp(self):
        AUDIT_LOG.clear()


class TestNormalBehavior(Base):
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

    def test_normal_kembalian_melebihi_kas_ditolak(self):
        k = KasirKoperasi(1_000)
        with self.assertRaises(ValueError):
            k.beri_kembalian(5_000)
        self.assertEqual(k.saldo_kas, 1_000)


class TestB1ReturnValue(Base):
    def test_b1_terima_pembayaran_mengembalikan_saldo(self):
        k = KasirKoperasi(2_000)
        self.assertEqual(k.terima_pembayaran(3_000), 5_000)

    def test_b1_beri_kembalian_mengembalikan_saldo(self):
        k = KasirKoperasi(5_000)
        self.assertEqual(k.beri_kembalian(1_500), 3_500)


class TestB2WaktuAudit(Base):
    def test_b2_audit_dicatat_saat_pemanggilan(self):
        k = KasirKoperasi(10_000)
        k.terima_pembayaran(1_000)
        k.terima_pembayaran(2_000)
        k.beri_kembalian(500)
        self.assertEqual(AUDIT_LOG, ["terima_pembayaran", "terima_pembayaran", "beri_kembalian"])

    def test_b2_operasi_gagal_tidak_dicatat(self):
        k = KasirKoperasi(100)
        k.terima_pembayaran(100)
        with self.assertRaises(ValueError):
            k.beri_kembalian(1_000)
        self.assertEqual(AUDIT_LOG, ["terima_pembayaran"])


class TestB3Metadata(Base):
    def test_b3_nama_method_terjaga(self):
        self.assertEqual(KasirKoperasi.terima_pembayaran.__name__, "terima_pembayaran")
        self.assertEqual(KasirKoperasi.beri_kembalian.__name__, "beri_kembalian")

    def test_b3_docstring_terjaga(self):
        self.assertIn("saldo kas terbaru", KasirKoperasi.beri_kembalian.__doc__ or "")


class TestB4KuotaPerKasir(Base):
    def test_b4_kuota_kasir_lain_tidak_terpakai(self):
        k1 = KasirKoperasi(10_000)
        k2 = KasirKoperasi(10_000)
        for _ in range(3):
            k1.beri_kembalian(100)
        with self.assertRaises(PermissionError):
            k1.beri_kembalian(100)
        k2.beri_kembalian(100)
        self.assertEqual(k2.saldo_kas, 9_900)


class TestB5KuotaHanyaUntukYangBerhasil(Base):
    def test_b5_kembalian_gagal_tidak_mengurangi_kuota(self):
        k = KasirKoperasi(0)
        for _ in range(3):
            with self.assertRaises(ValueError):
                k.beri_kembalian(100)  # kas tidak cukup
        k.terima_pembayaran(1_000)
        k.beri_kembalian(100)
        self.assertEqual(k.saldo_kas, 900)

    def test_b5_nominal_tidak_valid_tidak_mengurangi_kuota(self):
        k = KasirKoperasi(1_000)
        for _ in range(3):
            with self.assertRaises(ValueError):
                k.beri_kembalian(0)
        k.beri_kembalian(100)
        self.assertEqual(k.saldo_kas, 900)


if __name__ == "__main__":
    unittest.main()
