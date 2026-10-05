//! Shared declarations for messages with identical body layouts.

macro_rules! empty_message {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name;
        impl $crate::Message for $name {
            const OPCODE: $crate::opcodes::Opcode = $crate::opcodes::Opcode::$op;
            fn read(_: &mut $crate::archive::Reader<'_>) -> Result<Self, $crate::error::MessageError> { Ok(Self) }
            fn write(&self, _: &mut $crate::archive::Writer) -> Result<(), $crate::error::MessageError> { Ok(()) }
        }
    };
}

macro_rules! id_message {
    ($(#[$m:meta])* $name:ident, $op:ident, $field:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name { pub $field: dereth_primitives::ObjectId }
        impl $crate::Message for $name {
            const OPCODE: $crate::opcodes::Opcode = $crate::opcodes::Opcode::$op;
            fn read(r: &mut $crate::archive::Reader<'_>) -> Result<Self, $crate::error::MessageError> {
                Ok(Self { $field: dereth_primitives::ObjectId(r.u32()?) })
            }
            fn write(&self, w: &mut $crate::archive::Writer) -> Result<(), $crate::error::MessageError> {
                w.u32(self.$field.0);
                Ok(())
            }
        }
    };
}

macro_rules! scalar_message {
    ($(#[$m:meta])* $name:ident, $op:ident, $field:ident, $ty:ty, $rd:ident, $wr:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name { pub $field: $ty }
        impl $crate::Message for $name {
            const OPCODE: $crate::opcodes::Opcode = $crate::opcodes::Opcode::$op;
            fn read(r: &mut $crate::archive::Reader<'_>) -> Result<Self, $crate::error::MessageError> {
                Ok(Self { $field: r.$rd()? })
            }
            fn write(&self, w: &mut $crate::archive::Writer) -> Result<(), $crate::error::MessageError> {
                w.$wr(self.$field);
                Ok(())
            }
        }
    };
}

macro_rules! string_message {
    ($(#[$m:meta])* $name:ident, $op:ident, $field:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Default)]
        pub struct $name { pub $field: String }
        impl $crate::Message for $name {
            const OPCODE: $crate::opcodes::Opcode = $crate::opcodes::Opcode::$op;
            fn read(r: &mut $crate::archive::Reader<'_>) -> Result<Self, $crate::error::MessageError> {
                Ok(Self { $field: r.pstring()? })
            }
            fn write(&self, w: &mut $crate::archive::Writer) -> Result<(), $crate::error::MessageError> {
                w.pstring(&self.$field)
            }
        }
    };
}
