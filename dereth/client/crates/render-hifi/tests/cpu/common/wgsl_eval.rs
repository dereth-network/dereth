//! Behaviour: none (a helper: runs a straight-line shader function on the CPU, from its parsed
//! form, so a test can check what the shader computes against the code it must agree with).
//!
//! It knows only what the functions it is used on need: literals, arguments, component access,
//! arithmetic, comparison, bit shifts and casts on scalars and small vectors, `if` and `return`.
//! Anything else panics, naming what it met.

use naga::{BinaryOperator, Expression, Handle, Literal, ScalarKind, Statement};

/// A value the evaluated function computes with.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    F32(f32),
    I32(i32),
    U32(u32),
    Bool(bool),
    Vector(Vec<Value>),
}

impl Value {
    pub fn f32(&self) -> f32 {
        match self {
            Self::F32(v) => *v,
            other => panic!("not an f32: {other:?}"),
        }
    }

    pub fn bool(&self) -> bool {
        match self {
            Self::Bool(v) => *v,
            other => panic!("not a bool: {other:?}"),
        }
    }
}

/// A parsed shader module and the function to run.
pub struct Evaluator {
    module: naga::Module,
}

impl Evaluator {
    pub fn new(source: &str) -> Self {
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
        Self { module }
    }

    /// Run function `name` with `args`, and its result.
    pub fn call(&self, name: &str, args: &[Value]) -> Value {
        let (_, f) = self
            .module
            .functions
            .iter()
            .find(|(_, f)| f.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("no function {name}"));
        let mut run = Run {
            f,
            args,
            cache: vec![None; f.expressions.len()],
        };
        run.block(&f.body)
            .unwrap_or_else(|| panic!("{name} returned nothing"))
    }
}

struct Run<'a> {
    f: &'a naga::Function,
    args: &'a [Value],
    cache: Vec<Option<Value>>,
}

fn binary(op: BinaryOperator, a: Value, b: Value) -> Value {
    use BinaryOperator as B;
    match (a, b) {
        (Value::Vector(a), Value::Vector(b)) => Value::Vector(
            a.into_iter()
                .zip(b)
                .map(|(a, b)| binary(op, a, b))
                .collect(),
        ),
        (Value::Vector(a), b) => {
            Value::Vector(a.into_iter().map(|a| binary(op, a, b.clone())).collect())
        }
        (a, Value::Vector(b)) => {
            Value::Vector(b.into_iter().map(|b| binary(op, a.clone(), b)).collect())
        }
        (Value::F32(a), Value::F32(b)) => match op {
            B::Add => Value::F32(a + b),
            B::Subtract => Value::F32(a - b),
            B::Multiply => Value::F32(a * b),
            B::Divide => Value::F32(a / b),
            B::Less => Value::Bool(a < b),
            B::LessEqual => Value::Bool(a <= b),
            B::Greater => Value::Bool(a > b),
            B::GreaterEqual => Value::Bool(a >= b),
            B::Equal => Value::Bool(a == b),
            B::NotEqual => Value::Bool(a != b),
            other => panic!("f32 {other:?}"),
        },
        (Value::U32(a), Value::U32(b)) => match op {
            B::Add => Value::U32(a.wrapping_add(b)),
            B::Subtract => Value::U32(a.wrapping_sub(b)),
            B::Multiply => Value::U32(a.wrapping_mul(b)),
            B::ShiftRight => Value::U32(a >> (b & 31)),
            B::ShiftLeft => Value::U32(a << (b & 31)),
            B::And => Value::U32(a & b),
            B::Equal => Value::Bool(a == b),
            B::NotEqual => Value::Bool(a != b),
            other => panic!("u32 {other:?}"),
        },
        (Value::I32(a), Value::I32(b)) => match op {
            B::Add => Value::I32(a.wrapping_add(b)),
            B::Subtract => Value::I32(a.wrapping_sub(b)),
            B::Multiply => Value::I32(a.wrapping_mul(b)),
            B::Equal => Value::Bool(a == b),
            B::NotEqual => Value::Bool(a != b),
            other => panic!("i32 {other:?}"),
        },
        (a, b) => panic!("{op:?} on {a:?} and {b:?}"),
    }
}

fn cast(v: Value, kind: ScalarKind, convert: Option<u8>) -> Value {
    match (v, kind, convert) {
        (Value::Vector(v), ..) => {
            Value::Vector(v.into_iter().map(|v| cast(v, kind, convert)).collect())
        }
        (Value::I32(v), ScalarKind::Uint, None) => Value::U32(v.cast_unsigned()),
        (Value::U32(v), ScalarKind::Sint, None) => Value::I32(v.cast_signed()),
        #[allow(clippy::cast_precision_loss)]
        (Value::I32(v), ScalarKind::Float, Some(4)) => Value::F32(v as f32),
        (v, k, c) => panic!("cast of {v:?} to {k:?} {c:?}"),
    }
}

impl Run<'_> {
    fn block(&mut self, block: &naga::Block) -> Option<Value> {
        for s in block.iter() {
            match s {
                Statement::Emit(_) => {}
                Statement::Block(b) => {
                    if let Some(v) = self.block(b) {
                        return Some(v);
                    }
                }
                Statement::If {
                    condition,
                    accept,
                    reject,
                } => {
                    let taken = if self.eval(*condition).bool() {
                        accept
                    } else {
                        reject
                    };
                    if let Some(v) = self.block(taken) {
                        return Some(v);
                    }
                }
                Statement::Return { value } => {
                    return Some(self.eval(value.expect("a value")));
                }
                other => panic!("statement {other:?}"),
            }
        }
        None
    }

    fn eval(&mut self, h: Handle<Expression>) -> Value {
        if let Some(v) = &self.cache[h.index()] {
            return v.clone();
        }
        let v = match &self.f.expressions[h] {
            Expression::Literal(l) => match *l {
                Literal::F32(v) => Value::F32(v),
                Literal::U32(v) => Value::U32(v),
                Literal::I32(v) => Value::I32(v),
                Literal::Bool(v) => Value::Bool(v),
                other => panic!("literal {other:?}"),
            },
            Expression::FunctionArgument(i) => self.args[*i as usize].clone(),
            Expression::AccessIndex { base, index } => match self.eval(*base) {
                Value::Vector(v) => v[*index as usize].clone(),
                other => panic!("index into {other:?}"),
            },
            Expression::Binary { op, left, right } => {
                let (a, b) = (self.eval(*left), self.eval(*right));
                binary(*op, a, b)
            }
            Expression::As {
                expr,
                kind,
                convert,
            } => {
                let v = self.eval(*expr);
                cast(v, *kind, *convert)
            }
            other => panic!("expression {other:?}"),
        };
        self.cache[h.index()] = Some(v.clone());
        v
    }
}
