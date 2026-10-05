//! The raw and unescaped string-table lookup contract.

use dereth_primitives::DataId;

/// What a text element needs from the host to turn a `StringInfo` into characters.
///
/// Text set or appended from a `StringInfo` (attribute 0x17)
/// is resolved through the string tables **at the moment it is set**, using the
/// current language. The asset source supplies the language tables, decoded by the asset crate;
/// callers provide the resolver that selects them.
///
/// # The escape pass is the contract's, not the implementor's
///
/// A shipped row is stored **escaped** — a line break is a literal backslash and an `n` — and
/// every retail path out of a string table ends in the meta-language's unescape: the string
/// info's query, its literal-value getter and the meta-language's render
/// alike. The unescape therefore belongs to the *lookup*, and an implementor that forgets it
/// composes a screen the client does not draw.
///
/// So the two methods an implementor writes are the **raw** ones, which hand back the row exactly
/// as the dat stores it, and the two a caller uses are provided here and run
/// [`dereth_assets::escape::unescape`] over the answer. **Do not override
/// [`StringResolver::resolve`] or [`StringResolver::resolve_variants`]**; overriding one is the
/// only way back to the defect.
pub trait StringResolver: std::fmt::Debug {
    /// `StringTable`'s `(table DataID, string id)` → variant 0, the singular/default form,
    /// **escaped** — byte for byte what the row holds in the dat.
    fn resolve_raw(&self, table: DataId, string_id: u32) -> Option<String>;

    /// Every variant of the same row, **escaped**. A row with substitutions stores the literal
    /// pieces *around* its variables, so rebuilding one needs the whole list.
    fn resolve_variants_raw(&self, table: DataId, string_id: u32) -> Option<Vec<String>> {
        self.resolve_raw(table, string_id).map(|s| vec![s])
    }

    /// The row's **own** variable list: one
    /// string-hash id per substitution, in the order the row substitutes them.
    ///
    /// # Why the fragments alone are not enough
    ///
    /// The string table's lookup never takes a value by position. Both its arms walk
    /// **this** list and ask the caller's table for each id:
    ///
    /// ```text
    /// both arms:
    ///   look up the row's variable id in the caller's id-keyed value table
    /// ```
    ///
    /// and the caller's `vars` is itself keyed by the same id —
    /// the string info's own internal query walks its variable table, filled by its add-variable
    /// call, and copies
    /// it key for key. So the caller's *order* is meaningless and the row's is everything: the
    /// two shipped rows `ID_ActionKeyMap_OverwriteExistingBinding` (`KEY, ACTION`) and
    /// `ID_ActionKeyMap_Binding` (`ACTION, KEY`) take the same two values the other way round,
    /// and a localised dat is free to reorder either.
    ///
    /// `None` means *this resolver cannot say*, not "the row has no variables": a fixture that
    /// holds only fragments answers `None`, and [`crate::render_named`] then
    /// falls back to the positional order the caller supplied. `Some(vec![])` is a row that
    /// really substitutes nothing.
    fn resolve_variables(&self, table: DataId, string_id: u32) -> Option<Vec<u32>> {
        let _ = (table, string_id);
        None
    }

    /// Variant 0 as a text element receives it, after.
    fn resolve(&self, table: DataId, string_id: u32) -> Option<String> {
        self.resolve_raw(table, string_id)
            .map(dereth_assets::escape::unescape)
    }

    /// Every variant as a text element receives them, after. Each
    /// piece is unescaped on its own, which is what the client does: the pieces are separate
    /// `StringInfo` literals and the substituted values are never rescanned.
    fn resolve_variants(&self, table: DataId, string_id: u32) -> Option<Vec<String>> {
        self.resolve_variants_raw(table, string_id)
            .map(|v| v.into_iter().map(dereth_assets::escape::unescape).collect())
    }
}
