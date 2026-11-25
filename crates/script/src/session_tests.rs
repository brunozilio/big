use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use serde_json::{Value, json};

use super::{Evaluate, Evaluation, Limits, Message, Session};

fn evaluate(session: &Session, code: &str, answer: impl Fn(&str, &Value) -> Result<Value, String>) -> Evaluation {
    evaluate_within(session, code, Limits::default(), Arc::new(AtomicBool::new(false)), answer).0
}

fn evaluate_within(
    session: &Session,
    code: &str,
    limits: Limits,
    stop: Arc<AtomicBool>,
    answer: impl Fn(&str, &Value) -> Result<Value, String>,
) -> (Evaluation, Vec<(String, Value)>) {
    let functions = vec!["Read".to_owned(), "Grep".to_owned(), "mcp__notes__find".to_owned(), "has-dash".to_owned()];
    let messages = session.evaluate(Evaluate { code: code.to_owned(), functions, limits, stop });
    let mut seen = Vec::new();
    loop {
        match messages.recv_timeout(Duration::from_secs(20)).expect("the session answers") {
            Message::Call(call) => {
                seen.push((call.function.clone(), call.input.clone()));
                let result = answer(&call.function, &call.input);
                call.reply.send(result);
            }
            Message::Done(evaluation) => return (evaluation, seen),
        }
    }
}

fn nothing(_: &str, _: &Value) -> Result<Value, String> {
    Err("no tools here".into())
}

#[test]
fn recursion_without_end_is_an_error_the_session_survives() {
    let session = Session::start();

    let done = evaluate(&session, "function f() { return f() + 1 }\nf()", nothing);
    let refused = done.outcome.expect_err("the stack runs out");
    assert!(refused.to_lowercase().contains("stack"), "{refused}");

    let nested = evaluate(&session, "JSON.parse('['.repeat(200000) + ']'.repeat(200000)).length", nothing);
    assert!(nested.outcome.is_err() || nested.outcome == Ok("1".to_owned()), "{:?}", nested.outcome);
    assert_eq!(evaluate(&session, "1 + 1", nothing).outcome, Ok("2".to_owned()));
}

#[test]
fn the_last_expression_is_the_result_and_top_level_await_works() {
    let session = Session::start();

    let done = evaluate(&session, "const x = await Promise.resolve(20); x + 1", nothing);

    assert_eq!(done.outcome, Ok("21".to_owned()));
    assert_eq!(done.calls, 0);
}

#[test]
fn a_later_call_may_declare_a_name_again_and_return_from_its_top() {
    let session = Session::start();

    evaluate(&session, "const r = 1; let old = 'a';", nothing);
    let again = evaluate(&session, "const r = 2; let old = 'b'; r + old", nothing);
    let returned = evaluate(&session, "if (r > 1) { return r * 10 }\nreturn 0", nothing);

    assert_eq!(again.outcome, Ok("2b".to_owned()));
    assert_eq!(returned.outcome, Ok("20".to_owned()));
}

#[test]
fn what_one_call_declares_the_next_call_sees() {
    let session = Session::start();

    evaluate(&session, "const files = ['a.rs', 'b.rs']; function count() { return files.length }", nothing);
    let done = evaluate(&session, "count() * 10", nothing);

    assert_eq!(done.outcome, Ok("20".to_owned()));
}

#[test]
fn objects_come_back_as_json_and_their_promises_are_awaited() {
    let session = Session::start();

    let done = evaluate(&session, "({ status: Promise.resolve('clean'), count: 2 })", nothing);

    assert_eq!(done.outcome, Ok("{\n  \"status\": \"clean\",\n  \"count\": 2\n}".to_owned()));
}

