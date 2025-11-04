# finance/jobs.py
import logging
import yfinance as yf
import pandas as pd
from datetime import time
from django.utils import timezone
# from .models import StockData

logger = logging.getLogger(__name__)

def download_and_save_stocks():
    """
    # Unduh data saham dan simpan ke database.
    # Hanya dijalankan jika:
    # - Hari kerja (Senin=0 ... Jumat=4)
    # - Waktu lokal antara 09:00–15:00
    # """

    print("ddddddddddddddddd")
    # now = timezone.localtime(timezone.now())
    # current_time = now.time()
    # current_weekday = now.weekday()  # Senin = 0, Minggu = 6

    # # Cek hari kerja
    # if current_weekday >= 5:
    #     logger.debug("📆 Hari libur (Sabtu/Minggu). Lewati.")
    #     return

    # # Cek jam kerja
    # if not (time(9, 0) <= current_time <= time(15, 0)):
    #     logger.debug(f"🕒 Di luar jam 09:00–15:00. Waktu sekarang: {current_time}")
    #     return

    # # Daftar saham
    # tickers = ["AAPL", "GOOGL", "MSFT"]
    # for ticker in tickers:
    #     try:
    #         logger.info(f"📥 Mengunduh {ticker}...")
    #         data = yf.download(ticker, period="1d", interval="1d")

    #         if data.empty:
    #             logger.warning(f"⚠️ Tidak ada data untuk {ticker}")
    #             continue

    #         latest = data.iloc[-1]
    #         close = latest.get('Close', 0)
    #         volume = latest.get('Volume', 0)

    #         # Ganti NaN dengan 0 (sesuai preferensimu)
    #         if pd.isna(close):
    #             close = 0
    #         if pd.isna(volume):
    #             volume = 0

    #         # Simpan ke database
    #         StockData.objects.create(
    #             ticker=ticker,
    #             close_price=float(close),
    #             volume=int(volume),
    #             date=now.date()
    #         )
    #         logger.info(f"✅ {ticker} disimpan: {close}")
    #     except Exception as e:
    #         logger.error(f"❌ Gagal proses {ticker}: {e}")