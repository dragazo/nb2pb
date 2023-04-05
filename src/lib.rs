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
            Value::List(vals, _) => {
                let mut items = Vec::with_capacity(vals.len());
                for val in vals {
                    items.push(self.translate_value(val)?.0);
                }
                (format!("[{}]", Punctuated(items.iter(), ", ")), Type::Unknown)
            }
            Value::Ref(x) => unimplemented!("{x:?}"),
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
    fn translate_rpc(&mut self, service: &str, rpc: &str, args: &[(String, Expr)], comment_suffix: &str) -> Result<String, TranslateError> {
        let args_str = self.translate_kwargs(args, ", ", false)?;
        Ok(format!("nothrow(nb.call)('{}', '{}'{}){}", escape(service), escape(rpc), args_str, comment_suffix))
    }
    fn translate_fn_call(&mut self, function: &FnRef, args: &[Expr], comment_suffix: &str) -> Result<String, TranslateError> {
        let mut trans_args = Vec::with_capacity(args.len());
        for arg in args.iter() {
            trans_args.push(wrap(self.translate_expr(arg)?));
        }

        Ok(match function.location {
            FnLocation::Global => format!("{}(self, {}){}", function.trans_name, Punctuated(trans_args.iter(), ", "), comment_suffix),
            FnLocation::Method => format!("self.{}({}){}", function.trans_name, Punctuated(trans_args.iter(), ", "), comment_suffix),
        })
    }
    fn translate_variadic_bin_expr(&mut self, values: &VariadicInput, mapper: fn((String, Type)) -> (String, Type), single_op: &str, prefix_suffix: (&str, &str), default: fn() -> (String, Type)) -> Result<(String, Type), TranslateError> {
        match values {
            VariadicInput::Fixed(values) => match values.as_slice() {
                [] => Ok(default()),
                [val] => Ok(self.translate_expr(val)?),
                [first, rest @ ..] => {
                    let mut res = String::new();
                    res.push_str(prefix_suffix.0);
                    res.push_str(&mapper(self.translate_expr(first)?).0);
                    for value in rest {
                        res.push_str(single_op);
                        res.push_str(&wrap(self.translate_expr(value)?));
                    }
                    res.push_str(prefix_suffix.1);
                    Ok((res, Type::Wrapped))
                }
            }
            VariadicInput::VarArgs(_) => unimplemented!("variadic binary ops ({single_op})"),
        }
    }
    fn translate_expr(&mut self, expr: &Expr) -> Result<(String, Type), TranslateError> {
        Ok(match &expr.kind {
            ExprKind::Value(v) => self.translate_value(v)?,
            ExprKind::Variable { var } => (translate_var(var), Type::Wrapped), // all assignments are wrapped, so we can assume vars are wrapped

            ExprKind::This { .. } => ("self".into(), Type::Wrapped), // non-primitives are considered wrapped
            ExprKind::Entity { trans_name, .. } => (trans_name.into(), Type::Wrapped), // non-primitives are considered wrapped

            ExprKind::ImageOfEntity { entity } => (format!("{}.get_image()", self.translate_expr(entity)?.0), Type::Wrapped), // non-primitives are considered wrapped
            ExprKind::ImageOfDrawings { .. } => (format!("{}.get_drawings()", self.stage.name), Type::Wrapped), // non-primitives are considered wrapped

            ExprKind::IsTouchingEntity { entity } => (format!("self.is_touching({})", self.translate_expr(entity)?.0), Type::Wrapped), // bool is considered wrapped

            ExprKind::MakeList { values } => self.translate_variadic_bin_expr(values, |x| x, ", ", ("[", "]"), || ("[]".into(), Type::Unknown))?,

            ExprKind::Neg { value } => (format!("-{}", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Not { value } => (format!("snap.lnot({})", self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::Abs { value } => (format!("abs({})", wrap(self.translate_expr(value)?)), Type::Wrapped),

            ExprKind::Add { values } => self.translate_variadic_bin_expr(values, |x| x, " + ", ("(", ")"), || ("0".into(), Type::Unknown))?,
            ExprKind::Mul { values } => self.translate_variadic_bin_expr(values, |x| x, " * ", ("(", ")"), || ("1".into(), Type::Unknown))?,

            ExprKind::Sub { left, right } => (format!("({} - {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Div { left, right } => (format!("({} / {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Mod { left, right } => (format!("({} % {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),

            ExprKind::Pow { base, power } => (format!("({} ** {})", wrap(self.translate_expr(base)?), wrap(self.translate_expr(power)?)), Type::Wrapped),
            ExprKind::Log { value, base } => (format!("snap.log({}, {})", wrap(self.translate_expr(value)?), wrap(self.translate_expr(base)?)), Type::Wrapped),

            ExprKind::Sqrt { value } => (format!("snap.sqrt({})", self.translate_expr(value)?.0), Type::Wrapped),

            ExprKind::Round { value } => (format!("round({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Floor { value } => (format!("math.floor({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Ceil { value } => (format!("math.ceil({})", wrap(self.translate_expr(value)?)), Type::Wrapped),

            ExprKind::Sin { value } => (format!("snap.sin({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Cos { value } => (format!("snap.cos({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Tan { value } => (format!("snap.tan({})", wrap(self.translate_expr(value)?)), Type::Wrapped),

            ExprKind::Asin { value } => (format!("snap.asin({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Acos { value } => (format!("snap.acos({})", wrap(self.translate_expr(value)?)), Type::Wrapped),
            ExprKind::Atan { value } => (format!("snap.atan({})", wrap(self.translate_expr(value)?)), Type::Wrapped),

            ExprKind::And { left, right } => (format!("({} and {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Or { left, right } => (format!("({} or {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Conditional { condition, then, otherwise } => {
                let (then, otherwise) = (self.translate_expr(then)?, self.translate_expr(otherwise)?);
                (format!("({} if {} else {})", then.0, wrap(self.translate_expr(condition)?), otherwise.0), if then.1 == otherwise.1 { then.1 } else { Type::Unknown })
            }

            ExprKind::Identical { left, right } => (format!("snap.identical({}, {})", self.translate_expr(left)?.0, self.translate_expr(right)?.0), Type::Wrapped), // bool is considered wrapped
            ExprKind::Eq { left, right } => (format!("({} == {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Less { left, right } => (format!("({} < {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            ExprKind::Greater { left, right } => (format!("({} > {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),

            ExprKind::Random { a, b } => (format!("snap.rand({}, {})", self.translate_expr(a)?.0, self.translate_expr(b)?.0), Type::Wrapped), // python impl returns wrapped
            ExprKind::Range { start, stop } => (format!("snap.srange({}, {})", self.translate_expr(start)?.0, self.translate_expr(stop)?.0), Type::Wrapped), // python impl returns wrapped

            ExprKind::ListCat { lists } => self.translate_variadic_bin_expr(lists, |x| (format!("*{}", x.0), Type::Unknown), ", ", ("[", "]"), || ("[]".into(), Type::Unknown))?,
            ExprKind::StrCat { values } => self.translate_variadic_bin_expr(values, |x| (format!("str({})", x.0), Type::Unknown), " + ", ("(", ")"), || ("''".into(), Type::Unknown))?,

            ExprKind::ListLength { value } | ExprKind::StrLen { value } => (format!("len({})", self.translate_expr(value)?.0), Type::Unknown), // builtin __len__ can't be overloaded to return wrapped
            ExprKind::ListFind { list, value } => (format!("{}.index({})", wrap(self.translate_expr(list)?), self.translate_expr(value)?.0), Type::Wrapped),
            ExprKind::ListGet { list, index } => (format!("{}[{}]", wrap(self.translate_expr(list)?), self.translate_expr(index)?.0), Type::Wrapped),
            ExprKind::ListGetRandom { list } => (format!("snap.choice({})", wrap(self.translate_expr(list)?)), Type::Wrapped),
            ExprKind::ListGetLast { list } => (format!("{}[-1]", wrap(self.translate_expr(list)?)), Type::Wrapped),
            ExprKind::ListCdr { value } => (format!("{}.all_but_first()", wrap(self.translate_expr(value)?)), Type::Wrapped),

            ExprKind::UnicodeToChar { value } => (format!("snap.get_chr({})", self.translate_expr(value)?.0), Type::Unknown),
            ExprKind::CharToUnicode { value } => (format!("snap.get_ord({})", self.translate_expr(value)?.0), Type::Unknown),

            ExprKind::CallRpc { service, rpc, args } => (self.translate_rpc(service, rpc, args, "")?, Type::Unknown),
            ExprKind::CallFn { function, args } => (self.translate_fn_call(function, args, "")?, Type::Wrapped),

            ExprKind::XPos { .. } => ("self.x_pos".into(), Type::Unknown),
            ExprKind::YPos { .. } => ("self.y_pos".into(), Type::Unknown),
            ExprKind::Heading { .. } => ("self.heading".into(), Type::Unknown),

            ExprKind::MouseX { .. } => (format!("{}.mouse_pos[0]", self.stage.name), Type::Unknown),
            ExprKind::MouseY { .. } => (format!("{}.mouse_pos[1]", self.stage.name), Type::Unknown),

            ExprKind::StageWidth { .. } => (format!("{}.width", self.stage.name), Type::Unknown),
            ExprKind::StageHeight { .. } => (format!("{}.height", self.stage.name), Type::Unknown),

            ExprKind::Latitude { .. } => (format!("{}.gps_location[0]", self.stage.name), Type::Unknown),
            ExprKind::Longitude { .. } => (format!("{}.gps_location[1]", self.stage.name), Type::Unknown),

            ExprKind::PenDown { .. } => ("self.drawing".into(), Type::Wrapped), // bool is considered wrapped

            ExprKind::Scale { .. } => ("(self.scale * 100)".into(), Type::Wrapped),
            ExprKind::IsVisible { .. } => ("self.visible".into(), Type::Wrapped), // bool is considered wrapped

            ExprKind::RpcError { .. } => ("(get_error() or '')".into(), Type::Unknown),

            x => unimplemented!("{x:?}"),
        })
    }
    fn translate_stmts(&mut self, stmts: &[Stmt]) -> Result<String, TranslateError> {
        if stmts.is_empty() { return Ok("pass".into()) }

        let mut lines = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            let comment = fmt_comment(stmt.info.comment.as_deref());
            match &stmt.kind {
                StmtKind::DeclareLocals { vars } => lines.extend(vars.iter().map(|x| format!("{} = snap.wrap(0)", x.trans_name))),
                StmtKind::Assign { var, value } => lines.push(format!("{} = {}{}", translate_var(var), wrap(self.translate_expr(value)?), comment)),
                StmtKind::AddAssign { var, value } => lines.push(format!("{} += {}{}", translate_var(var), wrap(self.translate_expr(value)?), comment)),
                StmtKind::ListAssign { list, index, value } => lines.push(format!("{}[{}] = {}{}", wrap(self.translate_expr(list)?), self.translate_expr(index)?.0, self.translate_expr(value)?.0, comment)),
                StmtKind::Warp { stmts } => {
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("with Warp():{}\n{}", comment, indent(&code)));
                }
                StmtKind::If { condition, then } => {
                    let condition = wrap(self.translate_expr(condition)?);
                    let then = self.translate_stmts(then)?;
                    lines.push(format!("if {}:{}\n{}", condition, comment, indent(&then)));
                }
                StmtKind::IfElse { condition, then, otherwise } => {
                    let condition = wrap(self.translate_expr(condition)?);
                    let then = self.translate_stmts(then)?;
                    let otherwise = self.translate_stmts(otherwise)?;
                    lines.push(format!("if {}:{}\n{}\nelse:\n{}", condition, comment, indent(&then), indent(&otherwise)));
                }
                StmtKind::InfLoop { stmts } => {
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("while True:{}\n{}", comment, indent(&code)));
                }
                StmtKind::ForLoop { var, start, stop, stmts } => {
                    let start = self.translate_expr(start)?.0;
                    let stop = self.translate_expr(stop)?.0;
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("for {} in snap.sxrange({}, {}):{}\n{}", var.trans_name, start, stop, comment, indent(&code)));
                }
                StmtKind::ForeachLoop { var, items, stmts } => {
                    let items = wrap(self.translate_expr(items)?);
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("for {} in {}:{}\n{}", var.trans_name, items, comment, indent(&code)));
                }
                StmtKind::Repeat { times, stmts } => {
                    let times = wrap(self.translate_expr(times)?);
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("for _ in range(+{}):{}\n{}", times, comment, indent(&code)));
                }
                StmtKind::UntilLoop { condition, stmts } => {
                    let condition = wrap(self.translate_expr(condition)?);
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("while not {}:{}\n{}", condition, comment, indent(&code)));
                }
                StmtKind::SwitchCostume { costume } => {
                    let costume = match costume {
                        Some(v) => self.translate_expr(v)?.0,
                        None => "None".into(),
                    };
                    lines.push(format!("self.costume = {}{}", costume, comment));
                }
                StmtKind::ChangePos { dx, dy } => {
                    let mut comment = Some(comment.as_str());
                    for (var, val) in [("x_pos", dx), ("y_pos", dy)] {
                        if let Some(val) = val {
                            lines.push(format!("self.{} += {}{}", var, self.translate_expr(val)?.0, comment.take().unwrap_or("")));
                        }
                    }
                }
                StmtKind::SetPos { x, y } => match (x, y) {
                    (Some(x), Some(y)) => lines.push(format!("self.pos = ({}, {}){}", self.translate_expr(x)?.0, self.translate_expr(y)?.0, comment)),
                    (Some(x), None) => lines.push(format!("self.x_pos = {}{}", self.translate_expr(x)?.0, comment)),
                    (None, Some(y)) => lines.push(format!("self.y_pos = {}{}", self.translate_expr(y)?.0, comment)),
                    (None, None) => (), // the parser would never emit this, but it's not like it would matter...
                }
                StmtKind::SendLocalMessage { target, msg_type, wait } => {
                    if *wait { unimplemented!("blocking local messages") }
                    if target.is_some() { unimplemented!("send local message to target") }

                    match &msg_type.kind {
                        ExprKind::Value(Value::String(msg_type)) => lines.push(format!("nb.send_message('local::{}'){}", escape(msg_type), comment)),
                        _  => lines.push(format!("nb.send_message('local::' + str({})){}", self.translate_expr(msg_type)?.0, comment)),
                    }
                }
                StmtKind::SendNetworkMessage { target, msg_type, values } => {
                    let kwargs_str = self.translate_kwargs(values, ", ", false)?;
                    lines.push(format!("nb.send_message('{}', {}{}){}", escape(msg_type), self.translate_expr(target)?.0, kwargs_str, comment));
                }
                StmtKind::Say { content, duration } | StmtKind::Think { content, duration } => match duration {
                    Some(duration) => lines.push(format!("self.say(str({}), duration = {}){}", self.translate_expr(content)?.0, self.translate_expr(duration)?.0, comment)),
                    None => lines.push(format!("self.say(str({})){}", self.translate_expr(content)?.0, comment)),
                }
                StmtKind::ListInsertLast { list, value } => lines.push(format!("{}.append({}){}", wrap(self.translate_expr(list)?), wrap(self.translate_expr(value)?), comment)),
                StmtKind::ListRemoveLast { list } => lines.push(format!("{}.pop(){}", wrap(self.translate_expr(list)?), comment)),
                StmtKind::ListRemove { list, index } => lines.push(format!("del {}[{}]{}", wrap(self.translate_expr(list)?), self.translate_expr(index)?.0, comment)),
                StmtKind::ListRemoveAll { list } => lines.push(format!("{}.clear(){}", self.translate_expr(list)?.0, comment)),
                StmtKind::ChangePenSize { amount } => lines.push(format!("self.pen_size += {}{}", self.translate_expr(amount)?.0, comment)),
                StmtKind::SetPenSize { value } => lines.push(format!("self.pen_size = {}{}", self.translate_expr(value)?.0, comment)),
                StmtKind::SetVisible { value } => lines.push(format!("self.visible = {}{}", if *value { "True" } else { "False" }, comment)),
                StmtKind::WaitUntil { condition } => lines.push(format!("while not {}:{}\n    time.sleep(0.05)", wrap(self.translate_expr(condition)?), comment)),
                StmtKind::BounceOffEdge => lines.push(format!("self.keep_on_stage(bounce = True){}", comment)),
                StmtKind::Sleep { seconds } => lines.push(format!("time.sleep(+{}){}", wrap(self.translate_expr(seconds)?), comment)),
                StmtKind::Goto { target } => lines.push(format!("self.goto({}){}", self.translate_expr(target)?.0, comment)),
                StmtKind::RunRpc { service, rpc, args } => lines.push(self.translate_rpc(service, rpc, args, &comment)?),
                StmtKind::RunFn { function, args } => lines.push(self.translate_fn_call(function, args, &comment)?),
                StmtKind::Forward { distance } => lines.push(format!("self.forward({}){}", self.translate_expr(distance)?.0, comment)),
                StmtKind::TurnRight { angle } => lines.push(format!("self.turn_right({}){}", self.translate_expr(angle)?.0, comment)),
                StmtKind::TurnLeft { angle } => lines.push(format!("self.turn_left({}){}", self.translate_expr(angle)?.0, comment)),
                StmtKind::SetHeading { value } => lines.push(format!("self.heading = {}{}", self.translate_expr(value)?.0, comment)),
                StmtKind::Return { value } => lines.push(format!("return {}{}", wrap(self.translate_expr(value)?), comment)),
                StmtKind::Stamp => lines.push(format!("self.stamp(){}", comment)),
                StmtKind::Write { content, font_size } => lines.push(format!("self.write({}, size = {}){}", self.translate_expr(content)?.0, self.translate_expr(font_size)?.0, comment)),
                StmtKind::PenDown => lines.push(format!("self.drawing = True{}", comment)),
                StmtKind::PenUp => lines.push(format!("self.drawing = False{}", comment)),
                StmtKind::PenClear => lines.push(format!("{}.clear_drawings(){}", self.stage.name, comment)),
                StmtKind::SetPenColor { color } => lines.push(format!("self.pen_color = '#{:02x}{:02x}{:02x}'{}", color.0, color.1, color.2, comment)),
                StmtKind::ChangeScalePercent { amount } => lines.push(format!("self.scale += {}{}", self.translate_expr(amount)?.0, comment)),
                StmtKind::SetScalePercent { value } => lines.push(format!("self.scale = {}{}", self.translate_expr(value)?.0, comment)),
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
        let comment = fmt_comment(hat.info.comment.as_deref());
        Ok(match &hat.kind {
            HatKind::OnFlag => format!("@onstart(){}\ndef my_onstart_{}(self):\n", comment, self.scripts.len() + 1),
            HatKind::OnKey { key } => format!("@onkey('{}'){}\ndef my_onkey_{}(self):\n", key, comment, self.scripts.len() + 1),
            HatKind::MouseDown => format!("@onmouse('down'){}\ndef my_onmouse_{}(self, x, y):\n", comment, self.scripts.len() + 1),
            HatKind::MouseUp => format!("@onmouse('up'){}\ndef my_onmouse_{}(self, x, y):\n", comment, self.scripts.len() + 1),
            HatKind::MouseEnter => return Err(TranslateError::UnsupportedBlock("mouseenter interactions are not currently supported")),
            HatKind::MouseLeave => return Err(TranslateError::UnsupportedBlock("mouseleave interactions are not currently supported")),
            HatKind::ScrollDown => format!("@onmouse('scroll-down'){}\ndef my_onmouse_{}(self, x, y):\n", comment, self.scripts.len() + 1),
            HatKind::ScrollUp => format!("@onmouse('scroll-up'){}\ndef my_onmouse_{}(self, x, y):\n", comment, self.scripts.len() + 1),
            HatKind::Dropped => return Err(TranslateError::UnsupportedBlock("drop interactions are not currently supported")),
            HatKind::Stopped => return Err(TranslateError::UnsupportedBlock("stop interactions are not currently supported")),
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
                comment = comment,
                idx = self.scripts.len() + 1,
                condition = wrap(ScriptInfo::new(stage).translate_expr(condition)?))
            }
            HatKind::LocalMessage { msg_type } => {
                format!("@nb.on_message('local::{}'){}\ndef my_on_message_{}(self, **kwargs):\n", escape(msg_type), comment, self.scripts.len() + 1)
            }
            HatKind::NetworkMessage { msg_type, fields } => {
                let mut res = format!("@nb.on_message('{}'){}\ndef my_on_message_{}(self, **kwargs):\n", escape(msg_type), comment, self.scripts.len() + 1);
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
    let parser = Parser {
        name_transformer: Rc::new(&c_ident),
        adjust_to_zero_index: true,
        optimize: true,
        ..Default::default()
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
