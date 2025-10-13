from django.contrib import admin
from .models.daftar_emiten import DaftarEmiten
from .models.daftar_emiten import ListPolaSaham
from .models.daftar_emiten import SettingPersen
from .models.daftar_emiten import DataSemuaSaham


class ListDaftarEmiten(admin.ModelAdmin):
    list_display = ("id", "kode_emiten", "nama_perusahaan")


class ListPolaSahan(admin.ModelAdmin):
    list_display = (
        "id",
        "kode_emiten",
        "tanggal",
        "value",
        "ch",
        "cl",
        "cc",
        "pp",
        "ma5",
        "ma20",
        "ma50",
        "ma200",
    )

class ListDataSemuaSaham(admin.ModelAdmin):
    list_display = (
        "id",
        "kode_emiten",
        "tanggal",
        "open",
        "high",
        "low",
        "close",
        "volume",
    )


class ListSettingPersen(admin.ModelAdmin):
    list_display = (
        "id",
        "nama_settings",
        "setting_persen_naik",
        "setting_persen_turun",
    )


admin.site.register(DaftarEmiten, ListDaftarEmiten)
admin.site.register(ListPolaSaham, ListPolaSahan)
admin.site.register(SettingPersen, ListSettingPersen)
admin.site.register(DataSemuaSaham, ListDataSemuaSaham)

