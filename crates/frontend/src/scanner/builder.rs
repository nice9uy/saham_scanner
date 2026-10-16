//! Penyusun kondisi scan. Setiap kondisi dibaca seperti kalimat:
//! "[SMA 50] [memotong ke atas] [SMA 200] dalam [3] hari".

use leptos::prelude::*;
use shared::{Comparator, Condition, Indicator, Operand, OperandSource};

use super::catalog::{self, GROUPS, KINDS};

const CONSTANT: &str = "__angka";

/// Satu baris kondisi; `id` stabil supaya baris tidak dirender ulang saat diedit.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: u32,
    pub cond: Condition,
}

pub fn default_condition() -> Condition {
    Condition::new(
        Operand::indicator(Indicator::Close),
        Comparator::Gt,
        Operand::indicator(Indicator::Sma { period: 20 }),
    )
}

fn display(stored: u16, scale: u16) -> String {
    match scale {
        1 => stored.to_string(),
        10 => format!("{:.1}", f64::from(stored) / 10.0),
        _ => format!("{:.3}", f64::from(stored) / f64::from(scale)),
    }
}

fn parse_param(input: &str, scale: u16) -> Option<u16> {
    let v: f64 = input.trim().replace(',', ".").parse().ok()?;
    let stored = (v * f64::from(scale)).round();
    (stored >= 0.0 && stored <= f64::from(u16::MAX)).then_some(stored as u16)
}

fn number_text(v: f64) -> String {
    if v.fract() == 0.0 { format!("{v:.0}") } else { v.to_string() }
}

