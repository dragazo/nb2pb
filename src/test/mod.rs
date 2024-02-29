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
fn test_invalid_c_idents() {
    let code = get_code(include_str!("projects/invalid-c-idents.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap

def some_blocks_stuff():
    pass
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

@onstart()
def my_onstart_1(self):
    my_variable_name = snap.wrap(0)
    some_blocks_stuff()
"#.trim());
}

#[test]
fn test_costumes() {
    let code = get_code(include_str!("projects/costumes.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap

foo = snap.wrap('0')
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
    self.costumes.add('marcus', images.Sprite_cst_marcus)
    self.costumes.add('john cena', images.Sprite_cst_john_cena)
    self.costumes.add('kevin ()', images.Sprite_cst_kevin)
    self.costume = 'john cena'

@onstart()
def my_onstart_1(self):
    self.costume = ''
    self.costume = ''
    self.costume = ''
    self.costume = 'marcus'
    self.costume = 'john cena'
    self.costume = 'kevin ()'
    self.costume = (str(snap.wrap('marcus')))
    self.costume = (str(snap.wrap('john cena')))
    self.costume = (str(snap.wrap('kevin ()')))
    self.costume = (self.costumes.index(self.costume, -1) + 1) % len(self.costumes)
    self.say((self.costumes.index(self.costume, -1) + 1))
    self.say((self.costumes.index(self.costume, -1) + 1), duration = '2')
    self.scale += snap.wrap('12') / 100
    self.scale = snap.wrap('165') / 100
    self.say((self.scale * 100))
    self.say(self.visible, duration = '2')
    self.visible = True
    self.visible = False
"#.trim());
}

#[test]
fn test_control() {
    let code = get_code(include_str!("projects/control.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap

foo = snap.wrap('Init Foo!!')
bar = snap.wrap('Init Bar!!')
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

@onstart()
def my_onstart_1(self):
    time.sleep(+snap.wrap('2.4'))
    nb.send_message('local::my msg thing')

@onkey('space')
def my_onkey_2(self):
    while not ((globals()['foo'] + snap.wrap('2')) == snap.wrap('7')):
        time.sleep(0.05)
    raise RuntimeError(str(snap.wrap('oopsie!')))

@onmouse('up')
def my_onmouse_3(self, x, y):
    globals()['foo'] = snap.wrap('Mouse Up!')
    while not globals()['foo']:
        try:
            for item in globals()['bar']:
                globals()['foo'] = item[snap.wrap('1') - snap.wrap(1)]
                globals()['bar'] = item.last
        except Exception as err:
            globals()['bar'].append(err)
            globals()['foo'].append((str(snap.wrap('got error: ')) + str(err)))

@onmouse('down')
def my_onmouse_4(self, x, y):
    with Warp():
        globals()['foo'] = snap.wrap('Mouse Down!')
        globals()['foo'] = snap.wrap('more stuff')

@onmouse('scroll-up')
def my_onmouse_5(self, x, y):
    globals()['foo'] = snap.wrap('Scroll Up!')
    for _ in range(+snap.wrap('6')):
        globals()['foo'] = snap.wrap('starting...')
        nothrow(nb.call)('Chart', 'draw', lines = nothrow(nb.call)('MaunaLoaCO2Data', 'getCO2Trend', startyear = '', endyear = ''), options = '')
        globals()['foo'] = snap.wrap('done!')

@onmouse('scroll-down')
def my_onmouse_6(self, x, y):
    if (globals()['bar'] or globals()['foo']):
        globals()['foo'] = snap.wrap('Scroll Down!')
        globals()['bar'] = snap.wrap('more')
    else:
        globals()['bar'] = snap.wrap('cloning...')
        self.clone()

@nb.on_message('local::my msg thing')
def my_on_message_7(self):
    while True:
        globals()['foo'] = (globals()['foo'] if (globals()['foo'] > globals()['bar']) else globals()['bar'])
        globals()['bar'] = self.clone()

@onstart(when = 'clone')
def my_onstart_8(self):
    for xyz in snap.sxrange('4', '8'):
        if (snap.sqrt(xyz) < snap.wrap('9')):
            globals()['foo'] = snap.wrap('agony!!')
            globals()['bar'] = snap.wrap('pain!!')
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
