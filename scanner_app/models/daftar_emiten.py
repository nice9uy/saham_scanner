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
    ch = models.FloatField(null=True, blank=True)
    cl = models.FloatField(null=True, blank=True)
    cc = models.FloatField(null=True, blank=True)
    pp = models.IntegerField(null=True, blank=True)
    ma5 = models.FloatField(null=True, blank=True)
    ma20 = models.FloatField(null=True, blank=True)
    ma50 = models.FloatField(null=True, blank=True)
    ma200 = models.FloatField(null=True, blank=True)

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
