#![forbid(unsafe_code)]

use std::fmt::Write;
use std::rc::Rc;
use std::iter;

#[macro_use] extern crate serde_json;

pub use netsblox_ast::Error as ParseError;
use netsblox_ast::{*, util::*};

#[derive(Debug)]
pub enum TranslateError {
    ParseError(ParseError),
    MultipleRoles,
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
            Expr::Variable { var, .. } => (translate_var(var), Type::Unknown),

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
                        write!(&mut res, "{} = ", translate_var(var)).unwrap();
                    }
                    write!(&mut res, "{}{}", self.translate_expr(value)?.0, fmt_comment!(comment)).unwrap();
                    lines.push(res);
                }
                Stmt::AddAssign { var, value, comment } => {
                    lines.push(format!("{} += {}{}", translate_var(var), numerify(self.translate_expr(value)?)?, fmt_comment!(comment)));
                }

                Stmt::Warp { stmts, comment } => {
                    let code = self.translate_stmts(stmts)?;
                    lines.push(format!("with Warp():{}\n{}", fmt_comment!(comment), indent(&code)));
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
    fields: Vec<(String, String)>,
    costumes: Vec<(String, String)>,
}
impl SpriteInfo {
    fn new(name: String) -> Self {
        Self { name, scripts: vec![], fields: vec![], costumes: vec![] }
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

    let role = match project.roles.as_slice() {
        [x] => x,
        [] => return Err(TranslateError::NoRoles),
        _ => return Err(TranslateError::MultipleRoles),
    };

    let mut project_info = ProjectInfo::new(project.name.clone());

    for sprite in role.sprites.iter() {
        let mut sprite_info = SpriteInfo::new(sprite.name.clone());
        for costume in sprite.costumes.iter() {
            let content = match &costume.value {
                Value::String(s) => s,
                _ => panic!(), // the parser lib would never do this
            };
            sprite_info.costumes.push((costume.trans_name.clone(), content.clone()));
        }
        for field in sprite.fields.iter() {
            let value = ScriptInfo::default().translate_value(&field.value)?.0;
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
        project_info.sprites.push(sprite_info);
    }

    let mut editors = vec![];

    let mut content = String::new();
    for global in role.globals.iter() {
        let value = ScriptInfo::default().translate_value(&global.value)?.0;
        writeln!(&mut content, "{} = {}", global.trans_name, value).unwrap();
    }
    editors.push(json!({
        "type": "global",
        "name": "global",
        "value": content,
    }));

    for (i, sprite) in project_info.sprites.iter().enumerate() {
        let mut content = String::new();

        for (field, value) in sprite.fields.iter() {
            writeln!(&mut content, "{} = {}", field, value).unwrap();
        }
        if !sprite.fields.is_empty() { writeln!(&mut content).unwrap() }
        for script in sprite.scripts.iter() {
            writeln!(&mut content, "{}\n", &script).unwrap();
        }

        editors.push(json!({
            "type": if i == 0 { "stage" } else { "turtle" },
            "name": sprite.name,
            "value": content,
        }));
    }

    let mut images = serde_json::Map::new();
    for sprite in project_info.sprites.iter() {
        for (costume, content) in sprite.costumes.iter() {
            images.insert(format!("{}_cst_{}", sprite.name, costume), json!(content.clone()));
        }
    }

    let res = json!({
        "block_sources": [ "netsblox://assets/default-blocks.json" ],
        "blocks": {
            "global": [],
            "stage": [],
            "turtle": [],
        },
        "show_blocks": true,
        "imports": [],
        "editors": editors,
        "images": images,
    });

    Ok((project_info.name, res.to_string()))
}
