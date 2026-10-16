// Penghubung antara Leptos (Rust/WASM) dan TradingView Lightweight Charts v5.
// Rust memanggil ScannerChart.render(el, json) dengan JSON CandlesDto.
(function () {
  const LWC = window.LightweightCharts;
  const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

  const palette = () => ({
    ink: css("--color-ink"),
    panel: css("--color-panel"),
    line: css("--color-line"),
    paper: css("--color-paper"),
    muted: css("--color-muted"),
    brass: css("--color-brass"),
    up: css("--color-up"),
    down: css("--color-down"),
  });

  // Warna garis per indikator, diambil dari awalan key.
  const lineColor = (key, p) => {
    if (key.startsWith("sma_20") || key.startsWith("ema_")) return p.brass;
    if (key.startsWith("sma_50")) return "#7FA7D9";
    if (key.startsWith("sma_200")) return "#B58BD9";
    if (key.startsWith("sma_")) return "#9FB4C7";
    if (key.startsWith("bb_middle")) return "rgba(230,225,211,0.35)";
    if (key.startsWith("bb_")) return "rgba(230,225,211,0.55)";
    if (key.startsWith("supertrend")) return "#E0B35A";
    if (key.startsWith("ichimoku_tenkan")) return "#7FA7D9";
    if (key.startsWith("ichimoku_kijun")) return "#E5675E";
    if (key.startsWith("ichimoku_senkou_a")) return "rgba(79,191,139,0.7)";
    if (key.startsWith("ichimoku_senkou_b")) return "rgba(229,103,94,0.7)";
    if (key.startsWith("macd_signal")) return "#E0B35A";
    if (key.startsWith("plus_di")) return p.up;
    if (key.startsWith("minus_di")) return p.down;
    if (key === "obv" || key === "acc_dist") return "#7FA7D9";
    if (key.startsWith("cmf_") || key.startsWith("mfi_")) return p.brass;
    return p.paper;
  };

  const OVERLAY = ["sma_", "ema_", "bb_", "supertrend_", "psar_", "ichimoku_"];
  const paneOf = (key) => {
    if (OVERLAY.some((p) => key.startsWith(p))) return "price";
    if (key.startsWith("rsi_")) return "rsi";
    if (key.startsWith("macd_")) return "macd";
    if (key.startsWith("stoch_")) return "stoch";
    if (key.startsWith("adx_") || key.startsWith("plus_di") || key.startsWith("minus_di")) return "adx";
    if (key.startsWith("atr_")) return "atr";
    if (key === "obv") return "obv";
    if (key === "acc_dist") return "ad";
    if (key.startsWith("cmf_")) return "cmf";
    if (key.startsWith("mfi_")) return "mfi";
    if (key.startsWith("rel_volume_")) return "rvol";
    return "other";
  };

  const points = (candles, values) => {
    const out = [];
    for (let i = 0; i < candles.length; i++) {
      const v = values[i];
      if (v !== null && v !== undefined && Number.isFinite(v)) out.push({ time: candles[i].date, value: v });
    }
    return out;
  };

  const candleData = (candles) => candles.map((c) => ({ time: c.date, open: c.open, high: c.high, low: c.low, close: c.close }));
  const volumeData = (candles) => candles.map((c) => ({
    time: c.date, value: c.volume,
    color: c.close >= c.open ? "rgba(79,191,139,0.32)" : "rgba(229,103,94,0.32)",
  }));
  const histData = (pts) => pts.map((d) => ({ ...d, color: d.value >= 0 ? "rgba(79,191,139,0.6)" : "rgba(229,103,94,0.6)" }));
  // Grafik yang sama (saham, rentang, dan indikator sama) cukup diperbarui datanya. Candle pertama
  // (tanggal + penutupan) membedakan saham dan rentang; pindah saham atau rentang = grafik baru.
  const layoutKey = (data) => {
    const first = data.candles[0];
    return (first ? first.date + "@" + first.close : "") + "|" + data.series.map((s) => s.key).join(",");
  };

  // Pembaruan di tempat: zoom, posisi geser, dan tinggi panel tetap seperti yang diatur pengguna.
  function update(el, data) {
    const s = el.__series;
    s.price.setData(candleData(data.candles));
    s.volume.setData(volumeData(data.candles));
    for (const x of data.series) {
      const series = s.lines[x.key];
      if (!series) continue;
      const pts = points(data.candles, x.values);
      series.setData(x.key.startsWith("macd_hist") ? histData(pts) : pts);
    }
    el.__candles = data.candles;
    if (el.__showLast) el.__showLast();
  }

  function destroy(el) {
    if (el && el.__chart) {
      el.__chart.remove();
      el.__chart = null;
    }
    if (el && el.__legend) {
      el.__legend.remove();
      el.__legend = null;
    }
    if (el) {
      el.__key = el.__series = el.__candles = el.__showLast = null;
    }
  }

  const fmtPrice = (v) => v.toLocaleString("id-ID", { maximumFractionDigits: 2 });
  const fmtVolume = (v) => {
    const a = Math.abs(v);
    const [div, unit] = a >= 1e9 ? [1e9, " M"] : a >= 1e6 ? [1e6, " jt"] : a >= 1e3 ? [1e3, " rb"] : [1, ""];
    return (v / div).toLocaleString("id-ID", { maximumFractionDigits: 1 }) + unit;
  };
  const fmtDate = (iso) => {
    const [y, m, d] = iso.split("-").map(Number);
    return new Date(y, m - 1, d).toLocaleDateString("id-ID", { day: "numeric", month: "short", year: "numeric" });
  };

  // Legenda OHLC di pojok kiri atas, mengikuti candle yang disorot kursor (default: candle terakhir).
  function legend(el, chart, p) {
    el.style.position = "relative";
    const box = document.createElement("div");
    box.setAttribute("aria-live", "off");
    box.style.cssText =
      "position:absolute;top:6px;left:8px;z-index:3;pointer-events:none;display:flex;flex-wrap:wrap;gap:2px 12px;" +
      "font:11px 'IBM Plex Mono',ui-monospace,monospace;color:" + p.muted + ";padding:3px 6px;border-radius:4px;" +
      "background:color-mix(in srgb," + p.ink + " 70%,transparent);";
    el.appendChild(box);
    el.__legend = box;

    const show = (i) => {
      const candles = el.__candles;
      const c = candles[i];
      if (!c) return;
      const prev = i > 0 ? candles[i - 1].close : null;
      const change = prev ? (c.close / prev - 1) * 100 : null;
      const tone = change === null || change === 0 ? p.paper : change > 0 ? p.up : p.down;
      const field = (label, value, color) =>
        '<span>' + label + ' <b style="font-weight:500;color:' + color + '">' + value + "</b></span>";
      box.innerHTML =
        '<span style="color:' + p.paper + '">' + fmtDate(c.date) + "</span>" +
        field("O", fmtPrice(c.open), tone) +
        field("H", fmtPrice(c.high), tone) +
        field("L", fmtPrice(c.low), tone) +
        field("C", fmtPrice(c.close), tone) +
        (change === null ? "" : field("", (change > 0 ? "+" : "") + change.toLocaleString("id-ID", { maximumFractionDigits: 2 }) + "%", tone)) +
        field("Vol", fmtVolume(c.volume), p.paper);
    };
    const last = () => el.__candles.length - 1;
    el.__showLast = () => show(last());
    show(last());
    chart.subscribeCrosshairMove((param) => {
      const i = typeof param.logical === "number" ? Math.round(param.logical) : null;
      show(param.point && i !== null && i >= 0 && i <= last() ? i : last());
    });
  }

  function render(el, json) {
    const data = JSON.parse(json);
    if (el.__chart && el.__key === layoutKey(data)) {
      update(el, data);
      return;
    }
    destroy(el);
    const p = palette();
    const chart = LWC.createChart(el, {
      autoSize: true,
      layout: {
        background: { type: "solid", color: "transparent" },
        textColor: p.muted,
        fontFamily: "'IBM Plex Mono', ui-monospace, monospace",
        fontSize: 11,
        panes: { separatorColor: p.line, separatorHoverColor: p.brass, enableResize: true },
      },
      grid: { vertLines: { color: "rgba(36,48,74,0.45)" }, horzLines: { color: "rgba(36,48,74,0.45)" } },
      rightPriceScale: { borderColor: p.line },
      timeScale: { borderColor: p.line, rightOffset: 4 },
      crosshair: {
        mode: LWC.CrosshairMode.Normal,
        vertLine: { color: p.brass, labelBackgroundColor: p.brass, style: LWC.LineStyle.Dashed },
        horzLine: { color: p.brass, labelBackgroundColor: p.brass, style: LWC.LineStyle.Dashed },
      },
      localization: { locale: "id-ID", priceFormatter: (v) => v.toLocaleString("id-ID", { maximumFractionDigits: 2 }) },
    });
    el.__chart = chart;
    el.__key = layoutKey(data);
    el.__candles = data.candles;
    const lines = {};

    const candles = data.candles;
    const price = chart.addSeries(LWC.CandlestickSeries, {
      upColor: p.up, downColor: p.down, borderUpColor: p.up, borderDownColor: p.down,
      wickUpColor: p.up, wickDownColor: p.down,
    });
    price.setData(candleData(candles));

    const volume = chart.addSeries(LWC.HistogramSeries, {
      priceScaleId: "volume", priceFormat: { type: "volume" }, lastValueVisible: false, priceLineVisible: false,
    });
    volume.priceScale().applyOptions({ scaleMargins: { top: 0.82, bottom: 0 } });
    volume.setData(volumeData(candles));

    // Panel bawah dibuat sesuai urutan kemunculan indikatornya.
    const paneIndex = { price: 0 };
    let nextPane = 1;
    const indexFor = (name) => {
      if (!(name in paneIndex)) paneIndex[name] = nextPane++;
      return paneIndex[name];
    };

    for (const s of data.series) {
      const pane = paneOf(s.key);
      const idx = indexFor(pane);
      const pts = points(candles, s.values);
      const common = { lastValueVisible: pane !== "price", priceLineVisible: false, title: pane === "price" ? "" : s.label };
      let series;
      if (s.key.startsWith("macd_hist")) {
        series = chart.addSeries(LWC.HistogramSeries, common, idx);
        series.setData(histData(pts));
        lines[s.key] = series;
        continue;
      }
      if (s.key.startsWith("psar_")) {
        series = chart.addSeries(LWC.LineSeries, {
          ...common, color: p.brass, lineVisible: false, pointMarkersVisible: true, pointMarkersRadius: 1.5,
        }, idx);
      } else {
        const dashed = s.key.startsWith("bb_upper") || s.key.startsWith("bb_lower") || s.key.startsWith("ichimoku_senkou");
        series = chart.addSeries(LWC.LineSeries, {
          ...common, color: lineColor(s.key, p), lineWidth: s.key.startsWith("supertrend") ? 2 : 1.5,
          lineStyle: dashed ? LWC.LineStyle.Dashed : LWC.LineStyle.Solid,
        }, idx);
      }
      series.setData(pts);
      lines[s.key] = series;
      // Garis bantu: zona jenuh beli/jual dan garis nol aliran dana.
      const guides = { rsi: [70, 30], mfi: [80, 20], cmf: [0], rvol: [1] }[pane] || [];
      for (const level of guides) {
        series.createPriceLine({ price: level, color: p.line, lineStyle: LWC.LineStyle.Dotted, axisLabelVisible: true, title: "" });
      }
    }

    // Proporsi, bukan piksel: autoSize menata ulang tinggi setelah ukuran kontainer diketahui.
    const panes = chart.panes();
    panes[0].setStretchFactor(3);
    for (let i = 1; i < panes.length; i++) panes[i].setStretchFactor(1);
    chart.timeScale().fitContent();
    el.__series = { price, volume, lines };
    if (candles.length) legend(el, chart, p);
  }

  window.ScannerChart = { render, destroy };
})();
