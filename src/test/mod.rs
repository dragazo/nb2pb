use crate::*;

fn get_code(xml: &str) -> Result<Vec<String>, TranslateError> {
    let proj = serde_json::from_str::<serde_json::Value>(&translate(xml)?.1).unwrap();
    let mut res = vec![];
    for editor in proj.as_object().unwrap()["roles"].as_array().unwrap()[0].as_object().unwrap()["editors"].as_array().unwrap() {
        res.push(editor.as_object().unwrap()["value"].as_str().unwrap().to_owned());
    }
    Ok(res)
}

macro_rules! assert_code_eq {
    ($a:expr, $b:expr) => {{
        let (a, b) = ($a, $b);
        let (mut a_lines, mut b_lines) = (a.lines(), b.lines());
        let mut line_num = 0;
        loop {
            line_num += 1;
            fn fail(n: usize, x: &str, y: &str) {
                println!("code differs on line {n}");
                println!("left:  {x}");
                println!("right: {y}");
                panic!();
            }
            match (a_lines.next(), b_lines.next()) {
                (Some(a_line), Some(b_line)) => if a_line != b_line { fail(line_num, a_line, b_line) },
                (Some(a_line), None) => fail(line_num, a_line, "<EOF>"),
                (None, Some(b_line)) => fail(line_num, "<EOF>", b_line),
                (None, None) => break,
            }
        }
    }}
}

