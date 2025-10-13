from django.db import models


class DaftarEmiten(models.Model):
    id = models.AutoField(primary_key=True, unique=True)
    kode_emiten = models.CharField(max_length=7, null=True, blank=True, db_index=True)
    nama_perusahaan = models.CharField(
        max_length=50, null=True, blank=True, db_index=True
    )

    def __str__(self):
        return self.kode_emiten


class ListPolaSaham(models.Model):
    id = models.AutoField(primary_key=True, unique=True)
    kode_emiten = models.CharField(max_length=7, null=True, blank=True)
    tanggal = models.DateField()
    value = models.IntegerField()
    ch = models.FloatField()
    cl = models.FloatField()
    cc = models.FloatField()
    pp = models.IntegerField()
    ma5 = models.FloatField()
    ma20 = models.FloatField()
    ma50 = models.FloatField()
    ma200 = models.FloatField()

    class Meta:
        indexes = [
            models.Index(fields=['kode_emiten', 'tanggal']),
        ]


class DataSemuaSaham(models.Model):
    id = models.AutoField(primary_key=True, unique=True)
    kode_emiten = models.CharField(max_length=7, null=True, blank=True)
    tanggal = models.DateField()
    open = models.FloatField()
    high = models.FloatField()
    low = models.FloatField()
    close = models.FloatField()
    volume = models.IntegerField()

    class Meta:
        indexes = [
            models.Index(fields=['kode_emiten', 'tanggal']),
        ]


class SettingPersen(models.Model):
    id = models.AutoField(primary_key=True, unique=True)
    nama_settings = models.CharField()
    setting_persen_naik = models.IntegerField()
    setting_persen_turun = models.IntegerField()
    volume = models.IntegerField()
