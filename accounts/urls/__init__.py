from django.urls import path, include

urlpatterns = [
    path("", include("accounts.urls.auth" , namespace="auth")),
    # path("", include("accounts.urls.struktur_organisasi" , namespace="accounts")),
    # path("", include("accounts.urls.anggota" , namespace="anggota")),
]