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
import numpy as np


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
            data = yf.download(data_ticker, period="1mo", timeout=10)
            df = pd.DataFrame(data.sort_index(ascending=False))
            print(f"DATA EMITEN {data_ticker} BERHASIL DI AMBIL...")

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

            # 1. Kolom dasar
            df["Values"] = df["Close"] * df["Volume"]
            df["Pivot"] = (df["Close"] + df["High"] + df["Low"]) / 3

            # 2. CH, CL, CC → bandingkan hari ini dengan besok
            # Karena butuh hari berikutnya, hasilnya akan NaN di baris terakhir
            df["ch"] = (df["High"] - df["Close"].shift(-1)) / df["High"] * 100
            df["cl"] = (df["Low"] - df["Close"].shift(-1)) / df["Low"] * 100
            df["cc"] = (df["Close"] - df["Close"].shift(-1)) / df["Close"] * 100

            # 3. Moving Average
            df["ma5"] = df["Close"].rolling(window=5).mean()
            df["ma20"] = df["Close"].rolling(window=20).mean()
            df["ma50"] = df["Close"].rolling(window=50).mean()
            df["ma200"] = df["Close"].rolling(window=200).mean()

            ###########################################################

            close_data = df["Close"]
            # close_data = [item for sublist in close_list for item in sublist]

            values_data = df["Values"].to_list()

            tanggal = df.index.to_list()

            ch_data = df["ch"].round(2)
            cl_data = df["cl"].round(2)
            cc_data = df["cc"].round(2)

            pp_data = df["Pivot"].round(2)

            ma5_data = df["ma5"].round(2)
            ma20_data = df["ma20"].round(2)
            ma50_data = df["ma50"].round(2)
            ma200_data = df["ma200"].round(2)

            non_nan_ma5 = ma5_data.dropna().sort_index(ascending=False)
            values_ma5 = non_nan_ma5.values
            all_dates_ma5 = ma5_data.index
            new_values_ma5 = np.full(len(ma5_data), np.nan)
            new_values_ma5[: len(values_ma5)] = values_ma5

            non_nan_ma20 = ma20_data.dropna().sort_index(ascending=False)
            values_ma20 = non_nan_ma20.values
            all_dates_ma20 = ma20_data.index
            new_values_ma20 = np.full(len(ma20_data), np.nan)
            new_values_ma20[: len(values_ma20)] = values_ma20

            non_nan_ma50 = ma50_data.dropna().sort_index(ascending=False)
            values_ma50 = non_nan_ma50.values
            all_dates_ma50 = ma50_data.index
            new_values_ma50 = np.full(len(ma50_data), np.nan)
            new_values_ma50[: len(values_ma50)] = values_ma50

            non_nan_ma200 = ma200_data.dropna().sort_index(ascending=False)
            values_ma200 = non_nan_ma200.values
            all_dates_ma200 = ma200_data.index
            new_values_ma200 = np.full(len(ma200_data), np.nan)
            new_values_ma200[: len(values_ma200)] = values_ma200

            ma5_nilai = (
                pd.DataFrame(new_values_ma5, index=all_dates_ma5)
                .dropna()
                .rename(columns={0: "ma5_nilai"})
            )
            ma20_nilai = (
                pd.DataFrame(new_values_ma20, index=all_dates_ma20)
                .dropna()
                .rename(columns={0: "ma20_nilai"})
            )
            ma50_nilai = (
                pd.DataFrame(new_values_ma50, index=all_dates_ma50)
                .dropna()
                .rename(columns={0: "ma50_nilai"})
            )
            ma200_nilai = (
                pd.DataFrame(new_values_ma200, index=all_dates_ma200)
                .dropna()
                .rename(columns={0: "ma200_nilai"})
            )


            ma_combined = pd.concat(
                [close_data, ma5_nilai, ma20_nilai], axis=1
            ).sort_index(ascending=False)


        



            # print(ma_combined)

            # print(ma5)
            # print(ma20)
            # print(ma50)
            # print(ma200)
            # Konversi ke Series dengan index datetime

            # for data in range(len(tanggal)):
            #     ma5_test = (ma5_data * 0.02 )[data]

            #     print(ma5_test)
            #     close_test = close_data[data]

            #     if ma5_test > close_test:
            #         print("Buy")

            #     else:
            #         print("sell")

            time.sleep(100)

    except Exception as e:
        print(f"gagal dikarenakan {e}")

    context = {"page_title": "AMBIL DATA SAHAM"}

    return render(request, "ambil_data_saham.html", context)
