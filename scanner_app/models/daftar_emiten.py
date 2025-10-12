from django.db import models


class DaftarEmiten(models.Model):
    id = models.AutoField(primary_key=True, unique=True)
    kode_emiten = models.CharField(max_length=7, null=True, blank=True , db_index=True)
    nama_perusahaan = models.CharField(max_length=50, null=True, blank=True , db_index=True )

    def __str__(self):
        return self.kode_emiten


class ListPolaSaham(models.Model):
    id = models.AutoField(primary_key=True, unique=True)
    kode_emiten = models.CharField(max_length=7, null=True, blank=True)
    tanggal = models.DateField()
    close = models.IntegerField()
    open = models.IntegerField()
    high = models.IntegerField()
    low = models.IntegerField()
    volume = models.IntegerField()
    value = models.IntegerField()
    ch = models.IntegerField()
    cl = models.IntegerField()
    cc = models.IntegerField()
    pp = models.IntegerField()
    ma5 = models.IntegerField()
    ma20 = models.IntegerField()
    ma50 = models.IntegerField()
    ma200 = models.IntegerField()


class SettingPersen(models.Model):
    id = models.AutoField(primary_key=True, unique=True)
    nama_settings = models.CharField()
    setting_persen_naik = models.IntegerField()
    setting_persen_turun = models.IntegerField()
