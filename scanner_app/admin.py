from django.contrib import admin
from .models.daftar_emiten import DaftarEmiten


class ListDaftarEmiten(admin.ModelAdmin):
    list_display = ("id", "kode_emiten" , "nama_perusahaan" )


admin.site.register(DaftarEmiten, ListDaftarEmiten)
