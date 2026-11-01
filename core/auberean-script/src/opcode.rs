//! The instruction set.
//!
//! An instruction is a 32-bit word, sometimes followed by operand words: the low 7 bits are the
//! operation, bit 7 selects the 64-bit ("long") form, bits 8-15 carry a tag for the pushes, and
//! bits 16-31 a 16-bit immediate (signed for slots, values and displacements, unsigned for
//! counts). The machine dispatches on the word's low byte through a 256-entry table.

/// The long form: values are 64-bit, two slots.
pub const LONG: u32 = 0x80;

/// Operation numbers (the word's low 7 bits).
pub mod op {
    pub const NOTHING: u8 = 0x01;
    pub const PUSH: u8 = 0x02;
    pub const PUSH_SMALL: u8 = 0x03;
    pub const POP: u8 = 0x04;
    pub const POP_MANY: u8 = 0x05;
    pub const NEW: u8 = 0x06;
    pub const NEW_NATIVE: u8 = 0x07;
    pub const NEW_STRING: u8 = 0x08;
    pub const ENTER: u8 = 0x09;
    pub const LEAVE: u8 = 0x0A;
    pub const CALL: u8 = 0x0B;
    pub const CALL_NATIVE: u8 = 0x0C;
    pub const ADD: u8 = 0x0D;
    pub const SUB: u8 = 0x0E;
    pub const MUL: u8 = 0x0F;
    pub const DIV: u8 = 0x10;
    pub const REM: u8 = 0x11;
    pub const NEG: u8 = 0x12;
    pub const INC: u8 = 0x13;
    pub const DEC: u8 = 0x14;
    pub const FADD: u8 = 0x15;
    pub const FSUB: u8 = 0x16;
    pub const FMUL: u8 = 0x17;
    pub const FDIV: u8 = 0x18;
    pub const FNEG: u8 = 0x19;
    pub const FINC: u8 = 0x1A;
    pub const FDEC: u8 = 0x1B;
    pub const INT_TO_FLOAT: u8 = 0x1C;
    pub const FLOAT_TO_INT: u8 = 0x1D;
    pub const WIDEN_INT: u8 = 0x1E;
    pub const WIDEN_FLOAT: u8 = 0x1F;
    pub const FLOAT_TO_TIME: u8 = 0x20;
    pub const LOAD: u8 = 0x22;
    pub const LOAD_THIS: u8 = 0x23;
    pub const STORE: u8 = 0x24;
    pub const FIELD_LOAD: u8 = 0x25;
    pub const FIELD_STORE: u8 = 0x26;
    pub const SLOT_REF: u8 = 0x27;
    pub const FIELD_REF: u8 = 0x28;
    pub const IF_ZERO: u8 = 0x29;
    pub const JUMP: u8 = 0x2A;
    pub const SWITCH: u8 = 0x2B;
    pub const NOT: u8 = 0x2C;
    pub const OR: u8 = 0x2D;
    pub const AND: u8 = 0x2E;
    pub const EQ: u8 = 0x2F;
    pub const NE: u8 = 0x30;
    pub const LT: u8 = 0x31;
    pub const GT: u8 = 0x32;
    pub const LE: u8 = 0x33;
    pub const GE: u8 = 0x34;
    pub const ULT: u8 = 0x35;
    pub const UGT: u8 = 0x36;
    pub const ULE: u8 = 0x37;
    pub const UGE: u8 = 0x38;
    pub const FNOT: u8 = 0x39;
    pub const FOR: u8 = 0x3A;
    pub const FAND: u8 = 0x3B;
    pub const FEQ: u8 = 0x3C;
    pub const FNE: u8 = 0x3D;
    pub const FLT: u8 = 0x3E;
    pub const FGT: u8 = 0x3F;
    pub const FLE: u8 = 0x40;
    pub const FGE: u8 = 0x41;
    pub const BIT_NOT: u8 = 0x42;
    pub const BIT_OR: u8 = 0x43;
    pub const BIT_XOR: u8 = 0x44;
    pub const BIT_AND: u8 = 0x45;
    pub const SHL: u8 = 0x46;
    pub const SHR: u8 = 0x47;
    pub const REF_EQ: u8 = 0x48;
    pub const REF_NE: u8 = 0x49;
    pub const END: u8 = 0x4A;
    pub const OVERFLOW: u8 = 0x4B;
    pub const DEAD: u8 = 0x4C;
    pub const STORE_POP_MANY: u8 = 0x4D;
    pub const THIS_CALL: u8 = 0x4F;
    pub const THIS_CALL_NATIVE: u8 = 0x50;
    pub const LOAD_CALL: u8 = 0x51;
    pub const LOAD_CALL_NATIVE: u8 = 0x52;
    pub const FIELD_LOAD_CALL: u8 = 0x53;
    pub const FIELD_LOAD_CALL_NATIVE: u8 = 0x54;
    pub const THIS_FIELD_LOAD: u8 = 0x55;
    pub const CALL_AT: u8 = 0x56;
    pub const WIDEN_UINT: u8 = 0x60;
    pub const USHR: u8 = 0x61;
    pub const UINT_TO_FLOAT: u8 = 0x62;
}

