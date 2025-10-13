from django.shortcuts import get_object_or_404, redirect, render
from django.contrib.auth.decorators import login_required

from scanner_app.models.daftar_emiten import DaftarEmiten
import yfinance as yf
import time
# import json
# from django.contrib import messages
# import pandas as pd
# from ..models import DaftarEmiten
# from django.core.paginator import Paginator
# from django.http import JsonResponse
# import logging
# from django.db.models import Q


@login_required(login_url="/accounts/login/")
def ambil_data_saham(request):

    tickers = []

    all_tickers = DaftarEmiten.objects.values_list("kode_emiten", flat=True).iterator(
        chunk_size=2000
    )

    for ticker in all_tickers:
        tickers.append(ticker)

    try:
        for data_ticker in tickers:
            data = yf.download(data_ticker, period="1y", timeout=10)
            hasil_tickers = data.sort_index(ascending=False)

            print(hasil_tickers)
            time.sleep(100)

    except Exception as e:
        print(f"gagal dikarenakan {e}")

    context = {"page_title": "AMBIL DATA SAHAM"}

    return render(request, "ambil_data_saham.html", context)
