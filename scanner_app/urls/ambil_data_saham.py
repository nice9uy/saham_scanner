from django.urls import path
from ..views.ambil_data_saham import ambil_data_saham, ambil_data_saham_start, ambil_data_saham_stop

app_name = "ambil_data_saham"

urlpatterns = [
    path("ambil_data_saham/", ambil_data_saham, name="ambil_data_saham"),
    path(
        "ambil_data_saham/start/", ambil_data_saham_start, name="ambil_data_saham_start"
    ),
     path(
        "ambil_data_saham/stop/", ambil_data_saham_stop, name="ambil_data_saham_stop"
    ),
]
