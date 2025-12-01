/// Macro for efficiently syncing Copy types (e.g., booleans, primitives) from AppState.
/// Uses `std::mem::replace` to avoid unnecessary clones.
#[macro_export]
macro_rules! sync_field_copy {
    ($this:expr, $state:expr, $field:ident, $changed:expr) => {
        $changed |= std::mem::replace(&mut $this.$field, $state.$field) != $state.$field;
    };
}

/// Macro for syncing Clone types (e.g., Vec, String) from AppState.
/// Only clones when values differ to minimize allocations.
#[macro_export]
macro_rules! sync_field_clone {
    ($this:expr, $state:expr, $field:ident, $changed:expr) => {
        if $this.$field != $state.$field {
            $this.$field = $state.$field.clone();
            $changed = true;
        }
    };
}
