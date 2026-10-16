"""C12 — The Missing Behavior (reference solution)

Perbaikan:
- B1: ekspor_semua memakai duck typing (cek perilaku), bukan isinstance terhadap LaporanBase,
      dan menolak objek yang tidak exportable dengan TypeError.
- B2: Exportable diberi @runtime_checkable; bisa_diekspor juga memastikan export callable
      (runtime protocol check hanya memeriksa keberadaan attribute).
- B3: StrukKantin mengimplementasikan export() sebagai method.
"""

from typing import Protocol, runtime_checkable


@runtime_checkable
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
    def __init__(self, nomor):
        self.nomor = nomor

    def export(self) -> str:
        return f"PERPUS;{self.nomor}"


class StrukKantin:
    def __init__(self, total):
        self.total = total

    def export(self) -> str:
        return f"KANTIN;{self.total}"


def bisa_diekspor(obj) -> bool:
    return isinstance(obj, Exportable) and callable(getattr(obj, "export", None))


def ekspor_semua(dokumen):
    hasil = []
    for d in dokumen:
        if not bisa_diekspor(d):
            raise TypeError(f"{type(d).__name__} tidak memiliki perilaku export()")
        hasil.append(d.export())
    return hasil
