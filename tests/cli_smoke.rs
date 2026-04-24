use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn klassic_bin() -> &'static str {
    env!("CARGO_BIN_EXE_klassic")
}

#[test]
fn evaluates_expression_argument() {
    let output = Command::new(klassic_bin())
        .args(["-e", "1 + 2"])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn evaluates_expression_with_nested_comments() {
    let output = Command::new(klassic_bin())
        .args(["-e", "1 + /* outer /* inner */ outer */ 2"])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n");
}

#[test]
fn executes_file_argument_without_printing_the_return_value() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("klassic-cli-{unique}.kl"));
    fs::write(&path, "1 + 2").expect("temp file should be writable");

    let output = Command::new(klassic_bin())
        .arg(path.as_os_str())
        .output()
        .expect("binary should run");

    let _ = fs::remove_file(&path);

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn executes_dash_f_file_argument_without_printing_the_return_value() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("klassic-cli-dash-f-{unique}.kl"));
    fs::write(&path, "1 + 2").expect("temp file should be writable");

    let output = Command::new(klassic_bin())
        .args(["-f", path.to_string_lossy().as_ref()])
        .output()
        .expect("binary should run");

    let _ = fs::remove_file(&path);

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn executes_multiline_bindings_and_control_flow() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("klassic-cli-control-{unique}.kl"));
    fs::write(
        &path,
        "mutable i = 1\nwhile(i < 4) {\n  i += 1\n}\nif(i == 4) {\n  println(\"done\")\n}\n",
    )
    .expect("temp file should be writable");

    let output = Command::new(klassic_bin())
        .arg(path.as_os_str())
        .output()
        .expect("binary should run");

    let _ = fs::remove_file(&path);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "done\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn evaluates_module_imports_via_cli() {
    let output = Command::new(klassic_bin())
        .args(["-e", "import Map\nsize(%[\"A\": 1, \"B\": 2])"])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "2\n");
    assert!(output.stderr.is_empty());

    let excluded = Command::new(klassic_bin())
        .args([
            "-e",
            "import Map.{size, get => _}\nsize(%[\"A\": 1, \"B\": 2])",
        ])
        .output()
        .expect("binary should run");

    assert!(excluded.status.success());
    assert_eq!(String::from_utf8_lossy(&excluded.stdout), "2\n");
    assert!(excluded.stderr.is_empty());
}

