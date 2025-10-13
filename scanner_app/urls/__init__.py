from django.urls import path, include

urlpatterns = [
    path("", include("scanner_app.urls.dashboard" , namespace="authdashboard")),
    path("", include("scanner_app.urls.daftar_saham" , namespace="daftar_saham")),
    path("", include("scanner_app.urls.ambil_data_saham" , namespace="ambil_data_saham")),
]