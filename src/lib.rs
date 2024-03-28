#![forbid(unsafe_code)]

#[cfg(feature = "pyo3")]
mod python;

use std::fmt::Write;
use std::rc::Rc;
use std::iter;

use once_cell::unsync::OnceCell;
use compact_str::{CompactString, ToCompactString, format_compact};
use base64::engine::Engine as Base64Engine;
use regex::Regex;

#[macro_use] extern crate serde_json;
#[macro_use] extern crate lazy_static;

pub use netsblox_ast::Error as ParseError;
use netsblox_ast::{*, util::*};

#[cfg(test)]
mod test;

lazy_static! {
    static ref PY_IDENT_REGEX: Regex = Regex::new(r"^[_a-zA-Z][_a-zA-Z0-9]*$").unwrap();
}
fn is_py_ident(sym: &str) -> bool {
    PY_IDENT_REGEX.is_match(sym)
}
#[test]
fn test_py_ident() {
    assert!(is_py_ident("fooBar_23"));
    assert!(!is_py_ident("34hello"));
    assert!(!is_py_ident("hello world"));
}

#[derive(Debug)]
pub enum TranslateError {
    Parse(Box<Error>),
    NoRoles,

    UnsupportedExpr(Box<Expr>),
    UnsupportedStmt(Box<Stmt>),
    UnsupportedHat(Box<Hat>),

    UnknownImageFormat,

    Upvars,
    AnyMessage,
    RingTypeQuery,
    CommandRing,
    TellAskClosure,
}
impl From<Box<Error>> for TranslateError { fn from(e: Box<Error>) -> Self { Self::Parse(e) } }

