from django.shortcuts import get_object_or_404, redirect, render
from django.contrib.auth.decorators import login_required

from scanner_app.models.daftar_emiten import DaftarEmiten, DataSemuaSaham
import yfinance as yf
import time
from django.db import transaction
from openpyxl import load_workbook


# import json
# from django.contrib import messages
import pandas as pd

# from ..models import DaftarEmiten
# from django.core.paginator import Paginator
# from django.http import JsonResponse
# import logging
# from django.db.models import Q
import os

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
            df = pd.DataFrame(data.sort_index(ascending=False))

            df["Ticker"] = data_ticker
            cols = ["Close", "High", "Low", "Open"]

            df[cols] = df[cols].round(2)

            output_file = "data_saham.xlsx"

            df.to_excel(output_file, sheet_name="DataSaham")

            wb = load_workbook(output_file)
            ws = wb.active
            ws.delete_rows(2, 2)
            ws["A1"] = "Date"
            wb.save(output_file)

            data_saham = pd.read_excel(output_file)

            for index, row in data_saham.iterrows():
                try:
                    DataSemuaSaham.objects.create(
                        kode_emiten=row["Ticker"],
                        tanggal=pd.to_datetime(row["Date"]).date(),  # Konversi ke date
                        open=row["Open"],
                        high=row["High"],
                        low=row["Low"],
                        close=row["Close"],
                        volume=row["Volume"],
                    )
                except Exception as e:
                    print(f"Error, karena {e}")
                    continue
            wb.close() 
            if os.path.exists(output_file):
                os.remove(output_file)
            time.sleep(10)

    except Exception as e:
        print(f"gagal dikarenakan {e}")

    context = {"page_title": "AMBIL DATA SAHAM"}

    return render(request, "ambil_data_saham.html", context)
