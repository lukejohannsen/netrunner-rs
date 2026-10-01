use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CardConversionError {
    #[error("printing id {0:?} is not a NetrunnerDB printing code")]
    InvalidPrintingId(String),

    #[error("unknown card_type_id {0:?}")]
    UnknownCardType(String),

    #[error("unknown faction_id {0:?}")]
    UnknownFaction(String),

    #[error("unknown side_id {0:?}")]
    UnknownSide(String),

    #[error("field {field:?} had the value {value:?}, expected a non-negative number")]
    NotANumber { field: &'static str, value: String },
}
