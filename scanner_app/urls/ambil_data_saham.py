from django.urls import path
from ..views.ambil_data_saham import (
  ambil_data_saham
)

app_name = "ambil_data_saham"

urlpatterns = [
    path("ambil_data_saham/", ambil_data_saham, name="ambil_data_saham"),
  
]