/// A short name for an operation, for listings and messages.
#[must_use]
pub fn name(op: u8) -> &'static str {
    use op::*;
    match op & 0x7F {
        NOTHING => "nothing",
        PUSH => "push",
        PUSH_SMALL => "push.small",
        POP => "pop",
        POP_MANY => "pop.many",
        NEW => "new",
        NEW_NATIVE => "new.native",
        NEW_STRING => "new.string",
        ENTER => "enter",
        LEAVE => "leave",
        CALL => "call",
        CALL_NATIVE => "call.native",
        ADD => "add",
        SUB => "sub",
        MUL => "mul",
        DIV => "div",
        REM => "rem",
        NEG => "neg",
        INC => "inc",
        DEC => "dec",
        FADD => "fadd",
        FSUB => "fsub",
        FMUL => "fmul",
        FDIV => "fdiv",
        FNEG => "fneg",
        FINC => "finc",
        FDEC => "fdec",
        INT_TO_FLOAT => "int.to.float",
        FLOAT_TO_INT => "float.to.int",
        WIDEN_INT => "widen.int",
        WIDEN_FLOAT => "widen.float",
        FLOAT_TO_TIME => "float.to.time",
        LOAD => "load",
        LOAD_THIS => "load.this",
        STORE => "store",
        FIELD_LOAD => "field.load",
        FIELD_STORE => "field.store",
        SLOT_REF => "slot.ref",
        FIELD_REF => "field.ref",
        IF_ZERO => "if.zero",
        JUMP => "jump",
        SWITCH => "switch",
        NOT => "not",
        OR => "or",
        AND => "and",
        EQ => "eq",
        NE => "ne",
        LT => "lt",
        GT => "gt",
        LE => "le",
        GE => "ge",
        ULT => "ult",
        UGT => "ugt",
        ULE => "ule",
        UGE => "uge",
        FNOT => "fnot",
        FOR => "for",
        FAND => "fand",
        FEQ => "feq",
        FNE => "fne",
        FLT => "flt",
        FGT => "fgt",
        FLE => "fle",
        FGE => "fge",
        BIT_NOT => "bit.not",
        BIT_OR => "bit.or",
        BIT_XOR => "bit.xor",
        BIT_AND => "bit.and",
        SHL => "shl",
        SHR => "shr",
        REF_EQ => "ref.eq",
        REF_NE => "ref.ne",
        END => "end",
        OVERFLOW => "overflow",
        DEAD => "dead",
        STORE_POP_MANY => "store.pop.many",
        THIS_CALL => "this.call",
        THIS_CALL_NATIVE => "this.call.native",
        LOAD_CALL => "load.call",
        LOAD_CALL_NATIVE => "load.call.native",
        FIELD_LOAD_CALL => "field.load.call",
        FIELD_LOAD_CALL_NATIVE => "field.load.call.native",
        THIS_FIELD_LOAD => "this.field.load",
        CALL_AT => "call.at",
        WIDEN_UINT => "widen.uint",
        USHR => "ushr",
        UINT_TO_FLOAT => "uint.to.float",
        _ => "?",
    }
}

/// The word's operation byte (the dispatch index).
#[must_use]
pub const fn low_byte(word: u32) -> u8 {
    (word & 0xFF) as u8
}

/// The tag a push carries.
#[must_use]
pub const fn tag_of(word: u32) -> u8 {
    ((word >> 8) & 0xFF) as u8
}

/// The immediate, signed.
#[must_use]
pub const fn signed(word: u32) -> i32 {
    (word >> 16) as u16 as i16 as i32
}

/// The immediate, unsigned.
#[must_use]
pub const fn unsigned(word: u32) -> u32 {
    word >> 16
}

/// The length in words of the instruction at `at`, as the loader steps through the stream: one
/// operand word for the operations that read one, two for the long push, a switch's cases, and
/// one word for everything else. `None` when a switch's case count runs past the end.
///
/// The long forms of the operand-reading operations other than the push are sized at one word,
/// as the loader sizes them, although they would read an operand; none occurs in the library.
// D2-16626
#[must_use]
pub fn length(code: &[u32], at: usize) -> Option<usize> {
    let word = *code.get(at)?;
    Some(match low_byte(word) {
        0x02 | 0x06 | 0x07 | 0x08 | 0x0B | 0x0C | 0x4F..=0x54 | 0x56 => 2,
        0x82 => 3,
        0x2B => {
            let n = *code.get(at + 2)? as usize;
            n.checked_mul(2)?.checked_add(3)?
        }
        _ => 1,
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (instruction lengths on hand-built words)
    use super::*;

    #[test]
    fn a_switch_is_three_words_and_two_per_case() {
        let code = [0x2B, 0x10, 2, 1, 4, 2, 8];
        assert_eq!(length(&code, 0), Some(7));
        assert_eq!(length(&[0x2B, 0x10], 0), None);
    }

    #[test]
    fn only_the_long_push_reads_two_operand_words() {
        assert_eq!(length(&[0x82, 1, 2], 0), Some(3));
        assert_eq!(length(&[0x86], 0), Some(1));
        assert_eq!(length(&[0x0C, 9], 0), Some(2));
        assert_eq!(length(&[0x0D], 0), Some(1));
    }
}