#[test]
fn test_operators() {
    let code = get_code(include_str!("projects/operators.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap

def foo():
    bar = snap.wrap(0)
    bar = (snap.wrap('1') + snap.wrap('4'))
    bar = (snap.wrap('1') + snap.wrap('4') + snap.wrap('7'))
    bar = snap.wrap(sum(baz()))
    bar = (snap.wrap('6') - snap.wrap('3'))
    bar = (snap.wrap('6') * snap.wrap('2'))
    bar = (snap.wrap('6') * snap.wrap('2') * snap.wrap('8'))
    bar = snap.prod(baz())
    bar = (snap.wrap('8') / snap.wrap('3'))
    bar = (snap.wrap('2') ** snap.wrap('4'))
    bar = (snap.wrap('3') % snap.wrap('2'))
    bar = round(snap.wrap('6.4'))
    bar = abs(snap.wrap('10'))
    bar = -snap.wrap('10')
    bar = snap.sign('10')
    bar = math.ceil(snap.wrap('10'))
    bar = math.floor(snap.wrap('10'))
    bar = snap.sqrt('10')
    bar = snap.sin('10')
    bar = snap.cos('10')
    bar = snap.tan('10')
    bar = snap.asin('10')
    bar = snap.acos('10')
    bar = snap.atan('10')
    bar = snap.log('10', math.e)
    bar = snap.log('10', 10)
    bar = snap.log('10', 2)
    bar = (snap.wrap(math.e) ** snap.wrap('10'))
    bar = (snap.wrap(10) ** snap.wrap('10'))
    bar = (snap.wrap(2) ** snap.wrap('10'))
    bar = snap.wrap('10')
    bar = snap.atan2('6', '5')
    bar = min(snap.wrap(['2', '4']))
    bar = min(snap.wrap(['2', '4', '7']))
    bar = min(baz())
    bar = max(snap.wrap(['5', '2']))
    bar = max(snap.wrap(['5', '2', '98']))
    bar = max(baz())
    bar = snap.rand('1', '10')
    bar = (snap.wrap('6') < snap.wrap('3'))
    bar = (snap.wrap('6') <= snap.wrap('3'))
    bar = (snap.wrap('6') == snap.wrap('3'))
    bar = (snap.wrap('6') != snap.wrap('3'))
    bar = snap.identical('6', '3')
    bar = (snap.wrap('6') > snap.wrap('3'))
    bar = (snap.wrap('6') >= snap.wrap('3'))
    bar = (True and False)
    bar = (False or True)
    bar = snap.lnot(True)
    bar = snap.lnot(False)
    bar = False
    bar = True
    bar = snap.wrap((str(snap.wrap('hello ')) + str(snap.wrap('world'))))
    bar = snap.wrap((str(snap.wrap('hello ')) + str(snap.wrap('world')) + str(snap.wrap('again'))))
    bar = snap.wrap(''.join(str(x) for x in baz()))
    bar = snap.wrap(len('world'))
    bar = snap.split('hello world', ' ')
    bar = snap.split('hello world', '')
    bar = snap.split_words('hello world')
    bar = snap.split('hello world', '\n')
    bar = snap.split('hello world', '\t')
    bar = snap.split('hello world', '\r')
    bar = snap.split_csv('hello world')
    bar = snap.split_json('hello world')
    bar = snap.get_ord('c')
    bar = snap.get_chr('87')
    bar = snap.is_number('5')
    bar = snap.is_text('5')
    bar = snap.is_bool('5')
    bar = snap.is_list('5')
    bar = snap.is_sprite('5')
    bar = snap.is_costume('5')
    bar = snap.is_sound('5')

def baz():
    return snap.srange('1', '7')

def another():
    return snap.wrap('hello')
"#.trim());
    assert_code_eq!(code[1].trim(), r#"
def __init__(self):
    self.costume = None
"#.trim());
    assert_code_eq!(code[2].trim(), r#"
def __init__(self):
    self.pos = (0, 0)
    self.heading = 90
    self.pen_color = (80, 80, 80)
    self.scale = 1
    self.visible = True
    self.costume = None
"#.trim());
}

#[test]
fn test_variables() {
    let code = get_code(include_str!("projects/variables.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap

def foobar():
    a = snap.wrap(0)
    a = snap.wrap([])
    a = snap.wrap(['4', '1', ['2', (snap.wrap('3') + snap.wrap('1'))]])
    a = snap.wrap([*a])
    a += snap.wrap('1')
    a = snap.srange('1', '10')
    a = snap.wrap(['23', *a])
    a = a[1:]
    a = a[snap.wrap('1') - snap.wrap(1)]
    a = a[snap.wrap('4') - snap.wrap(1)]
    a = a[(snap.wrap('2') + snap.wrap('3')) - snap.wrap(1)]
    a = a.last
    a = a.rand
    a = a[snap.wrap(['1', '3', '2']) - snap.wrap(1)]
    a = (a.index('thing') + snap.wrap(1))
    a = (snap.wrap('thing') in a)
    a = (len(a) == 0)
    a = snap.wrap(len(a))
    a = snap.wrap(len(a.shape))
    a = a.shape
    a = a.flat
    a = a.T
    a = a[::-1]
    a = '\n'.join(str(x) for x in a)
    a = a.csv
    a = a.json
    for item in a:
        a.append(item)
        a.append('abc')
        a.pop()
        del a[snap.wrap('1') - snap.wrap(1)]
        del a[snap.wrap('7') - snap.wrap(1)]
        del a[(snap.wrap('4') + snap.wrap('1')) - snap.wrap(1)]
        a.clear()
        a.insert('1', 'abc')
        a.insert('16', 'abc')
        a.insert((snap.wrap('1') + snap.wrap('3')), 'abc')
        a.append('abc')
        a.insert_rand('abc')
        a[snap.wrap('1') - snap.wrap(1)] = 'zyx'
        a[snap.wrap('6') - snap.wrap(1)] = 'zyx'
        a[(snap.wrap('2') + snap.wrap('5')) - snap.wrap(1)] = 'zyx'
        a.last = 'zyx'
        a.rand = 'zyx'
    a = snap.wrap([])
    a = snap.wrap([*snap.wrap([])])
    a = snap.wrap([*snap.wrap([]), *snap.wrap([])])
    a = snap.wrap([*snap.wrap([]), *snap.wrap([]), *snap.wrap([])])
    a = snap.wrap([y for x in a for y in x])
    a = snap.wrap([]).reshaped([])
    a = snap.wrap([]).reshaped(['5'])
    a = snap.wrap([]).reshaped(['5', '3'])
    a = snap.wrap([]).reshaped(a)
    a = snap.combinations()
    a = snap.combinations([])
    a = snap.combinations([], [])
    a = snap.combinations([], [], [])
    a = snap.combinations(*a)
    a = snap.wrap('world')[snap.wrap('1') - snap.wrap(1)]
    a = snap.wrap('world')[snap.wrap('5') - snap.wrap(1)]
    a = snap.wrap('world').last
    a = snap.wrap('world').rand
    a = snap.wrap(len('hello world'))
"#.trim());
    assert_code_eq!(code[1].trim(), r#"
def __init__(self):
    self.costume = None
"#.trim());
    assert_code_eq!(code[2].trim(), r#"
def __init__(self):
    self.pos = (0, 0)
    self.heading = 90
    self.pen_color = (80, 80, 80)
    self.scale = 1
    self.visible = True
    self.costume = None
"#.trim());
}

#[test]
fn test_lambdas() {
    let code = get_code(include_str!("projects/lambdas.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap

def barkbark():
    a = snap.wrap(0)
    (lambda: (snap.wrap('6') + snap.wrap('3')))()
    (lambda _1: (_1 + snap.wrap('3')))(snap.wrap('12'))
    (lambda _1, _2: (_1 * _2))(snap.wrap('31'), snap.wrap('8'))
    a = (lambda: (snap.wrap('6') + snap.wrap('3')))()
    a = (lambda _1: snap.combinations(_1, ['6', '9']))(snap.wrap('12'))
    a = (lambda _1, _2: snap.combinations(_1, _2))(snap.wrap('31'), snap.wrap('8'))
    a = (lambda: (True and False))()
    a = (lambda _1: (_1 and True))(False)
    a = (lambda _1, _2: (_1 and _2))(True, False)
    a = snap.wrap([(lambda _1: (_1 ** snap.wrap('2')))(x) for x in snap.srange('1', '10')])
    a = snap.wrap([x for x in snap.srange('1', '10') if (lambda _1: ((_1 % snap.wrap('2')) == snap.wrap('0')))(x)])
    a = snap.srange('1', '100').index_where((lambda xc: (((xc % snap.wrap('7')) == snap.wrap('0')) and ((xc % snap.wrap('5')) == snap.wrap('0')))))
    a = snap.srange('1', '100').fold((lambda _1, _2: (_1 + _2)))
"#.trim());
    assert_code_eq!(code[1].trim(), r#"
def __init__(self):
    self.costume = None
"#.trim());
    assert_code_eq!(code[2].trim(), r#"
def __init__(self):
    self.pos = (0, 0)
    self.heading = 90
    self.pen_color = (80, 80, 80)
    self.scale = 1
    self.visible = True
    self.costume = None
"#.trim());
}
