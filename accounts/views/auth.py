from django.shortcuts import render, redirect
from django.utils.html import strip_tags
# from datetime import datetime
from django.contrib.auth import authenticate, login, logout
from django.contrib import messages


def clean_input(input_str):
    cleaned = strip_tags(input_str)
    return cleaned.strip()


def login_view(request):
    context = {"page_title": "login"}


    if request.method == "POST":
        # Cek apakah 'username' dan 'password' ada di POST
        username = request.POST.get('username')
        password = request.POST.get('password')

        
        if not username or not password:
            messages.error(request, "Username dan password wajib diisi.")
            return render(request, 'auth.html')

        user = authenticate(request, username=username, password=password)
        if user is not None:
            login(request, user)
            return redirect('home:dashboard')  # Ganti 'home' dengan nama URL tujuan
        else:
            messages.error(request, "Username atau password salah.")

    return render(request, "auth.html", context)


def logout_view(request):
    logout(request)
    return redirect("auth:login")


def custom_404_view(request):
    return render(request, "404.html", status=404)