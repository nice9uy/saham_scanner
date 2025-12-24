from django.shortcuts import  redirect, render
from django.contrib.auth.decorators import login_required

from scanner_app.models.daftar_emiten import DaftarEmiten, DataSemuaSaham, ListPolaSaham
import yfinance as yf
import time
# from django.db import transaction
# from openpyxl import load_workbook
# from datetime import datetime
# import itertools
# from datetime import date
import gc
import pandas as pd
# import pandas_ta as ta
import talib
import numpy as np
from numba import jit
from ..tasks import ambil_data_saham_task
from django.http import JsonResponse
from django.views.decorators.http import require_POST
from django.views.decorators.csrf import csrf_exempt
import uuid



CONFIG = {
    'PERIOD': '2y',               # Cukup untuk indikator jangka panjang
    'MIN_LIKUIDITAS': 5_000_000_000,  # 1 miliar Rupiah
    'BACKTEST_YEARS': 1,          # Hanya backtest 1 tahun terakhir
    'RISK_MGMT': {
        'SL_ATR_MULT': 5.0,
        'TP_ATR_MULT': 10
    }
}

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
@csrf_exempt
@require_POST
def ambil_data_saham_start(request):

    task_id = str(uuid.uuid4())  
    ambil_data_saham_task.delay(task_id)
    return JsonResponse({"task_id": task_id})

    # all_tickers = list(DaftarEmiten.objects.values_list("kode_emiten", flat=True))
    # counter = 0

    # try:
    #     for data_ticker in all_tickers:
    #         data = yf.download(data_ticker, period="2y", timeout=10)
    #         df = pd.DataFrame(data.sort_index(ascending=False))

    #         print(f"DATA EMITEN {data_ticker} BERHASIL DI AMBIL...")
    #         counter += 1

    #         df["Ticker"] = data_ticker
    #         cols = ["Close", "High", "Low", "Open"]

    #         df[cols] = df[cols].round(2)

    #         for index, row in df.iloc[::-1].iterrows():
    #             try:
    #                 DataSemuaSaham.objects.create(
    #                     kode_emiten=data_ticker,
    #                     tanggal=index.date(),
    #                     open=row["Open"],
    #                     high=row["High"],
    #                     low=row["Low"],
    #                     close=row["Close"],
    #                     volume=row["Volume"],
    #                 )
    #             except Exception as e:
    #                 print(f"Error, karena {e}")
    #                 continue

    #         time.sleep(50000)

    # except Exception as e:
    #     print(f"gagal dikarenakan {e}")

    # return redirect("ambil_data_saham:ambil_data_saham")

    
@jit(nopython=True)
def simulate_trades_numba(
    close_arr, high_arr, low_arr, atr_arr, signals_idx,
    sl_mult, tp_mult, max_hold_days
):
    trades = []
    position = 0  # 0 = no position, 1 = long
    entry_price = 0.0
    sl_price = 0.0
    tp_price = 0.0
    entry_idx = -1

    signal_set = set(signals_idx)

    for i in range(len(close_arr)):
        # Entry
        if position == 0 and i in signal_set:
            atr = atr_arr[i]
            entry_price = close_arr[i]
            sl_price = entry_price - (atr * sl_mult)
            tp_price = entry_price + (atr * tp_mult)
            # Round to nearest 5 (optional, but match your logic)
            sl_price = np.round(sl_price / 5) * 5
            tp_price = np.round(tp_price / 5) * 5
            position = 1
            entry_idx = i

        # Exit
        if position == 1:
            if i > entry_idx + max_hold_days:
                exit_price = close_arr[i]
                pnl_pct = (exit_price - entry_price) / entry_price
                trades.append(pnl_pct)
                position = 0
            elif low_arr[i] <= sl_price:
                exit_price = sl_price
                pnl_pct = (exit_price - entry_price) / entry_price
                trades.append(pnl_pct)
                position = 0
            elif high_arr[i] >= tp_price:
                exit_price = tp_price
                pnl_pct = (exit_price - entry_price) / entry_price
                trades.append(pnl_pct)
                position = 0

    return trades

def run_backtest(df, config, verbose=False):
    SL_MULT = config['RISK_MGMT']['SL_ATR_MULT']
    TP_MULT = config['RISK_MGMT']['TP_ATR_MULT']
    MAX_HOLD = 10

    # Generate BUY signals (same logic)
    signals_idx = []
    for i in range(2, len(df)):
        row = df.iloc[i]
        prev = df.iloc[i-1]
        is_strong_buy = (
            (row['macd'] > row['macd_signal']) &
            (prev['macd'] < prev['macd_signal']) &
            (row['close'] > row['ma200']) &
            (row['stoch_k'] < 80)
        )
        is_prep_buy = (
            (row['macd'] < row['macd_signal']) &
            (row['macd_hist'] > prev['macd_hist']) &
            (row['stoch_k'] < 25) &
            (
                (row['cdl_hammer'] > 0) |
                (row['cdl_morningstar'] > 0) |
                (row['cdl_piercing'] > 0) |
                (row['cdl_inv_hammer'] > 0) |
                (row['cdl_engulfing'] > 0) |
                (row['cdl_marubozu'] > 0 and row['close'] > row['open'])
            )
        )
        if is_strong_buy or is_prep_buy:
            signals_idx.append(i)

    if not signals_idx:
        return {"status": "no trades"}

    # Convert to numpy arrays (required for numba)
    close_arr = df['close'].values
    high_arr = df['high'].values
    low_arr = df['low'].values
    atr_arr = df['atr'].values

    # Run accelerated backtest
    trades = simulate_trades_numba(
        close_arr, high_arr, low_arr, atr_arr,
        np.array(signals_idx, dtype=np.int64),
        SL_MULT, TP_MULT, MAX_HOLD
    )

    # Hitung metrik (di luar numba, karena list biasa)
    if not trades:
        return {"status": "no trades"}

    trades = np.array(trades)
    wins = np.sum(trades > 0)
    losses = np.sum(trades <= 0)
    win_rate = wins / len(trades)
    total_return = np.sum(trades)
    avg_win = np.mean(trades[trades > 0]) if wins > 0 else 0.0
    avg_loss = np.mean(trades[trades < 0]) if losses > 0 else 0.0
    profit_factor = (avg_win * wins) / (abs(avg_loss) * losses) if losses > 0 else np.inf

    return {
        "total_trades": len(trades),
        "win_rate": win_rate,
        "profit_factor": profit_factor,
        "total_return_pct": total_return * 100,
        "avg_risk_reward": (avg_win / abs(avg_loss)) if losses > 0 else np.inf,
        "trades": trades.tolist()
    }


