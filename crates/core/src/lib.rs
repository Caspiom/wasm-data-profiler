//! mirante-core — CSV profiling with no I/O and no platform assumptions.
//!
//! The engine takes the file as `&[u8]` and returns a [`Profile`]. It never
//! touches the filesystem, the clock, or threads, so the same code runs under
//! `cargo test` natively and inside `wasm32-unknown-unknown`.
//!
//! ```
//! let csv = b"nome;preco\nvassoura;12,50\nrodo;8,90";
//! let profile = mirante_core::profile_csv(csv).unwrap();
//! assert_eq!(profile.row_count, 2);
//! assert_eq!(profile.delimiter, ";");
//! ```

mod decode;
mod dialect;
mod number;
mod profile;
mod reader;
mod stats;
mod value;

pub use decode::Encoding;
pub use number::DecimalStyle;
pub use profile::{ColumnProfile, Profile, ProfileError, TypeCounts, profile_csv};
pub use stats::{HISTOGRAM_BINS, Histogram, NumericSummary, TextSummary, ValueCount};
pub use value::ColumnType;
