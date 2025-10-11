from django.urls import path
from ..views.dashboard import (
    dashboard,upload_emiten,
    daftar_saham
)

app_name = "home"

urlpatterns = [
    path("dashboard/", dashboard, name="dashboard"),
    path("upload_emiten/", upload_emiten, name="upload_emiten"),
    path("daftar_saham/", daftar_saham, name="daftar_saham"),
]
