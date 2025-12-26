from django.shortcuts import redirect, render
from django.contrib.auth.decorators import login_required
import yfinance as yf
# import json
from django.contrib import messages
import pandas as pd
from ..models import DaftarEmiten
import talib
import numpy as np
from numba import jit

CONFIG = {
    'PERIOD': '2y',               # Cukup untuk indikator jangka panjang
    'MIN_LIKUIDITAS': 5_000_000_000,  # 1 miliar Rupiah
    'BACKTEST_YEARS': 1,          # Hanya backtest 1 tahun terakhir
    'RISK_MGMT': {
        'SL_ATR_MULT': 5.0,
        'TP_ATR_MULT': 10
    }
}

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
def dashboard(request):
    total_emiten = DaftarEmiten.objects.all().count()

    

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



   

    context = {"page_title": "DASHBOARD", "total_emiten": total_emiten}

    return render(request, "dashboard.html", context)


@login_required(login_url="/accounts/login/")
def upload_emiten(request):
    if request.method == "POST":
        uploaded_file = request.FILES.get("upload_file")

        try:
            if not uploaded_file:
                messages.error(request, "Tidak ada file yang dipilih !!")

            if not uploaded_file.name.endswith(".xlsx"):
                messages.error(request, "Hanya file .xlsx yang diizinkan.")

            try:
                DaftarEmiten.objects.all().delete()

                df = pd.read_excel(uploaded_file)
                data = df[["Kode", "Nama Perusahaan"]].to_dict("records")

                emiten_objects = [
                    DaftarEmiten(
                        kode_emiten=item["Kode"] + ".JK",
                        nama_perusahaan=item["Nama Perusahaan"],
                    )
                    for item in data
                ]

                DaftarEmiten.objects.bulk_create(emiten_objects, ignore_conflicts=True)

            except Exception as e:
                messages.error(request, f"Gagal memproses file: {str(e)}")

            messages.success(request, f"File {uploaded_file.name} berhasil diterima!")

        except Exception:
            messages.error(request, "Tidak ada file yang diupload!!")

    return redirect("home:dashboard")


