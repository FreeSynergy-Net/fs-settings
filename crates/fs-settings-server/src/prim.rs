//! The 1:1 maps between the port's values and the rows' values (the rows know nothing of fs-settings).
//! Pattern: Adapter (1:1 value maps).

use crate::ports::{Place, PlaceRow, Prim, Raw, Scope, ScopeRow};

/// `Raw` -> stored value.
#[must_use]
pub fn to_prim(raw: &Raw) -> Prim {
    match raw {
        Raw::Bool(b) => Prim::Bool(*b),
        Raw::Int(n) => Prim::Int(*n),
        Raw::Str(s) => Prim::Text(s.clone()),
    }
}

/// Stored value -> `Raw`.
#[must_use]
pub fn to_raw(prim: &Prim) -> Raw {
    match prim {
        Prim::Bool(b) => Raw::Bool(*b),
        Prim::Int(n) => Raw::Int(*n),
        Prim::Text(s) => Raw::Str(s.clone()),
    }
}

/// `Scope` -> row scope.
#[must_use]
pub fn scope_row(scope: &Scope) -> ScopeRow<'_> {
    match scope {
        Scope::General => ScopeRow::General,
        Scope::Program(p) => ScopeRow::Program(p.as_str()),
    }
}

/// `Place` -> row place.
#[must_use]
pub fn place_row(place: Place) -> PlaceRow {
    match place {
        Place::Profile => PlaceRow::Profile,
        Place::Device => PlaceRow::Device,
    }
}
