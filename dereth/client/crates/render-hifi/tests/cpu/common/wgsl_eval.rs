//! Behaviour: none (a helper: runs a straight-line shader function on the CPU, from its parsed
//! form, so a test can check what the shader computes against the code it must agree with).
//!
//! It knows only what the functions it is used on need: literals, constants, arguments, component
//! access and swizzles, vectors built and splatted, arithmetic, comparison, bitwise operations and
//! casts on scalars and small vectors, the common built-in functions, `select`, calls to the
//! module's other functions, `if` and `return`. Anything else panics, naming what it met.

use dereth_primitives::num::math;
use naga::{BinaryOperator, Expression, Handle, Literal, MathFunction, ScalarKind, Statement};

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

    /// The components of a vector of f32s.
    pub fn floats(&self) -> Vec<f32> {
        match self {
            Self::Vector(v) => v.iter().map(Self::f32).collect(),
            other => panic!("not a vector: {other:?}"),
        }
    }

    /// A vector of f32s.
    pub fn vector(v: &[f32]) -> Self {
        Self::Vector(v.iter().map(|f| Self::F32(*f)).collect())
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
        let (handle, _) = self
            .module
            .functions
            .iter()
            .find(|(_, f)| f.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("no function {name}"));
        run(&self.module, handle, args)
    }
}

/// Run the module's function `handle` with `args`.
fn run(module: &naga::Module, handle: Handle<naga::Function>, args: &[Value]) -> Value {
    let f = &module.functions[handle];
    let mut run = Run {
        module,
        f,
        args,
        cache: vec![None; f.expressions.len()],
    };
    run.block(&f.body).unwrap_or_else(|| {
        panic!(
            "{} returned nothing",
            f.name.as_deref().unwrap_or("a function")
        )
    })
}

struct Run<'a> {
    module: &'a naga::Module,
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
            B::InclusiveOr => Value::U32(a | b),
            B::ExclusiveOr => Value::U32(a ^ b),
            B::Equal => Value::Bool(a == b),
            B::NotEqual => Value::Bool(a != b),
            B::Less => Value::Bool(a < b),
            B::GreaterEqual => Value::Bool(a >= b),
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
        (Value::Bool(a), Value::Bool(b)) => match op {
            B::LogicalAnd | B::And => Value::Bool(a && b),
            B::LogicalOr | B::InclusiveOr => Value::Bool(a || b),
            B::Equal => Value::Bool(a == b),
            B::NotEqual => Value::Bool(a != b),
            other => panic!("bool {other:?}"),
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
        #[allow(clippy::cast_precision_loss)]
        (Value::U32(v), ScalarKind::Float, Some(4)) => Value::F32(v as f32),
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        (Value::F32(v), ScalarKind::Uint, Some(4)) => Value::U32(v as u32),
        #[allow(clippy::cast_possible_truncation)]
        (Value::F32(v), ScalarKind::Sint, Some(4)) => Value::I32(v as i32),
        (v, k, c) => panic!("cast of {v:?} to {k:?} {c:?}"),
    }
}

/// `f` applied to each float of `v`, a scalar or a vector.
fn each(v: &Value, f: &impl Fn(f32) -> f32) -> Value {
    match v {
        Value::F32(x) => Value::F32(f(*x)),
        Value::Vector(c) => Value::Vector(c.iter().map(|x| each(x, f)).collect()),
        other => panic!("not a float: {other:?}"),
    }
}