#[test]
fn tools_are_async_functions_whose_results_stay_in_the_script() {
    let session = Session::start();
    let answer = |name: &str, input: &Value| match name {
        "Grep" => Ok(json!(format!("src/a.rs\nsrc/b.rs\nsrc/c.rs (pattern {})", input["pattern"]))),
        _ => Err(format!("unknown {name}")),
    };

    let (done, seen) = evaluate_within(
        &session,
        "const found = await Grep({ pattern: 'fn main' }); found.split('\\n').length",
        Limits::default(),
        Arc::new(AtomicBool::new(false)),
        answer,
    );

    assert_eq!(done.outcome, Ok("3".to_owned()));
    assert_eq!(seen, [("Grep".to_owned(), json!({ "pattern": "fn main" }))]);
    assert_eq!(done.calls, 1);
}

#[test]
fn calls_fanned_out_with_promise_all_are_all_sent_before_any_answer() {
    let session = Session::start();
    let messages = session.evaluate(Evaluate {
        code: "const [a, b] = await Promise.all([Read({ file_path: 'a' }), Read({ file_path: 'b' })]); a + b".into(),
        functions: vec!["Read".into()],
        limits: Limits::default(),
        stop: Arc::new(AtomicBool::new(false)),
    });
    let mut held = Vec::new();
    while held.len() < 2 {
        match messages.recv_timeout(Duration::from_secs(20)).expect("a call") {
            Message::Call(call) => held.push(call),
            Message::Done(done) => panic!("finished before both calls were made: {done:?}"),
        }
    }
    for call in held.into_iter().rev() {
        let path = call.input["file_path"].as_str().unwrap_or_default().to_owned();
        call.reply.send(Ok(json!(path)));
    }

    let Ok(Message::Done(done)) = messages.recv_timeout(Duration::from_secs(20)) else { panic!("no result") };
    assert_eq!(done.outcome, Ok("ab".to_owned()));
}

#[test]
fn a_failed_tool_throws_a_tool_error_the_script_can_catch() {
    let session = Session::start();
    let answer = |_: &str, _: &Value| Err("File does not exist.".to_owned());

    let caught = evaluate(
        &session,
        "try { await Read({ file_path: 'gone' }) } catch (error) { `${error.name} from ${error.tool}: ${error.message}` }",
        answer,
    );
    let thrown = evaluate(&session, "await Read({ file_path: 'gone' })", answer);

    assert_eq!(caught.outcome, Ok("ToolError from Read: File does not exist.".to_owned()));
    assert_eq!(thrown.outcome, Err("ToolError: File does not exist.".to_owned()));
}

#[test]
fn a_tool_whose_name_is_no_identifier_is_reached_through_tools() {
    let session = Session::start();
    let answer = |name: &str, _: &Value| Ok(json!(name));

    let done = evaluate(&session, "[await tools['has-dash']({}), await mcp__notes__find({})].join(' ')", answer);

    assert_eq!(done.outcome, Ok("has-dash mcp__notes__find".to_owned()));
}

#[test]
fn self_log_reaches_the_output_and_console_stays_in_its_ring() {
    let session = Session::start();

    let done = evaluate(&session, "console.log('noise'); self.log('kept', { n: 1 }); console.tail(1)", nothing);

    assert_eq!(done.logs, ["kept {\n  \"n\": 1\n}"]);
    assert_eq!(done.outcome, Ok("noise".to_owned()));
}

#[test]
fn a_thrown_error_or_a_syntax_error_is_the_outcome() {
    let session = Session::start();

    let thrown = evaluate(&session, "throw new TypeError('bad input')", nothing);
    let broken = evaluate(&session, "const = 3", nothing);

    let thrown = thrown.outcome.expect_err("it threw");
    assert!(thrown.starts_with("TypeError: bad input"), "{thrown}");
    assert!(broken.outcome.expect_err("it does not parse").starts_with("SyntaxError"));
}

