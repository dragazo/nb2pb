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
fn test_media() {
    let got = serde_json::from_str::<serde_json::Value>(&translate(include_str!("projects/media.xml")).unwrap().1).unwrap();
    let expected = json!({
        "roles": [
            {
                "name": "myRole",
                "stage_size": [480, 360],
                "block_sources": [
                    "netsblox://assets/default-blocks.json",
                ],
                "blocks": [],
                "imports": [
                    "time",
                    "math",
                    "random",
                ],
                "editors": [
                    {
                        "type": "globals",
                        "name": "globals",
                        "value": "from netsblox import snap\n\n",
                    },
                    {
                        "type": "stage",
                        "name": "Stage",
                        "value": "last_answer = snap.wrap('')\n\ndef __init__(self):\n    self.costume = None\n\n",
                    },
                    {
                        "type": "sprite",
                        "name": "Sprite",
                        "value": "def __init__(self):\n    self.pos = (0, 0)\n    self.heading = 90\n    self.pen_color = (80, 80, 80)\n    self.scale = 1\n    self.visible = True\n\n    self.sounds.add('Dog 2', sounds.Sprite_snd_Dog_2)\n    self.sounds.add('Finger Snap', sounds.Sprite_snd_Finger_Snap)\n\n    self.costumes.add('untitled', images.Sprite_cst_untitled)\n    self.costumes.add('untitled(2)', images.Sprite_cst_untitled_2)\n    self.costumes.add('untitled(3)', images.Sprite_cst_untitled_3)\n\n    self.costume = 'untitled(3)'\n\n",
                    },
                ],
                "images": {
                    "Sprite_cst_untitled": {
                        "img": "iVBORw0KGgoAAAANSUhEUgAAAAYAAAAKCAYAAACXDi8zAAAAAXNSR0IArs4c6QAAAHNJREFUGFdjZGBgYNjMYOPAwMBg78twpBHEBwFGqOB+MIfh/wIfhqOJGBLIkoxQo+qBdAPMGAYGBkewBAhsYbCe/5+BMQHKbYBLbGawQdZFhMQmBqsERgam+diMAvkF7Ow/DH8U4XaABNYzWCiA6ECGEw8A2ZIg+Qd3twEAAAAASUVORK5CYII=",
                        "center": [0.0, -0.0],
                    },
                    "Sprite_cst_untitled_2" : {
                        "img": "iVBORw0KGgoAAAANSUhEUgAAABIAAAAOCAYAAAAi2ky3AAAAAXNSR0IArs4c6QAAAQ1JREFUOE+dk9FxwjAQRJ9EA5RACQoNYA/8JyXQASUQd0A6cAmhgIxN/pNxCSnBBcRWOFkzthV5DNy3dm93b6W4Z0yRoHlGsXQwS03LmSot1SSPKZYsSLBsPHAFGPAkPTCnIYsTyeYFRyCZFWw50YZEokK3e9AHFKIgNiWWMzhbP0BFlda9oriK+qpKgJcQGG7oiNYfouIYqCjFuwQ5aw9QmML4PF48oL4S5vzyRpWK9JtG8fS5QTevLlgrntuM721+E3rwaEwkwdFkfO3e7ycyxcpb27tArVN0eoRofHKxp0QZcrFuBg2eWtBd7X/gsfeuwVMHmOtRT+gbLOWLbRl/EWm2/CcdtHrQ4Clrf6ELaR0aXENQAAAAAElFTkSuQmCC",
                        "center": [13.0, 38.125],
                    },
                    "Sprite_cst_untitled_3": {
                        "img": "iVBORw0KGgoAAAANSUhEUgAAAAwAAAAQCAYAAAAiYZ4HAAAAAXNSR0IArs4c6QAAAV1JREFUOE91kq9TAkEUxz97J8cx44wXicSLNGnOJYlEIw2b0Yg0q00aNmhG2mrjT8BGxKSB8U4EVt7dnoKHO7Ozc3Pv+74/3lPY84YOyhCWcE9WrF8qRLP83+6r5CNG1xS0FJwDgYFRGQaK6P0vKAe0HegCNSlQcL+E3jHRvABYoKsedA1cAomCiYE7D8aKKCkAPtANN+veBGYKbkvwcKg4ZV+g66UM0ALmCoYG+mWi6UHTkk4F2gqurIcZOAPYjLyUcV9WavoTHQrAwAXgi7StxEnmZ/3q4MV51CnAoP1l5uEaaFgpYlhSkmgTiTpOo7ZH0nKh6cAZENob7Ph4/ILeDyBnSqC6NR4qnFPYCGtdZBp4cnFv9gB5N4MOYmgeQcdkEsVXkcGyBAm0HOjk3YGpDDOGYYFB9squSdsySnF/BUNZlQIgl+Nmi8gGntcwzvfqPw++mBeAn07/d3jfZf94T+p4L9EAAAAASUVORK5CYII=",
                        "center": [-24.0, -6.875],
                    },
                },
                "sounds": {
                    "Sprite_snd_Dog_2": {
                        "snd": "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",
                    },
                    "Sprite_snd_Finger_Snap": {
                        "snd": "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",
                    },
                },
            },
        ]
    });
    println!("{got:#?}");
    assert_eq!(got, expected);
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
    bar = snap.wrap('hello world')
    bar = snap.wrap('hello worldagain')
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
last_answer = snap.wrap('')

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
        a.append(snap.wrap('abc'))
        a.pop()
        del a[snap.wrap('1') - snap.wrap(1)]
        del a[snap.wrap('7') - snap.wrap(1)]
        del a[(snap.wrap('4') + snap.wrap('1')) - snap.wrap(1)]
        a.clear()
        a.insert('1', 'abc')
        a.insert('16', 'abc')
        a.insert((snap.wrap('1') + snap.wrap('3')), 'abc')
        a.append(snap.wrap('abc'))
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
last_answer = snap.wrap('')

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
last_answer = snap.wrap('')

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
fn test_looks() {
    let code = get_code(include_str!("projects/looks.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap

foo = snap.wrap('0')
"#.trim());
    assert_code_eq!(code[1].trim(), r#"
last_answer = snap.wrap('')

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
    self.costume = 'marcus'
    self.costume = 'john cena'
    self.costume = 'kevin ()'
    self.costume = (self.costumes.index(self.costume, -1) + 1) % len(self.costumes)
    self.say((self.costumes.index(self.costume, -1) + 1))
    self.say((self.costumes.index(self.costume, -1) + 1), duration = '2')
    self.scale += 12 / 100
    self.scale = 165 / 100
    self.scale += snap.wrap('gferg') / 100
    self.scale = snap.wrap('fgnrt') / 100
    self.say((self.scale * 100))
    self.say(self.visible, duration = '2')
    self.visible = True
    self.visible = False
"#.trim());
}

#[test]
fn test_sounds() {
    let code = get_code(include_str!("projects/sounds.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap

gf = snap.wrap('0')
"#.trim());
    assert_code_eq!(code[1].trim(), r#"
last_answer = snap.wrap('')

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

    self.sounds.add('Cat', sounds.Sprite_snd_Cat)
    self.sounds.add('Dog 1', sounds.Sprite_snd_Dog_1)

    self.costume = None

@onstart()
def my_onstart_1(self):
    self.play_sound('Cat')
    self.play_sound('Dog 1')
    self.play_sound(globals.gf)
    self.play_sound('Cat', wait = True)
    self.play_sound('Dog 1', wait = True)
    self.play_sound(globals.gf, wait = True)
    Stage.stop_sounds()
    globals.gf = self.sounds.lookup('Cat').duration
    globals.gf = self.sounds.lookup('Dog 1').duration
    globals.gf = self.sounds.lookup(globals.gf).duration
"#.trim());
}

#[test]
fn test_sensing() {
    let code = get_code(include_str!("projects/sensing.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap

something = snap.wrap('0')
"#.trim());
    assert_code_eq!(code[1].trim(), r#"
last_answer = snap.wrap('')

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
    Stage.last_answer = snap.wrap(input('hello world?'))
    globals.something = Stage.last_answer
    globals.something = snap.wrap(Stage.mouse_pos[0])
    globals.something = snap.wrap(Stage.mouse_pos[1])
    globals.something = Stage.is_key_down('space')
    globals.something = Stage.is_key_down('g')
    globals.something = snap.wrap(Stage.gps_location[0])
    globals.something = snap.wrap(Stage.gps_location[1])
    globals.something = snap.wrap(Stage.width)
    globals.something = snap.wrap(Stage.height)
"#.trim());
}

#[test]
fn test_motion() {
    let code = get_code(include_str!("projects/motion.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap

something = snap.wrap('158')
"#.trim());
    assert_code_eq!(code[1].trim(), r#"
last_answer = snap.wrap('')

def __init__(self):
    self.costume = None
"#.trim());
    assert_code_eq!(code[2].trim(), r#"
def __init__(self):
    self.pos = (-21, 172)
    self.heading = 183
    self.pen_color = (80, 80, 80)
    self.scale = 1
    self.visible = True
    self.costume = None

@onstart()
def my_onstart_1(self):
    self.forward(7)
    self.turn_right(21)
    self.turn_left(6)
    self.heading = 22
    self.pos = (-25, 32)
    self.x_pos += 8
    self.x_pos = -21
    self.y_pos += -7
    self.y_pos = 255
    self.keep_on_stage(bounce = True)
    globals.something = snap.wrap(self.x_pos)
    globals.something = snap.wrap(self.y_pos)
    globals.something = snap.wrap(self.heading)
    self.forward(snap.wrap('abc'))
    self.turn_right(snap.wrap('vr'))
    self.turn_left(snap.wrap('gerh'))
    self.heading = snap.wrap('gjrt')
    self.pos = (snap.wrap('kyu'), snap.wrap('erg'))
    self.x_pos += snap.wrap('er')
    self.x_pos = snap.wrap('dbnt')
    self.y_pos += snap.wrap('tyjk')
    self.y_pos = snap.wrap('ghn')
"#.trim());
}

#[test]
fn test_pen() {
    let code = get_code(include_str!("projects/pen.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap

something = snap.wrap('0')
"#.trim());
    assert_code_eq!(code[1].trim(), r#"
last_answer = snap.wrap('')

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
    Stage.clear_drawings()
    self.drawing = True
    self.drawing = False
    globals.something = self.drawing
    self.pen_color = '#911a44'
    self.pen_size += 17
    self.pen_size = 6
    self.pen_size += snap.wrap('help')
    self.pen_size = snap.wrap('me')
    self.stamp()
    self.write(snap.wrap('test msg!!'), size = snap.wrap('7'))
    globals.something = Stage.get_drawings()
"#.trim());
}

#[test]
fn test_join() {
    let code = get_code(include_str!("projects/join.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap

something = snap.wrap('')
"#.trim());
    assert_code_eq!(code[1].trim(), r#"
last_answer = snap.wrap('')

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
    globals.something = snap.wrap('')
    globals.something = snap.wrap('hello')
    globals.something = snap.wrap('helloworld')
    globals.something = snap.wrap(f'hello{globals.something}world')
    globals.something = snap.wrap(f"hello{(snap.wrap('3') * snap.wrap('5'))}world")
    globals.something = snap.wrap('helloworld')
    globals.something = snap.wrap('hellohelpworld')
    globals.something = snap.wrap('hellohelpmeworld')
    globals.something = snap.wrap(f'hellohelp{globals.something}meworld')
    globals.something = snap.wrap(f"hellohelp{(snap.wrap('3') + snap.wrap('5'))}meworld")
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
last_answer = snap.wrap('')

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
    time.sleep(2.4)
    time.sleep(+snap.wrap('merp'))
    nb.send_message('local::my msg thing')
    return snap.wrap('765')

@onkey('space')
def my_onkey_2(self):
    while not ((globals.foo + snap.wrap('2')) == snap.wrap('7')):
        time.sleep(0.05)
    raise RuntimeError(str(snap.wrap('oopsie!')))

@onmouse('up')
def my_onmouse_3(self, x, y):
    globals.foo = snap.wrap('Mouse Up!')
    while not globals.foo:
        try:
            for item in globals.bar:
                globals.foo = item[snap.wrap('1') - snap.wrap(1)]
                globals.bar = item.last
        except Exception as err:
            globals.bar.append(err)
            globals.foo.append(snap.wrap(f'got error: {err}'))

@onmouse('down')
def my_onmouse_4(self, x, y):
    with NoYield():
        globals.foo = snap.wrap('Mouse Down!')
        globals.foo = snap.wrap('more stuff')

@onmouse('scroll-up')
def my_onmouse_5(self, x, y):
    globals.foo = snap.wrap('Scroll Up!')
    for _ in range(6):
        globals.foo = snap.wrap('starting...')
        nothrow(nb.call)('Chart', 'draw', lines = nothrow(nb.call)('MaunaLoaCO2Data', 'getCO2Trend', startyear = '', endyear = ''), options = '')
        globals.foo = snap.wrap('done!')
    for _ in range(+snap.wrap('seven')):
        globals.foo = snap.wrap('starting...')
        nothrow(nb.call)('Chart', 'draw', lines = nothrow(nb.call)('MaunaLoaCO2Data', 'getCO2Trend', startyear = '', endyear = ''), options = '')
        globals.foo = snap.wrap('done!')

@onmouse('scroll-down')
def my_onmouse_6(self, x, y):
    if (globals.bar or globals.foo):
        globals.foo = snap.wrap('Scroll Down!')
        globals.bar = snap.wrap('more')
    else:
        globals.bar = snap.wrap('cloning...')
        self.clone()

@nb.on_message('local::my msg thing')
def my_on_message_7(self):
    while True:
        globals.foo = (globals.foo if (globals.foo > globals.bar) else globals.bar)
        globals.bar = self.clone()

@onstart('clone')
def my_onstart_8(self):
    for xyz in snap.sxrange(4, 8):
        if (snap.sqrt(xyz) < snap.wrap('9')):
            globals.foo = snap.wrap('agony!!')
            globals.bar = snap.wrap('pain!!')
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
last_answer = snap.wrap('')

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
fn test_empty_blocks() {
    let code = get_code(include_str!("projects/empty-blocks.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap
"#.trim());
    assert_code_eq!(code[1].trim(), r#"
last_answer = snap.wrap('')

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
    with NoYield():
        pass
    for _ in range(10):
        pass
    while not True:
        pass
    for i in snap.sxrange(1, 10):
        pass
    if True:
        pass
    if True:
        pass
    else:
        pass
    try:
        pass
    except Exception as err:
        pass
    for item in i:
        pass
    while True:
        pass
"#.trim());
}

#[test]
fn test_elif_opt() {
    let code = get_code(include_str!("projects/elif-opt.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap
"#.trim());
    assert_code_eq!(code[1].trim(), r#"
last_answer = snap.wrap('')

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
    a = snap.wrap(0)
    b = snap.wrap(0)
    c = snap.wrap(0)
    if a:
        self.say('1')
    elif b:
        self.say('2')
    elif c:
        self.say('3')
    else:
        self.say('4')
    if a:
        self.say('1')
    else:
        self.say('1.5')
        if b:
            self.say('2')
        elif c:
            self.say('3')
        else:
            self.say('4')
    if a:
        self.say('1')
    elif b:
        self.say('2')
    else:
        if c:
            self.say('3')
        else:
            self.say('4')
        self.say('4.5')
    if a:
        self.say('1')
    elif b:
        self.say('2')
    elif c:
        self.say('3')
    if a:
        self.say('1')
    else:
        self.say('1.5')
        if b:
            self.say('2')
        elif c:
            self.say('3')
    if a:
        self.say('1')
    elif b:
        self.say('2')
    else:
        if c:
            self.say('3')
        self.say('4.5')
"#.trim());
}

#[test]
fn test_timer() {
    let code = get_code(include_str!("projects/timer.xml")).unwrap();
    assert_eq!(code.len(), 3);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap

fr = snap.wrap('0')
"#.trim());
    assert_code_eq!(code[1].trim(), r#"
last_answer = snap.wrap('')

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
    Stage.timer = 0
    globals.fr = snap.wrap(Stage.timer)
"#.trim());
}

#[test]
fn test_cloning() {
    let code = get_code(include_str!("projects/cloning.xml")).unwrap();
    assert_eq!(code.len(), 4);
    assert_code_eq!(code[0].trim(), r#"
from netsblox import snap

fr = snap.wrap('0')
"#.trim());
    assert_code_eq!(code[1].trim(), r#"
last_answer = snap.wrap('')

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
    self.clone()
    Sprite_2.clone()
    globals.fr = self.clone()
    globals.fr = Sprite_2.clone()

@onstart('clone')
def my_onstart_2(self):
    self.say('what is my purpose?')
"#.trim());
assert_code_eq!(code[3].trim(), r#"
def __init__(self):
    self.pos = (-117, 147)
    self.heading = 90
    self.pen_color = (149, 0, 219)
    self.scale = 1
    self.visible = True
    self.costume = None
"#.trim());
}
