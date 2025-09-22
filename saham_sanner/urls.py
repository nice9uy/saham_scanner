"""
URL configuration for saham_sanner project.

The `urlpatterns` list routes URLs to views. For more information please see:
    https://docs.djangoproject.com/en/5.2/topics/http/urls/
Examples:
Function views
    1. Add an import:  from my_app import views
    2. Add a URL to urlpatterns:  path('', views.home, name='home')
Class-based views
    1. Add an import:  from other_app.views import Home
    2. Add a URL to urlpatterns:  path('', Home.as_view(), name='home')
Including another URLconf
    1. Import the include() function: from django.urls import include, path
    2. Add a URL to urlpatterns:  path('blog/', include('blog.urls'))
"""
from django.contrib import admin
from django.urls import path, include
from django.views.generic import RedirectView

urlpatterns = [
    path(
        "784ay7ighak43ghihayilgya34itaklu3t4aou28oqpy0tuno2PTQOPTUNJOALHJARLIUGNKAHZHZSHSZZASX/",
        admin.site.urls,
        name="admin",
    ),
    path("", RedirectView.as_view(url="accounts/login/")),
    path("accounts/", include("accounts.urls")),
    path("accounts/", include("django.contrib.auth.urls")),
    path("scanner/", include("scanner_app.urls")),
]