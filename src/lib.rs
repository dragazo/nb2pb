#![forbid(unsafe_code)]

#[cfg(feature = "pyo3")]
mod python;

use std::fmt::Write;
use std::iter;

use once_cell::unsync::OnceCell;
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

    Upvars,
    AnyMessage,
    RingTypeQuery,
}
impl From<Box<Error>> for TranslateError { fn from(e: Box<Error>) -> Self { Self::Parse(e) } }

fn fmt_comment(comment: Option<&str>) -> String {
    match comment {
        Some(v) => format!(" # {}", v.replace('\n', " -- ")),
        None => "".into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Type {
    Unknown, Wrapped,
}

fn wrap(val: (String, Type)) -> String {
    match &val.1 {
        Type::Wrapped => val.0,
        Type::Unknown => format!("snap.wrap({})", val.0),
    }
}

fn translate_var(var: &VariableRef) -> String {
    match &var.location {
        VarLocation::Local => var.trans_name.clone(),
        VarLocation::Field => format!("self.{}", var.trans_name),
        VarLocation::Global => format!("globals()['{}']", var.trans_name),
    }
}

struct ScriptInfo<'a> {
    stage: &'a SpriteInfo,
}
impl<'a> ScriptInfo<'a> {
    fn new(stage: &'a SpriteInfo) -> Self {
        Self { stage }
    }
    fn translate_value(&mut self, value: &Value) -> Result<(String, Type), TranslateError> {
        Ok(match value {
            Value::String(v) => (format!("'{}'", escape(v)), Type::Unknown),
            Value::Number(v) => (format!("{}", v), Type::Unknown),
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
                (format!("[{}]", Punctuated(items.iter(), ", ")), Type::Unknown)
            }
            Value::Image(_) => unreachable!(),
            Value::Ref(_) => unreachable!(),
        })
    }
    fn translate_kwargs(&mut self, kwargs: &[(String, Expr)], prefix: &str, wrap_vals: bool) -> Result<String, TranslateError> {
        let mut ident_args = vec![];
        let mut non_ident_args = vec![];
        for arg in kwargs {
            let val_raw = self.translate_expr(&arg.1)?;
            let val = if wrap_vals { wrap(val_raw) } else { val_raw.0 };
            match is_py_ident(&arg.0) {
                true => ident_args.push(format!("{} = {}", arg.0, val)),
                false => non_ident_args.push(format!("'{}': {}", escape(&arg.0), val)),
            }
        }

        Ok(match (ident_args.is_empty(), non_ident_args.is_empty()) {
            (false, false) => format!("{}{}, **{{ {} }}", prefix, Punctuated(ident_args.iter(), ", "), Punctuated(non_ident_args.iter(), ", ")),
            (false, true) => format!("{}{}", prefix, Punctuated(ident_args.iter(), ", ")),
            (true, false) => format!("{}**{{ {} }}", prefix, Punctuated(non_ident_args.iter(), ", ")),
            (true, true) => String::new(),
        })
    }
    fn translate_rpc(&mut self, service: &str, rpc: &str, args: &[(String, Expr)]) -> Result<String, TranslateError> {
        let args_str = self.translate_kwargs(args, ", ", false)?;
        Ok(format!("nothrow(nb.call)('{}', '{}'{})", escape(service), escape(rpc), args_str))
    }
    fn translate_fn_call(&mut self, function: &FnRef, args: &[Expr]) -> Result<String, TranslateError> {
        let mut trans_args = Vec::with_capacity(args.len());
        for arg in args.iter() {
            trans_args.push(wrap(self.translate_expr(arg)?));
        }

        Ok(match function.location {
            FnLocation::Global => format!("{}({})", function.trans_name, Punctuated(trans_args.iter(), ", ")),
            FnLocation::Method => format!("self.{}({})", function.trans_name, Punctuated(trans_args.iter(), ", ")),
        })
    }
    fn translate_expr(&mut self, expr: &Expr) -> Result<(String, Type), TranslateError> {
        Ok(match &expr.kind {
            ExprKind::Value(v) => self.translate_value(v)?,
            ExprKind::Variable { var, .. } => (translate_var(var), Type::Wrapped), // all assignments are wrapped, so we can assume vars are wrapped

            ExprKind::This => ("self".into(), Type::Wrapped), // non-primitives are considered wrapped
            ExprKind::Entity { trans_name, .. } => (trans_name.into(), Type::Wrapped), // non-primitives are considered wrapped

            ExprKind::ImageOfEntity { entity } => (format!("{}.get_image()", self.translate_expr(entity)?.0), Type::Wrapped), // non-primitives are considered wrapped
            ExprKind::ImageOfDrawings => (format!("{}.get_drawings()", self.stage.name), Type::Wrapped), // non-primitives are considered wrapped

            ExprKind::IsTouchingEntity { entity } => (format!("self.is_touching({})", self.translate_expr(entity)?.0), Type::Wrapped), // bool is considered wrapped

            ExprKind::MakeList { values } => {
                let trans = values.iter().map(|x| Ok(self.translate_expr(x)?.0)).collect::<Result<Vec<_>,TranslateError>>()?;
                (format!("[{}]", trans.join(", ")), Type::Unknown)
            }

            ExprKind::Neg { value } => (format!("-{}", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Not { value } => (format!("snap.lnot({})", self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::Abs { value } => (format!("abs({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Sign { value } => (format!("snap.sign({})", self.translate_expr(value)?.0), Type::Wrapped),

            ExprKind::Atan2 { y, x } => (format!("snap.atan2({}, {})", self.translate_expr(y)?.0, self.translate_expr(x)?.0), Type::Wrapped),

            ExprKind::Add { values } => match &values.kind {
                ExprKind::Value(Value::List(values, _)) => match values.as_slice() {
                    [] => ("0".into(), Type::Unknown),
                    _ => {
                        let trans = values.iter().map(|x| Ok(wrap(self.translate_value(x)?))).collect::<Result<Vec<_>,TranslateError>>()?;
                        (format!("({})", trans.join(" + ")), Type::Wrapped)
                    }
                }
                _ => (format!("sum({})", wrap(self.translate_expr(values)?)), Type::Unknown),
            }
            ExprKind::Mul { values } => match &values.kind {
                ExprKind::Value(Value::List(values, _)) => match values.as_slice() {
                    [] => ("1".into(), Type::Unknown),
                    _ => {
                        let trans = values.iter().map(|x| Ok(wrap(self.translate_value(x)?))).collect::<Result<Vec<_>,TranslateError>>()?;
                        (format!("({})", trans.join(" * ")), Type::Wrapped)
                    }
                }
                _ => (format!("snap.prod({})", wrap(self.translate_expr(values)?)), Type::Wrapped),
            }

            ExprKind::Min { values } => (format!("min({})", wrap(self.translate_expr(values)?)), Type::Wrapped),
            ExprKind::Max { values } => (format!("max({})", wrap(self.translate_expr(values)?)), Type::Wrapped),

            ExprKind::Sub { left, right } => (format!("({} - {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Div { left, right } => (format!("({} / {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Mod { left, right } => (format!("({} % {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),

            ExprKind::Pow { base, power } => (format!("({} ** {})", wrap(self.translate_expr(base)?), wrap(self.translate_expr(power)?)), Type::Wrapped),
            ExprKind::Log { value, base } => (format!("snap.log({}, {})", self.translate_expr(value)?.0, self.translate_expr(base)?.0), Type::Wrapped),

            ExprKind::Sqrt { value } => (format!("snap.sqrt({})", self.translate_expr(value)?.0), Type::Wrapped),

            ExprKind::Round { value } => (format!("round({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Floor { value } => (format!("math.floor({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Ceil { value } => (format!("math.ceil({})", wrap(self.translate_expr(value)?)), Type::Wrapped),

            ExprKind::Sin { value } => (format!("snap.sin({})", self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::Cos { value } => (format!("snap.cos({})", self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::Tan { value } => (format!("snap.tan({})", self.translate_expr(value)?.0), Type::Wrapped),

            ExprKind::Asin { value } => (format!("snap.asin({})", self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::Acos { value } => (format!("snap.acos({})", self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::Atan { value } => (format!("snap.atan({})", self.translate_expr(value)?.0), Type::Wrapped),

            ExprKind::And { left, right } => (format!("({} and {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Or { left, right } => (format!("({} or {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Conditional { condition, then, otherwise } => {
                let (then, otherwise) = (self.translate_expr(then)?, self.translate_expr(otherwise)?);
                (format!("({} if {} else {})", then.0, wrap(self.translate_expr(condition)?), otherwise.0), if then.1 == otherwise.1 { then.1 } else { Type::Unknown })
            }

            ExprKind::Identical { left, right } => (format!("snap.identical({}, {})", self.translate_expr(left)?.0, self.translate_expr(right)?.0), Type::Wrapped), // bool is considered wrapped

            ExprKind::Less { left, right } => (format!("({} < {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::LessEq { left, right } => (format!("({} <= {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Eq { left, right } => (format!("({} == {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Neq { left, right } => (format!("({} != {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Greater { left, right } => (format!("({} > {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::GreaterEq { left, right } => (format!("({} >= {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),

            ExprKind::Random { a, b } => (format!("snap.rand({}, {})", self.translate_expr(a)?.0, self.translate_expr(b)?.0), Type::Wrapped), // python impl returns wrapped
            ExprKind::Range { start, stop } => (format!("snap.srange({}, {})", self.translate_expr(start)?.0, self.translate_expr(stop)?.0), Type::Wrapped), // python impl returns wrapped

            ExprKind::TextSplit { text, mode } => match mode {
                TextSplitMode::Custom(x) => (format!("snap.split({}, {})", self.translate_expr(text)?.0, self.translate_expr(x)?.0), Type::Wrapped),
                TextSplitMode::LF => (format!("snap.split({}, '\\n')", self.translate_expr(text)?.0), Type::Wrapped),
                TextSplitMode::CR => (format!("snap.split({}, '\\r')", self.translate_expr(text)?.0), Type::Wrapped),
                TextSplitMode::Tab => (format!("snap.split({}, '\\t')", self.translate_expr(text)?.0), Type::Wrapped),
                TextSplitMode::Letter => (format!("snap.split({}, '')", self.translate_expr(text)?.0), Type::Wrapped),
                TextSplitMode::Word => (format!("snap.split_words({})", self.translate_expr(text)?.0), Type::Wrapped),
                TextSplitMode::Csv => (format!("snap.split_csv({})", self.translate_expr(text)?.0), Type::Wrapped),
                TextSplitMode::Json => (format!("snap.split_json({})", self.translate_expr(text)?.0), Type::Wrapped),
            }

            ExprKind::TypeQuery { value, ty } => match ty {
                ValueType::Bool => (format!("snap.is_bool({})", self.translate_expr(value)?.0), Type::Wrapped),
                ValueType::Text => (format!("snap.is_text({})", self.translate_expr(value)?.0), Type::Wrapped),
                ValueType::Number => (format!("snap.is_number({})", self.translate_expr(value)?.0), Type::Wrapped),
                ValueType::List => (format!("snap.is_list({})", self.translate_expr(value)?.0), Type::Wrapped),
                ValueType::Sprite => (format!("snap.is_sprite({})", self.translate_expr(value)?.0), Type::Wrapped),
                ValueType::Costume => (format!("snap.is_costume({})", self.translate_expr(value)?.0), Type::Wrapped),
                ValueType::Sound => (format!("snap.is_sound({})", self.translate_expr(value)?.0), Type::Wrapped),
                ValueType::Command | ValueType::Reporter | ValueType::Predicate => return Err(TranslateError::RingTypeQuery),
            }

            ExprKind::ListCat { lists } => match &lists.kind {
                ExprKind::Value(Value::List(values, _)) => {
                    let trans = values.iter().map(|x| Ok(format!("*{}", self.translate_value(x)?.0))).collect::<Result<Vec<_>,TranslateError>>()?;
                    (format!("[{}]", trans.join(", ")), Type::Unknown)
                }
                _ => (format!("snap.append({})", self.translate_expr(lists)?.0), Type::Unknown),
            }
            ExprKind::StrCat { values } => match &values.kind {
                ExprKind::Value(Value::List(values, _)) => match values.as_slice() {
                    [] => ("''".to_owned(), Type::Unknown),
                    _ => {
                        let trans = values.iter().map(|x| Ok(format!("str({})", wrap(self.translate_value(x)?)))).collect::<Result<Vec<_>,TranslateError>>()?;
                        (format!("({})", trans.join(" + ")), Type::Unknown)
                    }
                }
                _ => (format!("''.join({})", wrap(self.translate_expr(values)?)), Type::Unknown),
            }

            ExprKind::ListLen { value } | ExprKind::StrLen { value } => (format!("len({})", self.translate_expr(value)?.0), Type::Unknown), // builtin __len__ can't be overloaded to return wrapped
            ExprKind::ListFind { list, value } => (format!("{}.index({})", wrap(self.translate_expr(list)?), self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::ListGet { list, index } => (format!("{}[{}]", wrap(self.translate_expr(list)?), self.translate_expr(index)?.0), Type::Wrapped),
            ExprKind::ListGetRandom { list } => (format!("snap.choice({})", wrap(self.translate_expr(list)?)), Type::Wrapped),
            ExprKind::ListGetLast { list } => (format!("{}[-1]", wrap(self.translate_expr(list)?)), Type::Wrapped),
            ExprKind::ListCdr { value } => (format!("{}.all_but_first()", wrap(self.translate_expr(value)?)), Type::Wrapped),

            ExprKind::UnicodeToChar { value } => (format!("snap.get_chr({})", self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::CharToUnicode { value } => (format!("snap.get_ord({})", self.translate_expr(value)?.0), Type::Wrapped),

            ExprKind::CallRpc { service, rpc, args } => (self.translate_rpc(service, rpc, args)?, Type::Unknown),
            ExprKind::CallFn { function, args, upvars } => match upvars.as_slice() {
                [] => (self.translate_fn_call(function, args)?, Type::Wrapped),
                _ => return Err(TranslateError::Upvars),
            }

            ExprKind::XPos => ("self.x_pos".into(), Type::Unknown),
            ExprKind::YPos => ("self.y_pos".into(), Type::Unknown),
            ExprKind::Heading => ("self.heading".into(), Type::Unknown),

            ExprKind::MouseX => (format!("{}.mouse_pos[0]", self.stage.name), Type::Unknown),
            ExprKind::MouseY => (format!("{}.mouse_pos[1]", self.stage.name), Type::Unknown),

            ExprKind::StageWidth => (format!("{}.width", self.stage.name), Type::Unknown),
            ExprKind::StageHeight => (format!("{}.height", self.stage.name), Type::Unknown),

            ExprKind::Latitude => (format!("{}.gps_location[0]", self.stage.name), Type::Unknown),
            ExprKind::Longitude => (format!("{}.gps_location[1]", self.stage.name), Type::Unknown),

            ExprKind::PenDown => ("self.drawing".into(), Type::Wrapped), // bool is considered wrapped

            ExprKind::Size => ("(self.scale * 100)".into(), Type::Wrapped),
            ExprKind::IsVisible => ("self.visible".into(), Type::Wrapped), // bool is considered wrapped

            ExprKind::RpcError => ("(get_error() or '')".into(), Type::Unknown),

            _ => return Err(TranslateError::UnsupportedExpr(Box::new(expr.clone()))),
        })
    }
    fn translate_stmts(&mut self, stmts: &[Stmt]) -> Result<String, TranslateError> {
        if stmts.is_empty() { return Ok("pass".into()) }

        let mut lines = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::DeclareLocals { vars } => lines.extend(vars.iter().map(|x| format!("{} = snap.wrap(0)", x.trans_name))),
                StmtKind::Assign { var, value } => lines.push(format!("{} = {}{}", translate_var(var), wrap(self.translate_expr(value)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::AddAssign { var, value } => lines.push(format!("{} += {}{}", translate_var(var), wrap(self.translate_expr(value)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ListAssign { list, index, value } => lines.push(format!("{}[{}] = {}{}", wrap(self.translate_expr(list)?), self.translate_expr(index)?.0, self.translate_expr(value)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Warp { stmts } => {
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("with Warp():{}\n{}", fmt_comment(stmt.info.comment.as_deref()), indent(&code)));
                }
                StmtKind::If { condition, then } => {
                    let condition = wrap(self.translate_expr(condition)?);
                    let then = self.translate_stmts(then)?;
                    lines.push(format!("if {}:{}\n{}", condition, fmt_comment(stmt.info.comment.as_deref()), indent(&then)));
                }
                StmtKind::IfElse { condition, then, otherwise } => {
                    let condition = wrap(self.translate_expr(condition)?);
                    let then = self.translate_stmts(then)?;
                    let otherwise = self.translate_stmts(otherwise)?;
                    lines.push(format!("if {}:{}\n{}\nelse:\n{}", condition, fmt_comment(stmt.info.comment.as_deref()), indent(&then), indent(&otherwise)));
                }
                StmtKind::InfLoop { stmts } => {
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("while True:{}\n{}", fmt_comment(stmt.info.comment.as_deref()), indent(&code)));
                }
                StmtKind::ForLoop { var, start, stop, stmts } => {
                    let start = self.translate_expr(start)?.0;
                    let stop = self.translate_expr(stop)?.0;
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("for {} in snap.sxrange({}, {}):{}\n{}", var.trans_name, start, stop, fmt_comment(stmt.info.comment.as_deref()), indent(&code)));
                }
                StmtKind::ForeachLoop { var, items, stmts } => {
                    let items = wrap(self.translate_expr(items)?);
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("for {} in {}:{}\n{}", var.trans_name, items, fmt_comment(stmt.info.comment.as_deref()), indent(&code)));
                }
                StmtKind::Repeat { times, stmts } => {
                    let times = wrap(self.translate_expr(times)?);
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("for _ in range(+{}):{}\n{}", times, fmt_comment(stmt.info.comment.as_deref()), indent(&code)));
                }
                StmtKind::UntilLoop { condition, stmts } => {
                    let condition = wrap(self.translate_expr(condition)?);
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("while not {}:{}\n{}", condition, fmt_comment(stmt.info.comment.as_deref()), indent(&code)));
                }
                StmtKind::SetCostume { costume } => {
                    let costume = match costume {
                        Some(v) => self.translate_expr(v)?.0,
                        None => "None".into(),
                    };
                    lines.push(format!("self.costume = {}{}", costume, fmt_comment(stmt.info.comment.as_deref())));
                }

                StmtKind::SetX { value } => lines.push(format!("self.x_pos = {}{}", self.translate_expr(value)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::SetY { value } => lines.push(format!("self.y_pos = {}{}", self.translate_expr(value)?.0, fmt_comment(stmt.info.comment.as_deref()))),

                StmtKind::ChangeX { delta } => lines.push(format!("self.x_pos += {}{}", self.translate_expr(delta)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ChangeY { delta } => lines.push(format!("self.y_pos += {}{}", self.translate_expr(delta)?.0, fmt_comment(stmt.info.comment.as_deref()))),

                StmtKind::Goto { target } => match &target.kind {
                    ExprKind::Value(Value::List(values, _)) if values.len() == 2 => lines.push(format!("self.pos = ({}, {}){}", self.translate_value(&values[0])?.0, self.translate_value(&values[1])?.0, fmt_comment(stmt.info.comment.as_deref()))),
                    _ => lines.push(format!("self.pos = {}{}", self.translate_expr(target)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                }
                StmtKind::SendLocalMessage { target, msg_type, wait } => {
                    if *wait { unimplemented!() }
                    if target.is_some() { unimplemented!() }

                    match &msg_type.kind {
                        ExprKind::Value(Value::String(msg_type)) => lines.push(format!("nb.send_message('local::{}'){}", escape(msg_type), fmt_comment(stmt.info.comment.as_deref()))),
                        _  => lines.push(format!("nb.send_message('local::' + str({})){}", self.translate_expr(msg_type)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                    }
                }
                StmtKind::SendNetworkMessage { target, msg_type, values } => {
                    let kwargs_str = self.translate_kwargs(values, ", ", false)?;
                    lines.push(format!("nb.send_message('{}', {}{}){}", escape(msg_type), self.translate_expr(target)?.0, kwargs_str, fmt_comment(stmt.info.comment.as_deref())));
                }
                StmtKind::Say { content, duration } | StmtKind::Think { content, duration } => match duration {
                    Some(duration) => lines.push(format!("self.say(str({}), duration = {}){}", self.translate_expr(content)?.0, self.translate_expr(duration)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                    None => lines.push(format!("self.say(str({})){}", self.translate_expr(content)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                }
                StmtKind::CallFn { function, args, upvars } => match upvars.as_slice() {
                    [] => lines.push(format!("{}{}", self.translate_fn_call(function, args)?, fmt_comment(stmt.info.comment.as_deref()))),
                    _ => return Err(TranslateError::Upvars),
                }
                StmtKind::ListInsertLast { list, value } => lines.push(format!("{}.append({}){}", wrap(self.translate_expr(list)?), wrap(self.translate_expr(value)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ListRemoveLast { list } => lines.push(format!("{}.pop(){}", wrap(self.translate_expr(list)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ListRemove { list, index } => lines.push(format!("del {}[{}]{}", wrap(self.translate_expr(list)?), self.translate_expr(index)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ListRemoveAll { list } => lines.push(format!("{}.clear(){}", self.translate_expr(list)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ChangePenSize { delta } => lines.push(format!("self.pen_size += {}{}", self.translate_expr(delta)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::SetPenSize { value } => lines.push(format!("self.pen_size = {}{}", self.translate_expr(value)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::SetVisible { value } => lines.push(format!("self.visible = {}{}", if *value { "True" } else { "False" }, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::WaitUntil { condition } => lines.push(format!("while not {}:{}\n    time.sleep(0.05)", wrap(self.translate_expr(condition)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::BounceOffEdge => lines.push(format!("self.keep_on_stage(bounce = True){}", fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Sleep { seconds } => lines.push(format!("time.sleep(+{}){}", wrap(self.translate_expr(seconds)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::RunRpc { service, rpc, args } => lines.push(format!("{}{}", self.translate_rpc(service, rpc, args)?, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Forward { distance } => lines.push(format!("self.forward({}){}", self.translate_expr(distance)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::TurnRight { angle } => lines.push(format!("self.turn_right({}){}", self.translate_expr(angle)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::TurnLeft { angle } => lines.push(format!("self.turn_left({}){}", self.translate_expr(angle)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::SetHeading { value } => lines.push(format!("self.heading = {}{}", self.translate_expr(value)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Return { value } => lines.push(format!("return {}{}", wrap(self.translate_expr(value)?), fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Stamp => lines.push(format!("self.stamp(){}", fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::Write { content, font_size } => lines.push(format!("self.write({}, size = {}){}", self.translate_expr(content)?.0, self.translate_expr(font_size)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::SetPenDown { value } => lines.push(format!("self.drawing = {}{}", if *value { "True" } else { "False" }, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::PenClear => lines.push(format!("{}.clear_drawings(){}", self.stage.name, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::SetPenColor { color } => lines.push(format!("self.pen_color = '#{:02x}{:02x}{:02x}'{}", color.0, color.1, color.2, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::ChangeSize { delta } => lines.push(format!("self.scale += {}{}", self.translate_expr(delta)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                StmtKind::SetSize { value } => lines.push(format!("self.scale = {}{}", self.translate_expr(value)?.0, fmt_comment(stmt.info.comment.as_deref()))),
                x => panic!("{:?}", x),
            }
        }

        Ok(lines.join("\n"))
    }
}

struct RoleInfo {
    name: String,
    sprites: Vec<SpriteInfo>,
}
impl RoleInfo {
    fn new(name: String) -> Self {
        Self { name, sprites: vec![] }
    }
}

#[derive(Clone)]
struct SpriteInfo {
    name: String,
    scripts: Vec<String>,
    fields: Vec<(String, String)>,
    funcs: Vec<Function>,
    costumes: Vec<(String, String)>,

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
    fn translate_hat(&mut self, hat: &Hat, stage: &SpriteInfo) -> Result<String, TranslateError> {
        Ok(match &hat.kind {
            HatKind::OnFlag => format!("@onstart(){}\ndef my_onstart_{}(self):\n", fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
            HatKind::OnKey { key } => format!("@onkey('{}'){}\ndef my_onkey_{}(self):\n", key, fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
            HatKind::MouseDown => format!("@onmouse('down'){}\ndef my_onmouse_{}(self, x, y):\n", fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
            HatKind::MouseUp => format!("@onmouse('up'){}\ndef my_onmouse_{}(self, x, y):\n", fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
            HatKind::ScrollDown => format!("@onmouse('scroll-down'){}\ndef my_onmouse_{}(self, x, y):\n", fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
            HatKind::ScrollUp => format!("@onmouse('scroll-up'){}\ndef my_onmouse_{}(self, x, y):\n", fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
            HatKind::When { condition } => {
                format!(r#"@onstart(){comment}
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
                Some(msg_type) => format!("@nb.on_message('local::{}'){}\ndef my_on_message_{}(self, **kwargs):\n", escape(msg_type), fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1),
                None => return Err(TranslateError::AnyMessage),
            }
            HatKind::NetworkMessage { msg_type, fields } => {
                let mut res = format!("@nb.on_message('{}'){}\ndef my_on_message_{}(self, **kwargs):\n", escape(msg_type), fmt_comment(hat.info.comment.as_deref()), self.scripts.len() + 1);
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
pub fn translate(source: &str) -> Result<(String, String), TranslateError> {
    let parser = Parser::default();
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
                let content = match &costume.init {
                    Value::String(s) => s,
                    _ => panic!(), // the parser lib would never do this
                };
                sprite_info.costumes.push((costume.def.trans_name.clone(), content.clone()));
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
                let res = format!("{}{}", func_def, indent(&body));
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

            if !sprite.costumes.is_empty() {
                content += "costumes = {\n";
                for (costume, _) in sprite.costumes.iter() {
                    writeln!(&mut content, "    '{}': images.{}_cst_{},", costume, sprite.name, costume).unwrap();
                }
                content += "}\n\n";
            }

            for (field, value) in sprite.fields.iter() {
                writeln!(&mut content, "{} = {}", field, value).unwrap();
            }
            if !sprite.fields.is_empty() { content.push('\n'); }

            content += "def __init__(self):\n";
            if i != 0 { // don't generate these for stage
                writeln!(&mut content, "    self.pos = ({}, {})", sprite.pos.0, sprite.pos.1).unwrap();
                writeln!(&mut content, "    self.heading = {}", sprite.heading).unwrap();
                writeln!(&mut content, "    self.pen_color = ({}, {}, {})", sprite.color.0, sprite.color.1, sprite.color.2).unwrap();
                writeln!(&mut content, "    self.scale = {}", sprite.scale).unwrap();
                writeln!(&mut content, "    self.visible = {}", if sprite.visible { "True" } else { "False" }).unwrap();
            }
            match sprite.active_costume {
                Some(idx) => writeln!(&mut content, "    self.costume = self.costumes['{}']", sprite.costumes[idx].0).unwrap(),
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
                "type": if i == 0 { "stage" } else { "turtle" },
                "name": sprite.name,
                "value": content,
            }));
        }

        let mut images = serde_json::Map::new();
        for sprite in role_info.sprites.iter() {
            for (costume, content) in sprite.costumes.iter() {
                images.insert(format!("{}_cst_{}", sprite.name, costume), json!(content.clone()));
            }
        }

        roles.push(json!({
            "name": role_info.name,
            "stage_size": role.stage_size,
            "block_sources": [ "netsblox://assets/default-blocks.json" ],
            "blocks": {
                "global": [],
                "stage": [],
                "turtle": [],
            },
            "imports": ["time", "math"],
            "editors": editors,
            "images": images,
        }));
    }

    let res = json!({
        "show_blocks": true,
        "roles": roles,
    });

    Ok((project.name, res.to_string()))
}