/// `f` applied to the floats of `a`, `b` and `c` side by side, a scalar spread over a vector.
fn each3(a: &Value, b: &Value, c: &Value, f: &impl Fn(f32, f32, f32) -> f32) -> Value {
    let width = [a, b, c]
        .iter()
        .find_map(|v| match v {
            Value::Vector(c) => Some(c.len()),
            _ => None,
        })
        .unwrap_or(0);
    let at = |v: &Value, i: usize| match v {
        Value::Vector(c) => c[i].f32(),
        other => other.f32(),
    };
    if width == 0 {
        Value::F32(f(a.f32(), b.f32(), c.f32()))
    } else {
        Value::Vector(
            (0..width)
                .map(|i| Value::F32(f(at(a, i), at(b, i), at(c, i))))
                .collect(),
        )
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn math(fun: MathFunction, a: &Value, b: Option<&Value>, c: Option<&Value>) -> Value {
    use MathFunction as M;
    let b = || b.unwrap_or_else(|| panic!("{fun:?} takes two arguments"));
    let c = || c.unwrap_or_else(|| panic!("{fun:?} takes three arguments"));
    let zero = Value::F32(0.0);
    match fun {
        M::Abs => each(a, &f32::abs),
        M::Floor => each(a, &f32::floor),
        M::Ceil => each(a, &f32::ceil),
        M::Round => each(a, &f32::round_ties_even),
        M::Fract => each(a, &|x| x - x.floor()),
        M::Sqrt => each(a, &f32::sqrt),
        M::Sin => each(a, &math::sinf),
        M::Cos => each(a, &math::cosf),
        M::Exp => each(a, &math::expf),
        M::Min => each3(a, b(), &zero, &|x, y, _| x.min(y)),
        M::Max => each3(a, b(), &zero, &|x, y, _| x.max(y)),
        M::Pow => each3(a, b(), &zero, &|x, y, _| math::powf(x, y)),
        M::Step => each3(a, b(), &zero, &|e, x, _| if x < e { 0.0 } else { 1.0 }),
        M::Clamp => each3(a, b(), c(), &|x, lo, hi| x.max(lo).min(hi)),
        M::Mix => each3(a, b(), c(), &|x, y, t| x + (y - x) * t),
        M::SmoothStep => each3(a, b(), c(), &smoothstep),
        M::Dot => {
            let (x, y) = (a.floats(), b().floats());
            Value::F32(x.iter().zip(&y).map(|(x, y)| x * y).sum())
        }
        M::Length => Value::F32(a.floats().iter().map(|x| x * x).sum::<f32>().sqrt()),
        other => panic!("built-in {other:?}"),
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
                Statement::Call {
                    function,
                    arguments,
                    result,
                } => {
                    let args: Vec<Value> = arguments.iter().map(|a| self.eval(*a)).collect();
                    let value = run(self.module, *function, &args);
                    if let Some(r) = result {
                        self.cache[r.index()] = Some(value);
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

    /// A constant's value, from the module's own expressions.
    fn global(&self, h: Handle<Expression>) -> Value {
        match &self.module.global_expressions[h] {
            Expression::Literal(l) => literal(*l),
            Expression::Compose { components, .. } => Value::Vector(
                components
                    .iter()
                    .flat_map(|c| flatten(self.global(*c)))
                    .collect(),
            ),
            other => panic!("constant {other:?}"),
        }
    }

    fn eval(&mut self, h: Handle<Expression>) -> Value {
        if let Some(v) = &self.cache[h.index()] {
            return v.clone();
        }
        let v = match &self.f.expressions[h] {
            Expression::Literal(l) => literal(*l),
            Expression::Constant(c) => self.global(self.module.constants[*c].init),
            Expression::FunctionArgument(i) => self.args[*i as usize].clone(),
            Expression::AccessIndex { base, index } => match self.eval(*base) {
                Value::Vector(v) => v[*index as usize].clone(),
                other => panic!("index into {other:?}"),
            },
            Expression::Swizzle {
                size,
                vector,
                pattern,
            } => match self.eval(*vector) {
                Value::Vector(v) => Value::Vector(
                    pattern[..*size as usize]
                        .iter()
                        .map(|c| v[*c as usize].clone())
                        .collect(),
                ),
                other => panic!("swizzle of {other:?}"),
            },
            Expression::Compose { components, .. } => {
                let parts: Vec<Value> = components.iter().map(|c| self.eval(*c)).collect();
                Value::Vector(parts.into_iter().flat_map(flatten).collect())
            }
            Expression::Splat { size, value } => {
                let v = self.eval(*value);
                Value::Vector(vec![v; *size as usize])
            }
            Expression::Unary { op, expr } => {
                let v = self.eval(*expr);
                match op {
                    naga::UnaryOperator::Negate => each(&v, &|x| -x),
                    naga::UnaryOperator::LogicalNot => Value::Bool(!v.bool()),
                    other => panic!("unary {other:?}"),
                }
            }
            Expression::Binary { op, left, right } => {
                let (a, b) = (self.eval(*left), self.eval(*right));
                binary(*op, a, b)
            }
            Expression::Select {
                condition,
                accept,
                reject,
            } => {
                if self.eval(*condition).bool() {
                    self.eval(*accept)
                } else {
                    self.eval(*reject)
                }
            }
            Expression::Math {
                fun,
                arg,
                arg1,
                arg2,
                ..
            } => {
                let a = self.eval(*arg);
                let b = arg1.map(|x| self.eval(x));
                let c = arg2.map(|x| self.eval(x));
                math(*fun, &a, b.as_ref(), c.as_ref())
            }
            Expression::As {
                expr,
                kind,
                convert,
            } => {
                let v = self.eval(*expr);
                cast(v, *kind, *convert)
            }
            Expression::CallResult(_) => panic!("a call's result read before the call"),
            other => panic!("expression {other:?}"),
        };
        self.cache[h.index()] = Some(v.clone());
        v
    }
}

fn literal(l: Literal) -> Value {
    match l {
        Literal::F32(v) => Value::F32(v),
        Literal::U32(v) => Value::U32(v),
        Literal::I32(v) => Value::I32(v),
        Literal::Bool(v) => Value::Bool(v),
        #[allow(clippy::cast_possible_truncation)]
        Literal::AbstractFloat(v) => Value::F32(v as f32),
        #[allow(clippy::cast_possible_truncation)]
        Literal::AbstractInt(v) => Value::I32(v as i32),
        other => panic!("literal {other:?}"),
    }
}

/// A value's scalars, a vector's spread out.
fn flatten(v: Value) -> Vec<Value> {
    match v {
        Value::Vector(c) => c,
        s => vec![s],
    }
}