fn fmt_comment(comment: Option<&str>) -> CompactString {
    match comment {
        Some(v) => format_compact!(" # {}", v.replace('\n', " -- ")),
        None => "".into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Type {
    Unknown, Wrapped,
}

fn wrap(val: (CompactString, Type)) -> CompactString {
    match &val.1 {
        Type::Wrapped => val.0,
        Type::Unknown => format_compact!("snap.wrap({})", val.0),
    }
}

fn translate_var(var: &VariableRef) -> CompactString {
    match &var.location {
        VarLocation::Local => var.trans_name.clone(),
        VarLocation::Field => format_compact!("self.{}", var.trans_name),
        VarLocation::Global => format_compact!("globals()['{}']", var.trans_name),
    }
}

struct ScriptInfo<'a> {
    stage: &'a SpriteInfo,
}
impl<'a> ScriptInfo<'a> {
    fn new(stage: &'a SpriteInfo) -> Self {
        Self { stage }
    }
    fn translate_value(&mut self, value: &Value) -> Result<(CompactString, Type), TranslateError> {
        Ok(match value {
            Value::String(v) => (format_compact!("'{}'", escape(v)), Type::Unknown),
            Value::Number(v) => (format_compact!("{}", v), Type::Unknown),
            Value::Bool(v) => ((if *v { "True" } else { "False" }).into(), Type::Wrapped), // bool is considered wrapped since we can't extend it
            Value::Constant(c) => match c {
                Constant::Pi => ("math.pi".into(), Type::Unknown),
                Constant::E => ("math.e".into(), Type::Unknown),
            }
            Value::List(vals, _) => {
                let mut items = Vec::with_capacity(vals.len());
                for val in vals {
                    items.push(self.translate_value(val)?.0);
                }
                (format_compact!("[{}]", Punctuated(items.iter(), ", ")), Type::Unknown)
            }
            Value::Image(_) => unreachable!(),
            Value::Audio(_) => unreachable!(),
            Value::Ref(_) => unreachable!(),
        })
    }
    fn translate_kwargs(&mut self, kwargs: &[(CompactString, Expr)], prefix: &str, wrap_vals: bool) -> Result<CompactString, TranslateError> {
        let mut ident_args = vec![];
        let mut non_ident_args = vec![];
        for arg in kwargs {
            let val_raw = self.translate_expr(&arg.1)?;
            let val = if wrap_vals { wrap(val_raw) } else { val_raw.0 };
            match is_py_ident(&arg.0) {
                true => ident_args.push(format_compact!("{} = {}", arg.0, val)),
                false => non_ident_args.push(format_compact!("'{}': {}", escape(&arg.0), val)),
            }
        }

        Ok(match (ident_args.is_empty(), non_ident_args.is_empty()) {
            (false, false) => format_compact!("{}{}, **{{ {} }}", prefix, Punctuated(ident_args.iter(), ", "), Punctuated(non_ident_args.iter(), ", ")),
            (false, true) => format_compact!("{}{}", prefix, Punctuated(ident_args.iter(), ", ")),
            (true, false) => format_compact!("{}**{{ {} }}", prefix, Punctuated(non_ident_args.iter(), ", ")),
            (true, true) => CompactString::default(),
        })
    }
    fn translate_rpc(&mut self, service: &str, rpc: &str, args: &[(CompactString, Expr)]) -> Result<CompactString, TranslateError> {
        let args_str = self.translate_kwargs(args, ", ", false)?;
        Ok(format_compact!("nothrow(nb.call)('{}', '{}'{})", escape(service), escape(rpc), args_str))
    }
    fn translate_fn_call(&mut self, function: &FnRef, args: &[Expr], upvars: &[VariableRef]) -> Result<CompactString, TranslateError> {
        if !upvars.is_empty() {
            return Err(TranslateError::Upvars);
        }

        let mut trans_args = Vec::with_capacity(args.len());
        for arg in args.iter() {
            trans_args.push(wrap(self.translate_expr(arg)?));
        }

        Ok(match function.location {
            FnLocation::Global => format_compact!("{}({})", function.trans_name, Punctuated(trans_args.iter(), ", ")),
            FnLocation::Method => format_compact!("self.{}({})", function.trans_name, Punctuated(trans_args.iter(), ", ")),
        })
    }
    fn translate_closure_call(&mut self, new_entity: Option<&Expr>, closure: &Expr, args: &[Expr]) -> Result<CompactString, TranslateError> {
        if new_entity.is_some() {
            return Err(TranslateError::TellAskClosure);
        }

        let args = args.iter().map(|x| Ok(wrap(self.translate_expr(x)?))).collect::<Result<Vec<_>,TranslateError>>()?;
        Ok(format_compact!("{}({})", self.translate_expr(closure)?.0, args.join(", "))) // return values are always considered wrapped
    }
    fn translate_expr(&mut self, expr: &Expr) -> Result<(CompactString, Type), TranslateError> {
        Ok(match &expr.kind {
            ExprKind::Value(v) => self.translate_value(v)?,
            ExprKind::Variable { var, .. } => (translate_var(var), Type::Wrapped), // all assignments are wrapped, so we can assume vars are wrapped

            ExprKind::Closure { kind: _, params, captures: _, stmts } => match stmts.as_slice() {
                [Stmt { kind: StmtKind::Return { value }, info: _ }] => {
                    let mut params_string = CompactString::default();
                    for param in params {
                        if params_string.is_empty() {
                            params_string.push(' ');
                        } else {
                            params_string.push_str(", ");
                        }
                        params_string.push_str(&param.trans_name);
                    }
                    (format_compact!("(lambda{}: {})",params_string, wrap(self.translate_expr(value)?)), Type::Wrapped) // functions are always considered wrapped
                },
                _ => return Err(TranslateError::CommandRing),
            }

            ExprKind::This => ("self".into(), Type::Wrapped), // non-primitives are considered wrapped
            ExprKind::Entity { trans_name, .. } => (trans_name.clone(), Type::Wrapped), // non-primitives are considered wrapped

            ExprKind::ImageOfEntity { entity } => (format_compact!("{}.get_image()", self.translate_expr(entity)?.0), Type::Wrapped), // non-primitives are considered wrapped
            ExprKind::ImageOfDrawings => (format_compact!("{}.get_drawings()", self.stage.name), Type::Wrapped), // non-primitives are considered wrapped

            ExprKind::IsTouchingEntity { entity } => (format_compact!("self.is_touching({})", self.translate_expr(entity)?.0), Type::Wrapped), // bool is considered wrapped

            ExprKind::MakeList { values } => {
                let trans = values.iter().map(|x| Ok(self.translate_expr(x)?.0)).collect::<Result<Vec<_>,TranslateError>>()?;
                (format_compact!("[{}]", trans.join(", ")), Type::Unknown)
            }
            ExprKind::CopyList { list } => (format_compact!("[*{}]", wrap(self.translate_expr(list)?)), Type::Unknown),
            ExprKind::ListCons { item, list } => (format_compact!("[{}, *{}]", self.translate_expr(item)?.0, wrap(self.translate_expr(list)?)), Type::Unknown),
            ExprKind::ListCdr { value } => (format_compact!("{}[1:]", wrap(self.translate_expr(value)?)), Type::Wrapped),

            ExprKind::ListGet { list, index } => (format_compact!("{}[{} - snap.wrap(1)]", wrap(self.translate_expr(list)?), wrap(self.translate_expr(index)?)), Type::Wrapped),
            ExprKind::ListGetRandom { list } => (format_compact!("{}.rand", wrap(self.translate_expr(list)?)), Type::Wrapped),
            ExprKind::ListGetLast { list } => (format_compact!("{}.last", wrap(self.translate_expr(list)?)), Type::Wrapped),

            ExprKind::ListFind { list, value } => (format_compact!("({}.index({}) + snap.wrap(1))", wrap(self.translate_expr(list)?), self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::ListContains { list, value } => (format_compact!("({} in {})", wrap(self.translate_expr(value)?), wrap(self.translate_expr(list)?)), Type::Wrapped),

            ExprKind::ListLen { value } | ExprKind::StrLen { value } => (format_compact!("len({})", self.translate_expr(value)?.0), Type::Unknown), // builtin __len__ can't be overloaded to return wrapped
            ExprKind::ListIsEmpty { value } => (format_compact!("(len({}) == 0)", self.translate_expr(value)?.0), Type::Wrapped),

            ExprKind::ListRank { value } => (format_compact!("len({}.shape)", wrap(self.translate_expr(value)?)), Type::Unknown), // builtin __len__ can't be overloaded to return wrapped
            ExprKind::ListDims { value } => (format_compact!("{}.shape", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::ListFlatten { value } => (format_compact!("{}.flat", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::ListColumns { value } => (format_compact!("{}.T", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::ListRev { value } => (format_compact!("{}[::-1]", wrap(self.translate_expr(value)?)), Type::Wrapped),

            ExprKind::ListLines { value } => (format_compact!("'\\n'.join(str(x) for x in {})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::ListCsv { value } => (format_compact!("{}.csv", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::ListJson { value } => (format_compact!("{}.json", wrap(self.translate_expr(value)?)), Type::Wrapped),

            ExprKind::ListReshape { value, dims } => (format_compact!("{}.reshaped({})", wrap(self.translate_expr(value)?), self.translate_expr(dims)?.0), Type::Wrapped),

            ExprKind::Map { f, list } => (format_compact!("[{}(x) for x in {}]", self.translate_expr(f)?.0, wrap(self.translate_expr(list)?)), Type::Unknown),
            ExprKind::Keep { f, list } => (format_compact!("[x for x in {} if {}(x)]", wrap(self.translate_expr(list)?), self.translate_expr(f)?.0), Type::Unknown),
            ExprKind::FindFirst { f, list } => (format_compact!("{}.index_where({})", wrap(self.translate_expr(list)?), self.translate_expr(f)?.0), Type::Wrapped),
            ExprKind::Combine { f, list } => (format_compact!("{}.fold({})", wrap(self.translate_expr(list)?), self.translate_expr(f)?.0), Type::Wrapped),

            ExprKind::StrGet { string, index } => (format_compact!("{}[{} - snap.wrap(1)]", wrap(self.translate_expr(string)?), wrap(self.translate_expr(index)?)), Type::Wrapped),
            ExprKind::StrGetLast { string } => (format_compact!("{}.last", wrap(self.translate_expr(string)?)), Type::Wrapped),
            ExprKind::StrGetRandom { string } => (format_compact!("{}.rand", wrap(self.translate_expr(string)?)), Type::Wrapped),

            ExprKind::Neg { value } => (format_compact!("-{}", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Not { value } => (format_compact!("snap.lnot({})", self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::Abs { value } => (format_compact!("abs({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Sign { value } => (format_compact!("snap.sign({})", self.translate_expr(value)?.0), Type::Wrapped),

            ExprKind::Atan2 { y, x } => (format_compact!("snap.atan2({}, {})", self.translate_expr(y)?.0, self.translate_expr(x)?.0), Type::Wrapped),

            ExprKind::ListCombinations { sources } => match &sources.kind {
                ExprKind::Value(Value::List(values, _)) => (format_compact!("snap.combinations({})", values.iter().map(|x| Ok(self.translate_value(x)?.0)).collect::<Result<Vec<_>,TranslateError>>()?.join(", ")), Type::Wrapped),
                ExprKind::MakeList { values } => (format_compact!("snap.combinations({})", values.iter().map(|x| Ok(self.translate_expr(x)?.0)).collect::<Result<Vec<_>,TranslateError>>()?.join(", ")), Type::Wrapped),
                _ => (format_compact!("snap.combinations(*{})", wrap(self.translate_expr(sources)?)), Type::Wrapped),
            }
            ExprKind::Add { values } => match &values.kind {
                ExprKind::Value(Value::List(values, _)) => match values.as_slice() {
                    [] => ("0".into(), Type::Unknown),
                    _ => (format_compact!("({})", values.iter().map(|x| Ok(wrap(self.translate_value(x)?))).collect::<Result<Vec<_>,TranslateError>>()?.join(" + ")), Type::Wrapped),
                }
                ExprKind::MakeList { values } => match values.as_slice() {
                    [] => ("0".into(), Type::Unknown),
                    _ => (format_compact!("({})", values.iter().map(|x| Ok(wrap(self.translate_expr(x)?))).collect::<Result<Vec<_>,TranslateError>>()?.join(" + ")), Type::Wrapped),
                }
                _ => (format_compact!("sum({})", wrap(self.translate_expr(values)?)), Type::Unknown),
            }
            ExprKind::Mul { values } => match &values.kind {
                ExprKind::Value(Value::List(values, _)) => match values.as_slice() {
                    [] => ("1".into(), Type::Unknown),
                    _ => (format_compact!("({})", values.iter().map(|x| Ok(wrap(self.translate_value(x)?))).collect::<Result<Vec<_>,TranslateError>>()?.join(" * ")), Type::Wrapped),
                }
                ExprKind::MakeList { values } => match values.as_slice() {
                    [] => ("1".into(), Type::Unknown),
                    _ => (format_compact!("({})", values.iter().map(|x| Ok(wrap(self.translate_expr(x)?))).collect::<Result<Vec<_>,TranslateError>>()?.join(" * ")), Type::Wrapped),
                }
                _ => (format_compact!("snap.prod({})", self.translate_expr(values)?.0), Type::Wrapped),
            }

            ExprKind::Min { values } => (format_compact!("min({})", wrap(self.translate_expr(values)?)), Type::Wrapped),
            ExprKind::Max { values } => (format_compact!("max({})", wrap(self.translate_expr(values)?)), Type::Wrapped),

            ExprKind::Sub { left, right } => (format_compact!("({} - {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Div { left, right } => (format_compact!("({} / {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Mod { left, right } => (format_compact!("({} % {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),

            ExprKind::Pow { base, power } => (format_compact!("({} ** {})", wrap(self.translate_expr(base)?), wrap(self.translate_expr(power)?)), Type::Wrapped),
            ExprKind::Log { value, base } => (format_compact!("snap.log({}, {})", self.translate_expr(value)?.0, self.translate_expr(base)?.0), Type::Wrapped),

            ExprKind::Sqrt { value } => (format_compact!("snap.sqrt({})", self.translate_expr(value)?.0), Type::Wrapped),

            ExprKind::Round { value } => (format_compact!("round({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Floor { value } => (format_compact!("math.floor({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Ceil { value } => (format_compact!("math.ceil({})", wrap(self.translate_expr(value)?)), Type::Wrapped),

            ExprKind::Sin { value } => (format_compact!("snap.sin({})", self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::Cos { value } => (format_compact!("snap.cos({})", self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::Tan { value } => (format_compact!("snap.tan({})", self.translate_expr(value)?.0), Type::Wrapped),

            ExprKind::Asin { value } => (format_compact!("snap.asin({})", self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::Acos { value } => (format_compact!("snap.acos({})", self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::Atan { value } => (format_compact!("snap.atan({})", self.translate_expr(value)?.0), Type::Wrapped),

            ExprKind::And { left, right } => (format_compact!("({} and {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Or { left, right } => (format_compact!("({} or {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Conditional { condition, then, otherwise } => {
                let (then, otherwise) = (self.translate_expr(then)?, self.translate_expr(otherwise)?);
                (format_compact!("({} if {} else {})", then.0, wrap(self.translate_expr(condition)?), otherwise.0), if then.1 == otherwise.1 { then.1 } else { Type::Unknown })
            }

            ExprKind::Identical { left, right } => (format_compact!("snap.identical({}, {})", self.translate_expr(left)?.0, self.translate_expr(right)?.0), Type::Wrapped), // bool is considered wrapped

            ExprKind::Less { left, right } => (format_compact!("({} < {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::LessEq { left, right } => (format_compact!("({} <= {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Eq { left, right } => (format_compact!("({} == {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Neq { left, right } => (format_compact!("({} != {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Greater { left, right } => (format_compact!("({} > {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::GreaterEq { left, right } => (format_compact!("({} >= {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),

            ExprKind::Random { a, b } => (format_compact!("snap.rand({}, {})", self.translate_expr(a)?.0, self.translate_expr(b)?.0), Type::Wrapped), // python impl returns wrapped
            ExprKind::Range { start, stop } => (format_compact!("snap.srange({}, {})", self.translate_expr(start)?.0, self.translate_expr(stop)?.0), Type::Wrapped), // python impl returns wrapped

            ExprKind::CostumeNumber => (format_compact!("(self.costumes.index(self.costume, -1) + 1)"), Type::Unknown),

            ExprKind::TextSplit { text, mode } => match mode {
                TextSplitMode::Custom(x) => (format_compact!("snap.split({}, {})", self.translate_expr(text)?.0, self.translate_expr(x)?.0), Type::Wrapped),
                TextSplitMode::LF => (format_compact!("snap.split({}, '\\n')", self.translate_expr(text)?.0), Type::Wrapped),
                TextSplitMode::CR => (format_compact!("snap.split({}, '\\r')", self.translate_expr(text)?.0), Type::Wrapped),
                TextSplitMode::Tab => (format_compact!("snap.split({}, '\\t')", self.translate_expr(text)?.0), Type::Wrapped),
                TextSplitMode::Letter => (format_compact!("snap.split({}, '')", self.translate_expr(text)?.0), Type::Wrapped),
                TextSplitMode::Word => (format_compact!("snap.split_words({})", self.translate_expr(text)?.0), Type::Wrapped),
                TextSplitMode::Csv => (format_compact!("snap.split_csv({})", self.translate_expr(text)?.0), Type::Wrapped),
                TextSplitMode::Json => (format_compact!("snap.split_json({})", self.translate_expr(text)?.0), Type::Wrapped),
            }

            ExprKind::TypeQuery { value, ty } => match ty {
                ValueType::Bool => (format_compact!("snap.is_bool({})", self.translate_expr(value)?.0), Type::Wrapped),
                ValueType::Text => (format_compact!("snap.is_text({})", self.translate_expr(value)?.0), Type::Wrapped),
                ValueType::Number => (format_compact!("snap.is_number({})", self.translate_expr(value)?.0), Type::Wrapped),
                ValueType::List => (format_compact!("snap.is_list({})", self.translate_expr(value)?.0), Type::Wrapped),
                ValueType::Sprite => (format_compact!("snap.is_sprite({})", self.translate_expr(value)?.0), Type::Wrapped),
                ValueType::Costume => (format_compact!("snap.is_costume({})", self.translate_expr(value)?.0), Type::Wrapped),
                ValueType::Sound => (format_compact!("snap.is_sound({})", self.translate_expr(value)?.0), Type::Wrapped),
                ValueType::Command | ValueType::Reporter | ValueType::Predicate => return Err(TranslateError::RingTypeQuery),
            }

            ExprKind::ListCat { lists } => match &lists.kind {
                ExprKind::Value(Value::List(values, _)) => (format_compact!("[{}]", values.iter().map(|x| Ok(format_compact!("*{}", wrap(self.translate_value(x)?)))).collect::<Result<Vec<_>,TranslateError>>()?.join(", ")), Type::Unknown),
                _ => (format_compact!("[y for x in {} for y in x]", wrap(self.translate_expr(lists)?)), Type::Unknown),
            }
            ExprKind::StrCat { values } => match &values.kind {
                ExprKind::Value(Value::List(values, _)) => match values.as_slice() {
                    [] => (CompactString::new("''"), Type::Unknown),
                    _ => (format_compact!("({})", values.iter().map(|x| Ok(format_compact!("str({})", wrap(self.translate_value(x)?)))).collect::<Result<Vec<_>,TranslateError>>()?.join(" + ")), Type::Unknown),
                }
                ExprKind::MakeList { values } => match values.as_slice() {
                    [] => (CompactString::new("''"), Type::Unknown),
                    _ => (format_compact!("({})", values.iter().map(|x| Ok(format_compact!("str({})", wrap(self.translate_expr(x)?)))).collect::<Result<Vec<_>,TranslateError>>()?.join(" + ")), Type::Unknown),
                }
                _ => (format_compact!("''.join(str(x) for x in {})", wrap(self.translate_expr(values)?)), Type::Unknown),
            }

            ExprKind::UnicodeToChar { value } => (format_compact!("snap.get_chr({})", self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::CharToUnicode { value } => (format_compact!("snap.get_ord({})", self.translate_expr(value)?.0), Type::Wrapped),

            ExprKind::CallRpc { service, host: _, rpc, args } => (self.translate_rpc(service, rpc, args)?, Type::Unknown),
            ExprKind::CallFn { function, args, upvars } => (self.translate_fn_call(function, args, upvars)?, Type::Wrapped),
            ExprKind::CallClosure { new_entity, closure, args } => (self.translate_closure_call(new_entity.as_deref(), closure, args)?, Type::Wrapped),

            ExprKind::XPos => ("self.x_pos".into(), Type::Unknown),
            ExprKind::YPos => ("self.y_pos".into(), Type::Unknown),
            ExprKind::Heading => ("self.heading".into(), Type::Unknown),

            ExprKind::Answer => (format_compact!("{}.last_answer", self.stage.name), Type::Wrapped),

            ExprKind::MouseX => (format_compact!("{}.mouse_pos[0]", self.stage.name), Type::Unknown),
            ExprKind::MouseY => (format_compact!("{}.mouse_pos[1]", self.stage.name), Type::Unknown),

            ExprKind::StageWidth => (format_compact!("{}.width", self.stage.name), Type::Unknown),
            ExprKind::StageHeight => (format_compact!("{}.height", self.stage.name), Type::Unknown),

            ExprKind::Latitude => (format_compact!("{}.gps_location[0]", self.stage.name), Type::Unknown),
            ExprKind::Longitude => (format_compact!("{}.gps_location[1]", self.stage.name), Type::Unknown),

            ExprKind::KeyDown { key } => (format_compact!("{}.is_key_down({})", self.stage.name, self.translate_expr(key)?.0), Type::Wrapped), // bool is considered wrapped

            ExprKind::PenDown => ("self.drawing".into(), Type::Wrapped), // bool is considered wrapped
            ExprKind::Size => ("(self.scale * 100)".into(), Type::Wrapped),
            ExprKind::IsVisible => ("self.visible".into(), Type::Wrapped), // bool is considered wrapped

            ExprKind::RpcError => ("(get_error() or '')".into(), Type::Unknown),

            ExprKind::Clone { target } => (format_compact!("{}.clone()", self.translate_expr(target)?.0), Type::Wrapped), // sprites are considered wrapped

            _ => return Err(TranslateError::UnsupportedExpr(Box::new(expr.clone()))),
        })
    }
    fn translate_stmts(&mut self, stmts: &[Stmt]) -> Result<CompactString, TranslateError> {
        if stmts.is_empty() { return Ok("pass".into()) }

        let mut lines = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::DeclareLocals { vars } => lines.extend(vars.iter().map(|x| format_compact!("{} = snap.wrap(0)", x.trans_name))),
                StmtKind::Assign { var, value } => lines.push(format_compact!("{} = {}{}", translate_var(var), wrap(self.translate_expr(value)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::AddAssign { var, value } => lines.push(format_compact!("{} += {}{}", translate_var(var), wrap(self.translate_expr(value)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ListAssign { list, index, value } => lines.push(format_compact!("{}[{} - snap.wrap(1)] = {}{}", wrap(self.translate_expr(list)?), wrap(self.translate_expr(index)?), self.translate_expr(value)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ListAssignLast { list, value } => lines.push(format_compact!("{}.last = {}{}", wrap(self.translate_expr(list)?), self.translate_expr(value)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ListAssignRandom { list, value } => lines.push(format_compact!("{}.rand = {}{}", wrap(self.translate_expr(list)?), self.translate_expr(value)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ListInsert { list, index, value } => lines.push(format_compact!("{}.insert({}, {}){}", wrap(self.translate_expr(list)?), self.translate_expr(index)?.0, self.translate_expr(value)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ListInsertLast { list, value } => lines.push(format_compact!("{}.append({}){}", wrap(self.translate_expr(list)?), self.translate_expr(value)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ListInsertRandom { list, value } => lines.push(format_compact!("{}.insert_rand({}){}", wrap(self.translate_expr(list)?), self.translate_expr(value)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ListRemoveLast { list } => lines.push(format_compact!("{}.pop(){}", wrap(self.translate_expr(list)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ListRemove { list, index } => lines.push(format_compact!("del {}[{} - snap.wrap(1)]{}", wrap(self.translate_expr(list)?), wrap(self.translate_expr(index)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ListRemoveAll { list } => lines.push(format_compact!("{}.clear(){}", wrap(self.translate_expr(list)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Throw { error } => lines.push(format_compact!("raise RuntimeError(str({})){}", wrap(self.translate_expr(error)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Warp { stmts } => {
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format_compact!("with NoYield():{}\n{}", fmt_comment(stmt.info.comment.as_deref()), indent(&code)));
                }
                StmtKind::If { condition, then } => {
                    let condition = wrap(self.translate_expr(condition)?);
                    let then = self.translate_stmts(then)?;
                    lines.push(format_compact!("if {}:{}\n{}", condition, fmt_comment(stmt.info.comment.as_deref()), indent(&then)));
                }
                StmtKind::IfElse { condition, then, otherwise } => {
                    let condition = wrap(self.translate_expr(condition)?);
                    let then_code = self.translate_stmts(then)?;
                    let otherwise_code = self.translate_stmts(otherwise)?;

                    match otherwise.as_slice() {
                        [Stmt { kind: StmtKind::If { .. } | StmtKind::IfElse { .. }, .. }] => {
                            lines.push(format_compact!("if {}:{}\n{}\nel{}", condition, fmt_comment(stmt.info.comment.as_deref()), indent(&then_code), otherwise_code));
                        }
                        _ => {
                            lines.push(format_compact!("if {}:{}\n{}\nelse:\n{}", condition, fmt_comment(stmt.info.comment.as_deref()), indent(&then_code), indent(&otherwise_code)));
                        }
                    }
                }
                StmtKind::TryCatch { code, var, handler } => {
                    let code = self.translate_stmts(code)?;
                    let handler = self.translate_stmts(handler)?;
                    lines.push(format_compact!("try:{}\n{}\nexcept Exception as {}:\n{}", fmt_comment(stmt.info.comment.as_deref()), indent(&code), var.trans_name, indent(&handler)));
                }
                StmtKind::InfLoop { stmts } => {
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format_compact!("while True:{}\n{}", fmt_comment(stmt.info.comment.as_deref()), indent(&code)));
                }
                StmtKind::ForLoop { var, start, stop, stmts } => {
                    let start = self.translate_expr(start)?.0;
                    let stop = self.translate_expr(stop)?.0;
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format_compact!("for {} in snap.sxrange({}, {}):{}\n{}", var.trans_name, start, stop, fmt_comment(stmt.info.comment.as_deref()), indent(&code)));
                }
                StmtKind::ForeachLoop { var, items, stmts } => {
                    let items = wrap(self.translate_expr(items)?);
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format_compact!("for {} in {}:{}\n{}", var.trans_name, items, fmt_comment(stmt.info.comment.as_deref()), indent(&code)));
                }
                StmtKind::Repeat { times, stmts } => {
                    let times = wrap(self.translate_expr(times)?);
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format_compact!("for _ in range(+{}):{}\n{}", times, fmt_comment(stmt.info.comment.as_deref()), indent(&code)));
                }
                StmtKind::UntilLoop { condition, stmts } => {
                    let condition = wrap(self.translate_expr(condition)?);
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format_compact!("while not {}:{}\n{}", condition, fmt_comment(stmt.info.comment.as_deref()), indent(&code)));
                }
                StmtKind::SetCostume { costume } => {
                    let costume = self.translate_expr(costume)?.0;
                    lines.push(format_compact!("self.costume = {}{}", costume, fmt_comment(stmt.info.comment.as_deref())));
                }
                StmtKind::NextCostume => lines.push(format_compact!("self.costume = (self.costumes.index(self.costume, -1) + 1) % len(self.costumes)")),

                StmtKind::SetX { value } => lines.push(format_compact!("self.x_pos = {}{}", wrap(self.translate_expr(value)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::SetY { value } => lines.push(format_compact!("self.y_pos = {}{}", wrap(self.translate_expr(value)?), fmt_comment(stmt.info.comment.as_deref()))),

                StmtKind::ChangeX { delta } => lines.push(format_compact!("self.x_pos += {}{}", wrap(self.translate_expr(delta)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ChangeY { delta } => lines.push(format_compact!("self.y_pos += {}{}", wrap(self.translate_expr(delta)?), fmt_comment(stmt.info.comment.as_deref()))),

                StmtKind::Goto { target } => match &target.kind {
                    ExprKind::Value(Value::List(values, _)) if values.len() == 2 => lines.push(format_compact!("self.pos = ({}, {}){}", self.translate_value(&values[0])?.0, self.translate_value(&values[1])?.0, fmt_comment(stmt.info.comment.as_deref()))),
                    _ => lines.push(format_compact!("self.pos = {}{}", self.translate_expr(target)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                }
                StmtKind::GotoXY { x, y } => lines.push(format_compact!("self.pos = ({}, {}){}", wrap(self.translate_expr(x)?), wrap(self.translate_expr(y)?), fmt_comment(stmt.info.comment.as_deref()))),

                StmtKind::SendLocalMessage { target, msg_type, wait } => {
                    if *wait { unimplemented!() }
                    if target.is_some() { unimplemented!() }

                    match &msg_type.kind {
                        ExprKind::Value(Value::String(msg_type)) => lines.push(format_compact!("nb.send_message('local::{}'){}", escape(msg_type), fmt_comment(stmt.info.comment.as_deref()))),
                        _  => lines.push(format_compact!("nb.send_message('local::' + str({})){}", self.translate_expr(msg_type)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                    }
                }
                StmtKind::SendNetworkMessage { target, msg_type, values } => {
                    let kwargs_str = self.translate_kwargs(values, ", ", false)?;
                    lines.push(format_compact!("nb.send_message('{}', {}{}){}", escape(msg_type), self.translate_expr(target)?.0, kwargs_str, fmt_comment(stmt.info.comment.as_deref())));
                }
                StmtKind::Say { content, duration } | StmtKind::Think { content, duration } => match duration {
                    Some(duration) => lines.push(format_compact!("self.say({}, duration = {}){}", self.translate_expr(content)?.0, self.translate_expr(duration)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                    None => lines.push(format_compact!("self.say({}){}", self.translate_expr(content)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                }
                StmtKind::CallRpc { service, host: _, rpc, args } => lines.push(format_compact!("{}{}", self.translate_rpc(service, rpc, args)?, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::CallFn { function, args, upvars } => lines.push(format_compact!("{}{}", self.translate_fn_call(function, args, upvars)?, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::CallClosure { new_entity, closure, args } => lines.push(format_compact!("{}{}", self.translate_closure_call(new_entity.as_deref(), closure, args)?, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ChangePenSize { delta } => lines.push(format_compact!("self.pen_size += {}{}", wrap(self.translate_expr(delta)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::SetPenSize { value } => lines.push(format_compact!("self.pen_size = {}{}", wrap(self.translate_expr(value)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::SetVisible { value } => lines.push(format_compact!("self.visible = {}{}", if *value { "True" } else { "False" }, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::WaitUntil { condition } => lines.push(format_compact!("while not {}:{}\n    time.sleep(0.05)", wrap(self.translate_expr(condition)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::BounceOffEdge => lines.push(format_compact!("self.keep_on_stage(bounce = True){}", fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Sleep { seconds } => lines.push(format_compact!("time.sleep(+{}){}", wrap(self.translate_expr(seconds)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Forward { distance } => lines.push(format_compact!("self.forward({}){}", wrap(self.translate_expr(distance)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::TurnRight { angle } => lines.push(format_compact!("self.turn_right({}){}", wrap(self.translate_expr(angle)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::TurnLeft { angle } => lines.push(format_compact!("self.turn_left({}){}", wrap(self.translate_expr(angle)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::SetHeading { value } => lines.push(format_compact!("self.heading = {}{}", wrap(self.translate_expr(value)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Return { value } => lines.push(format_compact!("return {}{}", wrap(self.translate_expr(value)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Stamp => lines.push(format_compact!("self.stamp(){}", fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Write { content, font_size } => lines.push(format_compact!("self.write({}, size = {}){}", wrap(self.translate_expr(content)?), wrap(self.translate_expr(font_size)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::SetPenDown { value } => lines.push(format_compact!("self.drawing = {}{}", if *value { "True" } else { "False" }, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::PenClear => lines.push(format_compact!("{}.clear_drawings(){}", self.stage.name, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::SetPenColor { color } => lines.push(format_compact!("self.pen_color = '#{:02x}{:02x}{:02x}'{}", color.0, color.1, color.2, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ChangeSize { delta } => lines.push(format_compact!("self.scale += {} / 100{}", wrap(self.translate_expr(delta)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::SetSize { value } => lines.push(format_compact!("self.scale = {} / 100{}", wrap(self.translate_expr(value)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Clone { target } => lines.push(format_compact!("{}.clone(){}", self.translate_expr(target)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Ask { prompt } => lines.push(format_compact!("{}.last_answer = snap.wrap(input({})){}", self.stage.name, self.translate_expr(prompt)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                _ => return Err(TranslateError::UnsupportedStmt(Box::new(stmt.clone()))),
            }
        }

        Ok(lines.join("\n").into())
    }
}

struct RoleInfo {
    name: CompactString,
    sprites: Vec<SpriteInfo>,
}
impl RoleInfo {
    fn new(name: CompactString) -> Self {
        Self { name, sprites: vec![] }
    }
}

#[derive(Clone)]
struct SpriteInfo {
    name: CompactString,
    scripts: Vec<CompactString>,
    fields: Vec<(CompactString, CompactString)>,
    funcs: Vec<Function>,
    costumes: Vec<(CompactString, Rc<(Vec<u8>, Option<(f64, f64)>, CompactString)>)>,

    active_costume: Option<usize>,
    visible: bool,
    color: (u8, u8, u8, u8),
    pos: (f64, f64),
    heading: f64,
    scale: f64,
}
impl SpriteInfo {
    fn new(src: &Entity) -> Self {
        Self {
            name: src.trans_name.clone(),
            scripts: vec![],
            fields: vec![],
            costumes: vec![],
            funcs: src.funcs.clone(),

            active_costume: src.active_costume,
            visible: src.visible,
            color: src.color,
            pos: src.pos,
            heading: src.heading,
            scale: src.scale,
        }
    }
    fn translate_hat(&mut self, hat: &Hat, stage: &SpriteInfo) -> Result<CompactString, TranslateError> {
        Ok(match &hat.kind {
            HatKind::OnFlag => format_compact!("@onstart(){}\ndef my_onstart_{}(self):\n", fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
            HatKind::OnClone => format_compact!("@onstart(when = 'clone'){}\ndef my_onstart_{}(self):\n", fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
            HatKind::OnKey { key } => format_compact!("@onkey('{}'){}\ndef my_onkey_{}(self):\n", key, fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
            HatKind::MouseDown => format_compact!("@onmouse('down'){}\ndef my_onmouse_{}(self, x, y):\n", fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
            HatKind::MouseUp => format_compact!("@onmouse('up'){}\ndef my_onmouse_{}(self, x, y):\n", fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
            HatKind::ScrollDown => format_compact!("@onmouse('scroll-down'){}\ndef my_onmouse_{}(self, x, y):\n", fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
            HatKind::ScrollUp => format_compact!("@onmouse('scroll-up'){}\ndef my_onmouse_{}(self, x, y):\n", fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
            HatKind::When { condition } => {
                format_compact!(r#"@onstart(){comment}
def my_onstart{idx}(self):
    while True:
        try:
            time.sleep(0.05)
            if {condition}:
                self.my_oncondition{idx}()
        except Exception as e:
            import traceback, sys
            print(traceback.format_exc(), file = sys.stderr)
def my_oncondition{idx}(self):
"#,
                comment = fmt_comment(hat.info.comment.as_deref()),
                idx = self.scripts.len() + 1,
                condition = wrap(ScriptInfo::new(stage).translate_expr(condition)?))
            }
            HatKind::LocalMessage { msg_type } => match msg_type {
                Some(msg_type) => format_compact!("@nb.on_message('local::{}'){}\ndef my_on_message_{}(self):\n", escape(msg_type), fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
                None => return Err(TranslateError::AnyMessage),
            }
            HatKind::NetworkMessage { msg_type, fields } => {
                let mut res = format_compact!("@nb.on_message('{}'){}\ndef my_on_message_{}(self, **kwargs):\n", escape(msg_type), fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1);
                for field in fields {
                    writeln!(&mut res, "    {} = snap.wrap(kwargs['{}'])", field.trans_name, escape(&field.name)).unwrap();
                }
                if !fields.is_empty() { res.push('\n') }
                res
            }
            _ => return Err(TranslateError::UnsupportedHat(Box::new(hat.clone()))),
        })
    }
}

/// Translates NetsBlox project XML into PyBlox project JSON
///
/// On success, returns the project name and project json content as a tuple.
pub fn translate(source: &str) -> Result<(CompactString, CompactString), TranslateError> {
    let parser = Parser {
        name_transformer: Box::new(netsblox_ast::util::c_ident),
        autofill_generator: Box::new(|x| Ok(format_compact!("_{x}"))),
        omit_nonhat_scripts: true, // we don't need dangling blocks of code since they can't do anything
        expr_replacements: vec![],
        stmt_replacements: vec![],
    };
    let project = parser.parse(source)?;
    if project.roles.is_empty() { return Err(TranslateError::NoRoles) }

    let mut roles = vec![];
    for role in project.roles.iter() {
        let mut role_info = RoleInfo::new(role.name.clone());
        let stage = OnceCell::new();

        for sprite in role.entities.iter() {
            let mut sprite_info = SpriteInfo::new(sprite);
            stage.get_or_init(|| sprite_info.clone());

            for costume in sprite.costumes.iter() {
                let info = match &costume.init {
                    Value::Image(x) => x.clone(),
                    _ => panic!(), // the parser lib would never do this
                };
                sprite_info.costumes.push((costume.def.trans_name.clone(), info.clone()));
            }
            for field in sprite.fields.iter() {
                let value = wrap(ScriptInfo::new(stage.get().unwrap()).translate_value(&field.init)?);
                sprite_info.fields.push((field.def.trans_name.clone(), value));
            }
            for script in sprite.scripts.iter() {
                let func_def = match script.hat.as_ref() {
                    Some(x) => sprite_info.translate_hat(x, stage.get().unwrap())?,
                    None => continue, // dangling blocks of code need not be translated
                };
                let body = ScriptInfo::new(stage.get().unwrap()).translate_stmts(&script.stmts)?;
                let res = format_compact!("{}{}", func_def, indent(&body));
                sprite_info.scripts.push(res);
            }
            role_info.sprites.push(sprite_info);
        }

        let mut editors = vec![];

        let mut content = String::new();
        content += "from netsblox import snap\n\n";
        for global in role.globals.iter() {
            let value = wrap(ScriptInfo::new(stage.get().unwrap()).translate_value(&global.init)?);
            writeln!(&mut content, "{} = {}", global.def.trans_name, value).unwrap();
        }
        if !role.globals.is_empty() { content.push('\n') }
        for func in role.funcs.iter() {
            let params = func.params.iter().map(|v| v.trans_name.as_str());
            let code = ScriptInfo::new(stage.get().unwrap()).translate_stmts(&func.stmts)?;
            write!(&mut content, "def {}({}):\n{}\n\n", func.trans_name, Punctuated(params, ", "), indent(&code)).unwrap();
        }
        editors.push(json!({
            "type": "global",
            "name": "global",
            "value": content,
        }));

        for (i, sprite) in role_info.sprites.iter().enumerate() {
            let mut content = String::new();

            for (field, value) in sprite.fields.iter() {
                writeln!(&mut content, "{} = {}", field, value).unwrap();
            }
            if !sprite.fields.is_empty() { content.push('\n'); }

            if i == 0 { // don't generate these for sprites
                content += "last_answer = snap.wrap('')\n";
                content.push('\n');
            }

            content += "def __init__(self):\n";
            if i != 0 { // don't generate these for stage
                writeln!(&mut content, "    self.pos = ({}, {})", sprite.pos.0, sprite.pos.1).unwrap();
                writeln!(&mut content, "    self.heading = {}", sprite.heading).unwrap();
                writeln!(&mut content, "    self.pen_color = ({}, {}, {})", sprite.color.0, sprite.color.1, sprite.color.2).unwrap();
                writeln!(&mut content, "    self.scale = {}", sprite.scale).unwrap();
                writeln!(&mut content, "    self.visible = {}", if sprite.visible { "True" } else { "False" }).unwrap();

                for (trans_name, info) in sprite.costumes.iter() {
                    writeln!(&mut content, "    self.costumes.add(\'{}\', images.{}_cst_{})", escape(&info.2), sprite.name, trans_name).unwrap();
                }
            }
            match sprite.active_costume {
                Some(idx) => writeln!(&mut content, "    self.costume = '{}'", escape(&sprite.costumes[idx].1.2)).unwrap(),
                None => content += "    self.costume = None\n",
            }
            content.push('\n');

            for func in sprite.funcs.iter() {
                let params = iter::once("self").chain(func.params.iter().map(|v| v.trans_name.as_str()));
                let code = ScriptInfo::new(stage.get().unwrap()).translate_stmts(&func.stmts)?;
                write!(&mut content, "def {}({}):\n{}\n\n", func.trans_name, Punctuated(params, ", "), indent(&code)).unwrap();
            }

            for script in sprite.scripts.iter() {
                content += script;
                content += "\n\n";
            }

            editors.push(json!({
                "type": if i == 0 { "stage" } else { "sprite" },
                "name": sprite.name,
                "value": content,
            }));
        }

        let mut images = serde_json::Map::new();
        for sprite in role_info.sprites.iter() {
            for (costume, info) in sprite.costumes.iter() {
                let center = match info.1 {
                    Some(ui_center) => match image::load_from_memory(&info.0) {
                        Ok(img) => (ui_center.0 - img.width() as f64 / 2.0, -(ui_center.1 - img.height() as f64 / 2.0)),
                        Err(_) => return Err(TranslateError::UnknownImageFormat),
                    }
                    None => (0.0, 0.0),
                };
                images.insert(format!("{}_cst_{}", sprite.name, costume), json!({
                    "img": base64::engine::general_purpose::STANDARD.encode(info.0.as_slice()),
                    "center": center,
                }));
            }
        }

        roles.push(json!({
            "name": role_info.name,
            "stage_size": role.stage_size,
            "block_sources": [ "netsblox://assets/default-blocks.json" ],
            "blocks": [],
            "imports": ["time", "math"],
            "editors": editors,
            "images": images,
        }));
    }

    let res = json!({
        "roles": roles,
    });

    Ok((project.name, res.to_compact_string()))
}
