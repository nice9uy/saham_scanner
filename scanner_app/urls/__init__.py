from django.urls import path, include

urlpatterns = [
    path("", include("scanner_app.urls.dashboard" , namespace="authdashboard")),
    # path("", include("accounts.urls.struktur_organisasi" , namespace="accounts")),
    # path("", include("accounts.urls.anggota" , namespace="anggota")),
]