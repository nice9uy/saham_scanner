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
    ch = []
    cl = []
    cc = []
    ma5 = []

    all_tickers = DaftarEmiten.objects.values_list("kode_emiten", flat=True).iterator(
        chunk_size=2000
    )

    for ticker in all_tickers:
        tickers.append(ticker)

    try:
        for data_ticker in tickers:
            data = yf.download(data_ticker, period="1mo", timeout=10)
            df = pd.DataFrame(data.sort_index(ascending=False))

            df["Ticker"] = data_ticker
            cols = ["Close", "High", "Low", "Open"]

            df[cols] = df[cols].round(2)

            # for index, row in df.iterrows():
            #     try:
            #         DataSemuaSaham.objects.create(
            #             kode_emiten=data_ticker,
            #             tanggal=index.date(),
            #             open=row["Open"],
            #             high=row["High"],
            #             low=row["Low"],
            #             close=row["Close"],
            #             volume=row["Volume"],
            #         )
            #     except Exception as e:
            #         print(f"Error, karena {e}")
            #         continue

            ######### MANCARI VALUES ################
            df["Values"] = df["Close"] * df["Volume"]
            ##########################################

            ######## MANCARI CH #####################
            i = 0
            for i in range(len(df) - 1):
                high = df["High"].iloc[i]
                close = df["Close"].iloc[i + 1]
                ch_hasil = (high - close) / high * 100
                ch.append(ch_hasil)

            ##### MENCARI CL ########################
            j = 0
            for j in range(len(df) - 1):
                low = df["Low"].iloc[j]
                close = df["Close"].iloc[j + 1]
                cl_hasil = (low - close) / low * 100
                cl.append(cl_hasil)

            ##### MENCARI CC ########################
            k = 0
            for k in range(len(df) - 1):
                close1 = df["Close"].iloc[k]
                close = df["Close"].iloc[k + 1]
                cc_hasil = (close1 - close) / close1 * 100
                cc.append(cc_hasil)

            ###### MENCARI PP ########################
            df['Pivot'] = (df["Close"] + df["High"] + df["Low"]) / 3

            ##### Mencari MA 5 ########################
            o = 0
            for o in range(len(df)):
                m = o + 5
                ma5_list = df['Close'].iloc[o:m].mean()  # atau .sum() / 5
                ma5.append(ma5_list)

          

            time.sleep(10)

    except Exception as e:
        print(f"gagal dikarenakan {e}")

    context = {"page_title": "AMBIL DATA SAHAM"}

    return render(request, "ambil_data_saham.html", context)
