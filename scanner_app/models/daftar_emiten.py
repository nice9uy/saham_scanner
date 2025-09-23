from django.db import models


class DaftarEmiten(models.Model):
    id = models.AutoField(primary_key=True, unique=True)
    kode_emiten = models.CharField(max_length=7, null=True, blank=True)
    nama_perusahaan = models.CharField(max_length=50, null=True, blank=True)


    def __str__(self):
        return self.kode_emiten
    
