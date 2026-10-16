use polars::prelude::PolarsError;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Input dari pengguna tidak valid; pesannya aman untuk ditampilkan.
    #[error("{0}")]
    Invalid(String),
    #[error("gagal menghitung data: {0}")]
    Compute(#[from] PolarsError),
}
