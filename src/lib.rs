#![forbid(unsafe_code)]

#[cfg(feature = "pyo3")]
mod python;

use std::fmt::Write;
use std::rc::Rc;
use std::iter;

use once_cell::unsync::OnceCell;
use regex::Regex;

#[macro_use] extern crate serde_json;
#[macro_use] extern crate lazy_static;

pub use netsblox_ast::Error as ParseError;
use netsblox_ast::{*, util::*};

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
    ParseError(ParseError),
    NoRoles,

    UnsupportedBlock(&'static str),
}
impl From<ParseError> for TranslateError { fn from(e: ParseError) -> Self { Self::ParseError(e) } }

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
            Value::List(vals) => {
                let mut items = Vec::with_capacity(vals.len());
                for val in vals {
                    items.push(self.translate_value(val)?.0);
                }
                (format!("[{}]", Punctuated(items.iter(), ", ")), Type::Unknown)
            }
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
    fn translate_rpc(&mut self, service: &str, rpc: &str, args: &[(String, Expr)], comment: Option<&str>) -> Result<String, TranslateError> {
        let args_str = self.translate_kwargs(args, ", ", false)?;
        Ok(format!("nothrow(nb.call)('{}', '{}'{}){}", escape(service), escape(rpc), args_str, fmt_comment(comment)))
    }
    fn translate_fn_call(&mut self, function: &FnRef, args: &[Expr], comment: Option<&str>) -> Result<String, TranslateError> {
        let mut trans_args = Vec::with_capacity(args.len());
        for arg in args.iter() {
            trans_args.push(wrap(self.translate_expr(arg)?));
        }

        Ok(match function.location {
            FnLocation::Global => format!("{}(self, {}){}", function.trans_name, Punctuated(trans_args.iter(), ", "), fmt_comment(comment)),
            FnLocation::Method => format!("self.{}({}){}", function.trans_name, Punctuated(trans_args.iter(), ", "), fmt_comment(comment)),
        })
    }
    fn translate_expr(&mut self, expr: &Expr) -> Result<(String, Type), TranslateError> {
        Ok(match expr {
            Expr::Value(v) => self.translate_value(v)?,
            Expr::Variable { var, .. } => (translate_var(var), Type::Wrapped), // all assignments are wrapped, so we can assume vars are wrapped

            Expr::Closure { .. } => unimplemented!(),
            Expr::CallClosure { .. } => unimplemented!(),

            Expr::This { .. } => ("self".into(), Type::Wrapped), // non-primitives are considered wrapped
            Expr::Entity { trans_name, .. } => (trans_name.into(), Type::Wrapped), // non-primitives are considered wrapped

            Expr::ImageOfEntity { entity, .. } => (format!("{}.get_image()", self.translate_expr(entity)?.0), Type::Wrapped), // non-primitives are considered wrapped
            Expr::ImageOfDrawings { .. } => (format!("{}.get_drawings()", self.stage.name), Type::Wrapped), // non-primitives are considered wrapped

            Expr::IsTouchingEntity { entity, .. } => (format!("self.is_touching({})", self.translate_expr(entity)?.0), Type::Wrapped), // bool is considered wrapped
            Expr::IsTouchingMouse { .. } => unimplemented!(),
            Expr::IsTouchingEdge { .. } => unimplemented!(),
            Expr::IsTouchingDrawings { .. } => unimplemented!(),

            Expr::MakeList { values, .. } => {
                let mut items = Vec::with_capacity(values.len());
                for val in values {
                    items.push(self.translate_expr(val)?.0);
                }
                (format!("[{}]", items.join(", ")), Type::Unknown)
            }

            Expr::Neg { value, .. } => (format!("-{}", wrap(self.translate_expr(value)?)), Type::Wrapped),
            Expr::Not { value, .. } => (format!("snap.lnot({})", self.translate_expr(value)?.0), Type::Wrapped),
            Expr::Abs { value, .. } => (format!("abs({})", wrap(self.translate_expr(value)?)), Type::Wrapped),

            Expr::Add { left, right, .. } => (format!("({} + {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Sub { left, right, .. } => (format!("({} - {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Mul { left, right, .. } => (format!("({} * {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Div { left, right, .. } => (format!("({} / {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Mod { left, right, .. } => (format!("({} % {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),

            Expr::Pow { base, power, .. } => (format!("({} ** {})", wrap(self.translate_expr(base)?), wrap(self.translate_expr(power)?)), Type::Wrapped),
            Expr::Log { value, base, .. } => (format!("snap.log({}, {})", wrap(self.translate_expr(value)?), wrap(self.translate_expr(base)?)), Type::Wrapped),

            Expr::Sqrt { value, .. } => (format!("snap.sqrt({})", self.translate_expr(value)?.0), Type::Wrapped),

            Expr::Round { value, .. } => (format!("round({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            Expr::Floor { value, .. } => (format!("math.floor({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            Expr::Ceil { value, .. } => (format!("math.ceil({})", wrap(self.translate_expr(value)?)), Type::Wrapped),

            Expr::Sin { value, .. } => (format!("snap.sin({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            Expr::Cos { value, .. } => (format!("snap.cos({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            Expr::Tan { value, .. } => (format!("snap.tan({})", wrap(self.translate_expr(value)?)), Type::Wrapped),

            Expr::Asin { value, .. } => (format!("snap.asin({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            Expr::Acos { value, .. } => (format!("snap.acos({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            Expr::Atan { value, .. } => (format!("snap.atan({})", wrap(self.translate_expr(value)?)), Type::Wrapped),

            Expr::And { left, right, .. } => (format!("({} and {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Or { left, right, .. } => (format!("({} or {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Conditional { condition, then, otherwise, .. } => {
                let (then, otherwise) = (self.translate_expr(then)?, self.translate_expr(otherwise)?);
                (format!("({} if {} else {})", then.0, wrap(self.translate_expr(condition)?), otherwise.0), if then.1 == otherwise.1 { then.1 } else { Type::Unknown })
            }

            Expr::Identical { left, right, .. } => (format!("snap.identical({}, {})", self.translate_expr(left)?.0, self.translate_expr(right)?.0), Type::Wrapped), // bool is considered wrapped
            Expr::Eq { left, right, .. } => (format!("({} == {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Less { left, right, .. } => (format!("({} < {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Greater { left, right, .. } => (format!("({} > {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),

            Expr::RandInclusive { a, b, .. } => (format!("snap.rand({}, {})", self.translate_expr(a)?.0, self.translate_expr(b)?.0), Type::Wrapped), // python impl returns wrapped
            Expr::RangeInclusive { start, stop, .. } => (format!("snap.srange({}, {})", self.translate_expr(start)?.0, self.translate_expr(stop)?.0), Type::Wrapped), // python impl returns wrapped

            Expr::Listcat { lists, .. } => {
                let mut trans = Vec::with_capacity(lists.len());
                for list in lists {
                    trans.push(self.translate_expr(list)?.0);
                }
                (format!("[{}]", Punctuated(trans.iter().map(|s| format!("*{}", s)), ", ")), Type::Unknown)
            }
            Expr::Strcat { values, .. } => {
                let mut trans = Vec::with_capacity(values.len());
                for list in values {
                    trans.push(self.translate_expr(list)?.0);
                }
                (Punctuated(trans.iter().map(|s| format!("str({})", s)), " + ").to_string(), Type::Unknown)
            }

            Expr::Listlen { value, .. } | Expr::Strlen { value, .. } => (format!("len({})", self.translate_expr(value)?.0), Type::Unknown), // builtin __len__ can't be overloaded to return wrapped
            Expr::ListFind { list, value, .. } => (format!("{}.index({})", wrap(self.translate_expr(list)?), self.translate_expr(value)?.0), Type::Wrapped),
            Expr::ListIndex { list, index, .. } => (format!("{}[{}]", wrap(self.translate_expr(list)?), self.translate_expr(index)?.0), Type::Wrapped),
            Expr::ListRandIndex { list, .. } => (format!("snap.choice({})", wrap(self.translate_expr(list)?)), Type::Wrapped),
            Expr::ListLastIndex { list, .. } => (format!("{}[-1]", wrap(self.translate_expr(list)?)), Type::Wrapped),
            Expr::ListAllButFirst { value, .. } => (format!("{}.all_but_first()", wrap(self.translate_expr(value)?)), Type::Wrapped),

            Expr::UnicodeToChar { value, .. } => (format!("snap.get_chr({})", self.translate_expr(value)?.0), Type::Unknown),
            Expr::CharToUnicode { value, .. } => (format!("snap.get_ord({})", self.translate_expr(value)?.0), Type::Unknown),

            Expr::CallRpc { service, rpc, args, .. } => (self.translate_rpc(service, rpc, args, None)?, Type::Unknown),
            Expr::CallFn { function, args, .. } => (self.translate_fn_call(function, args, None)?, Type::Wrapped),

            Expr::XPos { .. } => ("self.x_pos".into(), Type::Unknown),
            Expr::YPos { .. } => ("self.y_pos".into(), Type::Unknown),
            Expr::Heading { .. } => ("self.heading".into(), Type::Unknown),

            Expr::MouseX { .. } => (format!("{}.mouse_pos[0]", self.stage.name), Type::Unknown),
            Expr::MouseY { .. } => (format!("{}.mouse_pos[1]", self.stage.name), Type::Unknown),

            Expr::StageWidth { .. } => (format!("{}.width", self.stage.name), Type::Unknown),
            Expr::StageHeight { .. } => (format!("{}.height", self.stage.name), Type::Unknown),

            Expr::Latitude { .. } => (format!("{}.gps_location[0]", self.stage.name), Type::Unknown),
            Expr::Longitude { .. } => (format!("{}.gps_location[1]", self.stage.name), Type::Unknown),

            Expr::PenDown { .. } => ("self.drawing".into(), Type::Wrapped), // bool is considered wrapped

            Expr::Scale { .. } => ("(self.scale * 100)".into(), Type::Wrapped),
            Expr::IsVisible { .. } => ("self.visible".into(), Type::Wrapped), // bool is considered wrapped

            Expr::RpcError { .. } => ("(get_error() or '')".into(), Type::Unknown),
        })
    }
    fn translate_stmts(&mut self, stmts: &[Stmt]) -> Result<String, TranslateError> {
        if stmts.is_empty() { return Ok("pass".into()) }

        let mut lines = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            match stmt {
                Stmt::Assign { var, value, comment } => lines.push(format!("{} = {}{}", translate_var(var), wrap(self.translate_expr(value)?), fmt_comment(comment.as_deref()))),
                Stmt::AddAssign { var, value, comment } => lines.push(format!("{} += {}{}", translate_var(var), wrap(self.translate_expr(value)?), fmt_comment(comment.as_deref()))),
                Stmt::IndexAssign { list, index, value, comment } => lines.push(format!("{}[{}] = {}{}", wrap(self.translate_expr(list)?), self.translate_expr(index)?.0, self.translate_expr(value)?.0, fmt_comment(comment.as_deref()))),
                Stmt::Warp { stmts, comment } => {
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("with Warp():{}\n{}", fmt_comment(comment.as_deref()), indent(&code)));
                }
                Stmt::If { condition, then, comment } => {
                    let condition = wrap(self.translate_expr(condition)?);
                    let then = self.translate_stmts(then)?;
                    lines.push(format!("if {}:{}\n{}", condition, fmt_comment(comment.as_deref()), indent(&then)));
                }
                Stmt::IfElse { condition, then, otherwise, comment } => {
                    let condition = wrap(self.translate_expr(condition)?);
                    let then = self.translate_stmts(then)?;
                    let otherwise = self.translate_stmts(otherwise)?;
                    lines.push(format!("if {}:{}\n{}\nelse:\n{}", condition, fmt_comment(comment.as_deref()), indent(&then), indent(&otherwise)));
                }
                Stmt::InfLoop { stmts, comment } => {
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("while True:{}\n{}", fmt_comment(comment.as_deref()), indent(&code)));
                }
                Stmt::ForLoop { var, start, stop, stmts, comment } => {
                    let start = self.translate_expr(start)?.0;
                    let stop = self.translate_expr(stop)?.0;
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("for {} in snap.sxrange({}, {}):{}\n{}", var.trans_name, start, stop, fmt_comment(comment.as_deref()), indent(&code)));
                }
                Stmt::ForeachLoop { var, items, stmts, comment } => {
                    let items = wrap(self.translate_expr(items)?);
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("for {} in {}:{}\n{}", var.trans_name, items, fmt_comment(comment.as_deref()), indent(&code)));
                }
                Stmt::Repeat { times, stmts, comment } => {
                    let times = wrap(self.translate_expr(times)?);
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("for _ in range(+{}):{}\n{}", times, fmt_comment(comment.as_deref()), indent(&code)));
                }
                Stmt::UntilLoop { condition, stmts, comment } => {
                    let condition = wrap(self.translate_expr(condition)?);
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("while not {}:{}\n{}", condition, fmt_comment(comment.as_deref()), indent(&code)));
                }
                Stmt::SwitchCostume { costume, comment } => {
                    let costume = match costume {
                        Some(v) => self.translate_expr(v)?.0,
                        None => "None".into(),
                    };
                    lines.push(format!("self.costume = {}{}", costume, fmt_comment(comment.as_deref())));
                }
                Stmt::ChangePos { dx, dy, comment } => {
                    let mut comment = comment.as_deref();
                    for (var, val) in [("x_pos", dx), ("y_pos", dy)] {
                        if let Some(val) = val {
                            lines.push(format!("self.{} += {}{}", var, self.translate_expr(val)?.0, fmt_comment(comment.take())));
                        }
                    }
                }
                Stmt::SetPos { x, y, comment } => match (x, y) {
                    (Some(x), Some(y)) => lines.push(format!("self.pos = ({}, {}){}", self.translate_expr(x)?.0, self.translate_expr(y)?.0, fmt_comment(comment.as_deref()))),
                    (Some(x), None) => lines.push(format!("self.x_pos = {}{}", self.translate_expr(x)?.0, fmt_comment(comment.as_deref()))),
                    (None, Some(y)) => lines.push(format!("self.y_pos = {}{}", self.translate_expr(y)?.0, fmt_comment(comment.as_deref()))),
                    (None, None) => (), // the parser would never emit this, but it's not like it would matter...
                }
                Stmt::SendLocalMessage { target, msg_type, wait, comment } => {
                    if *wait { unimplemented!() }
                    if target.is_some() { unimplemented!() }

                    match msg_type {
                        Expr::Value(Value::String(msg_type)) => lines.push(format!("nb.send_message('local::{}'){}", escape(msg_type), fmt_comment(comment.as_deref()))),
                        _  => lines.push(format!("nb.send_message('local::' + str({})){}", self.translate_expr(msg_type)?.0, fmt_comment(comment.as_deref()))),
                    }
                }
                Stmt::SendNetworkMessage { target, msg_type, values, comment } => {
                    let kwargs_str = self.translate_kwargs(values, ", ", false)?;
                    lines.push(format!("nb.send_message('{}', {}{}){}", escape(msg_type), self.translate_expr(target)?.0, kwargs_str, fmt_comment(comment.as_deref())));
                }
                Stmt::Say { content, comment, duration } | Stmt::Think { content, comment, duration } => match duration {
                    Some(duration) => lines.push(format!("self.say(str({}), duration = {}){}", self.translate_expr(content)?.0, self.translate_expr(duration)?.0, fmt_comment(comment.as_deref()))),
                    None => lines.push(format!("self.say(str({})){}", self.translate_expr(content)?.0, fmt_comment(comment.as_deref()))),
                }
                Stmt::Push { list, value, comment } => lines.push(format!("{}.append({}){}", wrap(self.translate_expr(list)?), wrap(self.translate_expr(value)?), fmt_comment(comment.as_deref()))),
                Stmt::Pop { list, comment } => lines.push(format!("{}.pop(){}", wrap(self.translate_expr(list)?), fmt_comment(comment.as_deref()))),
                Stmt::RemoveAt { list, index, comment } => lines.push(format!("del {}[{}]{}", wrap(self.translate_expr(list)?), self.translate_expr(index)?.0, fmt_comment(comment.as_deref()))),
                Stmt::RemoveAll { list, comment } => lines.push(format!("{}.clear(){}", self.translate_expr(list)?.0, fmt_comment(comment.as_deref()))),
                Stmt::ChangePenSize { amount, comment } => lines.push(format!("self.pen_size += {}{}", self.translate_expr(amount)?.0, fmt_comment(comment.as_deref()))),
                Stmt::SetPenSize { value, comment } => lines.push(format!("self.pen_size = {}{}", self.translate_expr(value)?.0, fmt_comment(comment.as_deref()))),
                Stmt::SetVisible { value, comment } => lines.push(format!("self.visible = {}{}", if *value { "True" } else { "False" }, fmt_comment(comment.as_deref()))),
                Stmt::WaitUntil { condition, comment } => lines.push(format!("while not {}:{}\n    time.sleep(0.05)", wrap(self.translate_expr(condition)?), fmt_comment(comment.as_deref()))),
                Stmt::BounceOffEdge { comment } => lines.push(format!("self.keep_on_stage(bounce = True){}", fmt_comment(comment.as_deref()))),
                Stmt::Sleep { seconds, comment } => lines.push(format!("time.sleep(+{}){}", wrap(self.translate_expr(seconds)?), fmt_comment(comment.as_deref()))),
                Stmt::Goto { target, comment } => lines.push(format!("self.goto({}){}", self.translate_expr(target)?.0, fmt_comment(comment.as_deref()))),
                Stmt::RunRpc { service, rpc, args, comment } => lines.push(self.translate_rpc(service, rpc, args, comment.as_deref())?),
                Stmt::RunFn { function, args, comment } => lines.push(self.translate_fn_call(function, args, comment.as_deref())?),
                Stmt::Forward { distance, comment } => lines.push(format!("self.forward({}){}", self.translate_expr(distance)?.0, fmt_comment(comment.as_deref()))),
                Stmt::TurnRight { angle, comment } => lines.push(format!("self.turn_right({}){}", self.translate_expr(angle)?.0, fmt_comment(comment.as_deref()))),
                Stmt::TurnLeft { angle, comment } => lines.push(format!("self.turn_left({}){}", self.translate_expr(angle)?.0, fmt_comment(comment.as_deref()))),
                Stmt::SetHeading { value, comment } => lines.push(format!("self.heading = {}{}", self.translate_expr(value)?.0, fmt_comment(comment.as_deref()))),
                Stmt::Return { value, comment } => lines.push(format!("return {}{}", wrap(self.translate_expr(value)?), fmt_comment(comment.as_deref()))),
                Stmt::Stamp { comment } => lines.push(format!("self.stamp(){}", fmt_comment(comment.as_deref()))),
                Stmt::Write { content, font_size, comment } => lines.push(format!("self.write({}, size = {}){}", self.translate_expr(content)?.0, self.translate_expr(font_size)?.0, fmt_comment(comment.as_deref()))),
                Stmt::PenDown { comment } => lines.push(format!("self.drawing = True{}", fmt_comment(comment.as_deref()))),
                Stmt::PenUp { comment } => lines.push(format!("self.drawing = False{}", fmt_comment(comment.as_deref()))),
                Stmt::PenClear { comment } => lines.push(format!("{}.clear_drawings(){}", self.stage.name, fmt_comment(comment.as_deref()))),
                Stmt::SetPenColor { color, comment } => lines.push(format!("self.pen_color = '#{:02x}{:02x}{:02x}'{}", color.0, color.1, color.2, fmt_comment(comment.as_deref()))),
                Stmt::ChangeScalePercent { amount, comment } => lines.push(format!("self.scale += {}{}", self.translate_expr(amount)?.0, fmt_comment(comment.as_deref()))),
                Stmt::SetScalePercent { value, comment } => lines.push(format!("self.scale = {}{}", self.translate_expr(value)?.0, fmt_comment(comment.as_deref()))),
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
    color: (u8, u8, u8),
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
        Ok(match hat {
            Hat::OnFlag { comment } => format!("@onstart(){}\ndef my_onstart_{}(self):\n", fmt_comment(comment.as_deref()), self.scripts.len() + 1),
            Hat::OnKey { key, comment } => format!("@onkey('{}'){}\ndef my_onkey_{}(self):\n", key, fmt_comment(comment.as_deref()), self.scripts.len() + 1),
            Hat::MouseDown { comment } => format!("@onmouse('down'){}\ndef my_onmouse_{}(self, x, y):\n", fmt_comment(comment.as_deref()), self.scripts.len() + 1),
            Hat::MouseUp { comment } => format!("@onmouse('up'){}\ndef my_onmouse_{}(self, x, y):\n", fmt_comment(comment.as_deref()), self.scripts.len() + 1),
            Hat::MouseEnter { .. } => return Err(TranslateError::UnsupportedBlock("mouseenter interactions are not currently supported")),
            Hat::MouseLeave { .. } => return Err(TranslateError::UnsupportedBlock("mouseleave interactions are not currently supported")),
            Hat::ScrollDown { comment } => format!("@onmouse('scroll-down'){}\ndef my_onmouse_{}(self, x, y):\n", fmt_comment(comment.as_deref()), self.scripts.len() + 1),
            Hat::ScrollUp { comment } => format!("@onmouse('scroll-up'){}\ndef my_onmouse_{}(self, x, y):\n", fmt_comment(comment.as_deref()), self.scripts.len() + 1),
            Hat::Dropped { .. } => return Err(TranslateError::UnsupportedBlock("drop interactions are not currently supported")),
            Hat::Stopped { .. } => return Err(TranslateError::UnsupportedBlock("stop interactions are not currently supported")),
            Hat::When { condition, comment } => {
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
                comment = fmt_comment(comment.as_deref()),
                idx = self.scripts.len() + 1,
                condition = wrap(ScriptInfo::new(stage).translate_expr(condition)?))
            }
            Hat::LocalMessage { msg_type, comment } => {
                format!("@nb.on_message('local::{}'){}\ndef my_on_message_{}(self, **kwargs):\n", escape(msg_type), fmt_comment(comment.as_deref()), self.scripts.len() + 1)
            }
            Hat::NetworkMessage { msg_type, fields, comment } => {
                let mut res = format!("@nb.on_message('{}'){}\ndef my_on_message_{}(self, **kwargs):\n", escape(msg_type), fmt_comment(comment.as_deref()), self.scripts.len() + 1);
                for field in fields {
                    writeln!(&mut res, "    {} = snap.wrap(kwargs['{}'])", field.trans_name, escape(&field.name)).unwrap();
                }
                if !fields.is_empty() { res.push('\n') }
                res
            }
        })
    }
}

/// Translates NetsBlox project XML into PyBlox project JSON
///
/// On success, returns the project name and project json content as a tuple.
pub fn translate(source: &str) -> Result<(String, String), TranslateError> {
    let parser = ParserBuilder::default()
        .name_transformer(Rc::new(&c_ident))
        .adjust_to_zero_index(true)
        .optimize(true)
        .build().unwrap();

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
                let content = match &costume.value {
                    Value::String(s) => s,
                    _ => panic!(), // the parser lib would never do this
                };
                sprite_info.costumes.push((costume.trans_name.clone(), content.clone()));
            }
            for field in sprite.fields.iter() {
                let value = wrap(ScriptInfo::new(stage.get().unwrap()).translate_value(&field.value)?);
                sprite_info.fields.push((field.trans_name.clone(), value));
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
            let value = wrap(ScriptInfo::new(stage.get().unwrap()).translate_value(&global.value)?);
            writeln!(&mut content, "{} = {}", global.trans_name, value).unwrap();
        }
        if !role.globals.is_empty() { content.push('\n') }
        for func in role.funcs.iter() {
            let params = iter::once("self").chain(func.params.iter().map(|v| v.trans_name.as_str()));
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