#[test]
fn the_helpers_and_tools_cannot_be_overwritten() {
    let session = Session::start();

    let assigned = evaluate(&session, "Read = 1", nothing);
    let declared = evaluate(&session, "const self = 2", nothing);
    let still = evaluate(&session, "typeof Read + ' ' + typeof self.log", nothing);

    assert!(assigned.outcome.is_err() && declared.outcome.is_err());
    assert_eq!(still.outcome, Ok("function function".to_owned()));
}

#[test]
fn the_context_is_sealed_from_the_machine() {
    let session = Session::start();

    let done =
        evaluate(&session, "[typeof require, typeof process, typeof fetch, typeof std, typeof os].join()", nothing);

    assert_eq!(done.outcome, Ok("undefined,undefined,undefined,undefined,undefined".to_owned()));
}

#[test]
fn a_loop_that_never_ends_runs_out_of_its_own_time() {
    let session = Session::start();
    let limits = Limits { running: Duration::from_millis(200), ..Limits::default() };

    let (done, _) = evaluate_within(&session, "while (true) {}", limits, Arc::new(AtomicBool::new(false)), nothing);
    let after = evaluate(&session, "'alive'", nothing);

    assert!(done.outcome.expect_err("stopped").starts_with("Timed out: the script's own JavaScript ran"));
    assert_eq!(after.outcome, Ok("alive".to_owned()));
}

#[test]
fn time_spent_in_tool_calls_does_not_count_against_the_script() {
    let session = Session::start();
    let limits = Limits { running: Duration::from_millis(300), ..Limits::default() };
    let slow = |_: &str, _: &Value| {
        std::thread::sleep(Duration::from_millis(150));
        Ok(json!("done"))
    };

    let (done, _) = evaluate_within(
        &session,
        "for (let i = 0; i < 4; i++) await Read({}); 'finished'",
        limits,
        Arc::new(AtomicBool::new(false)),
        slow,
    );

    assert_eq!(done.outcome, Ok("finished".to_owned()));
}

#[test]
fn the_tool_call_cap_refuses_calls_past_it() {
    let session = Session::start();
    let limits = Limits { calls: 2, ..Limits::default() };
    let answer = |_: &str, _: &Value| Ok(json!("x"));

    let (done, seen) = evaluate_within(
        &session,
        "for (let i = 0; i < 5; i++) await Read({})",
        limits,
        Arc::new(AtomicBool::new(false)),
        answer,
    );

    assert_eq!(seen.len(), 2);
    assert!(done.outcome.expect_err("capped").contains("limit of 2 tool calls"));
}

#[test]
fn a_stopped_session_ends_the_script() {
    let session = Session::start();
    let stop = Arc::new(AtomicBool::new(true));

    let (done, _) = evaluate_within(&session, "while (true) {}", Limits::default(), stop, nothing);

    assert_eq!(done.outcome, Err("Stopped: the user interrupted the script".to_owned()));
}

#[test]
fn awaiting_a_promise_nothing_settles_ends_the_script() {
    let session = Session::start();

    let done = evaluate(&session, "await new Promise(() => {})", nothing);

    assert_eq!(done.outcome, Err("The script is waiting for a promise that nothing will settle".to_owned()));
}

#[test]
fn helpers_are_fixed_globals_that_call_tools_and_keep_their_own_names_private() {
    let helpers = "(() => { const shout = (text) => text.toUpperCase(); \
                   __define('loud', async (path) => shout(await tools.Read({ file_path: path }))); })()";
    let session = Session::with_helpers(helpers.to_owned());
    let answer =
        |_: &str, input: &Value| Ok(json!(format!("text of {}", input["file_path"].as_str().unwrap_or_default())));

    let done = evaluate(&session, "const shout = 1; await loud('a.txt')", answer);
    let fixed = evaluate(&session, "loud = null", nothing);

    assert_eq!(done.outcome, Ok("TEXT OF A.TXT".to_owned()));
    assert!(fixed.outcome.is_err());
}

