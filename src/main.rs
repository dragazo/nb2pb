use nb2pb::*;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: {} [input]", args[0]);
        std::process::exit(1);
    }

    let input = &args[1];
    if input.ends_with(".xml") {
        let xml = std::fs::read_to_string(input).expect("failed to read file");
        let res = translate(&xml).expect("failed to translate");
        println!("{}", res.1);
    }
    else {
        eprintln!("unknown input file type");
        std::process::exit(1);
    }
}
