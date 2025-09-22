from django.shortcuts import render, redirect
from django.utils.html import strip_tags
from datetime import datetime
from django.contrib.auth import authenticate, login, logout
from django.contrib import messages


def clean_input(input_str):
    cleaned = strip_tags(input_str)
    return cleaned.strip()


def login_view(request):
    tahun_sekarang = datetime.now().year

    get_copyright = copyright

    context = {
        "page_title": "LOGIN",
        "tahun": tahun_sekarang,
        "get_copyright": get_copyright,
    }

    if request.method == "POST":
        username_login = clean_input(request.POST.get("nip", ""))
        password_login = clean_input(request.POST.get("password", ""))

        user = authenticate(
            request=request, nip=username_login, password=password_login
        )

        if user is not None:
            login(request, user)
            if password_login == "baranahan123":
                messages.warning(request, "Demi keamanan segera ganti Password!!!")
                return redirect("dashboard")
            return redirect("dashboard")
        else:
            messages.error(request, "Periksa kembali NIP dan Password Anda benar!!!")
            return redirect("login")

    return render(request, "auth.html", context)


def logout_view(request):
    logout(request)
    return redirect("auth:login")


def custom_404_view(request):
    return render(request, "404.html", status=404)