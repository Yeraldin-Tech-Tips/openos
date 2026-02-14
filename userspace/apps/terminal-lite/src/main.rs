use std::io::{self, Write};

fn main() {
    println!("OpenOS Terminal Lite");
    repl();
}

fn repl() {
    let mut line = String::new();
    loop {
        print!("openos$ ");
        let _ = io::stdout().flush();

        line.clear();
        if io::stdin().read_line(&mut line).is_err() {
            break;
        }
        let cmd = line.trim();
        if cmd == "exit" {
            break;
        }
        println!("unknown command: {cmd}");
    }
}
