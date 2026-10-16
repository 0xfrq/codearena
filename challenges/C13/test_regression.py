import dataclasses
import unittest

from solution import (
    Akun,
    AkunMahasiswa,
    AkunMerchant,
    PemrosesPembayaran,
    Transaksi,
    VoucherKampus,
)


class TestNormalBehavior(unittest.TestCase):
    def test_normal_akun_abstrak(self):
        with self.assertRaises(TypeError):
            Akun("X")

    def test_normal_topup(self):
        a = AkunMerchant("Kantin", 0)
        a.topup(25_000)
        self.assertEqual(a.saldo, 25_000)

    def test_normal_transfer_dengan_biaya_polimorfik(self):
        merchant = AkunMerchant("Kantin", 100_000)
        mhs = AkunMahasiswa("Ani", 0)
        biaya = PemrosesPembayaran().transfer(merchant, mhs, 50_000)
        self.assertEqual(biaya, 1_000)
        self.assertEqual(merchant.saldo, 49_000)
        self.assertEqual(mhs.saldo, 50_000)

    def test_normal_batas_harian_mahasiswa(self):
        mhs = AkunMahasiswa("Budi", 5_000_000)
        with self.assertRaises(ValueError):
            mhs.tarik(1_500_000)


class TestB1SharedHistory(unittest.TestCase):
    def test_b1_riwayat_tiap_akun_terpisah(self):
        a = AkunMerchant("Kantin A", 0)
        b = AkunMerchant("Kantin B", 0)
        a.topup(10_000)
        self.assertEqual(len(b.riwayat), 0)
        self.assertEqual(len(a.riwayat), 1)


class TestB2NameMangling(unittest.TestCase):
    def test_b2_tarik_mahasiswa_mengurangi_saldo(self):
        mhs = AkunMahasiswa("Citra", 300_000)
        mhs.tarik(100_000)
        self.assertEqual(mhs.saldo, 200_000)
        self.assertNotIn("_AkunMahasiswa__saldo", vars(mhs))

    def test_b2_tidak_bisa_menarik_uang_yang_sama_dua_kali(self):
        mhs = AkunMahasiswa("Dodi", 150_000)
        mhs.tarik(100_000)
        with self.assertRaises(ValueError):
            mhs.tarik(100_000)


class TestB3DecoratorOrder(unittest.TestCase):
    def test_b3_operasi_gagal_tidak_dicatat(self):
        a = AkunMerchant("Kantin C", 1_000)
        with self.assertRaises(ValueError):
            a.tarik(5_000)
        self.assertEqual(len(a.riwayat), 0)

    def test_b3_operasi_berhasil_dicatat(self):
        a = AkunMerchant("Kantin D", 0)
        a.topup(7_000)
        self.assertEqual(list(a.riwayat), [Transaksi("topup", 7_000)])


class TestB4ImmutableEvidence(unittest.TestCase):
    def test_b4_transaksi_frozen(self):
        a = AkunMerchant("Kantin E", 0)
        a.topup(9_000)
        with self.assertRaises(dataclasses.FrozenInstanceError):
            a.riwayat[0].jumlah = 1

    def test_b4_riwayat_read_only(self):
        a = AkunMerchant("Kantin F", 0)
        a.topup(9_000)
        self.assertIsInstance(a.riwayat, tuple)


class TestB5DuckTypedProcessor(unittest.TestCase):
    def test_b5_voucher_diterima(self):
        p = PemrosesPembayaran()
        self.assertEqual(p.proses(VoucherKampus("BEM-01", 20_000), 15_000), "VOUCHER BEM-01 15000")

    def test_b5_sumber_tanpa_bayar_ditolak(self):
        with self.assertRaises(TypeError):
            PemrosesPembayaran().proses(object(), 1_000)

    def test_b5_akun_tetap_diterima(self):
        p = PemrosesPembayaran()
        self.assertEqual(p.proses(AkunMerchant("Kantin G", 5_000), 2_000), "AKUN Kantin G 2000")


if __name__ == "__main__":
    unittest.main()
