#![no_main]

use domain::shell::{ShellCommand, SimpleCommand, simple_commands};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let line = String::from_utf8_lossy(data);
    let line = line.as_ref();

    let parsed = ShellCommand::parse(line);
    assert_eq!(parsed.text(), line);
    for command in parsed.commands() {
        assert!(line.contains(command), "a command is a piece of the line it came from");
        let _ = SimpleCommand::of(command);
        let _ = SimpleCommand::bare(command);
    }
    if parsed.flat() {
        assert!(!parsed.opaque());
    }
    let (commands, understood) = simple_commands(line);
    assert_eq!(commands, parsed.commands());
    assert_eq!(understood, !parsed.opaque());
});
