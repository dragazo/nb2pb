#![forbid(unsafe_code)]

use std::io::Read;
use std::fmt::Write;
use std::rc::Rc;
pub use netsblox_ast::Error as ParseError;
use netsblox_ast::{*, util::*};

#[derive(Debug)]
pub enum TranslateError {
    ParseError(ParseError),
    NoRoles, MultipleRoles,

    UnsupportedBlock(&'static str),
}
impl From<ParseError> for TranslateError { fn from(e: ParseError) -> Self { Self::ParseError(e) } }

macro_rules! fmt_comment {
    ($comment:expr) => {
        match $comment.as_ref() {
            Some(v) => format!(" # {}", v),
            None => "".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Type {
    Unknown, String, Number, Bool, List,
}

fn numerify(val: (String, Type)) -> Result<String, TranslateError> {
    Ok(match val.1 {
        Type::Number => val.0,
        _ => format!("float({})", val.0),
    })
}
fn boolify(val: (String, Type)) -> Result<String, TranslateError> {
    Ok(match val.1 {
        Type::Bool => val.0,
        _ => format!("jsbool({})", val.0),
    })
}

#[derive(Default)]
struct ScriptInfo {

}
impl ScriptInfo {
    fn translate_value(&mut self, value: &Value) -> Result<(String, Type), TranslateError> {
        Ok(match value {
            Value::String(v) => (format!("'{}'", escape(v)), Type::String),
            Value::Number(v) => (v.to_string(), Type::Number),
            Value::Bool(v) => ((if *v { "true" } else { "false" }).into(), Type::Bool),
            Value::Constant(c) => match c {
                Constant::Pi => ("math.pi".into(), Type::Number),
                Constant::E => ("math.e".into(), Type::Number),
            }
            Value::List(vals) => {
                let mut items = Vec::with_capacity(vals.len());
                for val in vals {
                    items.push(self.translate_value(val)?.0);
                }
                (format!("[{}]", items.join(", ")), Type::List)
            }
        })
    }
    fn translate_expr(&mut self, expr: &Expr) -> Result<(String, Type), TranslateError> {
        Ok(match expr {
            Expr::Value(v) => self.translate_value(v)?,
            Expr::Variable { var, .. } => match &var.location {
                VarLocation::Local => (var.trans_name.clone(), Type::Unknown),
                VarLocation::Field => (format!("self.{}", var.trans_name), Type::Unknown),
                VarLocation::Global => (format!("globals()['{}']", var.trans_name), Type::Unknown),
            }

            Expr::MakeList { values, .. } => {
                let mut items = Vec::with_capacity(values.len());
                for val in values {
                    items.push(self.translate_expr(val)?.0);
                }
                (format!("[{}]", items.join(", ")), Type::List)
            }

            Expr::Add { left, right, .. } => (format!("({} + {})", numerify(self.translate_expr(left)?)?, numerify(self.translate_expr(right)?)?), Type::Number),
            Expr::Sub { left, right, .. } => (format!("({} - {})", numerify(self.translate_expr(left)?)?, numerify(self.translate_expr(right)?)?), Type::Number),
            Expr::Mul { left, right, .. } => (format!("({} * {})", numerify(self.translate_expr(left)?)?, numerify(self.translate_expr(right)?)?), Type::Number),
            Expr::Div { left, right, .. } => (format!("({} / {})", numerify(self.translate_expr(left)?)?, numerify(self.translate_expr(right)?)?), Type::Number),
            Expr::Mod { left, right, .. } => (format!("({} % {})", numerify(self.translate_expr(left)?)?, numerify(self.translate_expr(right)?)?), Type::Number),

            Expr::Pow { base, power, .. } => (format!("({} ** {})", numerify(self.translate_expr(base)?)?, numerify(self.translate_expr(power)?)?), Type::Number),
            Expr::Log { value, base, .. } => (format!("math.log({}, {})", numerify(self.translate_expr(value)?)?, numerify(self.translate_expr(base)?)?), Type::Number),

            Expr::And { left, right, .. } => (format!("({} and {})", boolify(self.translate_expr(left)?)?, boolify(self.translate_expr(right)?)?), Type::Bool),
            Expr::Or { left, right, .. } => (format!("({} or {})", boolify(self.translate_expr(left)?)?, boolify(self.translate_expr(right)?)?), Type::Bool),
            Expr::Conditional { condition, then, otherwise, .. } => {
                let (then, otherwise) = (self.translate_expr(then)?, self.translate_expr(otherwise)?);
                (format!("({} if {} else {})", then.0, boolify(self.translate_expr(condition)?)?, otherwise.0), if then.1 == otherwise.1 { then.1} else { Type::Unknown })
            }

            Expr::RefEq { left, right, .. } => (format!("({} is {})", self.translate_expr(left)?.0, self.translate_expr(right)?.0), Type::Bool),
            Expr::Eq { left, right, .. } => (format!("({} == {})", self.translate_expr(left)?.0, self.translate_expr(right)?.0), Type::Bool),
            Expr::Less { left, right, .. } => (format!("({} < {})", numerify(self.translate_expr(left)?)?, numerify(self.translate_expr(right)?)?), Type::Bool),
            Expr::Greater { left, right, .. } => (format!("({} > {})", numerify(self.translate_expr(left)?)?, numerify(self.translate_expr(right)?)?), Type::Bool),

            Expr::RandInclusive { a, b, .. } => (format!("jsrand({}, {})", numerify(self.translate_expr(a)?)?, numerify(self.translate_expr(b)?)?), Type::Bool),

            Expr::Listlen { value, .. } => (format!("len({})", self.translate_expr(value)?.0), Type::Number),

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
                        match &var.location {
                            VarLocation::Local => write!(&mut res, "{} = ", var.trans_name).unwrap(),
                            VarLocation::Field => write!(&mut res, "self.{} = ", var.trans_name).unwrap(),
                            VarLocation::Global => write!(&mut res, "globals()['{}'] = ", var.trans_name).unwrap(),
                        }
                    }
                    write!(&mut res, "{}{}", self.translate_expr(value)?.0, fmt_comment!(comment)).unwrap();
                    lines.push(res);
                }
                Stmt::If { condition, then, comment } => {
                    let condition = boolify(self.translate_expr(condition)?)?;
                    let then = self.translate_stmts(then)?;
                    lines.push(format!("if {}:{}\n{}", condition, fmt_comment!(comment), indent(&then)));
                }
                Stmt::IfElse { condition, then, otherwise, comment } => {
                    let condition = boolify(self.translate_expr(condition)?)?;
                    let then = self.translate_stmts(then)?;
                    let otherwise = self.translate_stmts(otherwise)?;
                    lines.push(format!("if {}:{}\n{}\nelse:\n{}", condition, fmt_comment!(comment), indent(&then), indent(&otherwise)));
                }
                x => panic!("{:#?}", x),
            }
        }

        Ok(lines.join("\n"))
    }
}

struct ProjectInfo {
    name: String,
    sprites: Vec<SpriteInfo>,
}
impl ProjectInfo {
    fn new(name: String) -> Self {
        Self { name, sprites: vec![] }
    }
}

struct SpriteInfo {
    name: String,
    scripts: Vec<String>,
}
impl SpriteInfo {
    fn new(name: String) -> Self {
        Self { name, scripts: vec![] }
    }
    fn translate_hat(&mut self, hat: &Hat) -> Result<String, TranslateError> {
        Ok(match hat {
            Hat::OnFlag { comment } => format!("@onstart{}\ndef my_onstart_{}(self):\n", fmt_comment!(comment), self.scripts.len() + 1),
            Hat::OnKey { key, comment } => format!("@onkey('{}'){}\ndef my_onkey_{}(self):\n", key, fmt_comment!(comment), self.scripts.len() + 1),
        })
    }
}

pub fn translate<R: Read>(source: R) -> Result<String, TranslateError> {
    let parser = ParserBuilder::default().optimize(true).name_transformer(Rc::new(&c_ident)).build().unwrap();
    let project = parser.parse(source)?;

    let role = match project.roles.as_slice() {
        [x] => x,
        [] => return Err(TranslateError::NoRoles),
        _ => return Err(TranslateError::MultipleRoles),
    };

    let mut project_info = ProjectInfo::new(project.name.clone());

    for sprite in role.sprites.iter() {
        let mut sprite_info = SpriteInfo::new(sprite.name.clone());
        for script in sprite.scripts.iter() {
            let func_def = match script.hat.as_ref() {
                Some(x) => sprite_info.translate_hat(x)?,
                None => continue, // dangling blocks of code need not be translated
            };
            let body = ScriptInfo::default().translate_stmts(&script.stmts)?;
            let res = format!("{}{}", func_def, indent(&body));
            sprite_info.scripts.push(res);
        }
        project_info.sprites.push(sprite_info);
    }

    // ---- remove everythng below this line ------- //
    for global in role.globals.iter() {
        let value = ScriptInfo::default().translate_value(&global.value)?.0;
        println!("{} = {}", global.trans_name, value);
    }
    if !role.globals.is_empty() { println!() }
    for sprite in project_info.sprites.iter() {
        println!("class {}:\n", sprite.name);
        for script in sprite.scripts.iter() {
            println!("{}\n", indent(&script));
        }
    }
    panic!();
}
