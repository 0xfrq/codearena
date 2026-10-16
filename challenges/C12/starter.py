"""C12 — The Missing Behavior

Modul ekspor dokumen kampus. Dokumen dari berbagai unit (akademik, perpustakaan, kantin)
TIDAK berasal dari satu hierarki class. Yang penting adalah PERILAKU: memiliki method export() -> str.

Kontrak struktural (structural typing) dituliskan dengan typing.Protocol.
"""

from typing import Protocol


class Exportable(Protocol):
    def export(self) -> str:
        ...


class LaporanBase:
    def export(self) -> str:
        raise NotImplementedError


class LaporanNilai(LaporanBase):
    def __init__(self, nim, nilai):
        self.nim = nim
        self.nilai = nilai

    def export(self) -> str:
        return f"NILAI;{self.nim};{self.nilai}"


class KartuPerpus:
    """Unit perpustakaan: bukan turunan LaporanBase, tetapi memiliki perilaku export()."""

    def __init__(self, nomor):
        self.nomor = nomor

    def export(self) -> str:
        return f"PERPUS;{self.nomor}"


class StrukKantin:
    """Unit kantin: struk harus dapat diekspor seperti dokumen lain."""

    def __init__(self, total):
        self.total = total
        self.export = f"KANTIN;{total}"


def bisa_diekspor(obj) -> bool:
    """True jika obj memenuhi kontrak Exportable (punya export() yang dapat dipanggil)."""
    return isinstance(obj, Exportable)


def ekspor_semua(dokumen):
    """Mengekspor semua dokumen dari iterable apa pun.
    Objek tanpa perilaku export() ditolak dengan TypeError."""
    if not isinstance(dokumen, list):
        raise TypeError("dokumen harus berupa list")
    hasil = []
    for d in dokumen:
        if isinstance(d, LaporanBase):
            hasil.append(d.export())
    return hasil
