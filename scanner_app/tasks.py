import time
import yfinance as yf
import pandas as pd
from celery import shared_task
from asgiref.sync import async_to_sync
from channels.layers import get_channel_layer

# from django.db import IntegrityError
from .models.daftar_emiten import DaftarEmiten, DataSemuaSaham
import gc


@shared_task(bind=True)
def ambil_data_saham_task(self, task_id):
    def send_progress(current, total, message):
        progress_pct = int((current / total) * 100) if total > 0 else 0
        channel_layer = get_channel_layer()
        async_to_sync(channel_layer.group_send)(
            f"progress_{task_id}",
            {
                "type": "progress.update",
                "current": current,
                "total": total,
                "progress": progress_pct,
                "message": message,
            },
        )

    try:
        all_tickers = list(DaftarEmiten.objects.values_list("kode_emiten", flat=True))
        total = len(all_tickers)
        send_progress(0, total, "Mulai mengambil data saham...")

        for idx, ticker in enumerate(all_tickers, 1):
            data = None
            df = None
            message = f"📥 Mengambil data {ticker} ({idx}/{total})"
            send_progress(idx - 1, total, message)

            try:
                data = yf.download(ticker, period="2y", timeout=10)
                if data.empty:
                    send_progress(idx, total, f"⚠️ {ticker}: Data kosong, dilewati.")
                    time.sleep(2)
                    continue

                df = pd.DataFrame(data.sort_index(ascending=False))
                df["Ticker"] = ticker
                cols = ["Close", "High", "Low", "Open", "Volume"]
                df[cols] = df[cols].round(2)

                for dt_index, row in df.iloc[::-1].iterrows():
                    try:
                        DataSemuaSaham.objects.update_or_create(
                            kode_emiten=ticker,
                            tanggal=dt_index.date(),
                            defaults={
                                "open": float(row["Open"].item()),
                                "high": float(row["High"].item()),
                                "low": float(row["Low"].item()),
                                "close": float(row["Close"].item()),
                                "volume": int(row["Volume"].item()),  # ← AMAN
                            },
                        )
                    except Exception as e:
                        send_progress(idx, total, f"Error di {ticker}: {e}")

                del data, df  # hapus referensi
                gc.collect()  # f

                time.sleep(5)  # rate limit

            except Exception as e:
                send_progress(idx, total, f"❌ {ticker}: Gagal - {str(e)[:50]}")
                del data, df  # hapus referensi
                gc.collect()
                time.sleep(2)
                continue

        send_progress(
            total, total, f"✅ Selesai! Data {total} saham berhasil disimpan."
        )
        return {"status": "success", "total": total}

    except Exception as e:
        send_progress(0, 1, f"💥 Error: {str(e)}")
        raise