@login_required(login_url="/accounts/login/")
def ambil_data_saham_startx(request):
 
    try:
        stock = yf.Ticker("BBTN.JK")
        df = stock.history(period=CONFIG['PERIOD'])
        info = stock.info
    except Exception as e:
        print(f"❌ Error: Gagal download data saham. {e}")
        return

    if df.empty:
        print("❌ Error: Data saham kosong.")
        return

    # Cleaning
    df.columns = df.columns.str.lower()
    df.dropna(inplace=True)
    if df.empty:
        print("❌ Error: Tidak cukup data.")
        return

    # Filter Likuiditas
    avg_tx = (df['close'] * df['volume']).rolling(20).mean().iloc[-1]
    if avg_tx < CONFIG['MIN_LIKUIDITAS']:
        print(f"❌ REJECTED: Saham Tidak Likuid (Rata-rata transaksi cuma Rp {avg_tx:,.0f}).")
        return
    else:
        print(f"✅ LIKUIDITAS      : Aman (Rp {avg_tx:,.0f}/hari)")
    
    # Fundamental
    roe = info.get('returnOnEquity', 0)
    fund_msg = "✅ PROFITABLE" if roe > 0 else "⚠️ LOSS MAKER (Rugi)"
    print(f"🏢 FUNDAMENTAL     : ROE {roe*100:.2f}% ({fund_msg})")

    # --- INDIKATOR TEKNIS ---
    df['ma5'] = talib.SMA(df['close'], 5)
    df['ma20'] = talib.SMA(df['close'], 20)
    df['ma50'] = talib.SMA(df['close'], 50)
    df['ma200'] = talib.SMA(df['close'], 200)
    
    df['resistance'] = df['high'].rolling(20).max()
    df['support'] = df['low'].rolling(20).min()

    df['rsi'] = talib.RSI(df['close'], 14)
    df['macd'], df['macd_signal'], df['macd_hist'] = talib.MACD(df['close'], 12, 26, 9)
    
    slowk, slowd = talib.STOCH(df['high'], df['low'], df['close'], 
                               fastk_period=14, slowk_period=3, slowk_matype=0, 
                               slowd_period=3, slowd_matype=0)
    df['stoch_k'] = slowk

    df['atr'] = talib.ATR(df['high'], df['low'], df['close'], 14)
    upper, mid, lower = talib.BBANDS(df['close'], 20, 2, 2)
    df['bb_upper'], df['bb_lower'] = upper, lower
    df['bb_pos'] = (df['close'] - lower) / (upper - lower)
    df['bb_width'] = (upper - lower) / mid
    df['avg_bb_width'] = df['bb_width'].rolling(120).mean()

    df['obv'] = talib.OBV(df['close'], df['volume'])
    df['vol_ma'] = talib.SMA(df['volume'], 20)
    
    # Candlestick
    o, h, l, c = df['open'], df['high'], df['low'], df['close']
    df['cdl_hammer'] = talib.CDLHAMMER(o, h, l, c)
    df['cdl_morningstar'] = talib.CDLMORNINGSTAR(o, h, l, c)
    df['cdl_piercing'] = talib.CDLPIERCING(o, h, l, c)
    df['cdl_inv_hammer'] = talib.CDLINVERTEDHAMMER(o, h, l, c)
    df['cdl_engulfing'] = talib.CDLENGULFING(o, h, l, c)
    df['cdl_shootingstar'] = talib.CDLSHOOTINGSTAR(o, h, l, c)
    df['cdl_eveningstar'] = talib.CDLEVENINGSTAR(o, h, l, c)
    df['cdl_darkcloud'] = talib.CDLDARKCLOUDCOVER(o, h, l, c)
    df['cdl_hangingman'] = talib.CDLHANGINGMAN(o, h, l, c)
    df['cdl_doji'] = talib.CDLDOJI(o, h, l, c)
    df['cdl_spinningtop'] = talib.CDLSPINNINGTOP(o, h, l, c)
    df['cdl_marubozu'] = talib.CDLMARUBOZU(o, h, l, c)

    df['upper_shadow'] = df['high'] - df[['open', 'close']].max(axis=1)
    df['body'] = (df['close'] - df['open']).abs()

    df.dropna(inplace=True)
    if df.empty:
        print("❌ Error: Tidak cukup data setelah perhitungan indikator.")
        return

    last = df.iloc[-1]
    prev = df.iloc[-2]

    # --- ANALISIS POLA CANDLESTICK ---
    bull_pats, bear_pats = [], []
    if last['cdl_hammer'] > 0: bull_pats.append("Hammer 🔨")
    if last['cdl_morningstar'] > 0: bull_pats.append("Morning Star ⭐")
    if last['cdl_piercing'] > 0: bull_pats.append("Piercing Line 📈")
    if last['cdl_inv_hammer'] > 0: bull_pats.append("Inverted Hammer ⛏️")
    if last['cdl_engulfing'] > 0: bull_pats.append("Bullish Engulfing 🔥")
    if last['cdl_marubozu'] > 0 and last['close'] > last['open']: bull_pats.append("Bullish Marubozu 🧱")

    if last['cdl_shootingstar'] < 0: bear_pats.append("Shooting Star 🌠")
    if last['cdl_eveningstar'] < 0: bear_pats.append("Evening Star 🌑")
    if last['cdl_darkcloud'] < 0: bear_pats.append("Dark Cloud Cover 📉")
    if last['cdl_hangingman'] < 0: bear_pats.append("Hanging Man 😵")
    if last['cdl_engulfing'] < 0: bear_pats.append("Bearish Engulfing 🩸")

    has_bullish = len(bull_pats) > 0
    has_bearish = len(bear_pats) > 0

    # --- DETEKSI BANDAR ---
    bandar_msgs = []
    if (last['close'] <= df['close'].iloc[-6]) and (last['obv'] > df['obv'].iloc[-6]):
        bandar_msgs.append("🟢 SILENT ACCUMULATION (Divergence: Harga Diam, Uang Masuk)")
    if last['volume'] > (last['vol_ma'] * 2.0):
        bandar_msgs.append("💥 VOLUME SPIKE (Ledakan Volume Hari Ini)")
    if last['close'] > prev['close'] and last['volume'] > last['vol_ma']:
        bandar_msgs.append("🚀 MARK-UP ACTIVITY (Harga dipompa naik)")
    if last['upper_shadow'] > (last['body'] * 2) and last['volume'] > last['vol_ma']:
        bandar_msgs.append("💀 JARUM SUNTIK / GUYURAN (Trap: Ekor Atas Panjang)")

    # --- LOGIKA SINYAL ---
    is_strong_buy = (
        (last['macd'] > last['macd_signal']) & 
        (prev['macd'] < prev['macd_signal']) & 
        (last['close'] > last['ma200']) & 
        (last['stoch_k'] < 80)
    )
    is_prep_buy = (
        (last['macd'] < last['macd_signal']) &
        (last['macd_hist'] > prev['macd_hist']) &
        (last['stoch_k'] < 25) &
        (has_bullish) 
    )
    is_strong_sell = (last['macd'] < last['macd_signal']) & (prev['macd'] > prev['macd_signal'])
    is_prep_sell = (last['bb_pos'] > 0.95) | (has_bearish)
    is_squeeze = last['bb_width'] < last['avg_bb_width']

    # --- LAPORAN ---
    print(f"\n📊 DATA TEKNIKAL ({df.index[-1].date()})")
    print(f"   Harga Close : {last['close']:,.0f}")
    
    # RSI (ditambahkan sesuai permintaan)
    rsi_val = last['rsi']
    rsi_status = "OVERSOLD" if rsi_val < 30 else "OVERBOUGHT" if rsi_val > 70 else "Netral"
    print(f"   RSI         : {rsi_val:.1f} ({rsi_status})")
    
    stoch_st = "OVERSOLD" if last['stoch_k'] < 20 else "OVERBOUGHT" if last['stoch_k'] > 80 else "Netral"
    hist_trend = "MENGUAT (Positif)" if last['macd_hist'] > prev['macd_hist'] else "MELEMAH (Negatif)"
    print(f"   Stochastic  : K={last['stoch_k']:.1f} ({stoch_st})")
    print(f"   MACD Hist   : {last['macd_hist']:.4f} -> Momentum {hist_trend}")
    print(f"   Volatilitas : {'⚡ SQUEEZE (Siap Meledak)' if is_squeeze else 'Normal'}")

    print(f"\n🕯️ CANDLESTICK PATTERN:")
    if has_bullish: print(f"   🐂 BULLISH: {', '.join(bull_pats)}")
    if has_bearish: print(f"   🐻 BEARISH: {', '.join(bear_pats)}")
    if not (has_bullish or has_bearish): print("   ⚪ Pola Netral.")

    print(f"\n🕵️ DETEKSI BANDAR:")
    if bandar_msgs:
        for msg in bandar_msgs: print(f"   {msg}")
    else: print("   ⚪ Normal (Retail Activity).")

    # --- REKOMENDASI ---
    print(f"\n📢 REKOMENDASI SISTEM:")
    status = "WAIT / NEUTRAL"
    if is_strong_buy: status = "STRONG BUY"
    elif is_prep_buy: status = "PREPARING BUY"
    elif is_strong_sell: status = "STRONG SELL"
    elif is_prep_sell: status = "PREPARING SELL"
    
    if status == "STRONG BUY" and is_squeeze:
        status += " + BIG BREAKOUT POTENTIAL 💥"
    if status == "WAIT / NEUTRAL" and "Morning Star ⭐" in bull_pats:
        status = "PREPARING BUY (STRONG CANDLE)"

    if "STRONG BUY" in status:
        print(f"   🚀 STATUS: {status}")
        print("   Alasan: Golden Cross + Uptrend + Stoch Aman.")
    elif "PREPARING BUY" in status:
        print(f"   🟡 STATUS: {status}")
        print("   Alasan: Harga Murah + Pola Candle Reversal Valid.")
    elif "STRONG SELL" in status:
        print(f"   🔻 STATUS: {status}")
    elif "PREPARING SELL" in status:
        print(f"   🟠 STATUS: {status}")
    else:
        print(f"   💤 STATUS: {status}")

    # --- WIN RATE CEPAT (Hanya MACD Cross) ---
    if "BUY" in status:
        wins, total = 0, 0
        sigs = df[(df['macd'] > df['macd_signal']) & (df['macd'].shift(1) < df['macd_signal'].shift(1))]
        for _, row in sigs.iterrows():
            idx = df.index.get_loc(row.name)
            if idx+5 < len(df) and df.iloc[idx+5]['close'] > row['close']: wins += 1
            total += 1
        wr = (wins/total*100) if total > 0 else 0
        print(f"   Win Rate Historis: {wr:.1f}% ({total} trades)")

    # --- LEVEL HARGA ---
    atr = last['atr']
    sl = round((last['close'] - (atr * CONFIG['RISK_MGMT']['SL_ATR_MULT']))/5)*5
    tp = round((last['close'] + (atr * CONFIG['RISK_MGMT']['TP_ATR_MULT']))/5)*5
    risk_pct = ((last['close'] - sl) / last['close']) * 100
    reward_pct = ((tp - last['close']) / last['close']) * 100

    title_plan = "📝 TRADING PLAN OTOMATIS (Sinyal Valid)" if "BUY" in status else "ℹ️ REFERENSI LEVEL HARGA (Watchlist Only)"
    action_msg = "🔵 ENTRY SEKARANG" if "BUY" in status else "⚪ HARGA SAAT INI "
    print(f"\n{title_plan}")
    print(f"   ---------------------------------------")
    print(f"   {action_msg} : Rp {last['close']:,.0f}")
    print(f"   🔴 STOP LOSS   : Rp {sl:,.0f} (-{risk_pct:.1f}%)")
    print(f"   🟢 TAKE PROFIT : Rp {tp:,.0f} (+{reward_pct:.1f}%)")
    print(f"   ---------------------------------------")
    print(f"   Ratio Risk:Reward = 1 : {reward_pct/risk_pct:.1f}")
    if "SELL" in status or status == "WAIT / NEUTRAL":
        print(f"   ⚠️ Catatan: Status '{status}'. Level di atas hanya simulasi.")

    # --- BACKTEST 1 TAHUN TERAKHIR ---
    print(f"\n🔍 BACKTEST ({CONFIG['BACKTEST_YEARS']} TAHUN TERAKHIR):")
    backtest_start = df.index[-1] - pd.DateOffset(years=CONFIG['BACKTEST_YEARS'])
    df_backtest = df[df.index >= backtest_start].copy()

    if len(df_backtest) < 50:
        print("   ⚪ Data backtest terlalu singkat (<50 hari).")
    else:
        bt_result = run_backtest(df_backtest, CONFIG)
        if bt_result.get("status") != "no trades":
            print(f"   Total Trades    : {bt_result['total_trades']}")
            print(f"   Win Rate        : {bt_result['win_rate']*100:.1f}%")
            print(f"   Profit Factor   : {bt_result['profit_factor']:.2f}")
            print(f"   Total Return    : {bt_result['total_return_pct']:.2f}%")
            print(f"   Avg Risk:Reward : 1 : {bt_result['avg_risk_reward']:.1f}")
        else:
            print("   ⚪ Tidak ada sinyal backtest dalam periode ini.")




    time.sleep(9000)

    # try:
    #     for data_ticker in all_tickers:
    #         counter += 1

    #         # df = yf.download(data_ticker, period="1y", timeout=10)

    #         # if df.empty:
    #         #     print("!!! ERROR: Data Download Kosong. Cek Ticker Anda !!!")
    #         # else:
    #         #     print(f"   > Download berhasil. Jumlah baris awal: {len(df)}")

    #         #     if isinstance(df.columns, pd.MultiIndex):
    #         #         if "Close" in df.columns.get_level_values(0):
    #         #             df.columns = df.columns.get_level_values(0)
    #         #         else:
    #         #             df.columns = df.columns.get_level_values(-1)

    #         #     df.columns = df.columns.str.lower()

    #         #     if "close" not in df.columns:
    #         #         print(
    #         #             f"!!! ERROR: Kolom 'close' tidak ditemukan. Kolom yang ada: {df.columns.tolist()}"
    #         #         )
    #         #     else:
    #         #         df.fillna(0, inplace=True)
    #         #         print(f"   > Setelah cleaning awal. Jumlah baris: {len(df)}")

    #         #         macd, macd_signal, macd_hist = talib.MACD(
    #         #             df["close"], fastperiod=12, slowperiod=26, signalperiod=9
    #         #         )

    #         #         try:
    #         #             df["ma5"] = talib.SMA(df["close"], timeperiod=5).round(2)
    #         #             df["ma20"] = talib.SMA(df["close"], timeperiod=20).round(2)
    #         #             df["ma50"] = talib.SMA(df["close"], timeperiod=50).round(2)
    #         #             df["ma200"] = talib.SMA(df["close"], timeperiod=200).round(2)
    #         #             df["rsi"] = talib.RSI(df["close"], timeperiod=14).round(2)

    #         #             df["macd"] = macd
    #         #             df["macd_signal"] = macd_signal
    #         #             df["macd_hist"] = macd_hist

    #         #             df['prev_macd'] = df['macd'].shift(1)
    #         #             df['prev_signal'] = df['macd_signal'].shift(1)

    #         #             bullish_cond = (df['prev_macd'] < df['prev_signal']) & (df['macd'] > df['macd_signal'])
    #         #             bearish_cond = (df['prev_macd'] > df['prev_signal']) & (df['macd'] < df['macd_signal'])

    #         #             df['macd_action'] = np.select([bullish_cond, bearish_cond], [1, -1], default=0)

    #         #             df[
    #         #                 [
    #         #                     "ma5",
    #         #                     "ma20",
    #         #                     "ma50",
    #         #                     "ma200",
    #         #                     "rsi",
    #         #                     "macd",
    #         #                     "macd_signal",
    #         #                     "macd_hist",
    #         #                     "macd_action",
    #         #                 ]
    #         #             ] = df[
    #         #                 [
    #         #                     "ma5",
    #         #                     "ma20",
    #         #                     "ma50",
    #         #                     "ma200",
    #         #                     "rsi",
    #         #                     "macd",
    #         #                     "macd_signal",
    #         #                     "macd_hist",
    #         #                     "macd_action",
    #         #                 ]
    #         #             ].fillna(0)

    #         #             print(
    #         #                 df[
    #         #                     [
    #         #                         "ma5",
    #         #                         "ma20",
    #         #                         "ma50",
    #         #                         "ma200",
    #         #                         "rsi",
    #         #                         "macd",
    #         #                         "macd_signal",
    #         #                         "macd_hist",
    #         #                         "macd_action",
    #         #                     ]
    #         #                 ]
    #         #             )

    #         #             signals = df[df['macd_action'] != 0].copy()

    #         #             if not signals.empty:
    #         #                 # Tambah kolom teks agar mudah dibaca
    #         #                 signals['keputusan'] = signals['macd_action'].map({
    #         #                     1: 'BUY (Golden Cross)',
    #         #                     -1: 'SELL (Dead Cross)'
    #         #                 })

    #         #                 # Tampilkan 10 sinyal terakhir saja
    #         #                 print(signals[["close", "rsi", "macd", "macd_signal", "keputusan"]])

    #         #                 # Cek status hari ini
    #         #                 last_date = df.index[-1]
    #         #                 last_signal_date = signals.index[-1]

    #         #                 if last_date == last_signal_date:
    #         #                     action = signals.iloc[-1]['keputusan']
    #         #                     print(f"\n[ALERT] HARI INI ADA SINYAL: {action}!")
    #         #                 else:
    #         #                     print(f"\nHari ini ({last_date.date()}) tidak ada perpotongan MACD.")
    #         #             else:
    #         #                 print("Tidak ada sinyal MACD Cross ditemukan dalam periode ini.")

    #         #         except Exception as e:
    #         #             print(f"Error saat perhitungan: {e}")

    #     try:
    #         df = yf.download(data_ticker, period=PERIOD_BACKTEST, timeout=20, progress=False)
    #     except Exception as e:
    #         print(f"[ERROR] Gagal download: {e}")
    #         return

    #     if df.empty:
    #         print("[ERROR] Data Kosong.")
    #         return

    #     # Handle MultiIndex & Lowercase
    #     if isinstance(df.columns, pd.MultiIndex):
    #         if "Close" in df.columns.get_level_values(0):
    #             df.columns = df.columns.get_level_values(0)
    #         else:
    #             df.columns = df.columns.get_level_values(-1)
    #     df.columns = df.columns.str.lower()
    #     df.dropna(inplace=True)

    #     # ---------------------------------------------------------
    #     # 2. FILTER 1: LIQUIDITY CHECK (Saringan Awal)
    #     # ---------------------------------------------------------
    #     # Menghitung rata-rata nilai transaksi harian (Close x Volume) selama 20 hari terakhir
    #     df['tx_value'] = df['close'] * df['volume']
    #     avg_daily_value = df['tx_value'].rolling(window=20).mean().iloc[-1]

    #     print(f"Rata-rata Transaksi Harian: Rp {avg_daily_value:,.0f}")

    #     if avg_daily_value < MIN_TRANSACTION_VALUE:
    #         print(f"❌ [REJECTED] Saham tidak likuid (< Rp {MIN_TRANSACTION_VALUE:,.0f}).")
    #         print("   Risiko tinggi sulit menjual (Bid/Offer tipis). Analisis dihentikan.")
    #         return # Stop di sini, tidak perlu lanjut hitung indikator

    #     print("✅ [PASSED] Saham Cukup Likuid.")

    #     # ---------------------------------------------------------
    #     # 3. HITUNG INDIKATOR TEKNIKAL
    #     # ---------------------------------------------------------
    #     # Moving Averages
    #     df["ma5"] = talib.SMA(df["close"], timeperiod=5)
    #     df["ma200"] = talib.SMA(df["close"], timeperiod=200) # Trend Filter

    #     # Volume MA (Untuk validasi ledakan volume)
    #     df["vol_ma20"] = talib.SMA(df["volume"], timeperiod=20)

    #     # RSI & MACD
    #     df["rsi"] = talib.RSI(df["close"], timeperiod=14)
    #     macd, macd_signal, _ = talib.MACD(df["close"], fastperiod=12, slowperiod=26, signalperiod=9)
    #     df["macd"] = macd
    #     df["macd_signal"] = macd_signal

    #     df.dropna(inplace=True) # Hapus data awal yg NaN akibat indikator

    #     # ---------------------------------------------------------
    #     # 4. LOGIKA STRATEGI (HIGH PROBABILITY SETUP)
    #     # ---------------------------------------------------------

    #     # Syarat 1: MACD Golden Cross
    #     cross_up = (df['macd'] > df['macd_signal']) & (df['macd'].shift(1) < df['macd_signal'].shift(1))

    #     # Syarat 2: Trend Uptrend (Harga di atas MA200) - Opsional tapi disarankan
    #     uptrend = df['close'] > df['ma200']

    #     # Syarat 3: Volume Confirmation (PENTING BUAT WINRATE)
    #     # Volume hari ini > Rata-rata Volume 20 hari
    #     valid_volume = df['volume'] > (df['vol_ma20'] * VOLUME_SPIKE_THRESHOLD)

    #     # Syarat 4: RSI tidak Overbought (Masih ada ruang naik)
    #     rsi_safe = df['rsi'] < 70

    #     # Gabungkan Logika
    #     # Sinyal Grade A: Cross + Uptrend + Volume Kuat + RSI Aman
    #     df['signal_buy'] = cross_up & uptrend & valid_volume & rsi_safe

    #     # ---------------------------------------------------------
    #     # 5. FITUR SPESIAL: SIMPLE BACKTEST (Hitung Win Rate)
    #     # ---------------------------------------------------------
    #     # Kita akan cek setiap sinyal Buy di masa lalu:
    #     # Apakah 5 hari setelah beli harganya naik?

    #     signals = df[df['signal_buy']].copy()

    #     if signals.empty:
    #         print("\n⚠️ Tidak ditemukan Sinyal Grade A dalam periode ini.")
    #     else:
    #         print(f"\n--- BACKTEST HISTORY (Performa Strategi di {data_ticker}) ---")
    #         wins = 0
    #         total_trades = 0

    #         # Loop setiap sinyal yang pernah muncul
    #         for date, row in signals.iterrows():
    #             # Cari index lokasi tanggal tersebut
    #             idx = df.index.get_loc(date)

    #             # Cek harga 5 hari kemudian (Exit Plan sederhana)
    #             if idx + 5 < len(df):
    #                 price_buy = row['close']
    #                 price_sell = df.iloc[idx + 5]['close'] # Harga 5 hari kemudian

    #                 profit_pct = ((price_sell - price_buy) / price_buy) * 100

    #                 status = "WIN ✅" if profit_pct > 0 else "LOSS ❌"
    #                 if profit_pct > 0: wins += 1
    #                 total_trades += 1

    #                 # Tampilkan detail 5 transaksi terakhir saja agar tidak penuh
    #                 if total_trades > len(signals) - 5:
    #                     print(f"Tgl: {date.date()} | Beli: {price_buy:.0f} | Jual (H+5): {price_sell:.0f} | Hasil: {profit_pct:.2f}% {status}")

    #         if total_trades > 0:
    #             win_rate = (wins / total_trades) * 100
    #             print(f"\n📊 TOTAL WIN RATE HISTORIS: {win_rate:.1f}% ({wins}/{total_trades} Menang)")
    #             if win_rate > 60:
    #                 print("KESIMPULAN: Strategi ini COCOK untuk saham ini.")
    #             else:
    #                 print("KESIMPULAN: Hati-hati, saham ini sering memberi sinyal palsu.")

    #     # ---------------------------------------------------------
    #     # 6. STATUS HARI INI
    #     # ---------------------------------------------------------
    #     print("\n--- ANALISIS HARI INI ---")
    #     last_row = df.iloc[-1]

    #     # Cek satu per satu syarat untuk memberi info detail
    #     is_trend_up = last_row['close'] > last_row['ma200']
    #     is_vol_ok   = last_row['volume'] > last_row['vol_ma20']
    #     is_macd_ok  = last_row['macd'] > last_row['macd_signal'] # Posisi MACD (bukan cross, tapi posisi)

    #     print(f"Harga Close  : {last_row['close']}")
    #     print(f"Volume       : {'MENGUAT ✅' if is_vol_ok else 'LEMAH ❌'} ({last_row['volume']:.0f} vs Avg {last_row['vol_ma20']:.0f})")
    #     print(f"Trend Major  : {'UPTREND ✅' if is_trend_up else 'DOWNTREND ⚠️'} (Diatas MA200?)")
    #     print(f"Momentum     : {'BULLISH' if is_macd_ok else 'BEARISH'}")

    #     # Cek apakah HARI INI ada sinyal entry?
    #     if last_row['signal_buy']:
    #         print("\n🔥 [ALERT] SINYAL STRONG BUY HARI INI! 🔥")
    #         print("Alasan: MACD Cross + Volume Spike + Uptrend MA200.")
    #     else:
    #         # Berikan saran
    #         if not is_vol_ok and is_trend_up and is_macd_ok:
    #             print("\nStatus: WAIT.")
    #             print("Indikator bagus, tapi Volume belum meledak. Tunggu Big Player masuk.")
    #         else:
    #             print("\nStatus: NO ACTION.")

    #         time.sleep(9000)

    #         ####  SIMPAN DATA KE DATABASE #############
    #         df["Ticker"] = data_ticker
    #         cols = ["Close", "High", "Low", "Open"]
    #         df[cols] = df[cols].round(2)

    #         for index, row in df.iterrows():
    #             try:
    #                 DataSemuaSaham.objects.create(
    #                     kode_emiten=data_ticker,
    #                     tanggal=index.date(),
    #                     open=row["Open"],
    #                     high=row["High"],
    #                     low=row["Low"],
    #                     close=row["Close"],
    #                     volume=row["Volume"],
    #                 )
    #             except Exception as e:
    #                 print(f"Error, karena {e}")
    #                 continue

    #         try:
    #             del data
    #             del df
    #         except UnboundLocalError:
    #             pass

    #         gc.collect()

    #         time.sleep(5000)

    # except Exception as e:
    #     print(f"gagal dikarenakan {e}")

    # try:
    #     for data_ticker in all_tickers:
    #         data = yf.download(data_ticker, period="1y", timeout=10)
    #         df = pd.DataFrame(data.sort_index(ascending=False))

    #         print(f"DATA EMITEN {data_ticker} BERHASIL DI AMBIL...")
    #         counter += 1

    #         df["Ticker"] = data_ticker
    #         cols = ["Close", "High", "Low", "Open"]

    #         df[cols] = df[cols].round(2)

    #         for index, row in df.iloc[::-1].iterrows():
    #             try:
    #                 DataSemuaSaham.objects.create(
    #                     kode_emiten=data_ticker,
    #                     tanggal=index.date(),
    #                     open=row["Open"],
    #                     high=row["High"],
    #                     low=row["Low"],
    #                     close=row["Close"],
    #                     volume=row["Volume"],
    #                 )
    #             except Exception as e:
    #                 print(f"Error, karena {e}")
    #                 continue

    #         ####################################################################
    #         # 1. Kolom dasar
    #         df["Values"] = df["Close"] * df["Volume"]
    #         df["Pivot"] = (df["Close"] + df["High"] + df["Low"]) / 3
    #         ###########################################################
    #         close_list = df["Close"]
    #         #########################################################

    #         ####################################################################
    #         # 2. CH, CL, CC → bandingkan hari ini dengan besok
    #         # Karena butuh hari berikutnya, hasilnya akan NaN di baris terakhir
    #         df["ch"] = (df["High"] - df["Close"].shift(-1)) / df["High"] * 100
    #         df["cl"] = (df["Low"] - df["Close"].shift(-1)) / df["Low"] * 100
    #         df["cc"] = (df["Close"] - df["Close"].shift(-1)) / df["Close"] * 100
    #         #####################################################################
    #         # 3. Moving Average
    #         df["ma5"] = df["Close"].rolling(window=5).mean()
    #         df["ma20"] = df["Close"].rolling(window=20).mean()
    #         df["ma50"] = df["Close"].rolling(window=50).mean()
    #         df["ma200"] = df["Close"].rolling(window=200).mean()

    #         ma5_data = df["ma5"].round(2)
    #         ma20_data = df["ma20"].round(2)
    #         ma50_data = df["ma50"].round(2)
    #         ma200_data = df["ma200"].round(2)

    #         non_nan_ma5 = ma5_data.dropna().sort_index(ascending=False)
    #         values_ma5 = non_nan_ma5.values
    #         all_dates_ma5 = ma5_data.index
    #         new_values_ma5 = np.full(len(ma5_data), np.nan)
    #         new_values_ma5[: len(values_ma5)] = values_ma5

    #         non_nan_ma20 = ma20_data.dropna().sort_index(ascending=False)
    #         values_ma20 = non_nan_ma20.values
    #         all_dates_ma20 = ma20_data.index
    #         new_values_ma20 = np.full(len(ma20_data), np.nan)
    #         new_values_ma20[: len(values_ma20)] = values_ma20

    #         non_nan_ma50 = ma50_data.dropna().sort_index(ascending=False)
    #         values_ma50 = non_nan_ma50.values
    #         all_dates_ma50 = ma50_data.index
    #         new_values_ma50 = np.full(len(ma50_data), np.nan)
    #         new_values_ma50[: len(values_ma50)] = values_ma50

    #         non_nan_ma200 = ma200_data.dropna().sort_index(ascending=False)
    #         values_ma200 = non_nan_ma200.values
    #         all_dates_ma200 = ma200_data.index
    #         new_values_ma200 = np.full(len(ma200_data), np.nan)
    #         new_values_ma200[: len(values_ma200)] = values_ma200

    #         ma5_nilai = (
    #             pd.DataFrame(new_values_ma5, index=all_dates_ma5)
    #             .dropna()
    #             .rename(columns={0: "ma5_nilai"})
    #         )
    #         ma20_nilai = (
    #             pd.DataFrame(new_values_ma20, index=all_dates_ma20)
    #             .dropna()
    #             .rename(columns={0: "ma20_nilai"})
    #         )
    #         ma50_nilai = (
    #             pd.DataFrame(new_values_ma50, index=all_dates_ma50)
    #             .dropna()
    #             .rename(columns={0: "ma50_nilai"})
    #         )
    #         ma200_nilai = (
    #             pd.DataFrame(new_values_ma200, index=all_dates_ma200)
    #             .dropna()
    #             .rename(columns={0: "ma200_nilai"})
    #         )

    #         #########################
    #         list_close = [x[0] for x in close_list.values.tolist()]
    #         ##########################
    #         list_ma5 = [x[0] for x in ma5_nilai.values.tolist()]
    #         ##########################
    #         list_ma20 = [x[0] for x in ma20_nilai.values.tolist()]
    #         ##########################
    #         list_ma50 = [x[0] for x in ma50_nilai.values.tolist()]
    #         #########################
    #         list_ma200 = [x[0] for x in ma200_nilai.values.tolist()]

    #         ### CARI SIGNAL MA5 #######################################################
    #         length_ma5 = len(close_list) - len(list_ma5)
    #         close_untuk_ma5 = list_close[:-length_ma5]
    #         cari_ma5 = pd.DataFrame({"Close": close_untuk_ma5, "MA5_data": list_ma5})
    #         cari_ma5["MA5_signal"] = (
    #             (cari_ma5["MA5_data"] - cari_ma5["Close"]) / 100
    #         ).round(2)
    #         cari_ma5 = cari_ma5.rename(columns={"MA5_signal": "MA5"})
    #         ### CARI SIGNAL MA20 #####################################################
    #         length_ma20 = len(close_list) - len(list_ma20)
    #         close_untuk_ma20 = list_close[:-length_ma20]
    #         cari_ma20 = pd.DataFrame(
    #             {
    #                 "Close": close_untuk_ma20,
    #                 "MA20_data": list_ma20,
    #             }
    #         )
    #         cari_ma20["MA20_signal"] = (
    #             (cari_ma20["MA20_data"] - cari_ma20["Close"]) / 100
    #         ).round(2)
    #         cari_ma20 = cari_ma20.rename(columns={"MA20_signal": "MA20"})
    #         ### CARI SIGNAL MA50 #####################################################
    #         length_ma50 = len(close_list) - len(list_ma50)
    #         close_untuk_ma50 = list_close[:-length_ma50]
    #         cari_ma50 = pd.DataFrame(
    #             {
    #                 "Close": close_untuk_ma50,
    #                 "MA50_data": list_ma50,
    #             }
    #         )
    #         cari_ma50["MA50_signal"] = (
    #             (cari_ma50["MA50_data"] - cari_ma50["Close"]) / 100
    #         ).round(2)
    #         cari_ma50 = cari_ma50.rename(columns={"MA50_signal": "MA50"})
    #         ### CARI SIGNAL MA200 #####################################################
    #         length_ma200 = len(close_list) - len(list_ma200)
    #         close_untuk_ma200 = list_close[:-length_ma200]
    #         cari_ma200 = pd.DataFrame(
    #             {
    #                 "Close": close_untuk_ma200,
    #                 "MA200_data": list_ma200,
    #             }
    #         )
    #         cari_ma200["MA200_signal"] = (
    #             (cari_ma200["MA200_data"] - cari_ma200["Close"]) / 100
    #         ).round(2)
    #         cari_ma200 = cari_ma200.rename(columns={"MA200_signal": "MA200"})
    #         ##########################################################################
    #         values = pd.DataFrame(df["Values"].values, columns=["Values"])
    #         tanggal = pd.DataFrame(df.index.values, columns=["Tanggal"])
    #         ch_data = pd.DataFrame(df["ch"].round(2).values, columns=["ch"])
    #         cl_data = pd.DataFrame(df["cl"].round(2).values, columns=["cl"])
    #         cc_data = pd.DataFrame(df["cc"].round(2).values, columns=["cc"])
    #         pp = pd.DataFrame(df["Pivot"].round(2).values, columns=["pp"])

    #         ################################################

    #         gabungan = pd.concat(
    #             {
    #                 "TANGGAL": tanggal["Tanggal"],
    #                 "Values": values["Values"],
    #                 "CH": ch_data["ch"],
    #                 "CL": cl_data["cl"],
    #                 "CC": cc_data["cc"],
    #                 "PP": pp["pp"],
    #                 "MA5": cari_ma5["MA5"],
    #                 "MA20": cari_ma20["MA20"],
    #                 "MA50": cari_ma50["MA50"],
    #                 "MA200": cari_ma200["MA200"],
    #             },
    #             axis=1,
    #         ).fillna(0)

    #         for _, row in gabungan.iloc[::-1].iterrows():
    #             try:
    #                 ListPolaSaham.objects.create(
    #                     kode_emiten=data_ticker,
    #                     tanggal=row["TANGGAL"],
    #                     value=row["Values"],
    #                     ch=row["CH"],
    #                     cl=row["CL"],
    #                     cc=row["CC"],
    #                     pp=row["PP"],
    #                     ma5=row["MA5"],
    #                     ma20=row["MA20"],
    #                     ma50=row["MA50"],
    #                     ma200=row["MA200"],
    #                 )
    #             except Exception as e:
    #                 print(f"Error, karena {e}")
    #                 continue

    #         sekarang = datetime.now()
    #         jam_akhir = sekarang.strftime("%H:%M:%S")

    #         print("#" * 40)
    #         print(f"Mulai               : {jam}")
    #         print(f"Saham yang berhasil : {counter}")
    #         print(f"Jam Berakhir        : {jam_akhir}")
    #         print("#" * 40)
    #         time.sleep(10)

    # except Exception as e:
    #     print(f"gagal dikarenakan {e}")
    # print("#" * 50)
    # print(f"DATA SAHAM BERHASIL ADA : {counter} ")
    # print("#" * 50)

    return redirect("ambil_data_saham:ambil_data_saham_start")
