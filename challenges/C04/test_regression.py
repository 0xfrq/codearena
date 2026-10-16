import unittest

from solution import Dosen, Pegawai, Satpam


class ResetTarif(unittest.TestCase):
    def setUp(self):
        Pegawai.tarif_lembur = 50_000
        Dosen.tarif_lembur = 75_000
        if "tarif_lembur" in vars(Satpam):
            del Satpam.tarif_lembur

    tearDown = setUp


class TestNormalBehavior(ResetTarif):
    def test_normal_pajak_static_tanpa_instance(self):
        self.assertEqual(Pegawai.pajak(1_000_000), 50_000)

    def test_normal_gaji_bersih_dengan_lembur(self):
        d = Dosen("Bu Rina", 8_000_000)
        self.assertEqual(d.gaji_bersih(2), (8_000_000 + 150_000) * 95 // 100)

    def test_normal_gaji_pokok_tidak_valid(self):
        with self.assertRaises(ValueError):
            Pegawai("X", 0)


class TestB1FactoryUsesCls(ResetTarif):
    def test_b1_factory_menghasilkan_class_pemanggil(self):
        d = Dosen.dari_csv("Pak Budi; 7000000")
        self.assertIsInstance(d, Dosen)
        self.assertEqual(d.tarif_lembur, 75_000)
        self.assertEqual(d.nama, "Pak Budi")

    def test_b1_factory_subclass_lain(self):
        s = Satpam.dari_csv("Joko;4000000")
        self.assertIs(type(s), Satpam)


class TestB2ClassmethodTarif(ResetTarif):
    def test_b2_ubah_tarif_dosen_tidak_mengubah_pegawai(self):
        Dosen.ubah_tarif_lembur(80_000)
        self.assertEqual(Dosen.tarif_lembur, 80_000)
        self.assertEqual(Pegawai.tarif_lembur, 50_000)

    def test_b2_ubah_tarif_pegawai_diwarisi_satpam(self):
        Pegawai.ubah_tarif_lembur(60_000)
        self.assertEqual(Satpam.tarif_lembur, 60_000)
        self.assertEqual(Dosen.tarif_lembur, 75_000)


class TestB3Counter(ResetTarif):
    def test_b3_factory_menambah_counter_tepat_satu(self):
        sebelum = Pegawai.jumlah_pegawai
        Pegawai.dari_csv("Ani;5000000")
        self.assertEqual(Pegawai.jumlah_pegawai, sebelum + 1)


if __name__ == "__main__":
    unittest.main()
