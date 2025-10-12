from django.urls import path
from ..views.daftar_saham import (
    daftar_saham,
    daftar_saham_api,
    delete_emiten,
    delete_all_emiten,
)

app_name = "daftar_saham"

urlpatterns = [
    path("daftar_saham/", daftar_saham, name="daftar_saham"),
    path("daftar_saham_api/", daftar_saham_api, name="daftar_saham_api"),
    path("delete_emiten/<int:id_emiten>/", delete_emiten, name="delete_emiten"),
    path("delete_all_emiten", delete_all_emiten, name="delete_all_emiten"),
]