#[test]
fn broken_helpers_fail_every_script_with_their_error() {
    let session = Session::with_helpers("throw new Error('no helpers')".to_owned());

    let done = evaluate(&session, "1", nothing);

    let failure = done.outcome.expect_err("the helpers failed");
    assert!(failure.starts_with("the helpers failed: Error: no helpers"), "{failure}");
}

fn start(session: &Session, code: &str) -> mpsc::Receiver<Message> {
    session.evaluate(Evaluate {
        code: code.to_owned(),
        functions: vec!["Read".into()],
        limits: Limits::default(),
        stop: Arc::new(AtomicBool::new(false)),
    })
}

fn next(messages: &mpsc::Receiver<Message>) -> Message {
    messages.recv_timeout(Duration::from_secs(20)).expect("a message")
}

#[test]
fn scripts_run_side_by_side_in_one_context_and_keep_their_own_logs() {
    let session = Session::start();
    let first = start(
        &session,
        "self.log('first starts'); const text = await Read({}); self.log('first got', text); globalThis.got = text; 'first'",
    );
    let Message::Call(held) = next(&first) else { panic!("the first script calls Read") };

    let second = start(&session, "self.log('second runs'); typeof got");
    let Message::Done(second) = next(&second) else { panic!("the second script ends while the first waits") };
    held.reply.send(Ok(json!("words")));
    let Message::Done(first) = next(&first) else { panic!("the first script ends") };
    let third = evaluate(&session, "got", nothing);

    assert_eq!((second.outcome, second.logs), (Ok("undefined".to_owned()), vec!["second runs".to_owned()]));
    assert_eq!(first.outcome, Ok("first".to_owned()));
    assert_eq!(first.logs, ["first starts", "first got words"]);
    assert!(first.number < second.number);
    assert_eq!(third.outcome, Ok("words".to_owned()), "they share one context");
}

#[test]
fn a_function_an_earlier_script_declared_serves_the_script_that_calls_it() {
    let session = Session::start();
    let answer =
        |_: &str, input: &Value| Ok(json!(format!("text of {}", input["file_path"].as_str().unwrap_or_default())));
    evaluate(
        &session,
        "async function both(a, b) { const one = await Read({ file_path: a }); self.log('read', a); return [one, await Read({ file_path: b })] }",
        answer,
    );

    let (done, seen) =
        evaluate_within(&session, "await both('a', 'b')", Limits::default(), Arc::new(AtomicBool::new(false)), answer);

    assert_eq!(seen.len(), 2, "both calls reach the script that is running");
    assert_eq!(done.logs, ["read a"]);
    assert!(done.outcome.expect("both read").contains("text of b"));
}

#[test]
fn a_script_stuck_on_another_s_promise_ends_only_when_nothing_can_settle_it() {
    let session = Session::start();
    let waiting = start(&session, "globalThis.gate = {}; await new Promise((open) => { gate.open = open })");
    let opener = evaluate(&session, "gate.open('opened'); 'done'", nothing);
    let Message::Done(waited) = next(&waiting) else { panic!("the waiting script ends") };

    assert_eq!(opener.outcome, Ok("done".to_owned()));
    assert_eq!(waited.outcome, Ok("opened".to_owned()), "a script may wait on a promise another one settles");
}

#[test]
fn a_call_left_unawaited_by_an_ended_script_never_lands_in_the_next_one() {
    let session = Session::start();
    let (sender, late) = mpsc::channel();
    let messages = session.evaluate(Evaluate {
        code: "globalThis.left = Read({}).then(() => 'late', () => 'refused'); 'first'".into(),
        functions: vec!["Read".into()],
        limits: Limits::default(),
        stop: Arc::new(AtomicBool::new(false)),
    });
    let mut first = None;
    for message in messages.iter() {
        match message {
            Message::Call(call) => sender.send(call).expect("held"),
            Message::Done(done) => first = Some(done),
        }
    }
    late.recv().expect("the unawaited call").reply.send(Ok(json!("too late")));

    let next = evaluate(&session, "'second'", nothing);

    assert_eq!(first.map(|done| done.outcome), Some(Ok("first".to_owned())));
    assert_eq!(next.outcome, Ok("second".to_owned()));
}