/// Pemilih indikator/angka beserta parameternya.
#[component]
pub fn OperandEditor(
    #[prop(into)] value: Signal<Operand>,
    on_change: Callback<Operand>,
    #[prop(default = true)] allow_constant: bool,
) -> impl IntoView {
    // Hanya berubah saat jenisnya berganti, supaya input parameter tidak dibuat ulang ketika diketik.
    let kind_id = Memo::new(move |_| match &value.get().source {
        OperandSource::Constant(_) => CONSTANT.to_string(),
        OperandSource::Indicator(i) => catalog::decompose(i).map(|(id, _)| id.to_string()).unwrap_or_default(),
    });
    let params = move || match &value.get().source {
        OperandSource::Indicator(i) => catalog::decompose(i).map(|(_, p)| p).unwrap_or_default(),
        OperandSource::Constant(_) => vec![],
    };

    let set_kind = move |id: String| {
        let mut op = value.get_untracked();
        op.source = if id == CONSTANT {
            OperandSource::Constant(30.0)
        } else {
            match catalog::build(&id, &catalog::defaults(&id)) {
                Some(ind) => OperandSource::Indicator(ind),
                None => return,
            }
        };
        on_change.run(op);
    };

    let set_param = move |idx: usize, stored: u16| {
        let mut op = value.get_untracked();
        let id = kind_id.get_untracked();
        let mut p = params();
        if let Some(slot) = p.get_mut(idx) {
            *slot = stored;
        }
        if let Some(ind) = catalog::build(&id, &p) {
            op.source = OperandSource::Indicator(ind);
            on_change.run(op);
        }
    };

    view! {
        <span class="inline-flex flex-wrap items-center gap-1.5">
            <select
                aria-label="Indikator"
                class="max-w-[12rem] font-medium"
                prop:value=move || kind_id.get()
                on:change=move |ev| set_kind(event_target_value(&ev))
            >
                {(kind_id.get_untracked().is_empty())
                    .then(|| view! { <option value="">{value.get_untracked().source_label()}</option> })}
                {GROUPS
                    .iter()
                    .map(|g| {
                        view! {
                            <optgroup label=*g>
                                {KINDS
                                    .iter()
                                    .filter(|k| k.group == *g)
                                    .map(|k| view! { <option value=k.id>{k.label}</option> })
                                    .collect_view()}
                            </optgroup>
                        }
                    })
                    .collect_view()}
                {allow_constant.then(|| view! { <option value=CONSTANT>"Angka tetap"</option> })}
            </select>
            {move || {
                let id = kind_id.get();
                if id == CONSTANT {
                    return view! {
                        <input
                            type="text"
                            inputmode="decimal"
                            aria-label="Nilai"
                            class="num w-24"
                            prop:value=move || match value.get().source {
                                OperandSource::Constant(c) => number_text(c),
                                _ => String::new(),
                            }
                            on:change=move |ev| {
                                if let Ok(v) = event_target_value(&ev).trim().replace(',', ".").parse::<f64>() {
                                    let mut op = value.get_untracked();
                                    op.source = OperandSource::Constant(v);
                                    on_change.run(op);
                                }
                            }
                        />
                    }
                        .into_any();
                }
                let Some(kind) = catalog::kind(&id) else { return ().into_any() };
                kind.params
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let scale = p.scale;
                        view! {
                            <label class="inline-flex items-center gap-1 text-xs text-muted">
                                {p.name}
                                <input
                                    type="text"
                                    inputmode="decimal"
                                    class="num w-14 text-center"
                                    prop:value=move || params().get(i).map(|v| display(*v, scale)).unwrap_or_default()
                                    on:change=move |ev| {
                                        if let Some(v) = parse_param(&event_target_value(&ev), scale) {
                                            set_param(i, v);
                                        }
                                    }
                                />
                            </label>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
            <Show when=move || matches!(value.get().source, OperandSource::Indicator(_))>
                <label class="inline-flex items-center gap-1 text-xs text-muted" title="Pengali, mis. 2 × rata-rata volume">
                    "×"
                    <input
                        type="text"
                        inputmode="decimal"
                        class="num w-12 text-center"
                        prop:value=move || number_text(value.get().multiplier)
                        on:change=move |ev| {
                            if let Ok(m) = event_target_value(&ev).trim().replace(',', ".").parse::<f64>() {
                                let mut op = value.get_untracked();
                                op.multiplier = m;
                                on_change.run(op);
                            }
                        }
                    />
                </label>
                <select
                    aria-label="Hari"
                    class="text-xs"
                    prop:value=move || value.get().offset.to_string()
                    on:change=move |ev| {
                        let mut op = value.get_untracked();
                        op.offset = event_target_value(&ev).parse().unwrap_or(0);
                        on_change.run(op);
                    }
                >
                    <option value="0">"hari ini"</option>
                    {(1..=10u16).map(|n| view! { <option value=n.to_string()>{format!("{n} hari lalu")}</option> }).collect_view()}
                </select>
            </Show>
        </span>
    }
}

trait SourceLabel {
    fn source_label(&self) -> String;
}

impl SourceLabel for Operand {
    fn source_label(&self) -> String {
        match &self.source {
            OperandSource::Indicator(i) => i.label(),
            OperandSource::Constant(c) => number_text(*c),
        }
    }
}

fn cmp_id(c: Comparator) -> &'static str {
    match c {
        Comparator::Gt => "gt",
        Comparator::Gte => "gte",
        Comparator::Lt => "lt",
        Comparator::Lte => "lte",
        Comparator::CrossAbove { .. } => "cross_above",
        Comparator::CrossBelow { .. } => "cross_below",
    }
}

fn cmp_from(id: &str, within: u8) -> Comparator {
    match id {
        "gte" => Comparator::Gte,
        "lt" => Comparator::Lt,
        "lte" => Comparator::Lte,
        "cross_above" => Comparator::CrossAbove { within },
        "cross_below" => Comparator::CrossBelow { within },
        _ => Comparator::Gt,
    }
}

fn within_of(c: Comparator) -> Option<u8> {
    match c {
        Comparator::CrossAbove { within } | Comparator::CrossBelow { within } => Some(within),
        _ => None,
    }
}

#[component]
pub fn ConditionRow(id: u32, index: Signal<usize>, rows: RwSignal<Vec<Row>>) -> impl IntoView {
    let cond = Signal::derive(move || {
        rows.with(|v| v.iter().find(|r| r.id == id).map(|r| r.cond.clone())).unwrap_or_else(default_condition)
    });
    let edit = move |f: &dyn Fn(&mut Condition)| {
        rows.update(|v| {
            if let Some(r) = v.iter_mut().find(|r| r.id == id) {
                f(&mut r.cond);
            }
        })
    };
    let on_left = Callback::new(move |op: Operand| edit(&|c| c.left = op.clone()));
    let on_right = Callback::new(move |op: Operand| edit(&|c| c.right = op.clone()));
    let left = Signal::derive(move || cond.get().left);
    let right = Signal::derive(move || cond.get().right);

    view! {
        <li class="group relative rounded-lg border border-line bg-ink/60 p-3">
            <div class="mb-2 flex items-center justify-between">
                <span class="eyebrow">{move || format!("Kondisi {}", index.get() + 1)}</span>
                <button
                    class="rounded px-1.5 text-muted hover:text-down"
                    aria-label="Hapus kondisi"
                    on:click=move |_| rows.update(|v| v.retain(|r| r.id != id))
                >
                    "✕"
                </button>
            </div>
            <div class="flex flex-wrap items-center gap-x-2 gap-y-2 text-sm">
                <OperandEditor value=left on_change=on_left allow_constant=false />
                <select
                    aria-label="Perbandingan"
                    class="border-brass/50 text-brass"
                    prop:value=move || cmp_id(cond.get().cmp)
                    on:change=move |ev| {
                        let v = event_target_value(&ev);
                        edit(&|c| {
                            let within = within_of(c.cmp).unwrap_or(1);
                            c.cmp = cmp_from(&v, within);
                        })
                    }
                >
                    <option value="gt">"di atas"</option>
                    <option value="gte">"di atas atau sama dengan"</option>
                    <option value="lt">"di bawah"</option>
                    <option value="lte">"di bawah atau sama dengan"</option>
                    <option value="cross_above">"memotong ke atas"</option>
                    <option value="cross_below">"memotong ke bawah"</option>
                </select>
                <OperandEditor value=right on_change=on_right />
                <Show when=move || within_of(cond.get().cmp).is_some()>
                    <label class="inline-flex items-center gap-1.5 text-sm text-muted">
                        "dalam"
                        <input
                            type="number"
                            min="1"
                            max="20"
                            class="num w-14 text-center"
                            prop:value=move || within_of(cond.get().cmp).unwrap_or(1).to_string()
                            on:change=move |ev| {
                                let n: u8 = event_target_value(&ev).parse().unwrap_or(1).clamp(1, 20);
                                edit(&|c| c.cmp = cmp_from(cmp_id(c.cmp), n));
                            }
                        />
                        "hari terakhir"
                    </label>
                </Show>
            </div>
        </li>
    }
}
