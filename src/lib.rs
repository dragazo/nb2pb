#![forbid(unsafe_code)]

mod python;

use std::fmt::Write;
use std::rc::Rc;
use std::iter;

#[macro_use] extern crate serde_json;

pub use netsblox_ast::Error as ParseError;
use netsblox_ast::{*, util::*};

#[derive(Debug)]
pub enum TranslateError {
    ParseError(ParseError),
    NoRoles,

    UnsupportedBlock(&'static str),
}
impl From<ParseError> for TranslateError { fn from(e: ParseError) -> Self { Self::ParseError(e) } }

macro_rules! fmt_comment {
    ($comment:expr) => {
        match $comment.as_ref() {
            Some(v) => format!(" # {}", v.replace('\n', " -- ")),
            None => "".into(),
        }
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

#[derive(Default)]
struct ScriptInfo {

}
impl ScriptInfo {
    fn translate_value(&mut self, value: &Value) -> Result<(String, Type), TranslateError> {
        Ok(match value {
            Value::String(v) => (format!("'{}'", escape(v)), Type::Unknown),
            Value::Number(v) => (format!("{}", v), Type::Unknown),
            Value::Bool(v) => ((if *v { "true" } else { "false" }).into(), Type::Wrapped), // bool is considered wrapped since we can't extend it
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
    fn translate_rpc(&mut self, service: &str, rpc: &str, args: &[(String, Expr)], comment: Option<&str>) -> Result<String, TranslateError> {
        let mut trans_args = Vec::with_capacity(args.len());
        for arg in args.iter() {
            trans_args.push(format!("'{}': {}", escape(&arg.0), self.translate_expr(&arg.1)?.0));
        }
        Ok(format!("nb.call('{}', '{}', {{ {} }}){}", escape(service), escape(rpc), Punctuated(trans_args.iter(), ", "), fmt_comment!(comment)))
    }
    fn translate_fn_call(&mut self, function: &FnRef, args: &[Expr], comment: Option<&str>) -> Result<String, TranslateError> {
        let mut trans_args = Vec::with_capacity(args.len());
        for arg in args.iter() {
            trans_args.push(wrap(self.translate_expr(arg)?));
        }
        let comment = fmt_comment!(comment);

        Ok(match function.location {
            FnLocation::Global => format!("{}(self, {}){}", function.trans_name, Punctuated(trans_args.iter(), ", "), comment),
            FnLocation::Method => format!("self.{}({}){}", function.trans_name, Punctuated(trans_args.iter(), ", "), comment),
        })
    }
    fn translate_expr(&mut self, expr: &Expr) -> Result<(String, Type), TranslateError> {
        Ok(match expr {
            Expr::Value(v) => self.translate_value(v)?,
            Expr::Variable { var, .. } => (translate_var(var), Type::Wrapped), // all assignments are wrapped, so we can assume vars are wrapped

            Expr::MakeList { values, .. } => {
                let mut items = Vec::with_capacity(values.len());
                for val in values {
                    items.push(self.translate_expr(val)?.0);
                }
                (format!("[{}]", items.join(", ")), Type::Unknown)
            }

            Expr::Add { left, right, .. } => (format!("({} + {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Sub { left, right, .. } => (format!("({} - {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Mul { left, right, .. } => (format!("({} * {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Div { left, right, .. } => (format!("({} / {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Mod { left, right, .. } => (format!("({} % {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),

            Expr::Pow { base, power, .. } => (format!("({} ** {})", wrap(self.translate_expr(base)?), wrap(self.translate_expr(power)?)), Type::Wrapped),
            Expr::Log { value, base, .. } => (format!("math.log({}, {})", wrap(self.translate_expr(value)?), wrap(self.translate_expr(base)?)), Type::Wrapped),

            Expr::And { left, right, .. } => (format!("({} and {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Or { left, right, .. } => (format!("({} or {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Conditional { condition, then, otherwise, .. } => {
                let (then, otherwise) = (self.translate_expr(then)?, self.translate_expr(otherwise)?);
                (format!("({} if {} else {})", then.0, wrap(self.translate_expr(condition)?), otherwise.0), if then.1 == otherwise.1 { then.1 } else { Type::Unknown })
            }

            Expr::RefEq { left, right, .. } => (format!("({} is {})", self.translate_expr(left)?.0, self.translate_expr(right)?.0), Type::Wrapped), // bool is considered wrapped
            Expr::Eq { left, right, .. } => (format!("({} == {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Less { left, right, .. } => (format!("({} < {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),
            Expr::Greater { left, right, .. } => (format!("({} > {})", wrap(self.translate_expr(left)?), wrap(self.translate_expr(right)?)), Type::Wrapped),

            Expr::RandInclusive { a, b, .. } => (format!("snap.rand({}, {})", self.translate_expr(a)?.0, self.translate_expr(b)?.0), Type::Wrapped), // python impl returns wrapped

            Expr::Listlen { value, .. } => (format!("len({})", self.translate_expr(value)?.0), Type::Unknown), // builtin __len__ can't be overloaded to return wrapped

            Expr::CallRpc { service, rpc, args, .. } => (self.translate_rpc(service, rpc, args, None)?, Type::Unknown),
            Expr::CallFn { function, args, .. } => (self.translate_fn_call(function, args, None)?, Type::Wrapped),

            x => panic!("{:#?}", x),
        })
    }
    fn translate_stmts(&mut self, stmts: &[Stmt]) -> Result<String, TranslateError> {
        if stmts.is_empty() { return Ok("pass".into()) }

        let mut lines = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            match stmt {
                Stmt::Assign { vars, value, comment } => {
                    let mut res = String::new();
                    for var in vars.iter() {
                        write!(&mut res, "{} = ", translate_var(var)).unwrap();
                    }
                    write!(&mut res, "{}{}", wrap(self.translate_expr(value)?), fmt_comment!(comment)).unwrap();
                    lines.push(res);
                }
                Stmt::AddAssign { var, value, comment } => {
                    lines.push(format!("{} += {}{}", translate_var(var), wrap(self.translate_expr(value)?), fmt_comment!(comment)));
                }
                Stmt::Warp { stmts, comment } => {
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("with Warp():{}\n{}", fmt_comment!(comment), indent(&code)));
                }
                Stmt::If { condition, then, comment } => {
                    let condition = wrap(self.translate_expr(condition)?);
                    let then = self.translate_stmts(then)?;
                    lines.push(format!("if {}:{}\n{}", condition, fmt_comment!(comment), indent(&then)));
                }
                Stmt::IfElse { condition, then, otherwise, comment } => {
                    let condition = wrap(self.translate_expr(condition)?);
                    let then = self.translate_stmts(then)?;
                    let otherwise = self.translate_stmts(otherwise)?;
                    lines.push(format!("if {}:{}\n{}\nelse:\n{}", condition, fmt_comment!(comment), indent(&then), indent(&otherwise)));
                }
                Stmt::SwitchCostume { costume, comment } => {
                    let costume = match costume {
                        Some(v) => self.translate_expr(v)?.0,
                        None => "None".into(),
                    };
                    lines.push(format!("self.costume = {}{}", costume, fmt_comment!(comment)));
                }
                Stmt::RunRpc { service, rpc, args, comment } => lines.push(self.translate_rpc(service, rpc, args, comment.as_deref())?),
                Stmt::RunFn { function, args, comment } => lines.push(self.translate_fn_call(function, args, comment.as_deref())?),
                Stmt::Forward { distance, comment } => lines.push(format!("self.forward({}){}", self.translate_expr(distance)?.0, fmt_comment!(comment))),
                Stmt::TurnRight { angle, comment } => lines.push(format!("self.turn_right({}){}", self.translate_expr(angle)?.0, fmt_comment!(comment))),
                Stmt::TurnLeft { angle, comment } => lines.push(format!("self.turn_left({}){}", self.translate_expr(angle)?.0, fmt_comment!(comment))),
                Stmt::Return { value, comment } => lines.push(format!("return {}{}", wrap(self.translate_expr(value)?), fmt_comment!(comment))),
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

struct SpriteInfo {
    name: String,
    scripts: Vec<String>,
    fields: Vec<(String, String)>,
    funcs: Vec<Function>,
    costumes: Vec<(String, String)>,

    active_costume: Option<usize>,
    color: (u8, u8, u8),
    pos: (f64, f64),
    heading: f64,
    scale: f64,
}
impl SpriteInfo {
    fn new(src: &Sprite) -> Self {
        Self {
            name: src.trans_name.clone(),
            scripts: vec![],
            fields: vec![],
            costumes: vec![],
            funcs: src.funcs.clone(),

            active_costume: src.active_costume,
            color: src.color,
            pos: src.pos,
            heading: src.heading,
            scale: src.scale,
        }
    }
    fn translate_hat(&mut self, hat: &Hat) -> Result<String, TranslateError> {
        Ok(match hat {
            Hat::OnFlag { comment } => format!("@onstart(){}\ndef my_onstart_{}(self):\n", fmt_comment!(comment), self.scripts.len() + 1),
            Hat::OnKey { key, comment } => format!("@onkey('{}'){}\ndef my_onkey_{}(self):\n", key, fmt_comment!(comment), self.scripts.len() + 1),
            Hat::MouseDown { comment } => format!("@onclick(when = 'down'){}\ndef my_onclick_{}(self):\n", fmt_comment!(comment), self.scripts.len() + 1),
            Hat::MouseUp { comment } => format!("@onclick(when = 'up'){}\ndef my_onclick_{}(self):\n", fmt_comment!(comment), self.scripts.len() + 1),
            Hat::MouseEnter { .. } => return Err(TranslateError::UnsupportedBlock("mouseenter interactions are not currently supported")),
            Hat::MouseLeave { .. } => return Err(TranslateError::UnsupportedBlock("mouseleave interactions are not currently supported")),
            Hat::ScrollUp { .. } => return Err(TranslateError::UnsupportedBlock("scrollup interactions are not currently supported")),
            Hat::ScrollDown { .. } => return Err(TranslateError::UnsupportedBlock("scrolldown interactions are not currently supported")),
            Hat::Dropped { .. } => return Err(TranslateError::UnsupportedBlock("drop interactions are not currently supported")),
            Hat::Stopped { .. } => return Err(TranslateError::UnsupportedBlock("stop interactions are not currently supported")),
            Hat::Message { msg, fields, comment } => {
                let params = iter::once("self").chain(fields.iter().map(String::as_str)).chain(iter::once("**kwargs"));
                format!("@nb.on_message('{}'){}\ndef my_on_message_{}({}):\n", escape(msg), fmt_comment!(comment), self.scripts.len() + 1, Punctuated(params, ", "))
            }
        })
    }
}

/// Translates NetsBlox project XML into PyBlox project JSON
///
/// On success, returns the project name and project json content as a tuple.
pub fn translate(source: &str) -> Result<(String, String), TranslateError> {
    let parser = ParserBuilder::default().optimize(true).name_transformer(Rc::new(&c_ident)).build().unwrap();
    let project = parser.parse(&mut source.as_bytes())?;
    if project.roles.is_empty() { return Err(TranslateError::NoRoles) }

    let mut roles = vec![];
    for role in project.roles.iter() {
        let mut role_info = RoleInfo::new(role.name.clone());

        for sprite in role.sprites.iter() {
            let mut sprite_info = SpriteInfo::new(sprite);
            for costume in sprite.costumes.iter() {
                let content = match &costume.value {
                    Value::String(s) => s,
                    _ => panic!(), // the parser lib would never do this
                };
                sprite_info.costumes.push((costume.trans_name.clone(), content.clone()));
            }
            for field in sprite.fields.iter() {
                let value = wrap(ScriptInfo::default().translate_value(&field.value)?);
                sprite_info.fields.push((field.trans_name.clone(), value));
            }
            for script in sprite.scripts.iter() {
                let func_def = match script.hat.as_ref() {
                    Some(x) => sprite_info.translate_hat(x)?,
                    None => continue, // dangling blocks of code need not be translated
                };
                let body = ScriptInfo::default().translate_stmts(&script.stmts)?;
                let res = format!("{}{}", func_def, indent(&body));
                sprite_info.scripts.push(res);
            }
            role_info.sprites.push(sprite_info);
        }

        let mut editors = vec![];

        let mut content = String::new();
        content += "from netsblox import snap\n\n";
        for global in role.globals.iter() {
            let value = wrap(ScriptInfo::default().translate_value(&global.value)?);
            write!(&mut content, "{} = {}\n", global.trans_name, value).unwrap();
        }
        if !role.globals.is_empty() { content.push('\n') }
        for func in role.funcs.iter() {
            let params = iter::once("self").chain(func.params.iter().map(|v| v.trans_name.as_str()));
            let code = ScriptInfo::default().translate_stmts(&func.stmts)?;
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
                write!(&mut content, "{} = {}\n", field, value).unwrap();
            }
            if !sprite.fields.is_empty() { content.push('\n'); }

            content += "def __init__(self):\n";
            if i != 0 { // don't generate these for stage
                writeln!(&mut content, "    self.pos = ({}, {})", sprite.pos.0, sprite.pos.1).unwrap();
                writeln!(&mut content, "    self.heading = {}", sprite.heading).unwrap();
                writeln!(&mut content, "    self.pen_color = ({}, {}, {})", sprite.color.0, sprite.color.1, sprite.color.2).unwrap();
                writeln!(&mut content, "    self.scale = {}", sprite.scale).unwrap();
            }
            match sprite.active_costume {
                Some(idx) => writeln!(&mut content, "    self.costume = self.costumes['{}']", sprite.costumes[idx].0).unwrap(),
                None => content += "    self.costume = None\n",
            }
            content.push('\n');

            for func in sprite.funcs.iter() {
                let params = iter::once("self").chain(func.params.iter().map(|v| v.trans_name.as_str()));
                let code = ScriptInfo::default().translate_stmts(&func.stmts)?;
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
            "block_sources": [ "netsblox://assets/default-blocks.json" ],
            "blocks": {
                "global": [],
                "stage": [],
                "turtle": [],
            },
            "imports": [],
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
