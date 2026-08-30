use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CardConversionError {
    #[error("card code {0:?} is not a valid NetrunnerDB numeric code")]
    InvalidCardCode(String),

    #[error("unknown card type_code {0:?}")]
    UnknownCardType(String),

    #[error("unknown faction_code {0:?}")]
    UnknownFaction(String),

    #[error("unknown side_code {0:?}")]
    UnknownSide(String),

    #[error("field {field:?} had a negative value {value}, expected non-negative")]
    NegativeValue { field: &'static str, value: i32 },

    #[error("ice keywords {0:?} don't start with a recognized ice subtype (Barrier/Code Gate/Sentry)")]
    UnrecognizedIceKeywords(String),
}
