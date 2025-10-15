from django.shortcuts import get_object_or_404, redirect, render
from django.contrib.auth.decorators import login_required

from scanner_app.models.daftar_emiten import DaftarEmiten, DataSemuaSaham, ListPolaSaham
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
    ma20 = []
    ma50 = []
    ma200 = []

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

            for index, row in df.iterrows():
                try:
                    DataSemuaSaham.objects.create(
                        kode_emiten=data_ticker,
                        tanggal=index.date(),
                        open=row["Open"],
                        high=row["High"],
                        low=row["Low"],
                        close=row["Close"],
                        volume=row["Volume"],
                    )
                except Exception as e:
                    print(f"Error, karena {e}")
                    continue

            ######### MANCARI VALUES ################
            df["Values"] = df["Close"] * df["Volume"]
            ##########################################

            ######## MANCARI CH #####################
            i = 0
            for i in range(len(df) - 1):
                if i == len(df) - 1:
                    break
                else:
                    high = df["High"].iloc[i]
                    close = df["Close"].iloc[i + 1]
                    ch_hasil = (high - close) / high * 100
                    ch.append(ch_hasil)

            ##### MENCARI CL ########################
            j = 0
            for j in range(len(df) - 1):
                if j == len(df) - 1:
                    break
                low = df["Low"].iloc[j]
                close = df["Close"].iloc[j + 1]
                cl_hasil = (low - close) / low * 100
                cl.append(cl_hasil)

            ##### MENCARI CC ########################
            k = 0
            for k in range(len(df) - 1):
                if k == len(df) - 1:
                    break
                close1 = df["Close"].iloc[k]
                close = df["Close"].iloc[k + 1]
                cc_hasil = (close1 - close) / close1 * 100
                cc.append(cc_hasil)

            ###### MENCARI PP ########################
            df["Pivot"] = (df["Close"] + df["High"] + df["Low"]) / 3

            ##### Mencari MA 5 ########################
            o = 0
            for o in range(len(df)):
                m = o + 5
                ma5_list = df["Close"].iloc[o:m].mean()

                if m > len(df):
                    break
                else:
                    ma5.append(ma5_list)

            ##### Mencari MA 20 ########################
            p = 0
            for p in range(len(df)):
                q = p + 20
                ma20_list = df["Close"].iloc[p:q].mean()

                if q > len(df):
                    break
                else:
                    ma20.append(ma20_list)

            ##### Mencari MA 50 ########################
            r = 0
            for r in range(len(df)):
                s = r + 50
                ma50_list = df["Close"].iloc[r:s].mean()

                if s > len(df):
                    break
                else:
                    ma50.append(ma50_list)

            ##### Mencari MA 200 ########################
            t = 0
            for t in range(len(df)):
                u = t + 200
                ma200_list = df["Close"].iloc[t:u].mean()

                if u > len(df):
                    break
                else:
                    ma200.append(ma200_list)

            # ch = df['Ch']
            # cl = df['Cl']
            # cc = df['Cc']

            records = []
            for _, row in df.iterrows():
                records.append(
                    ListPolaSaham(
                        kode_emiten=data_ticker,
                        tanggal=index.date(),
                        value=df["Values"],
                        ch=ch,
                        cl=cl,
                        cc=cc,
                        pp=df["Pivot"],
                        ma5=ma5,
                        ma20=ma20,
                        ma50=ma50,
                        ma200=ma200,
                    )
                )

            ListPolaSaham.objects.bulk_create(records)

            time.sleep(100)

    except Exception as e:
        print(f"gagal dikarenakan {e}")

    context = {"page_title": "AMBIL DATA SAHAM"}

    return render(request, "ambil_data_saham.html", context)
