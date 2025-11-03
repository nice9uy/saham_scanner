from django.shortcuts import get_object_or_404, redirect, render
from django.contrib.auth.decorators import login_required

from scanner_app.models.daftar_emiten import DaftarEmiten, DataSemuaSaham, ListPolaSaham
import yfinance as yf
import time
from django.db import transaction
from openpyxl import load_workbook
from datetime import datetime
import itertools


# import json
# from django.contrib import messages
import pandas as pd

# from ..models import DaftarEmiten
# from django.core.paginator import Paginator
# from django.http import JsonResponse
# import logging
# from django.db.models import Q
import numpy as np


@login_required(login_url="/accounts/login/")
def ambil_data_saham(request):
    context = {"page_title": "AMBIL DATA SAHAM"}

    return render(request, "ambil_data_saham.html", context)


@login_required(login_url="/accounts/login/")
def ambil_data_saham_stop(request):
    if request.method == "POST":
        stop_semua_saham = DataSemuaSaham.objects.all()
        stop_semua_saham.delete()

        stop_list_pola = ListPolaSaham.objects.all()
        stop_list_pola.delete()

    return redirect("ambil_data_saham:ambil_data_saham")


@login_required(login_url="/accounts/login/")
def ambil_data_saham_start(request):
    counter = 0
    
    all_tickers = DaftarEmiten.objects.values_list("kode_emiten", flat=True).iterator(
        chunk_size=2000
    )

    try:
        for data_ticker in all_tickers:
            data = yf.download(data_ticker, period="1y", timeout=10)
            df = pd.DataFrame(data.sort_index(ascending=False))
            print(f"DATA EMITEN {data_ticker} BERHASIL DI AMBIL...")
            counter += 1

            df["Ticker"] = data_ticker
            cols = ["Close", "High", "Low", "Open"]

            df[cols] = df[cols].round(2)

            for index, row in df.iloc[::-1].iterrows():
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

            ####################################################################
            # 1. Kolom dasar
            df["Values"] = df["Close"] * df["Volume"]
            df["Pivot"] = (df["Close"] + df["High"] + df["Low"]) / 3
            ###########################################################
            close_list = df["Close"]
            #########################################################

            ####################################################################
            # 2. CH, CL, CC → bandingkan hari ini dengan besok
            # Karena butuh hari berikutnya, hasilnya akan NaN di baris terakhir
            df["ch"] = (df["High"] - df["Close"].shift(-1)) / df["High"] * 100
            df["cl"] = (df["Low"] - df["Close"].shift(-1)) / df["Low"] * 100
            df["cc"] = (df["Close"] - df["Close"].shift(-1)) / df["Close"] * 100
            #####################################################################
            # 3. Moving Average
            df["ma5"] = df["Close"].rolling(window=5).mean()
            df["ma20"] = df["Close"].rolling(window=20).mean()
            df["ma50"] = df["Close"].rolling(window=50).mean()
            df["ma200"] = df["Close"].rolling(window=200).mean()

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

            #########################
            list_close = [x[0] for x in close_list.values.tolist()]
            ##########################
            list_ma5 = [x[0] for x in ma5_nilai.values.tolist()]
            ##########################
            list_ma20 = [x[0] for x in ma20_nilai.values.tolist()]
            ##########################
            list_ma50 = [x[0] for x in ma50_nilai.values.tolist()]
            #########################
            list_ma200 = [x[0] for x in ma200_nilai.values.tolist()]

            ### CARI SIGNAL MA5 #######################################################
            length_ma5 = len(close_list) - len(list_ma5)
            close_untuk_ma5 = list_close[:-length_ma5]
            cari_ma5 = pd.DataFrame({"Close": close_untuk_ma5, "MA5_data": list_ma5})
            cari_ma5["MA5_signal"] = (
                (cari_ma5["MA5_data"] - cari_ma5["Close"]) / 100
            ).round(2)
            cari_ma5 = cari_ma5.rename(columns={"MA5_signal": "MA5"})
            ### CARI SIGNAL MA20 #####################################################
            length_ma20 = len(close_list) - len(list_ma20)
            close_untuk_ma20 = list_close[:-length_ma20]
            cari_ma20 = pd.DataFrame(
                {
                    "Close": close_untuk_ma20,
                    "MA20_data": list_ma20,
                }
            )
            cari_ma20["MA20_signal"] = (
                (cari_ma20["MA20_data"] - cari_ma20["Close"]) / 100
            ).round(2)
            cari_ma20 = cari_ma20.rename(columns={"MA20_signal": "MA20"})
            ### CARI SIGNAL MA50 #####################################################
            length_ma50 = len(close_list) - len(list_ma50)
            close_untuk_ma50 = list_close[:-length_ma50]
            cari_ma50 = pd.DataFrame(
                {
                    "Close": close_untuk_ma50,
                    "MA50_data": list_ma50,
                }
            )
            cari_ma50["MA50_signal"] = (
                (cari_ma50["MA50_data"] - cari_ma50["Close"]) / 100
            ).round(2)
            cari_ma50 = cari_ma50.rename(columns={"MA50_signal": "MA50"})
            ### CARI SIGNAL MA200 #####################################################
            length_ma200 = len(close_list) - len(list_ma200)
            close_untuk_ma200 = list_close[:-length_ma200]
            cari_ma200 = pd.DataFrame(
                {
                    "Close": close_untuk_ma200,
                    "MA200_data": list_ma200,
                }
            )
            cari_ma200["MA200_signal"] = (
                (cari_ma200["MA200_data"] - cari_ma200["Close"]) / 100
            ).round(2)
            cari_ma200 = cari_ma200.rename(columns={"MA200_signal": "MA200"})
            ##########################################################################
            values = pd.DataFrame(df["Values"].values, columns=["Values"])
            tanggal = pd.DataFrame(df.index.values, columns=["Tanggal"])
            ch_data = pd.DataFrame(df["ch"].round(2).values, columns=["ch"])
            cl_data = pd.DataFrame(df["cl"].round(2).values, columns=["cl"])
            cc_data = pd.DataFrame(df["cc"].round(2).values, columns=["cc"])
            pp = pd.DataFrame(df["Pivot"].round(2).values, columns=["pp"])

            ################################################

            gabungan = pd.concat(
                {
                    "TANGGAL": tanggal["Tanggal"],
                    "Values": values["Values"],
                    "CH": ch_data["ch"],
                    "CL": cl_data["cl"],
                    "CC": cc_data["cc"],
                    "PP": pp["pp"],
                    "MA5": cari_ma5["MA5"],
                    "MA20": cari_ma20["MA20"],
                    "MA50": cari_ma50["MA50"],
                    "MA200": cari_ma200["MA200"],
                },
                axis=1,
            ).fillna(0)

            for _, row in gabungan.iloc[::-1].iterrows():
                try:
                    ListPolaSaham.objects.create(
                        kode_emiten=data_ticker,
                        tanggal=row["TANGGAL"],
                        value=row["Values"],
                        ch=row["CH"],
                        cl=row["CL"],
                        cc=row["CC"],
                        pp=row["PP"],
                        ma5=row["MA5"],
                        ma20=row["MA20"],
                        ma50=row["MA50"],
                        ma200=row["MA200"],
                    )
                except Exception as e:
                    print(f"Error, karena {e}")
                    continue

            time.sleep(10)

    except Exception as e:
        print(f"gagal dikarenakan {e}")

    print(f"DATA SAHAM BERHASIL ADA : {counter} ")

    return redirect("ambil_data_saham:ambil_data_saham_start")
