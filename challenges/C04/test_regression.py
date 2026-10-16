import unittest

from solution import Dosen, Pegawai, Satpam


class TarifTerjaga(unittest.TestCase):
    """Mengembalikan tarif lembur setiap class setelah test yang mengubahnya."""

    def setUp(self):
        self._tarif = {c: vars(c).get("tarif_lembur") for c in (Pegawai, Dosen, Satpam)}

    def tearDown(self):
        for c, nilai in self._tarif.items():
            if nilai is None:
                if "tarif_lembur" in vars(c):
                    delattr(c, "tarif_lembur")
            else:
                c.tarif_lembur = nilai


class TestNormalBehavior(TarifTerjaga):
    def test_normal_gaji_bersih_pegawai(self):
        p = Pegawai("Ani", 4_000_000)
        # kotor 4.100.000, pajak 5% = 205.000
        self.assertEqual(p.gaji_bersih(2), 3_895_000)

    def test_normal_pajak_utilitas_statis(self):
        self.assertEqual(Pegawai.pajak(1_000_019), 50_000)

    def test_normal_factory_membaca_nama_dan_gaji(self):
        p = Pegawai.dari_csv(" Budi ;3000000")
        self.assertEqual((p.nama, p.gaji_pokok), ("Budi", 3_000_000))


class TestB1FactoryCls(TarifTerjaga):
    def test_b1_dosen_dari_csv_menghasilkan_dosen(self):
        d = Dosen.dari_csv("Dr. Citra;8000000")
        self.assertIsInstance(d, Dosen)
        self.assertEqual(d.tarif_lembur, 75_000)

    def test_b1_satpam_dari_csv_menghasilkan_satpam(self):
        self.assertIs(type(Satpam.dari_csv("Dedi;3000000")), Satpam)


class TestB2TarifPerClass(TarifTerjaga):
    def test_b2_ubah_tarif_dosen_tidak_mengubah_pegawai(self):
        Dosen.ubah_tarif_lembur(90_000)
        self.assertEqual(Dosen.tarif_lembur, 90_000)
        self.assertEqual(Pegawai.tarif_lembur, 50_000)
        self.assertEqual(Satpam.tarif_lembur, 50_000)

    def test_b2_ubah_tarif_pegawai_diwarisi_turunan_tanpa_override(self):
        Pegawai.ubah_tarif_lembur(60_000)
        self.assertEqual(Satpam.tarif_lembur, 60_000)
        self.assertEqual(Dosen.tarif_lembur, 75_000)


class TestB3CounterGanda(TarifTerjaga):
    def test_b3_factory_menambah_counter_tepat_satu(self):
        sebelum = Pegawai.jumlah_pegawai
        Dosen.dari_csv("Eka;5000000")
        self.assertEqual(Pegawai.jumlah_pegawai, sebelum + 1)


class TestB4CounterGagal(TarifTerjaga):
    def test_b4_object_gagal_dibuat_tidak_dihitung(self):
        sebelum = Pegawai.jumlah_pegawai
        with self.assertRaises(ValueError):
            Pegawai("Fajar", 0)
        with self.assertRaises(ValueError):
            Pegawai.dari_csv("Gita;-100")
        self.assertEqual(Pegawai.jumlah_pegawai, sebelum)


class TestB5PajakPolimorfik(TarifTerjaga):
    def test_b5_satpam_memakai_pajak_2_persen(self):
        s = Satpam("Hadi", 4_000_000)
        self.assertEqual(s.gaji_bersih(), 4_000_000 - 80_000)

    def test_b5_dosen_tetap_pajak_5_persen(self):
        d = Dosen("Indah", 6_000_000)
        self.assertEqual(d.gaji_bersih(), 6_000_000 - 300_000)


if __name__ == "__main__":
    unittest.main()