#[test]
fn evaluates_placeholder_and_cleanup_via_cli() {
    let output = Command::new(klassic_bin())
        .args([
            "-e",
            "mutable i = 0\nwhile(i < 3) {\n  i += 1\n} cleanup {\n  i += 10\n}\nval xs = [1 2 3]\nprintln(map(xs)(_ + 1))\ni",
        ])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "[2, 3, 4]\n13\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn evaluates_list_map_and_reduce_syntax_via_cli() {
    let output = Command::new(klassic_bin())
        .args([
            "-e",
            "println([1 2 3] map x => x + 1)\n[1 2 3 4] reduce 0 => r + e",
        ])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "[2, 3, 4]\n10\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn evaluates_typeclass_constrained_functions_via_cli() {
    let output = Command::new(klassic_bin())
        .args([
            "-e",
            "typeclass Show<'a> where {\n  show: ('a) => String\n}\ntypeclass Eq<'a> where {\n  eq: ('a, 'a) => Boolean\n}\ninstance Show<Int> where {\n  def show(x: Int): String = \"Int(\" + x + \")\"\n}\ninstance Eq<Int> where {\n  def eq(x: Int, y: Int): Boolean = x == y\n}\ndef display<'a>(x: 'a): String where Show<'a> = show(x)\ndef show_if_equal<'a>(x: 'a, y: 'a): String where (Show<'a>, Eq<'a>) = if(eq(x, y)) show(x) else show(y)\nprintln(display(42))\nshow_if_equal(1, 2)",
        ])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Int(42)\nInt(2)\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn evaluates_fresh_constrained_instantiation_via_cli() {
    let output = Command::new(klassic_bin())
        .args([
            "-e",
            "typeclass Show<'a> where {\n  show: ('a) => String\n}\ninstance Show<Int> where {\n  def show(x: Int): String = \"Int(\" + x + \")\"\n}\ninstance Show<String> where {\n  def show(x: String): String = \"Str(\" + x + \")\"\n}\nrecord Person {\n  name: String\n  age: Int\n}\ninstance Show<Person> where {\n  def show(p: Person): String = \"Person(\" + p.name + \")\"\n}\ndef display<'a>(x: 'a): String where Show<'a> = show(x)\nprintln(display(42))\nprintln(display(\"hello\"))\ndisplay(#Person(\"Alice\", 30))",
        ])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Int(42)\nStr(hello)\nPerson(Alice)\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn evaluates_higher_kinded_constrained_functions_via_cli() {
    let output = Command::new(klassic_bin())
        .args([
            "-e",
            "typeclass Functor<'f: * => *> where {\n  map: (('a) => 'b, 'f<'a>) => 'f<'b>\n}\ninstance Functor<List> where {\n  def map(f: ('a) => 'b, xs: List<'a>): List<'b> = xs.map(f)\n}\ndef liftTwice<'f, 'a, 'b, 'c>(xs: 'f<'a>, f: ('a) => 'b, g: ('b) => 'c): 'f<'c> where Functor<'f> = map(g, map(f, xs))\nliftTwice([1, 2, 3], (x) => x + 1, (y) => y * 2)",
        ])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "[4, 6, 8]\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn evaluates_forward_proof_references_via_cli() {
    let output = Command::new(klassic_bin())
        .args([
            "-e",
            "theorem earlier(x: Int): { later(x) } = assert(true)\naxiom later(y: Int): { true }\nprintln(earlier(1))",
        ])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "true\n()\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn rejects_mismatched_proof_terms_via_cli() {
    let output = Command::new(klassic_bin())
        .args([
            "-e",
            "axiom left(): { true }\naxiom right(): { false }\ntheorem bad(): { left } = right",
        ])
        .output()
        .expect("binary should run");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("proof body does not establish declared proposition")
    );
}

#[test]
fn evaluates_higher_kinded_monad_constraints_via_cli() {
    let output = Command::new(klassic_bin())
        .args([
            "-e",
            "typeclass Monad<'m: * => *> where {\n  bind: ('m<'a>, ('a) => 'm<'b>) => 'm<'b>;\n  unit: ('a) => 'm<'a>\n}\ninstance Monad<List> where {\n  def bind(xs: List<'a>, f: ('a) => List<'b>): List<'b> = f(head(xs))\n  def unit(x: 'a): List<'a> = [x]\n}\ndef pairWithNext<'m>(xs: 'm<Int>): 'm<Int> where Monad<'m> = bind(xs, (x) => unit(x + 1))\npairWithNext([1, 2, 3])",
        ])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "[2]\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn shares_mutable_thread_captures_via_cli() {
    let output = Command::new(klassic_bin())
        .args([
            "-e",
            "mutable counter = 0\nthread(() => {\n  sleep(1)\n  counter = counter + 1\n})\nsleep(10)\ncounter",
        ])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn warns_for_trusted_proofs_via_cli() {
    let output = Command::new(klassic_bin())
        .args([
            "--warn-trust",
            "-e",
            "trust theorem foo(): { true } = assert(true)",
        ])
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "()\n");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("[trust] proof 'foo' is trusted (level 1); depends on []")
    );
}

#[test]
fn warns_for_transitively_trusted_proofs_with_levels() {
    let output = Command::new(klassic_bin())
        .args([
            "--warn-trust",
            "-e",
            "axiom base(): { true }\ntheorem mid(): { base } = base\ntheorem top(): { mid } = mid",
        ])
        .output()
        .expect("binary should run");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "()\n");
    assert!(stderr.contains("[trust] proof 'base' is trusted (level 1); depends on []"));
    assert!(stderr.contains("[trust] proof 'mid' is trusted (level 2); depends on [base]"));
    assert!(stderr.contains("[trust] proof 'top' is trusted (level 3); depends on [mid]"));
}

#[test]
fn denies_trusted_proofs_via_cli() {
    let output = Command::new(klassic_bin())
        .args([
            "--deny-trust",
            "-e",
            "trust theorem foo(): { true } = assert(true)",
        ])
        .output()
        .expect("binary should run");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("trusted proof 'foo' is not allowed (level 1)")
    );
}

#[test]
fn repl_supports_history_and_exit() {
    let mut child = Command::new(klassic_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should start repl");

    {
        let stdin = child.stdin.as_mut().expect("stdin should be piped");
        stdin
            .write_all(b"val answer = 42\nanswer\n:history\n:exit\n")
            .expect("repl input should be writable");
    }

    let output = child.wait_with_output().expect("repl should finish");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert!(stdout.contains("value = ()"));
    assert!(stdout.contains("value = 42"));
    assert!(stdout.contains("1: val answer = 42"));
    assert!(stdout.contains("2: answer"));
}

#[test]
fn repl_buffers_multiline_input_until_complete() {
    let mut child = Command::new(klassic_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should start repl");

    {
        let stdin = child.stdin.as_mut().expect("stdin should be piped");
        stdin
            .write_all(b"def addOne(x) = {\n  x + 1\n}\naddOne(2)\n:exit\n")
            .expect("repl input should be writable");
    }

    let output = child.wait_with_output().expect("repl should finish");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert!(stdout.contains("value = ()"));
    assert!(stdout.contains("value = 3"));
}
