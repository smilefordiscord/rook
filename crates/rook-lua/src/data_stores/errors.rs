//! The errors data store requests raise. Numbered ones read
//! `<code>: <message>`, with the message from creator-docs "Data store
//! error codes and limits" unless a comment says otherwise; the code's
//! prefix and the wording of the ones the docs don't give are Inference
//! (docs/behavior/scripting/data-stores.md#errors).

use crate::runtime::ctx::ScriptError;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum StoreError {
    #[error("101: Key name can't be empty.")]
    KeyEmpty,
    #[error("102: Key name exceeds the 50 character limit.")]
    KeyTooLong,
    /// The docs word it "Can't allow X in DataStore"; this is the wording
    /// scripts report seeing (Inference).
    #[error("103: {0} is not allowed in data stores.")]
    NotAllowed(String),
    /// The docs word it "Can't store X in DataStore"; this is the wording
    /// scripts report seeing (Inference).
    #[error(
        "104: Cannot store {0} in data store. Data stores can only accept valid UTF-8 characters."
    )]
    CantStore(String),
    #[error("105: Serialized value exceeds 4MB limit.")]
    TooLarge,
    #[error("106: MaxValue must be an integer.")]
    MaxNotInteger,
    #[error("106: MinValue must be an integer.")]
    MinNotInteger,
    #[error("106: PageSize must be within a predefined range.")]
    PageSize,
    #[error("107: MaxValue must be greater than or equal to MinValue.")]
    MinAboveMax,
    #[error("{}", dropped(*.code, .method))]
    Dropped {
        code: Option<u16>,
        method: &'static str,
    },
    #[error("511: Metadata attribute size exceeds 300 limit.")]
    MetadataTooLarge,
    #[error("512: UserID size exceeds 4 limit.")]
    TooManyUserIds,
    #[error("513: Attribute userId format is invalid.")]
    BadUserId,
    #[error("513: Attribute metadata format is invalid.")]
    BadMetadata,
    #[error("Timestamp must be positive and not more than ten minutes in the future.")]
    BadTimestamp,
    #[error("No pages to advance to.")]
    NoMorePages,
    #[error("DataStore can't be accessed from client")]
    Client,
    #[error("DataStore name can't be empty.")]
    NameEmpty,
    #[error("DataStore name can't be longer than 50 characters.")]
    NameTooLong,
    #[error("DataStore scope can't be empty.")]
    ScopeEmpty,
    #[error("DataStore scope can't be longer than 50 characters.")]
    ScopeTooLong,
    // Rook's wording below: the docs say these fail, not how.
    #[error("The scope must be empty when DataStoreOptions.AllScopes is true.")]
    ScopeWithAllScopes,
    #[error("Keys must start with their scope, as in \"scope/key\", when AllScopes is true.")]
    NoScopeInKey,
    #[error("BatchGetAsync is only supported on OrderedDataStore.")]
    BatchOnStandard,
    #[error("BatchGetAsync takes from 1 to 100 keys.")]
    BatchSize,
    #[error("IncrementAsync's result is outside the range of a 64-bit integer.")]
    IncrementOverflow,
    #[error("{0} can't be configured with SetRateLimitForRequestType.")]
    NotConfigurable(&'static str),
    #[error("{name}'s {argument} must be from 0 to {max}.")]
    LimitRange {
        name: &'static str,
        argument: &'static str,
        max: u32,
    },
}

/// A throttled request dropped because its queue was full: the methods
/// with a number say the queue was full, the others only that they were
/// throttled (creator-docs).
fn dropped(code: Option<u16>, method: &str) -> String {
    match code {
        Some(code) => {
            format!("{code}: {method} request dropped. Request was throttled but queue was full.")
        }
        None => format!("{method} request dropped. Request was throttled."),
    }
}

impl From<StoreError> for ScriptError {
    fn from(e: StoreError) -> Self {
        Self(e.to_string())
    }
}