#[test]
fn a_name_a_failed_cell_declared_can_be_used_and_declared_again() {
    let session = Session::start();

    let failed = evaluate(
        &session,
        "const kept = 1;\nconst later = JSON.parse('Started in the background as bash_1');\nclass Shape {}",
        nothing,
    );
    let used = evaluate(&session, "[kept, typeof later, typeof Shape].join()", nothing);
    let again =
        evaluate(&session, "const later = 2;\nclass Shape { size() { return later } }\nnew Shape().size()", nothing);
    let thrown_after = evaluate(&session, "class Shape { size() { return 3 } }\nthrow new Error('after')", nothing);
    let replaced = evaluate(&session, "class Shape { size() { return 4 } }\nnew Shape().size()", nothing);

    let failed = failed.outcome.expect_err("the JSON is bad");
    assert!(failed.starts_with("SyntaxError: unexpected token: 'Started'"), "{failed}");
    assert_eq!(used.outcome, Ok("1,undefined,undefined".to_owned()));
    assert_eq!(again.outcome, Ok("2".to_owned()));
    assert!(thrown_after.outcome.is_err());
    assert_eq!(replaced.outcome, Ok("4".to_owned()));
}

#[test]
fn set_timeout_and_sleep_wait_and_a_cleared_timer_never_fires() {
    let session = Session::start();

    let done = evaluate(
        &session,
        "const order = [];\nsetTimeout(() => order.push('late'), 60);\nconst t = setTimeout(() => order.push('cleared'), 10);\nclearTimeout(t);\nsetTimeout((word) => order.push(word), 20, 'early');\nawait sleep(120);\norder.join()",
        nothing,
    );
    let failing = evaluate(&session, "setTimeout(() => { throw new Error('boom') }, 0);\nawait sleep(20);\n1", nothing);

    assert_eq!(done.outcome, Ok("early,late".to_owned()));
    assert_eq!(failing.outcome, Ok("1".to_owned()));
    assert!(
        failing.logs.iter().any(|line| line.contains("setTimeout callback failed: Error: boom")),
        "{:?}",
        failing.logs
    );
}

#[test]
fn a_timer_left_behind_does_not_keep_a_finished_cell_alive() {
    let session = Session::start();
    let started = std::time::Instant::now();

    let done =
        evaluate(&session, "globalThis.fired = 0;\nsetTimeout(() => { globalThis.fired = 1 }, 300);\n'done'", nothing);
    let finished_in = started.elapsed();
    std::thread::sleep(Duration::from_millis(400));
    let after = evaluate(&session, "await sleep(10);\nfired", nothing);

    assert_eq!(done.outcome, Ok("done".to_owned()));
    assert!(finished_in < Duration::from_millis(250), "{finished_in:?}");
    assert_eq!(after.outcome, Ok("0".to_owned()));
}

#[test]
fn a_stop_or_the_total_limit_ends_a_cell_that_sleeps() {
    let session = Session::start();
    let stop = Arc::new(AtomicBool::new(false));
    let stopper = Arc::clone(&stop);
    let _stopping = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        stopper.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let started = std::time::Instant::now();

    let (stopped, _) = evaluate_within(&session, "await sleep(60000)", Limits::default(), stop, nothing);
    let limits = Limits { total: Duration::from_millis(200), ..Limits::default() };
    let (timed, _) = evaluate_within(&session, "await sleep(60000)", limits, Arc::new(AtomicBool::new(false)), nothing);

    assert_eq!(stopped.outcome, Err("Stopped: the user interrupted the script".to_owned()));
    assert!(timed.outcome.expect_err("over its limit").starts_with("Timed out"));
    assert!(started.elapsed() < Duration::from_secs(5));
}
