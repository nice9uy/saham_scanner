# finance/jobs.py
import logging
import yfinance as yf
import pandas as pd
from datetime import time
from django.utils import timezone
from scanner_app.models.daftar_emiten import DaftarEmiten, DataSemuaSaham
import holidays


logger = logging.getLogger(__name__)

def download_and_save_stocks():
    """
    # Unduh data saham dan simpan ke database.
    # Hanya dijalankan jika:
    # - Hari kerja (Senin=0 ... Jumat=4)
    # - Waktu lokal antara 09:00–15:00
    # """

    

    # now = timezone.localtime(timezone.now())
    # current_time = now.time()

    # id_holidays = holidays.Indonesia()

    # now = timezone.localtime(timezone.now())
    # today = now.date()
    # weekday = now.weekday()

    # if weekday >= 5 or today in id_holidays:
    #     logger.debug(f"📅 Hari libur: {id_holidays.get(today) or 'Akhir pekan'}. Lewati.")
    #     return

    # # Cek jam kerja
    # if not (time(9, 0) <= current_time <= time(15, 0)):
    #     logger.debug(f"🕒 Di luar jam 09:00–15:00. Waktu sekarang: {current_time}")
    #     return

    # all_tickers = list(DaftarEmiten.objects.values_list("kode_emiten", flat=True))
    # total_ticker_diminta = len(all_tickers)

    # if total_ticker_diminta == 0:
    #     df_data = pd.DataFrame()
    #     ticker_gagal = 0
    # else:
    #     n_batches = 5
    #     batch_size = (total_ticker_diminta + n_batches - 1) // n_batches
    #     ticker_batches = [
    #         all_tickers[i : i + batch_size]
    #         for i in range(0, total_ticker_diminta, batch_size)
    #     ]

    #     batch_results = []

    #     for i, batch in enumerate(ticker_batches):
    #         try:
    #             batch_data = yf.download(batch, period="1d", timeout=10, threads=True)
    #             if not batch_data.empty:
    #                 if "Adj Close" in batch_data.columns.get_level_values(0):
    #                     batch_data = batch_data.drop(columns="Adj Close")
    #                 batch_df = (
    #                     batch_data.sort_index(ascending=False)
    #                     .stack(level=1, future_stack=False)
    #                     .reset_index()
    #                 )
    #                 batch_results.append(batch_df)
    #         except Exception as e:
    #             print(f"Error in batch {i + 1}: {e}")
    #         time.sleep(1)

    #     df_data = (
    #         pd.concat(batch_results, ignore_index=True)
    #         if batch_results
    #         else pd.DataFrame()
    #     )

    #     for index, row in df_data.iterrows():
    #         try:
    #             DataSemuaSaham.objects.create(
    #                 kode_emiten=row["Ticker"],
    #                 tanggal=row["Date"],
    #                 open=row["Open"],
    #                 high=row["High"],
    #                 low=row["Low"],
    #                 close=row["Close"],
    #                 volume=row["Volume"],
    #             )
    #         except Exception as e:
    #             print(f"Error, karena {e}")
    #             continue

    #     # Hitung ticker yang berhasil (unik)
    #     if not df_data.empty and "Ticker" in df_data.columns:
    #         ticker_berhasil_list = df_data["Ticker"].dropna().unique().tolist()
    #         ticker_berhasil = len(ticker_berhasil_list)
    #     else:
    #         ticker_berhasil_list = []
    #         ticker_berhasil = 0

    #     ticker_gagal = total_ticker_diminta - ticker_berhasil

    # for data_ticker in ticker_berhasil_list:
    #     df_data_saham = DataSemuaSaham.objects.filter(kode_emiten=data_ticker).values(
    #         "kode_emiten", "tanggal", "open", "high", "low", "close", "volume"
    #     )
    #     xdata_saham = (
    #         pd.DataFrame(df_data_saham)
    #         .sort_index(ascending=False)
    #         .reset_index()
    #         .drop(columns="index")
    #     )
    #     ###### CLOSE    #########################################
    #     df_close = xdata_saham.iloc[:1]
    #     df_close = pd.DataFrame(df_close["close"])
    #     ######  VALUE   #########################################3
    #     df_value = xdata_saham.iloc[:1]
    #     df_value = pd.DataFrame(
    #         df_value["close"] * df_value["volume"], columns=["Values"]
    #     )
    #     ######  PIVOT  ###########################################3
    #     df_pivot = xdata_saham.iloc[:1]
    #     df_pivot = pd.DataFrame(
    #         (df_pivot["close"] + df_pivot["high"] + df_pivot["low"]) / 3,
    #         columns=["Pivot"],
    #     )
    #     ###### CH    ################################################
    #     df_ch = xdata_saham.iloc[:2]
    #     df_ch = (
    #         pd.DataFrame(
    #             ((df_ch["high"] - df_ch["close"].shift(-1)) / df_ch["high"] * 100),
    #             columns=["CH"],
    #         )
    #         .round(2)
    #         .dropna()
    #     )
    #     ##### CL     ##################################################
    #     df_cl = xdata_saham.iloc[:2]
    #     df_cl = (
    #         pd.DataFrame(
    #             ((df_cl["low"] - df_cl["close"].shift(-1)) / df_cl["low"] * 100),
    #             columns=["CL"],
    #         )
    #         .round(2)
    #         .dropna()
    #     )
    #     ##### CC  #################################################
    #     df_cc = xdata_saham.iloc[:2]
    #     df_cc = (
    #         pd.DataFrame(
    #             ((df_cc["close"] - df_cc["close"].shift(-1)) / df_cc["close"] * 100),
    #             columns=["CC"],
    #         )
    #         .round(2)
    #         .dropna()
    #     )

    #     ###### MA5   ##########################################
    #     df_ma5 = xdata_saham.iloc[:5]
    #     df_ma5 = (df_ma5["close"].sum()) / 5
    #     df_ma5 = (df_ma5 - df_close) / 100
 
    #     ##### MA20  ##########################################
    #     df_ma20 = xdata_saham.iloc[:20]
    #     df_ma20 = (df_ma20["close"].sum()) / 20
    #     df_ma20 = (df_ma20 - df_close) / 100

    #     #### MA50  ##########################################
    #     df_ma50 = xdata_saham.iloc[:50]
    #     df_ma50 = (df_ma50["close"].sum()) / 50
    #     df_ma50 = (df_ma50 - df_close) / 100

    #     #### MA200  #########################################
    #     df_ma200 = xdata_saham.iloc[:200]
    #     df_ma200 = (df_ma200["close"].sum()) / 200
    #     df_ma200 = (df_ma200 - df_close) / 100

    #     ######################################################
    #     break

    # print("#" * 30)
    # print(f"Total Ticker     : {total_ticker_diminta}")
    # print(f"Ticker berhasil  : {ticker_berhasil}")
    # print(f"Ticker gagal     : {ticker_gagal}")
    # print("#" * 30)