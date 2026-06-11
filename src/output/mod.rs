pub mod pager;

mod prompt;
mod status;
mod style;
mod table;

pub use prompt::{assume_yes, confirm_or_cancel, set_assume_yes};
pub(crate) use status::error_summary_with_limit;
pub use status::{
    Status, error_summary, status_cell, status_label, status_line, status_line_text, summary_line,
};
pub use style::{
    action_note, divider, kv, meta, section, success, title, truncate_end, truncate_middle, warning,
};
pub use table::{
    SizeImpactRow, TransactionRow, TransactionTableLayout, print_disk_impact,
    print_disk_impact_with_size_rows, print_transaction_table,
    print_transaction_table_with_size_rows, print_transaction_table_without_size,
};
