from django.shortcuts import redirect, render
from django.contrib.auth.decorators import login_required
import yfinance as yf
import json
from django.contrib import messages
import pandas as pd
from ..models import DaftarEmiten


@login_required(login_url="/accounts/login/")
def dashboard(request):
    total_emiten = DaftarEmiten.objects.all().count()




    # print(json.dumps(xdata, indent=4, ensure_ascii=False))

    # tickers = ["BBCA.JK"]


    # tickers_jk = [t + ".JK" for t in tickers]

    # data = yf.download(tickers, period="1y", group_by="ticker")

    # data.to_csv


    # for ticker in tickers_jk:
    #     if ticker in data.columns:
    #         price = data[ticker]["Close"].iloc[-1]
    #         print(f"{ticker}: Rp {price:,.0f}")
    #     else:
    #         print(f"{ticker}: Data tidak tersedia")

    context = {"page_title": "DASHBOARD", "total_emiten": total_emiten}

    return render(request, "dashboard.html", context)


@login_required(login_url="/accounts/login/")
def upload_emiten(request):
    if request.method == "POST":
        uploaded_file = request.FILES.get("upload_file")

        try:
            if not uploaded_file:
                messages.error(request, "Tidak ada file yang dipilih !!")

            if not uploaded_file.name.endswith(".xlsx"):
                messages.error(request, "Hanya file .xlsx yang diizinkan.")

            try:
                DaftarEmiten.objects.all().delete()

                df = pd.read_excel(uploaded_file)
                data = df[["Kode", "Nama Perusahaan"]].to_dict("records")

                emiten_objects = [
                    DaftarEmiten(
                        kode_emiten=item["Kode"] + ".JK",
                        nama_perusahaan=item["Nama Perusahaan"],
                    )
                    for item in data
                ]

                DaftarEmiten.objects.bulk_create(emiten_objects, ignore_conflicts=True)

            except Exception as e:
                messages.error(request, f"Gagal memproses file: {str(e)}")

            messages.success(request, f"File {uploaded_file.name} berhasil diterima!")

        except Exception:
            messages.error(request, "Tidak ada file yang diupload!!")

    return redirect("home:dashboard")


def daftar_saham(request):

    return render(request, "tabel/daftar_emiten.html")