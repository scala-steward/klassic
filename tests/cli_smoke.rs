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
fn evaluates_process_exit_via_cli() {
    let output = Command::new(klassic_bin())
        .args([
            "-e",
            "println(\"before exit\")\nProcess#exit({ println(\"code path\"); 7 })\nprintln(\"after exit\")",
        ])
        .output()
        .expect("binary should run");

    assert_eq!(output.status.code(), Some(7));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "before exit\ncode path\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn evaluates_standard_input_via_cli() {
    let mut child = Command::new(klassic_bin())
        .args([
            "-e",
            "val text = StandardInput#all()\nprintln(trimRight(text))\nprintln(length(text))\nassertResult(\"alpha\\nbeta\\n\")(text)",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should run");

    {
        let mut stdin = child.stdin.take().expect("stdin should be piped");
        stdin
            .write_all(b"alpha\nbeta\n")
            .expect("stdin should accept input");
    }

    let output = child
        .wait_with_output()
        .expect("binary should finish after stdin closes");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "alpha\nbeta\n11\n()\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn evaluates_environment_vars_via_cli() {
    let output = Command::new(klassic_bin())
        .args([
            "-e",
            "val vars = env()\nmutable found = false\nforeach(entry in vars) {\n  if(entry == \"KLASSIC_EVAL_ENV_TEST=alpha\") {\n    found = true\n  }\n}\nprintln(found)\nassert(found)",
        ])
        .env("KLASSIC_EVAL_ENV_TEST", "alpha")
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "true\n()\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn evaluates_environment_get_and_exists_via_cli() {
    let output = Command::new(klassic_bin())
        .args([
            "-e",
            "println(getEnv(\"KLASSIC_EVAL_ENV_GET_TEST\"))\nprintln(hasEnv(\"KLASSIC_EVAL_ENV_GET_TEST\"))\nprintln(Environment#exists(\"KLASSIC_EVAL_ENV_GET_MISSING\"))\nassertResult(\"alpha\")(Environment#get(\"KLASSIC_EVAL_ENV_GET_TEST\"))",
        ])
        .env("KLASSIC_EVAL_ENV_GET_TEST", "alpha")
        .env_remove("KLASSIC_EVAL_ENV_GET_MISSING")
        .output()
        .expect("binary should run");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "alpha\ntrue\nfalse\n()\n"
    );
    assert!(output.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_basic_program() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-basic-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-basic-{unique}"));
    fs::write(
        &source_path,
        "println(1 + 2)\nprintln(true)\nval parsed = {\n  rule {\n    S = \"a\";\n  }\n  7\n}\nprintln(parsed)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3\ntrue\n7\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_control_flow_and_mutation() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-control-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-control-{unique}"));
    fs::write(
        &source_path,
        "mutable i = 1\nwhile(i < 4) {\n  i += 1\n}\nif(i == 4) {\n  println(\"done\")\n}\nmutable total = 0\nforeach(e in [1, 2, 3]) {\n  total += e\n}\nprintln(total)\nassertResult(6)(total)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "done\n6\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_recursive_integer_functions() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-fact-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-fact-{unique}"));
    fs::write(
        &source_path,
        "def fact(n) = if(n < 2) 1 else n * fact(n - 1)\nprintln(fact(5))\nassertResult(120)(fact(5))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "120\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_recursive_runtime_string_parameter() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-recursive-string-param-{unique}.kl"));
    let input_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-string-param-{unique}.txt"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-recursive-string-param-{unique}"));
    fs::write(
        &source_path,
        format!(
            r#"def countA(s: String, i: Int): Int = if(i >= length(s)) 0 else if(s.at(i) == "a") 1 + countA(s, i + 1) else countA(s, i + 1)
val text = FileInput#all("{}")
println(countA(text, 0))
assertResult(3)(countA(text, 0))
"#,
            input_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "recursive runtime string build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path, "banana").expect("input source should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "recursive runtime string run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_build_rejects_recursive_runtime_string_parameter_rewrite() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-string-param-rewrite-{unique}.kl"
    ));
    let input_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-string-param-rewrite-{unique}.txt"
    ));
    let output_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-string-param-rewrite-{unique}"
    ));
    fs::write(
        &source_path,
        format!(
            r#"def consume(s: String, i: Int): Int = if(i >= length(s)) i else consume(substring(s, 1, length(s)), i + 1)
val text = FileInput#all("{}")
println(consume(text, 0))
"#,
            input_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(!build.status.success());
    assert!(build.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&build.stderr)
            .contains("native recursive string parameter must be passed unchanged"),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_recursive_runtime_line_list_parameter() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-recursive-lines-param-{unique}.kl"));
    let input_path =
        std::env::temp_dir().join(format!("klassic-native-recursive-lines-param-{unique}.txt"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-recursive-lines-param-{unique}"));
    fs::write(
        &source_path,
        format!(
            r#"def countLines(lines: List<String>, i: Int): Int = if(i >= lines.size()) i else countLines(lines, i + 1)
val lines = FileInput#lines("{}")
println(countLines(lines, 0))
println(countLines(["one", "two"], 0))
assertResult(3)(countLines(lines, 0))
assertResult(2)(countLines(["one", "two"], 0))
"#,
            input_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "recursive runtime line-list build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path, "alpha\nbeta\ngamma")
        .expect("input source should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "recursive runtime line-list run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3\n2\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_build_rejects_recursive_runtime_line_list_parameter_rewrite() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-lines-param-rewrite-{unique}.kl"
    ));
    let input_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-lines-param-rewrite-{unique}.txt"
    ));
    let output_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-lines-param-rewrite-{unique}"
    ));
    fs::write(
        &source_path,
        format!(
            r#"def consume(lines: List<String>, count: Int): Int = if(lines.isEmpty()) count else consume(tail(lines), count + 1)
val lines = FileInput#lines("{}")
println(consume(lines, 0))
"#,
            input_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(!build.status.success());
    assert!(build.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&build.stderr)
            .contains("native recursive line-list parameter must be passed unchanged"),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_recursive_runtime_string_return() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-string-return-{unique}.kl"
    ));
    let input_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-string-return-{unique}.txt"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-recursive-string-return-{unique}"));
    fs::write(
        &source_path,
        format!(
            r#"def reverseFrom(s: String, i: Int): String = if(i < 0) "" else s.at(i) + reverseFrom(s, i - 1)
val text = FileInput#all("{}")
println(reverseFrom(text, length(text) - 1))
println(reverseFrom("xy", 1) + reverseFrom("ab", 1))
assertResult("cba")(reverseFrom(text, length(text) - 1))
assertResult("yxba")(reverseFrom("xy", 1) + reverseFrom("ab", 1))
"#,
            input_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "recursive runtime string return build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path, "abc").expect("input source should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "recursive runtime string return run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "cba\nyxba\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_recursive_runtime_line_list_return() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-recursive-lines-return-{unique}.kl"));
    let input_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-lines-return-{unique}.txt"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-recursive-lines-return-{unique}"));
    fs::write(
        &source_path,
        format!(
            r#"def keepLines(lines: List<String>, n: Int): List<String> = if(n <= 0) lines else keepLines(lines, n - 1)
val lines = FileInput#lines("{}")
println(join(keepLines(lines, 2), "|"))
println(join(keepLines(["x", "y"], 1), ":"))
assertResult(["a", "b", "c"])(keepLines(lines, 2))
assertResult(["x", "y"])(keepLines(["x", "y"], 1))
"#,
            input_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "recursive runtime line-list return build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path, "a\nb\nc").expect("input source should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "recursive runtime line-list return run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "a|b|c\nx:y\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_recursive_function_static_top_level_capture() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-static-capture-{unique}.kl"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-recursive-static-capture-{unique}"));
    fs::write(
        &source_path,
        "val one = 1\ndef fact(n: Int): Int = if(n < 2) one else n * fact(n - one)\nprintln(fact(5))\nassertResult(120)(fact(5))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "120\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_recursive_function_builtin_alias_capture() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-builtin-capture-{unique}.kl"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-recursive-builtin-capture-{unique}"));
    fs::write(
        &source_path,
        "val print = println\ndef countdown(n: Int): Int = if(n < 1) 0 else {\n  print(n)\n  countdown(n - 1)\n}\nprintln(countdown(3))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3\n2\n1\n0\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_recursive_function_lambda_capture() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-lambda-capture-{unique}.kl"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-recursive-lambda-capture-{unique}"));
    fs::write(
        &source_path,
        "val inc = (x) => x + 1\ndef repeatInc(n: Int, x: Int): Int = if(n < 1) x else repeatInc(n - 1, inc(x))\nprintln(repeatInc(3, 0))\nassertResult(3)(repeatInc(3, 0))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_user_function_direct_call_shadows_builtin_name() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-function-shadows-builtin-{unique}.kl"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-function-shadows-builtin-{unique}"));
    fs::write(
        &source_path,
        "def repeat(n: Int): Int = n + 1\nval r = repeat\nprintln(repeat(1))\nprintln(r(2))\nassertResult(2)(repeat(1))\nassertResult(3)(r(2))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "2\n3\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_boolean_function_arguments_and_returns() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-bool-fn-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-bool-fn-{unique}"));
    fs::write(
        &source_path,
        "def isTwo(n: Int): Boolean = n == 2\ndef isThree(n) = n == 3\nval isSmall = (x) => x < 5\ndef choose(flag: Boolean): Int = if(flag) 10 else 20\nprintln(isTwo(2))\nprintln(isThree(3))\nprintln(isSmall(4))\nprintln(choose(isTwo(1)))\nassert(isTwo(2) && isThree(3) && isSmall(4) && !isTwo(3))\nassertResult(false)(isTwo(3))\nassertResult(true)(isThree(3))\nassertResult(true)(isSmall(4))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "true\ntrue\ntrue\n20\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_stack_argument_functions() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-stack-args-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-stack-args-{unique}"));
    fs::write(
        &source_path,
        "def encode7(a: Int, b: Int, c: Int, d: Int, e: Int, f: Int, g: Int): Int = a * 1000000 + b * 100000 + c * 10000 + d * 1000 + e * 100 + f * 10 + g\ndef encode8(a: Int, b: Int, c: Int, d: Int, e: Int, f: Int, g: Int, h: Int): Int = a * 10000000 + b * 1000000 + c * 100000 + d * 10000 + e * 1000 + f * 100 + g * 10 + h\ndef makeAdder(n: Int) = (x: Int) => x + n\nval add2 = makeAdder(2)\nval t = stopwatch( => 1)\nval seven = encode7(1, 2, 3, 4, 5, 6, t) - t\nval eight = encode8(1, 2, 3, 4, 5, 6, t, 8) - t * 10\nval withClosureArg = encode7(1, add2(1), 3, 4, 5, 6, 7)\nprintln(seven)\nprintln(eight)\nprintln(withClosureArg)\nassertResult(1234560)(seven)\nassertResult(12345608)(eight)\nassertResult(1334567)(withClosureArg)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "1234560\n12345608\n1334567\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_top_level_lambda_bindings() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-lambda-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-lambda-{unique}"));
    fs::write(
        &source_path,
        "def inc(x: Int): Int = x + 1\ndef dec(x: Int): Int = x - 1\nval add = (x, y) => x + y\nmutable f = inc\nprintln(inc)\nprintln([inc])\nprintln(record { f: inc })\nprintln(add)\nprintln(add(2, 3))\nprintln(f)\nprintln(f(2))\nf = dec\nprintln(f(2))\nmutable g = (x) => x + 1\nprintln(g)\nprintln(g(2))\ng = (x) => x + 2\nprintln(g(2))\nval base = 3\nmutable noisy = (x) => { println(\"noise\"); x + base }\nmutable n = 4\nprintln(noisy(n))\nnoisy = (x) => { println(\"again\"); x + 4 }\nprintln(noisy(n))\nprintln(({ println(\"pick\"); noisy })(n))\nassertResult(5)(add(2, 3))\nassertResult(1)(f(2))\nassertResult(4)(g(2))\nassertResult(8)(noisy(n))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "<function>\n[<function>]\n#(<function>)\n<function>\n5\n<function>\n3\n1\n<function>\n3\n4\nnoise\n7\nagain\n8\npick\nagain\n8\nagain\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_functions_capturing_top_level_bindings() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-top-level-captures-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-top-level-captures-{unique}"));
    fs::write(
        &source_path,
        "val base = 40\nmutable counter = 0\nval print = println\ndef addBase(x: Int): Int = x + base + 2\ndef bump(x: Int): Int = {\n  counter += x\n  counter\n}\ndef say(x: Int): Int = {\n  print(\"say\")\n  x\n}\nval t = stopwatch(() => 1)\nprintln(addBase(t) - t)\nprintln(bump(2))\nprintln(bump(3))\nprintln(say(7))\nassertResult(5)(counter)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "42\n2\n5\nsay\n7\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_block_local_mutable_closure_capture() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-local-closure-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-local-closure-{unique}"));
    fs::write(
        &source_path,
        "val f = {\n  mutable x = 0\n  (y) => {\n    x = x + 1\n    x + y\n  }\n}\nprintln(f(1))\nprintln(f(1))\nassertResult(4)(f(1))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "2\n3\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_record_closures_sharing_local_capture() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-record-closure-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-record-closure-{unique}"));
    fs::write(
        &source_path,
        "val pair = {\n  mutable x = 0\n  val inc = (y) => {\n    x = x + y\n    x\n  }\n  val get = () => x\n  record { inc: inc, get: get }\n}\nprintln(pair.inc(2))\nprintln(pair.get())\nprintln(pair.inc(3))\nprintln(pair.get())\nassertResult(5)(pair.get())\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "2\n2\n5\n5\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_function_returning_mutable_closure_capture() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-function-closure-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-function-closure-{unique}"));
    fs::write(
        &source_path,
        "def make() = {\n  mutable x = 0\n  (y) => {\n    x = x + y\n    x\n  }\n}\nval f = make()\nprintln(f(2))\nprintln(f(3))\nassertResult(5)(f(0))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "2\n5\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_build_honors_deny_trust() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-trust-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-trust-{unique}"));
    fs::write(
        &source_path,
        "trust theorem foo(): { true } = assert(true)\nprintln(1)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "--deny-trust",
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(!build.status.success());
    assert!(build.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&build.stderr)
            .contains("trusted proof 'foo' is not allowed (level 1)")
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_print_string_concat() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-concat-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-concat-{unique}"));
    fs::write(
        &source_path,
        "println(\"x = \" + (1 + 2))\nprintln(\"ok? \" + true)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "x = 3\nok? true\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_print_string_interpolation() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-interpolation-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-interpolation-{unique}"));
    fs::write(
        &source_path,
        "val x = 10\nval y = 20\nval parts = split(\"a,b\", \",\")\nval message = \"x = #{x :> *}, y = #{y :> *}\"\nprintln(\"x = #{x :> *}, sum = #{(x + 5) :> *}, parts = #{parts :> *}\")\nprintln(message)\nassertResult(\"x = 10, y = 20\")(message)\nassertResult(\"x + y = 30\")(\"x + y = #{(x + y) :> *}\")\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "x = 10, sum = 15, parts = [a, b]\nx = 10, y = 20\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_string_interpolation_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-interpolation-side-effects-{unique}.kl"
    ));
    let output_path = std::env::temp_dir().join(format!(
        "klassic-native-interpolation-side-effects-{unique}"
    ));
    fs::write(
        &source_path,
        "mutable hits = 0\nval text = \"x=#{ { hits += 1; 42 } }\"\nprintln(hits)\nprintln(text)\nprintln(\"inline=#{ { hits += 1; 7 } }\")\nprintln(hits)\nassertResult(2)(hits)\nassertResult(\"x=42\")(text)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "1\nx=42\ninline=7\n2\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_integer_numeric_helpers() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-numeric-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-numeric-{unique}"));
    fs::write(
        &source_path,
        "println(\"abs = \" + abs(-10))\nprintln(\"int = \" + int(7))\nprintln(\"floor = \" + floor(8))\nprintln(\"ceil = \" + ceil(9))\nprintln([1 + 1, 6 / 2, 2 * 3])\nassertResult(10)(abs(-10))\nassertResult(7)(int(7))\nassertResult(8)(floor(8))\nassertResult(9)(ceil(9))\nassertResult([2, 3, 6])([1 + 1, 6 / 2, 2 * 3])\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "abs = 10\nint = 7\nfloor = 8\nceil = 9\n[2, 3, 6]\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_bitwise_folds() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-bitwise-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-bitwise-{unique}"));
    fs::write(
        &source_path,
        "val xor = _ ^ _\nprintln([1 & 1, 1 | 0, 1 ^ 1])\nprintln(foldLeft([1, 3])(7)(xor))\nassertResult([1, 1, 0])([1 & 1, 1 | 0, 1 ^ 1])\nassertResult(5)(foldLeft([1, 3])(7)(xor))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "[1, 1, 0]\n5\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_double_helpers() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-double-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-double-{unique}"));
    fs::write(
        &source_path,
        "def neg() = -1.25\nval hyp = sqrt(3.0 * 3.0 + 4.0 * 4.0)\nval product = foldLeft([1.0, 2.0, 3.0, 4.0])(1.0)((x, y) => x * y)\nval shifted = map([1.0, 2.0])((x) => x + 0.5)\nprintln(1.5)\nprintln(double(10))\nprintln(\"sqrt = \" + sqrt(9.0))\nprintln([1.0, 2.5, abs(-3.5)])\nprintln(product)\nprintln(shifted)\nprintln(neg())\nassertResult(3.0)(sqrt(9.0))\nassertResult(5.0)(hyp)\nassertResult(3)(int(3.14159265359))\nassertResult(1)(floor(1.5))\nassertResult(-1)(floor(-1.5))\nassertResult(5)(ceil(4.4))\nassertResult(-4)(ceil(-4.5))\nassertResult(10.5)(abs(-10.5))\nassertResult(24.0)(product)\nassertResult([1.5, 2.5])(shifted)\nassertResult([1.0, 2.5, 3.5])([1.0, 2.5, abs(-3.5)])\nassertResult(-1.25)(neg())\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "1.5\n10.0\nsqrt = 3.0\n[1.0, 2.5, 3.5]\n24.0\n[1.5, 2.5]\n-1.25\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_float_helpers() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-float-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-float-{unique}"));
    fs::write(
        &source_path,
        "val pure = 1.23456789F\nval widened = 1.25F + 2\nval product = foldLeft([1.5F, 2.0F])(1.0F)((x, y) => x * y)\nval shifted = map([1.0F, 2.0F])((x) => x + 0.25F)\nprintln(pure)\nprintln([1.0F, abs(-2.5F), widened])\nprintln(product)\nprintln(shifted)\nprintln(\"float = \" + pure)\nprintln(\"sqrt = \" + sqrt(double(9.0F)))\nassert(widened > pure)\nassertResult(true)(1.0F == 1)\nassertResult(1.2345679F)(pure)\nassertResult(3.25F)(widened)\nassertResult(3.0F)(product)\nassertResult([1.25F, 2.25F])(shifted)\nassertResult([1.0F, 2.5F, 3.25F])([1.0F, abs(-2.5F), widened])\nassertResult(3)(int(3.75F))\nassertResult(-1)(floor(-1.25F))\nassertResult(2)(ceil(1.25F))\nassertResult(2.5F)(abs(-2.5F))\nassertResult(3.0)(sqrt(double(9.0F)))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "1.2345679\n[1.0, 2.5, 3.25]\n3.0\n[1.25, 2.25]\nfloat = 1.2345679\nsqrt = 3.0\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_numeric_helper_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-numeric-side-effects-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-numeric-side-effects-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nval a = sqrt({ hits += 1; 9.0 })\nval b = int({ hits += 1; 3.8 })\nval c = floor({ hits += 1; -1.25F })\nval d = ceil({ hits += 1; 1.25F })\nval e = abs({ hits += 1; -2.5 })\nval f = double({ hits += 1; 4 })\nprintln(hits)\nprintln(a)\nprintln(b)\nprintln(c)\nprintln(d)\nprintln(e)\nprintln(f)\nassertResult(6)(hits)\nassertResult(3.0)(a)\nassertResult(3)(b)\nassertResult(-1)(c)\nassertResult(2)(d)\nassertResult(2.5)(e)\nassertResult(4.0)(f)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "6\n3.0\n3\n-1\n2\n2.5\n4.0\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_if_values() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-static-if-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-static-if-{unique}"));
    fs::write(
        &source_path,
        "val label = if(true) \"yes\" else \"no\"\nval xs = if(false) [0] else [1, 2]\nval row = if(size(xs) == 2) record { name: \"ok\", count: size(xs) } else record { name: \"bad\", count: 0 }\nprintln(label)\nprintln(xs)\nprintln(row.name)\nprintln(row.count)\nassertResult(\"yes\")(label)\nassertResult([1, 2])(xs)\nassertResult(record { name: \"ok\", count: 2 })(row)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "yes\n[1, 2]\nok\n2\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_string_helpers() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-string-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-string-{unique}"));
    fs::write(
        &source_path,
        "val text = \"  Abc123  \"\nval parts = split(\"a,b,c\", \",\")\nval label = \"count=\" + 3\nval decorated = \"parts=\" + parts\nval row = record { label: \"user=\" + \"Alice\", size: \"n=\" + size(parts), methodSize: \"m=\" + parts.size(), first: \"first=\" + head(parts) }\nval folded = foldLeft([\"A\", \"B\", \"C\"])(\"\")((x, y) => x + y)\nval exclaimed = map(parts)((x) => x + \"!\")\nprintln(substring(\"abcdef\", 1, 4))\nprintln(at(\"abc\", 1))\nprintln(trim(text))\nprintln(trimLeft(text))\nprintln(trimRight(text))\nprintln(replace(\"abab\", \"a\", \"x\"))\nprintln(replaceAll(\"a1b2\", \"[0-9]\", \"?\"))\nprintln(toLowerCase(\"AbC\"))\nprintln(toUpperCase(\"AbC\"))\nprintln(\"starts? \" + startsWith(\"hello\", \"he\"))\nprintln(\"ends? \" + endsWith(\"hello\", \"lo\"))\nprintln(\"contains? \" + \"hello\".contains(\"ell\"))\nprintln(\"matches? \" + matches(\"123\", \"[0-9]+\"))\nprintln(\"empty? \" + isEmptyString(\"\"))\nprintln(\"index = \" + indexOf(\"hello world\", \"world\"))\nprintln(\"last = \" + lastIndexOf(\"ababa\", \"ba\"))\nprintln(\"length = \" + length(\"hé\"))\nprintln(repeat(\"ha\", 3))\nprintln(reverse(\"abc\"))\nprintln(parts)\nprintln(join(parts, \"-\"))\nprintln(parts.join(\"|\"))\nprintln(label)\nprintln(decorated)\nprintln(row.label)\nprintln(row.size)\nprintln(row.methodSize)\nprintln(row.first)\nprintln(folded)\nprintln(exclaimed)\nprintln(\"chars = \" + split(\"ab\", \"\"))\nprintln(\"head = \" + head(parts))\nprintln(\"tail = \" + tail(parts))\nprintln(\"parts size = \" + size(parts))\nprintln(\"method size = \" + parts.size())\nmutable seen = 0\nforeach(part in parts) {\n  seen += 1\n  println(\"part = \" + part)\n}\nprintln(\"seen = \" + seen)\nassertResult(\"bcd\")(substring(\"abcdef\", 1, 4))\nassertResult(\"b\")(at(\"abc\", 1))\nassertResult(\"Abc123\")(trim(text))\nassertResult(\"xbab\")(replace(\"abab\", \"a\", \"x\"))\nassertResult(\"a?b?\")(replaceAll(\"a1b2\", \"[0-9]\", \"?\"))\nassertResult(true)(\"hello\".contains(\"ell\"))\nassertResult(true)(matches(\"123\", \"[0-9]+\"))\nassertResult(6)(indexOf(\"hello world\", \"world\"))\nassertResult(3)(lastIndexOf(\"ababa\", \"ba\"))\nassertResult(2)(length(\"hé\"))\nassertResult(\"hahaha\")(repeat(\"ha\", 3))\nassertResult(\"cba\")(reverse(\"abc\"))\nassertResult([\"a\", \"b\", \"c\"])(parts)\nassertResult(\"a-b-c\")(join(parts, \"-\"))\nassertResult(\"a|b|c\")(parts.join(\"|\"))\nassertResult(\"count=3\")(label)\nassertResult(\"parts=[a, b, c]\")(decorated)\nassertResult(\"user=Alice\")(row.label)\nassertResult(\"n=3\")(row.size)\nassertResult(\"m=3\")(row.methodSize)\nassertResult(\"first=a\")(row.first)\nassertResult(\"ABC\")(folded)\nassertResult([\"a!\", \"b!\", \"c!\"])(exclaimed)\nassertResult([\"b\", \"c\"])(tail(parts))\nassertResult(\"a\")(head(parts))\nassertResult(3)(size(parts))\nassertResult(3)(parts.size())\nassertResult(3)(seen)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "bcd\nb\nAbc123\nAbc123  \n  Abc123\nxbab\na?b?\nabc\nABC\nstarts? true\nends? true\ncontains? true\nmatches? true\nempty? true\nindex = 6\nlast = 3\nlength = 2\nhahaha\ncba\n[a, b, c]\na-b-c\na|b|c\ncount=3\nparts=[a, b, c]\nuser=Alice\nn=3\nm=3\nfirst=a\nABC\n[a!, b!, c!]\nchars = [a, b]\nhead = a\ntail = [b, c]\nparts size = 3\nmethod size = 3\npart = a\npart = b\npart = c\nseen = 3\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_string_dynamic_indices() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-static-string-dynamic-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-static-string-dynamic-{unique}"));
    fs::write(
        &source_path,
        r#"val text = "abacad"
mutable i = 0
mutable count = 0
while(i < length(text)) {
  if(text.at(i) == "a") { count += 1 } else { count += 0 }
  i += 1
}
mutable start = 1
mutable end = 4
val direct = substring("abcdef", start, end)
val method = "abcdef".substring(start, end)
println(count)
println(direct)
println(method)
println("xy".at(start))
assertResult(3)(count)
assertResult("bcd")(direct)
assertResult("bcd")(method)
assertResult("y")("xy".at(start))
"#,
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "static string dynamic index build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "static string dynamic index run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3\nbcd\nbcd\ny\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_split_join_runtime_delimiters() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let delimiter_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-delimiter-{unique}.txt"));
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-delimiter-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-delimiter-{unique}"));
    fs::write(
        &source_path,
        format!(
            r#"val delimiter = FileInput#all("{}")
val parts = "a,b,c".split(delimiter)
val joined = join(["a", "b", "c"], delimiter)
val methodJoined = ["x", "y"].join(delimiter)
println(parts)
println(join(parts, "|"))
println(joined)
println(methodJoined)
assertResult(["a", "b", "c"])(parts)
assertResult("a,b,c")(joined)
assertResult("x,y")(methodJoined)
"#,
            delimiter_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime delimiter build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&delimiter_path, ",").expect("delimiter should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&delimiter_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime delimiter run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[a, b, c]\na|b|c\na,b,c\nx,y\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_replace_runtime_operands() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let from_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-replace-from-{unique}.txt"));
    let to_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-replace-to-{unique}.txt"));
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-replace-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-runtime-replace-{unique}"));
    fs::write(
        &source_path,
        format!(
            r#"val from = FileInput#all("{}")
val to = FileInput#all("{}")
val direct = replace("a-b-a", from, to)
val method = "left-right-left".replace(from, to)
println(direct)
println(method)
assertResult("a_b-a")(direct)
assertResult("left_right-left")(method)
"#,
            from_path.display(),
            to_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime replace operand build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&from_path, "-").expect("from should write after native build");
    fs::write(&to_path, "_").expect("to should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&from_path);
    let _ = fs::remove_file(&to_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime replace operand run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "a_b-a\nleft_right-left\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_repeat_runtime_count() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let count_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-repeat-count-{unique}.txt"));
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-repeat-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-runtime-repeat-{unique}"));
    fs::write(
        &source_path,
        format!(
            r#"val count = length(FileInput#all("{}"))
val direct = repeat("ha", count)
val method = "ho".repeat(count)
println(direct)
println(method)
assertResult("hahaha")(direct)
assertResult("hohoho")(method)
"#,
            count_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime repeat count build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&count_path, "xxx").expect("count source should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&count_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime repeat count run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "hahaha\nhohoho\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_replace_all_runtime_replacement() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let input_path =
        std::env::temp_dir().join(format!("klassic-native-replace-all-input-{unique}.txt"));
    let replacement_path = std::env::temp_dir().join(format!(
        "klassic-native-replace-all-replacement-{unique}.txt"
    ));
    let digit_pattern_path = std::env::temp_dir().join(format!(
        "klassic-native-replace-all-digit-pattern-{unique}.txt"
    ));
    let literal_pattern_path = std::env::temp_dir().join(format!(
        "klassic-native-replace-all-literal-pattern-{unique}.txt"
    ));
    let empty_pattern_path = std::env::temp_dir().join(format!(
        "klassic-native-replace-all-empty-pattern-{unique}.txt"
    ));
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-replace-all-dynamic-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-replace-all-dynamic-{unique}"));
    fs::write(
        &source_path,
        format!(
            r#"val input = FileInput#all("{}")
val replacement = FileInput#all("{}")
val digitPattern = FileInput#all("{}")
val literalPattern = FileInput#all("{}")
val emptyPattern = FileInput#all("{}")
val dynamicInput = replaceAll(input, "[0-9]", replacement)
val staticInput = replaceAll("a1b2", "[0-9]", replacement)
val methodInput = "c3d4".replaceAll("[0-9]", replacement)
val runtimePattern = replaceAll(input, digitPattern, replacement)
val staticRuntimePattern = replaceAll("e5f6", digitPattern, replacement)
val literalRuntimePattern = replaceAll("ab_ab", literalPattern, replacement)
val emptyRuntimePattern = replaceAll("hé", emptyPattern, "-")
println(dynamicInput)
println(staticInput)
println(methodInput)
println(runtimePattern)
println(staticRuntimePattern)
println(literalRuntimePattern)
println(emptyRuntimePattern)
assertResult("aXbX")(dynamicInput)
assertResult("aXbX")(staticInput)
assertResult("cXdX")(methodInput)
assertResult("aXbX")(runtimePattern)
assertResult("eXfX")(staticRuntimePattern)
assertResult("X_X")(literalRuntimePattern)
assertResult("-h-é-")(emptyRuntimePattern)
"#,
            input_path.display(),
            replacement_path.display(),
            digit_pattern_path.display(),
            literal_pattern_path.display(),
            empty_pattern_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime replaceAll replacement build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path, "a1b2").expect("input source should write after native build");
    fs::write(&replacement_path, "X").expect("replacement source should write after native build");
    fs::write(&digit_pattern_path, "[0-9]").expect("digit pattern should write after native build");
    fs::write(&literal_pattern_path, "ab")
        .expect("literal pattern should write after native build");
    fs::write(&empty_pattern_path, "").expect("empty pattern should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&replacement_path);
    let _ = fs::remove_file(&digit_pattern_path);
    let _ = fs::remove_file(&literal_pattern_path);
    let _ = fs::remove_file(&empty_pattern_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime replaceAll replacement run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "aXbX\naXbX\ncXdX\naXbX\neXfX\nX_X\n-h-é-\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_builtin_function_aliases() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-builtin-alias-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-builtin-alias-{unique}"));
    fs::write(
        &source_path,
        "val sub = substring\nval lower = toLowerCase\nval lengthOf = length\nval sameSub = sub\nval make = cons\nval mapper = map\nval folder = foldLeft\nval check = assertResult\nval fs = [substring]\nval r = record { f: substring }\nmutable dyn = substring\ndyn = sameSub\nval xs = make(0)([1])\nval ys = mapper([1, 2])((x) => x + 1)\nval trimmed = mapper([\" a \", \" b \"])(trim)\nval lengths = mapper([\"ab\", \"cd\"])(length)\nval total = folder([1, 2, 3])(0)((acc, x) => acc + x)\nprintln(substring)\nprintln(sub)\nprintln(fs)\nprintln(r)\nprintln(r.f(\"abcd\", 1, 3))\nprintln(head(fs)(\"abcd\", 1, 3))\nprintln(({ println(\"pick builtin\"); sub })(\"abcd\", 1, 3))\nprintln(dyn)\nprintln(dyn(\"abcd\", 1, 3))\nprintln(sub(\"BAR\", 0, 1))\nprintln(lower(\"ABC\"))\nprintln(lengthOf(\"hé\"))\nprintln(sameSub(\"abc\", 1, 3))\nprintln(xs)\nprintln(ys)\nprintln(trimmed)\nprintln(lengths)\nprintln(total)\nassertResult(\"bc\")(r.f(\"abcd\", 1, 3))\nassertResult(\"bc\")(head(fs)(\"abcd\", 1, 3))\nassertResult(\"bc\")(dyn(\"abcd\", 1, 3))\nassertResult(\"B\")(sub(\"BAR\", 0, 1))\nassertResult(\"abc\")(lower(\"ABC\"))\nassertResult(2)(lengthOf(\"hé\"))\nassertResult(\"bc\")(sameSub(\"abc\", 1, 3))\ncheck([0, 1])(xs)\ncheck([2, 3])(ys)\ncheck([\"a\", \"b\"])(trimmed)\ncheck([2, 2])(lengths)\ncheck(6)(total)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "<builtin:substring>\n<builtin:substring>\n[<builtin:substring>]\n#(<builtin:substring>)\nbc\nbc\npick builtin\nbc\n<builtin:substring>\nbc\nB\nabc\n2\nbc\n[0, 1]\n[2, 3]\n[a, b]\n[2, 2]\n6\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_thread_builtin_function_values() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-thread-function-value-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-thread-function-value-{unique}"));
    fs::write(
        &source_path,
        "val fs = [thread]\n({ println(\"pick thread\"); thread })(() => println(\"inline value\"))\nhead(fs)(() => println(\"list value\"))\nprintln(\"main\")\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "pick thread\nmain\ninline value\nlist value\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_side_effect_builtin_function_values() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-side-effect-function-value-{unique}.kl"
    ));
    let output_path = std::env::temp_dir().join(format!(
        "klassic-native-side-effect-function-value-{unique}"
    ));
    fs::write(
        &source_path,
        "val printers = [println]\n({ println(\"pick println\"); println })(\"inline print\")\nhead(printers)(\"list print\")\n({ println(\"pick sleep\"); sleep })(0)\n({ println(\"pick assert\"); assert })(true)\nprintln(\"done\")\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "pick println\ninline print\nlist print\npick sleep\npick assert\ndone\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_curried_builtin_function_values_with_effectful_callees() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-curried-function-value-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-curried-function-value-{unique}"));
    fs::write(
        &source_path,
        "import Set.{contains}\n({ println(\"pick assertResult\"); assertResult })([1])([1])\nval xs = ({ println(\"pick cons\"); cons })(0)([1])\nval ys = ({ println(\"pick map\"); map })([\"a\", \"bb\"])((x) => length(x))\nval total = ({ println(\"pick foldLeft\"); foldLeft })([1, 2])(0)((acc, x) => acc + x)\nval hasRed = ({ println(\"pick contains\"); contains })(%(\"red\", \"blue\"))(\"red\")\nprintln(xs)\nprintln(ys)\nprintln(total)\nprintln(hasRed)\nassertResult([0, 1])(xs)\nassertResult([1, 2])(ys)\nassertResult(3)(total)\nassert(hasRed)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "pick assertResult\npick cons\npick map\npick foldLeft\npick contains\n[0, 1]\n[1, 2]\n3\ntrue\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_collection_builtin_function_values_with_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-collection-function-value-{unique}.kl"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-collection-function-value-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nval n = ({ println(\"pick size\"); size })({ hits += 1; [1, 2] })\nval first = ({ println(\"pick head\"); head })({ hits += 1; [9, 10] })\nval rest = ({ println(\"pick tail\"); tail })({ hits += 1; [9, 10] })\nval empty = ({ println(\"pick empty\"); isEmpty })({ hits += 1; [] })\nval mapSize = ({ println(\"pick map size\"); Map#size })({ hits += 1; %[\"a\": 1] })\nval hasKey = ({ println(\"pick map key\"); Map#containsKey })({ hits += 1; %[\"a\": 1] }, { hits += 1; \"a\" })\nval hasValue = ({ println(\"pick map value\"); Map#containsValue })({ hits += 1; %[\"a\": 1] }, { hits += 1; 1 })\nval got = ({ println(\"pick map get\"); Map#get })({ hits += 1; %[\"a\": 1] }, { hits += 1; \"a\" })\nval setHas = ({ println(\"pick set contains\"); Set#contains })({ hits += 1; %(\"x\", \"y\") }, { hits += 1; \"x\" })\nprintln(hits)\nprintln(n)\nprintln(first)\nprintln(rest)\nprintln(empty)\nprintln(mapSize)\nprintln(hasKey)\nprintln(hasValue)\nprintln(got)\nprintln(setHas)\nassertResult(13)(hits)\nassertResult(2)(n)\nassertResult(9)(first)\nassertResult([10])(rest)\nassert(empty)\nassertResult(1)(mapSize)\nassert(hasKey)\nassert(hasValue)\nassertResult(1)(got)\nassert(setHas)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "pick size\npick head\npick tail\npick empty\npick map size\npick map key\npick map value\npick map get\npick set contains\n13\n2\n9\n[10]\ntrue\n1\ntrue\ntrue\n1\ntrue\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_stopwatch_builtin_function_value() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-stopwatch-function-value-{unique}.kl"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-stopwatch-function-value-{unique}"));
    fs::write(
        &source_path,
        "val elapsed = ({ println(\"pick stopwatch\"); stopwatch })(() => 1)\nassert(elapsed >= 0)\nprintln(\"ok\")\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "pick stopwatch\nok\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_println_error_builtin_function_value() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-println-error-value-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-println-error-value-{unique}"));
    fs::write(
        &source_path,
        "({ println(\"pick error\"); printlnError })(\"err value\")\nprintln(\"out value\")\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "pick error\nout value\n"
    );
    assert_eq!(String::from_utf8_lossy(&run.stderr), "err value\n");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_builtin_functions_sample() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test-programs/builtin_functions.kl");
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-builtin-functions-{unique}"));

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    let _ = fs::remove_file(&output_path);

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_build_ignores_unreferenced_function_bodies() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test-programs/cleanup-expression.kl");
    let output_path = std::env::temp_dir().join(format!("klassic-native-unused-fn-{unique}"));

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert!(run.stdout.is_empty());
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_record_function_calls() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-programs/distance.kl");
    let output_path = std::env::temp_dir().join(format!("klassic-native-distance-{unique}"));

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert!(run.stdout.is_empty());
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_inline_lambda_mutable_capture() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test-programs/function-params-evaluation-count.kl");
    let output_path = std::env::temp_dir().join(format!("klassic-native-eval-count-{unique}"));

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert!(run.stdout.is_empty());
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_typeclass_methods() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test-programs/typeclass-example.kl");
    let output_path = std::env::temp_dir().join(format!("klassic-native-typeclass-{unique}"));

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "Int: 42\nString: Hello\nList[3 elements]\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_advanced_typeclass_dictionary_examples() {
    let cases = [
        (
            "examples/typeclass-final-example.kl",
            "Int(42)\nString(\"Hello\")\n[Int(1), Int(2), Int(3)]\nInt(5) is equal to Int(5)\nString(\"foo\") is not equal to String(\"bar\")\nInt(42)\nString(\"Hello\")\n[Int(1), Int(2), Int(3)]\nInt(5) is equal to Int(5)\nString(\"foo\") is not equal to String(\"bar\")\n",
        ),
        (
            "test-programs/future-features/typeclass-polymorphic.kl",
            "=== Testing polymorphic display ===\nDisplaying: Int(42)\nDisplaying: \"Hello, Klassic!\"\nDisplaying: true\n\n=== Testing showIfEqual ===\nThey are equal: Int(10)\nInt(10) != Int(20)\nThey are equal: \"foo\"\n\"foo\" != \"bar\"\n\n=== Testing showList ===\nOriginal: [Int(1),Int(2),Int(3),Int(4),Int(5)]\nAs strings: Int(1), Int(2), Int(3), Int(4), Int(5)\n\n=== Testing with custom type ===\nDisplaying: Person(name=\"Alice\", age=Int(30))\nDisplaying: Person(name=\"Bob\", age=Int(25))\nThey are equal: Person(name=\"Alice\", age=Int(30))\nPerson(name=\"Alice\", age=Int(30)) != Person(name=\"Bob\", age=Int(25))\n",
        ),
    ];

    for (index, (program, expected_stdout)) in cases.iter().enumerate() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        let source_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(program);
        let output_path =
            std::env::temp_dir().join(format!("klassic-native-typeclass-dict-{unique}-{index}"));

        let build = Command::new(klassic_bin())
            .args([
                "build",
                source_path.to_string_lossy().as_ref(),
                "-o",
                output_path.to_string_lossy().as_ref(),
            ])
            .output()
            .expect("klassic build should run");

        assert!(
            build.status.success(),
            "{program} build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&build.stdout),
            String::from_utf8_lossy(&build.stderr)
        );
        assert!(build.stdout.is_empty());
        assert!(build.stderr.is_empty());

        let run = Command::new(&output_path)
            .output()
            .expect("generated executable should run");

        let _ = fs::remove_file(&output_path);

        assert!(
            run.status.success(),
            "{program} run failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), *expected_stdout);
        assert!(run.stderr.is_empty());
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_higher_kinded_list_calls() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test-programs/hkt-no-constraints.kl");
    let output_path = std::env::temp_dir().join(format!("klassic-native-hkt-list-{unique}"));

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "Original: [1, 2, 3, 4, 5]\nDoubled: [2, 4, 6, 8, 10]\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_list_monad_calls() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test-programs/working-monad-example.kl");
    let output_path = std::env::temp_dir().join(format!("klassic-native-list-monad-{unique}"));

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "[2]\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_file_output_sample() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let work_dir = std::env::temp_dir().join(format!("klassic-native-file-output-{unique}"));
    fs::create_dir(&work_dir).expect("temp work dir should be created");
    let source_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-programs/file-output.kl");
    let output_path = work_dir.join("file-output");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .current_dir(&work_dir)
        .output()
        .expect("generated executable should run");

    let leftover_file = work_dir.join("test-output.txt").exists();
    let leftover_lines = work_dir.join("test-lines.txt").exists();
    let _ = fs::remove_dir_all(&work_dir);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "File written successfully\nContent appended successfully\nFile exists: true\nMultiple lines written\nFile content: Hello, Klassic!\nAppended line\nLines read: [Line 1, Line 2, Line 3]\nTest files cleaned up\n"
    );
    assert!(run.stderr.is_empty());
    assert!(!leftover_file);
    assert!(!leftover_lines);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_file_helper_argument_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let work_dir = std::env::temp_dir().join(format!("klassic-native-file-effect-{unique}"));
    fs::create_dir(&work_dir).expect("temp work dir should be created");
    let source_path = work_dir.join("file-effect.kl");
    let output_path = work_dir.join("file-effect");
    fs::write(
        &source_path,
        "mutable hits = 0\nFileOutput#write({ hits += 1; \"effect.txt\" }, { hits += 1; \"hello\" })\nFileOutput#append({ hits += 1; \"effect.txt\" }, { hits += 1; \"!\" })\nval exists = FileOutput#exists({ hits += 1; \"effect.txt\" })\nval content = FileInput#all({ hits += 1; \"effect.txt\" })\nFileOutput#writeLines({ hits += 1; \"lines.txt\" }, { hits += 1; [\"a\", \"b\"] })\nval lines = FileInput#lines({ hits += 1; \"lines.txt\" })\nFileOutput#delete({ hits += 1; \"effect.txt\" })\nFileOutput#delete({ hits += 1; \"lines.txt\" })\nprintln(hits)\nassertResult(11)(hits)\nassert(exists)\nassertResult(\"hello!\")(content)\nassertResult([\"a\", \"b\"])(lines)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .current_dir(&work_dir)
        .output()
        .expect("generated executable should run");

    let leftover_effect = work_dir.join("effect.txt").exists();
    let leftover_lines = work_dir.join("lines.txt").exists();
    let _ = fs::remove_dir_all(&work_dir);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "11\n");
    assert!(run.stderr.is_empty());
    assert!(!leftover_effect);
    assert!(!leftover_lines);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_file_builtin_function_values() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let work_dir = std::env::temp_dir().join(format!("klassic-native-file-values-{unique}"));
    fs::create_dir(&work_dir).expect("temp work dir should be created");
    let source_path = work_dir.join("file-values.kl");
    let output_path = work_dir.join("file-values");
    fs::write(
        &source_path,
        "val path = \"value.txt\"\nval linesPath = \"lines.txt\"\n({ println(\"pick write\"); FileOutput#write })(path, \"hello\")\nval appenders = [FileOutput#append]\nhead(appenders)(path, \"!\")\nprintln(({ println(\"pick all\"); FileInput#all })(path))\nprintln(({ println(\"pick open\"); FileInput#open })(path, (stream) => FileInput#readAll(stream)))\n({ println(\"pick writeLines\"); FileOutput#writeLines })(linesPath, [\"a\", \"b\"])\nprintln(({ println(\"pick lines\"); FileInput#lines })(linesPath))\n({ println(\"pick delete\"); FileOutput#delete })(path)\n({ println(\"pick delete lines\"); FileOutput#delete })(linesPath)\nprintln(({ println(\"pick exists\"); FileOutput#exists })(path))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "file builtin value build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .current_dir(&work_dir)
        .output()
        .expect("generated executable should run");

    let leftover_value = work_dir.join("value.txt").exists();
    let leftover_lines = work_dir.join("lines.txt").exists();
    let _ = fs::remove_dir_all(&work_dir);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "pick write\npick all\nhello!\npick open\nhello!\npick writeLines\npick lines\n[a, b]\npick delete\npick delete lines\npick exists\nfalse\n"
    );
    assert!(run.stderr.is_empty());
    assert!(!leftover_value);
    assert!(!leftover_lines);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_file_input_sample() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-programs/file-input.kl");
    let output_path = std::env::temp_dir().join(format!("klassic-native-file-input-{unique}"));

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert!(run.stdout.is_empty());
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_file_input_printing() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let input_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-input-{unique}.txt"));
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-input-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-runtime-input-{unique}"));
    fs::write(
        &source_path,
        format!(
            "val path = \"{}\"\nval all = FileInput#all\nmutable hits = 0\nprintln(\"content=\" + all({{ hits += 1; path }}))\nprintln(hits)\n",
            input_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime file input build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path, "runtime file").expect("input should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime file input run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "content=runtime file\n1\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_file_input_open_callback_body() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let input_path_holder = std::env::temp_dir().join(format!(
        "klassic-native-runtime-open-callback-holder-{unique}.txt"
    ));
    let input_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-open-callback-{unique}.txt"));
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-open-callback-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-open-callback-{unique}"));
    fs::write(
        &source_path,
        format!(
            r#"val path = FileInput#all("{}")
mutable hits = 0
val openedPath = FileInput#open(path, (stream) => {{
  hits += 1
  stream
}})
val lengthViaOpen = FileInput#open(path, (stream) => {{
  hits += 1
  length(FileInput#readAll(stream))
}})
val linesViaOpen = FileInput#open(path, (stream) => {{
  hits += 1
  FileInput#readLines(stream)
}})
val cleanupText = FileInput#open(path, (stream) => {{
  FileInput#readAll(stream) cleanup {{ hits += 1 }}
}})
println(openedPath == path)
println(lengthViaOpen)
println(linesViaOpen)
println(join(linesViaOpen, "|"))
println(cleanupText)
println(hits)
assert(openedPath == path)
assertResult(11)(lengthViaOpen)
assertResult(["hello", "world"])(linesViaOpen)
assertResult("hello\nworld")(cleanupText)
assertResult(4)(hits)
"#,
            input_path_holder.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime FileInput#open callback build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path_holder, input_path.to_string_lossy().as_bytes())
        .expect("input path holder should write after native build");
    fs::write(&input_path, "hello\nworld").expect("input should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&input_path_holder);
    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime FileInput#open callback run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "true\n11\n[hello, world]\nhello|world\nhello\nworld\n4\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_file_input_open_runtime_callback_body() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-static-open-runtime-callback-{unique}.kl"
    ));
    let output_path = std::env::temp_dir().join(format!(
        "klassic-native-static-open-runtime-callback-{unique}"
    ));
    fs::write(
        &source_path,
        r#"val opened = FileInput#open("static-name", (stream) => {
  println("stream=" + stream)
  args()
})
println(opened)
println(join(opened, "|"))
assertResult(["alpha", "beta"])(opened)
"#,
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "static FileInput#open runtime callback build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .arg("alpha")
        .arg("beta")
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "static FileInput#open runtime callback run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "stream=static-name\n[alpha, beta]\nalpha|beta\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_file_input_open_callable_values() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let input_path_holder =
        std::env::temp_dir().join(format!("klassic-native-open-callable-holder-{unique}.txt"));
    let input_path =
        std::env::temp_dir().join(format!("klassic-native-open-callable-{unique}.txt"));
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-open-callable-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-open-callable-{unique}"));
    fs::write(
        &source_path,
        format!(
            r#"val runtimePath = FileInput#all("{}")
val readAll = FileInput#readAll
val readLines = FileInput#readLines
println(FileInput#open("Cargo.toml", readAll).contains("klassic"))
println(FileInput#open(runtimePath, readAll))
println(join(FileInput#open(runtimePath, readLines), "|"))
println(FileInput#open(runtimePath, {{
  println("pick callback")
  FileInput#readAll
}}))
assert(FileInput#open("Cargo.toml", FileInput#readAll).contains("klassic"))
assertResult("dynamic callback")(FileInput#open(runtimePath, readAll))
assertResult(["dynamic callback"])(FileInput#open(runtimePath, FileInput#readLines))
"#,
            input_path_holder.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "FileInput#open callable value build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path_holder, input_path.to_string_lossy().as_bytes())
        .expect("input path holder should write after native build");
    fs::write(&input_path, "dynamic callback").expect("input should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&input_path_holder);
    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "FileInput#open callable value run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "true\ndynamic callback\ndynamic callback\npick callback\ndynamic callback\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_file_input_binding() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let input_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-input-binding-{unique}.txt"));
    let empty_path = std::env::temp_dir().join(format!(
        "klassic-native-runtime-input-binding-empty-{unique}.txt"
    ));
    let unicode_path = std::env::temp_dir().join(format!(
        "klassic-native-runtime-input-binding-unicode-{unique}.txt"
    ));
    let needle_path = std::env::temp_dir().join(format!(
        "klassic-native-runtime-input-binding-needle-{unique}.txt"
    ));
    let padded_path = std::env::temp_dir().join(format!(
        "klassic-native-runtime-input-binding-padded-{unique}.txt"
    ));
    let mixed_case_path = std::env::temp_dir().join(format!(
        "klassic-native-runtime-input-binding-mixed-case-{unique}.txt"
    ));
    let digit_path = std::env::temp_dir().join(format!(
        "klassic-native-runtime-input-binding-digit-{unique}.txt"
    ));
    let digits_path = std::env::temp_dir().join(format!(
        "klassic-native-runtime-input-binding-digits-{unique}.txt"
    ));
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-input-binding-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-input-binding-{unique}"));
    fs::write(
        &source_path,
        format!(
            "val path = \"{}\"\nval emptyPath = \"{}\"\nval unicodePath = \"{}\"\nval needlePath = \"{}\"\nval paddedPath = \"{}\"\nval mixedCasePath = \"{}\"\nval digitPath = \"{}\"\nval digitsPath = \"{}\"\nmutable hits = 0\nval text = FileInput#all({{ hits += 1; path }})\nval empty = FileInput#all({{ hits += 1; emptyPath }})\nval unicode = FileInput#all({{ hits += 1; unicodePath }})\nval needle = FileInput#all({{ hits += 1; needlePath }})\nval padded = FileInput#all({{ hits += 1; paddedPath }})\nval mixedCase = FileInput#all({{ hits += 1; mixedCasePath }})\nval digit = FileInput#all({{ hits += 1; digitPath }})\nval digits = FileInput#all({{ hits += 1; digitsPath }})\nval combined = \"content=\" + text\nval shouted = text + \"!\"\nval interpolated = \"message=#{{text}}!\"\nprintln(combined)\nprintln(shouted)\nprintln(interpolated)\nprintln(combined == \"content=bound runtime file\")\nprintln(text == \"bound runtime file\")\nprintln(text != \"other\")\nprintln(isEmptyString(text))\nprintln(isEmptyString(empty))\nprintln(empty.isEmpty())\nprintln(length(text))\nprintln(length(unicode))\nprintln(substring(text, 6, 13))\nprintln(text.substring(0, 5))\nprintln(at(text, 6))\nprintln(text.at(7))\nprintln(substring(unicode, 1, 2))\nprintln(at(unicode, 1))\nprintln(substring(unicode, length(digit), length(unicode)))\nprintln(at(unicode, length(digit)))\nprintln(startsWith(text, \"bound\"))\nprintln(endsWith(text, \"file\"))\nprintln(text.startsWith(\"bound\"))\nprintln(text.endsWith(\"file\"))\nprintln(text.contains(needle))\nprintln(indexOf(text, needle))\nprintln(text.indexOf(needle))\nprintln(lastIndexOf(text, \"i\"))\nprintln(text.lastIndexOf(\"i\"))\nprintln(indexOf(text, \"missing\"))\nprintln(repeat(digit, length(unicode)))\nprintln(digit.repeat(length(unicode)))\nprintln(hits)\nassertResult(\"bound runtime file\")(text)\nassertResult(\"bound runtime file\")(text.toString())\nassertResult(text)(\"bound runtime file\")\nassertResult(combined)(\"content=bound runtime file\")\nassertResult(\"message=bound runtime file!\")(interpolated)\nassertResult(\"runtime\")(substring(text, 6, 13))\nassertResult(\"bound\")(text.substring(0, 5))\nassertResult(\"r\")(at(text, 6))\nassertResult(\"u\")(text.at(7))\nassertResult(\"é\")(substring(unicode, 1, 2))\nassertResult(\"é\")(at(unicode, 1))\nassertResult(\"é\")(substring(unicode, length(digit), length(unicode)))\nassertResult(\"é\")(at(unicode, length(digit)))\nassertResult(\"spaced runtime\")(trim(padded))\nassertResult(\"spaced runtime  \")(trimLeft(padded))\nassertResult(\"  spaced runtime\")(trimRight(padded))\nassertResult(\"bound runtime filebound runtime file\")(repeat(text, 2))\nassertResult(\"bound runtime filebound runtime file\")(text.repeat(2))\nassertResult(\"\")(repeat(empty, 3))\nassertResult(\"77\")(repeat(digit, length(unicode)))\nassertResult(\"77\")(digit.repeat(length(unicode)))\nassertResult(\"abc-é\")(toLowerCase(mixedCase))\nassertResult(\"ABC-é\")(toUpperCase(mixedCase))\nassertResult(\"bound native file\")(replace(text, \"runtime\", \"native\"))\nassertResult(\"bound native file\")(text.replace(\"runtime\", \"native\"))\nassertResult(\"bound 7 file\")(replace(text, needle, digit))\nassertResult(\"bound 7 file\")(text.replace(needle, digit))\nassertResult(\"bound runtime file\")(replace(text, \"missing\", \"native\"))\nassertResult(\"xbound runtime file\")(replace(text, \"\", \"x\"))\nassertResult(\"bound runtXme fXle\")(replaceAll(text, \"i\", \"X\"))\nassertResult(\"bound runtXme fXle\")(text.replaceAll(\"i\", \"X\"))\nassertResult(\"?????\")(replaceAll(digits, \"[0-9]\", \"?\"))\nassertResult(\"-h-é-\")(replaceAll(unicode, \"\", \"-\"))\nassertResult(\"elif emitnur dnuob\")(reverse(text))\nassertResult(\"éh\")(reverse(unicode))\nassert(matches(digits, \"[0-9]+\"))\nassert(matches(digit, \"[0-9]\"))\nassert(matches(text, \".*\"))\nassert(matches(text, \"bound runtime file\"))\nassert(matches(digit, digit))\nassert(matches(text, replace(digit, digit, \".*\")))\nassert(matches(digits, replace(digit, digit, \"[0-9]+\")))\nassert(matches(digit, replace(digit, digit, \"[0-9]\")))\nassert(!matches(text, \"bound\"))\nassert(!matches(text, \"[0-9]+\"))\nassert(!matches(text, replace(digit, digit, \"[0-9]+\")))\nassert(!matches(digits, \"[0-9]\"))\nassert(startsWith(text, \"bound\"))\nassert(endsWith(text, \"file\"))\nassert(text.contains(needle))\nassertResult(6)(indexOf(text, needle))\nassertResult(15)(lastIndexOf(text, \"i\"))\n",
            input_path.display(),
            empty_path.display(),
            unicode_path.display(),
            needle_path.display(),
            padded_path.display(),
            mixed_case_path.display(),
            digit_path.display(),
            digits_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime file input binding build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path, "bound runtime file").expect("input should write after native build");
    fs::write(&empty_path, "").expect("empty input should write after native build");
    fs::write(&unicode_path, "hé").expect("unicode input should write after native build");
    fs::write(&needle_path, "runtime").expect("needle input should write after native build");
    fs::write(&padded_path, "  spaced runtime  ")
        .expect("padded input should write after native build");
    fs::write(&mixed_case_path, "AbC-é").expect("mixed case input should write after native build");
    fs::write(&digit_path, "7").expect("digit input should write after native build");
    fs::write(&digits_path, "12345").expect("digits input should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&empty_path);
    let _ = fs::remove_file(&unicode_path);
    let _ = fs::remove_file(&needle_path);
    let _ = fs::remove_file(&padded_path);
    let _ = fs::remove_file(&mixed_case_path);
    let _ = fs::remove_file(&digit_path);
    let _ = fs::remove_file(&digits_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime file input binding run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "content=bound runtime file\nbound runtime file!\nmessage=bound runtime file!\ntrue\ntrue\ntrue\nfalse\ntrue\ntrue\n18\n2\nruntime\nbound\nr\nu\né\né\né\né\ntrue\ntrue\ntrue\ntrue\ntrue\n6\n6\n15\n15\n-1\n77\n77\n8\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_recursive_runtime_top_level_captures() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let input_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-runtime-input-{unique}.txt"
    ));
    let input_path_holder = std::env::temp_dir().join(format!(
        "klassic-native-recursive-runtime-input-holder-{unique}.txt"
    ));
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-recursive-runtime-capture-{unique}.kl"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-recursive-runtime-capture-{unique}"));
    fs::write(
        &source_path,
        format!(
            "val path = FileInput#all(\"{}\")\nval text = FileInput#all(path)\nval lines = FileInput#lines(path)\ndef textLengthAfter(n: Int): Int = if(n == 0) length(text) else textLengthAfter(n - 1)\ndef lineCountAfter(n: Int): Int = if(n == 0) lines.size() else lineCountAfter(n - 1)\ndef firstLineLengthAfter(n: Int): Int = if(n == 0) length(lines.head()) else firstLineLengthAfter(n - 1)\nprintln(textLengthAfter(2))\nprintln(lineCountAfter(3))\nprintln(firstLineLengthAfter(1))\nassertResult(8)(textLengthAfter(2))\nassertResult(3)(lineCountAfter(3))\nassertResult(3)(firstLineLengthAfter(1))\n",
            input_path_holder.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "recursive runtime capture build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path_holder, input_path.to_string_lossy().as_bytes())
        .expect("path holder should write after native build");
    fs::write(&input_path, "abc\nxy\nz").expect("input should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&input_path_holder);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "recursive runtime capture run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "8\n3\n3\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_file_output_content() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let input_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-output-input-{unique}.txt"));
    let output_content_path = std::env::temp_dir().join(format!(
        "klassic-native-runtime-output-content-{unique}.txt"
    ));
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-output-content-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-output-content-{unique}"));
    fs::write(
        &source_path,
        format!(
            "val inputPath = \"{}\"\nval outputPath = \"{}\"\nval text = FileInput#all(inputPath)\nFileOutput#write(outputPath, text)\nFileOutput#append(outputPath, replaceAll(text, \"i\", \"!\"))\nprintln(FileInput#all(outputPath))\nprintln(FileOutput#exists(outputPath))\nprintln(Dir#isFile(outputPath))\n",
            input_path.display(),
            output_content_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime file output content build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path, "runtime").expect("input should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");
    let output_content =
        fs::read_to_string(&output_content_path).expect("output content should be readable");

    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&output_content_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime file output content run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "runtimerunt!me\ntrue\ntrue\n"
    );
    assert!(run.stderr.is_empty());
    assert_eq!(output_content, "runtimerunt!me");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_file_paths() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let input_path_holder =
        std::env::temp_dir().join(format!("klassic-native-runtime-path-holder-{unique}.txt"));
    let output_path_holder = std::env::temp_dir().join(format!(
        "klassic-native-runtime-output-path-holder-{unique}.txt"
    ));
    let target_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-path-target-{unique}.txt"));
    let output_content_path = std::env::temp_dir().join(format!(
        "klassic-native-runtime-path-output-content-{unique}.txt"
    ));
    let dynamic_dir_path =
        std::path::PathBuf::from(format!("{}.dir", output_content_path.display()));
    let dynamic_copy_path =
        std::path::PathBuf::from(format!("{}.copy", output_content_path.display()));
    let dynamic_moved_path =
        std::path::PathBuf::from(format!("{}.moved", output_content_path.display()));
    let dynamic_delete_path =
        std::path::PathBuf::from(format!("{}.delete", output_content_path.display()));
    let dynamic_lines_path =
        std::path::PathBuf::from(format!("{}.lines", output_content_path.display()));
    let dynamic_rewritten_lines_path =
        std::path::PathBuf::from(format!("{}.rewritten-lines", output_content_path.display()));
    let dynamic_parent_dir_path =
        std::path::PathBuf::from(format!("{}.nested", output_content_path.display()));
    let dynamic_nested_dir_path = dynamic_parent_dir_path.join("inner");
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-paths-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-runtime-paths-{unique}"));
    fs::write(
        &source_path,
        format!(
            "val pathHolder = \"{}\"\nval outputPathHolder = \"{}\"\nval targetPath = FileInput#all(pathHolder)\nval outputPath = FileInput#all(outputPathHolder)\nval dirPath = outputPath + \".dir\"\nval parentPath = outputPath + \".nested\"\nval nestedPath = parentPath + \"/inner\"\nval copyPath = outputPath + \".copy\"\nval movedPath = outputPath + \".moved\"\nval deletePath = outputPath + \".delete\"\nval linesPath = outputPath + \".lines\"\nval rewrittenLinesPath = outputPath + \".rewritten-lines\"\nprintln(FileInput#all(targetPath))\nprintln(FileInput#open(targetPath, (stream) => FileInput#readAll(stream)))\nprintln(FileOutput#exists(targetPath))\nprintln(Dir#isFile(targetPath))\nprintln(Dir#isDirectory(targetPath))\nFileOutput#write(outputPath, \"runtime path \")\nFileOutput#append(outputPath, FileInput#all(targetPath))\nprintln(FileInput#all(outputPath))\nval outputContent = FileInput#all(outputPath)\nval summary = \"len=#{{length(outputContent)}}, empty=#{{isEmptyString(outputContent)}}, exists=#{{FileOutput#exists(outputPath)}}\"\nval plusSummary = \"plus len=\" + length(outputContent) + \", empty=\" + isEmptyString(outputContent) + \", exists=\" + FileOutput#exists(outputPath)\nprintln(summary)\nprintln(plusSummary)\nFileOutput#writeLines(linesPath, [\"line-a\", \"line-b\"])\nprintln(FileInput#all(linesPath))\nprintln(FileInput#lines(linesPath))\nprintln(FileInput#open(linesPath, (stream) => FileInput#readLines(stream)))\nval runtimeLines = FileInput#lines(linesPath)\nval openLines = FileInput#open(linesPath, (stream) => FileInput#readLines(stream))\nprintln(runtimeLines)\nprintln(openLines)\nprintln(size(runtimeLines))\nprintln(isEmpty(runtimeLines))\nprintln(size(openLines))\nprintln(head(runtimeLines))\nprintln(head(openLines))\nprintln(tail(runtimeLines))\nprintln(size(tail(runtimeLines)))\nprintln(isEmpty(tail(tail(runtimeLines))))\nprintln(join(runtimeLines, \"|\"))\nprintln(tail(runtimeLines).join(\"/\"))\nprintln(runtimeLines == [\"line-a\", \"line-b\"])\nprintln([\"line-a\", \"line-b\"] == runtimeLines)\nprintln(runtimeLines != [\"line-a\"])\nprintln(runtimeLines == openLines)\nassertResult([\"line-a\", \"line-b\"])(runtimeLines)\nassertResult(runtimeLines)(openLines)\nassert(runtimeLines == [\"line-a\", \"line-b\"])\nassert([\"line-a\"] != runtimeLines)\nassert(runtimeLines == openLines)\nmutable lineCount = 0\nforeach(line in runtimeLines) {{\n  println(\"line \" + line)\n  lineCount += 1\n}}\nprintln(lineCount)\nassertResult(2)(lineCount)\nFileOutput#writeLines(rewrittenLinesPath, runtimeLines)\nprintln(FileInput#all(rewrittenLinesPath))\nassertResult(runtimeLines)(FileInput#lines(rewrittenLinesPath))\nprintln(FileOutput#exists(outputPath))\nprintln(Dir#isFile(outputPath))\nDir#mkdir(dirPath)\nprintln(Dir#isDirectory(dirPath))\nDir#delete(dirPath)\nprintln(Dir#exists(dirPath))\nDir#mkdirs(nestedPath)\nprintln(Dir#isDirectory(nestedPath))\nDir#delete(nestedPath)\nDir#delete(parentPath)\nprintln(Dir#exists(parentPath))\nDir#copy(outputPath, copyPath)\nprintln(FileInput#all(copyPath))\nDir#move(copyPath, movedPath)\nprintln(FileOutput#exists(copyPath))\nprintln(FileInput#all(movedPath))\nFileOutput#write(deletePath, \"bye\")\nprintln(FileOutput#exists(deletePath))\nFileOutput#delete(deletePath)\nprintln(FileOutput#exists(deletePath))\nFileOutput#delete(linesPath)\nFileOutput#delete(rewrittenLinesPath)\nFileOutput#delete(movedPath)\nprintln(FileOutput#exists(outputPath))\n",
            input_path_holder.display(),
            output_path_holder.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime file path build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path_holder, target_path.to_string_lossy().as_bytes())
        .expect("input path holder should write after native build");
    fs::write(
        &output_path_holder,
        output_content_path.to_string_lossy().as_bytes(),
    )
    .expect("output path holder should write after native build");
    fs::write(&target_path, "dynamic target").expect("target should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");
    let output_content =
        fs::read_to_string(&output_content_path).expect("output content should be readable");

    let _ = fs::remove_file(&input_path_holder);
    let _ = fs::remove_file(&output_path_holder);
    let _ = fs::remove_file(&target_path);
    let _ = fs::remove_file(&output_content_path);
    let _ = fs::remove_file(&dynamic_copy_path);
    let _ = fs::remove_file(&dynamic_moved_path);
    let _ = fs::remove_file(&dynamic_delete_path);
    let _ = fs::remove_file(&dynamic_lines_path);
    let _ = fs::remove_file(&dynamic_rewritten_lines_path);
    let _ = fs::remove_dir(&dynamic_dir_path);
    let _ = fs::remove_dir(&dynamic_nested_dir_path);
    let _ = fs::remove_dir(&dynamic_parent_dir_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime file path run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "dynamic target\ndynamic target\ntrue\ntrue\nfalse\nruntime path dynamic target\nlen=27, empty=false, exists=true\nplus len=27, empty=false, exists=true\nline-a\nline-b\n[line-a, line-b]\n[line-a, line-b]\n[line-a, line-b]\n[line-a, line-b]\n2\nfalse\n2\nline-a\nline-a\n[line-b]\n1\ntrue\nline-a|line-b\nline-b\ntrue\ntrue\ntrue\ntrue\nline line-a\nline line-b\n2\nline-a\nline-b\ntrue\ntrue\ntrue\nfalse\ntrue\nfalse\nruntime path dynamic target\nfalse\nruntime path dynamic target\ntrue\nfalse\ntrue\n"
    );
    assert!(run.stderr.is_empty());
    assert_eq!(output_content, "runtime path dynamic target");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_dir_list_paths() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let dir_path_holder =
        std::env::temp_dir().join(format!("klassic-native-dir-list-path-holder-{unique}.txt"));
    let dir_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-dir-list-dir-{unique}"));
    let first_entry_path = dir_path.join("a.txt");
    let second_entry_path = dir_path.join("b.txt");
    let third_entry_path = dir_path.join("c.txt");
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-dir-list-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-dir-list-{unique}"));
    let source = format!(
        r#"val dirPathHolder = "{}"
val dirPath = FileInput#all(dirPathHolder)
Dir#mkdir(dirPath)
val secondEntryPath = dirPath + "/b.txt"
val firstEntryPath = dirPath + "/a.txt"
val thirdEntryPath = dirPath + "/c.txt"
FileOutput#write(secondEntryPath, "b")
FileOutput#write(firstEntryPath, "a")
FileOutput#write(thirdEntryPath, "c")
val entries = Dir#list(dirPath)
val fullEntries = Dir#listFull(dirPath)
println(entries)
println(size(entries))
println(head(entries))
println(join(entries, "|"))
println(fullEntries)
println(size(fullEntries))
println(head(fullEntries))
foreach(entry in entries) {{
  println("entry=" + entry)
}}
mutable seen = 0
foreach(entry in fullEntries) {{
  if(entry.endsWith("/a.txt") || entry.endsWith("/b.txt") || entry.endsWith("/c.txt")) {{
    seen += 1
  }}
}}
println(seen)
assertResult(["a.txt", "b.txt", "c.txt"])(entries)
assertResult(3)(size(fullEntries))
assert(endsWith(head(fullEntries), "/a.txt"))
assertResult(3)(seen)
FileOutput#delete(firstEntryPath)
FileOutput#delete(secondEntryPath)
FileOutput#delete(thirdEntryPath)
Dir#delete(dirPath)
"#,
        dir_path_holder.display()
    );
    fs::write(&source_path, source).expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime dir list build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&dir_path_holder, dir_path.to_string_lossy().as_bytes())
        .expect("dir path holder should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&dir_path_holder);
    let _ = fs::remove_file(&first_entry_path);
    let _ = fs::remove_file(&second_entry_path);
    let _ = fs::remove_file(&third_entry_path);
    let _ = fs::remove_dir(&dir_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime dir list run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        format!(
            "[a.txt, b.txt, c.txt]\n3\na.txt\na.txt|b.txt|c.txt\n[{}, {}, {}]\n3\n{}\nentry=a.txt\nentry=b.txt\nentry=c.txt\n3\n",
            first_entry_path.display(),
            second_entry_path.display(),
            third_entry_path.display(),
            first_entry_path.display()
        )
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_dir_current() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let run_dir = std::env::temp_dir().join(format!("klassic-native-current-dir-{unique}"));
    let marker_path = run_dir.join("marker.txt");
    let source_path = std::env::temp_dir().join(format!("klassic-native-current-dir-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-current-dir-bin-{unique}"));
    fs::create_dir_all(&run_dir).expect("run dir should be created");
    fs::write(&marker_path, "marker").expect("marker should write");
    fs::write(
        &source_path,
        "val here = Dir#current()\nprintln(here)\nprintln(Dir#isDirectory(here))\nprintln(Dir#list(here))\nassert(Dir#isDirectory(here))\nassertResult([\"marker.txt\"])(Dir#list(here))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime current dir build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .current_dir(&run_dir)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&marker_path);
    let _ = fs::remove_dir(&run_dir);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime current dir run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        format!("{}\ntrue\n[marker.txt]\n", run_dir.display())
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_dir_home_and_temp() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let home_dir = std::env::temp_dir().join(format!("klassic-native-home-{unique}"));
    let temp_dir = std::env::temp_dir().join(format!("klassic-native-temp-{unique}"));
    let home_text = home_dir.display().to_string();
    let temp_text = temp_dir.display().to_string();
    let source_path = std::env::temp_dir().join(format!("klassic-native-home-temp-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-home-temp-{unique}"));
    fs::create_dir_all(&home_dir).expect("home dir should be created");
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    fs::write(
        &source_path,
        format!(
            "val home = Dir#home()\nval temp = Dir#temp()\nprintln(home)\nprintln(temp)\nprintln(Dir#exists(home))\nprintln(Dir#isDirectory(temp))\nassertResult(\"{home_text}\")(home)\nassertResult(\"{temp_text}\")(temp)\nassert(Dir#exists(Dir#home()))\nassert(Dir#isDirectory(Dir#temp()))\n"
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime home/temp build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .env("HOME", &home_dir)
        .env("TMPDIR", &temp_dir)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_dir(&home_dir);
    let _ = fs::remove_dir(&temp_dir);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime home/temp run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        format!("{home_text}\n{temp_text}\ntrue\ntrue\n")
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_command_line_args() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-args-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-args-{unique}"));
    fs::write(
        &source_path,
        "def captured() = CommandLine#args()\nval getArgs = CommandLine#args\nval xs = getArgs()\nval ys = captured()\nval zs = args()\nprintln(xs)\nprintln(ys)\nprintln(zs)\nprintln(size(xs))\nprintln(head(xs))\nprintln(join(xs, \"|\"))\nassertResult([\"alpha\", \"two words\", \"gamma\"])(xs)\nassertResult(xs)(ys)\nassertResult(xs)(zs)\nassertResult(3)(size(ys))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "command line args build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .args(["alpha", "two words", "gamma"])
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "command line args run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[alpha, two words, gamma]\n[alpha, two words, gamma]\n[alpha, two words, gamma]\n3\nalpha\nalpha|two words|gamma\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_process_exit() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-process-exit-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-process-exit-{unique}"));
    fs::write(
        &source_path,
        "val direct = Process#exit\nval quit = exit\nprintln(\"before exit\")\nquit({ println(\"code path\"); 7 })\nprintln(\"after exit\")\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "process exit build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert_eq!(run.status.code(), Some(7));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "before exit\ncode path\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_standard_input_all() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-stdin-all-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-stdin-all-{unique}"));
    fs::write(
        &source_path,
        "val read = StandardInput#all\nval text = read()\nprintln(trimRight(text))\nprintln(length(text))\nassertResult(\"alpha\\nbeta\\n\")(text)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "standard input build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let mut child = Command::new(&output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("generated executable should run");

    {
        let mut stdin = child.stdin.take().expect("stdin should be piped");
        stdin
            .write_all(b"alpha\nbeta\n")
            .expect("stdin should accept input");
    }

    let run = child
        .wait_with_output()
        .expect("generated executable should finish after stdin closes");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "standard input run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "alpha\nbeta\n11\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_standard_input_lines() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-stdin-lines-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-stdin-lines-{unique}"));
    fs::write(
        &source_path,
        "val lines = stdinLines()\nprintln(lines)\nprintln(join(lines, \"|\"))\nassertResult([\"alpha\", \"beta\"])(lines)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "standard input lines build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let mut child = Command::new(&output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("generated executable should run");

    {
        let mut stdin = child.stdin.take().expect("stdin should be piped");
        stdin
            .write_all(b"alpha\nbeta\n")
            .expect("stdin should accept input");
    }

    let run = child
        .wait_with_output()
        .expect("generated executable should finish after stdin closes");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "standard input lines run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[alpha, beta]\nalpha|beta\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_environment_vars() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-env-vars-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-env-vars-{unique}"));
    fs::write(
        &source_path,
        "val vars = Environment#vars()\nmutable found = false\nforeach(entry in vars) {\n  if(entry == \"KLASSIC_NATIVE_ENV_TEST=alpha\") {\n    found = true\n  }\n}\nprintln(found)\nassert(found)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "environment vars build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .env("KLASSIC_NATIVE_ENV_TEST", "alpha")
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "environment vars run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "true\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_environment_get_and_exists() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-env-get-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-env-get-{unique}"));
    fs::write(
        &source_path,
        "val get = Environment#get\nval exists = hasEnv\nprintln(get(\"KLASSIC_NATIVE_ENV_GET_TEST\"))\nprintln(exists(\"KLASSIC_NATIVE_ENV_GET_TEST\"))\nprintln(Environment#exists(\"KLASSIC_NATIVE_ENV_GET_MISSING\"))\nassertResult(\"alpha\")(getEnv(\"KLASSIC_NATIVE_ENV_GET_TEST\"))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "environment get build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .env("KLASSIC_NATIVE_ENV_GET_TEST", "alpha")
        .env_remove("KLASSIC_NATIVE_ENV_GET_MISSING")
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "environment get run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "alpha\ntrue\nfalse\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_environment_key_lookup() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-env-key-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-env-key-{unique}"));
    fs::write(
        &source_path,
        "val key = head(args())\nval missing = \"KLASSIC_NATIVE_ENV_DYNAMIC_MISSING\"\nprintln(Environment#get(key))\nprintln(Environment#exists(key))\nprintln(hasEnv(missing))\nassertResult(\"beta\")(getEnv(key))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime environment key build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .arg("KLASSIC_NATIVE_ENV_DYNAMIC_KEY")
        .env("KLASSIC_NATIVE_ENV_DYNAMIC_KEY", "beta")
        .env_remove("KLASSIC_NATIVE_ENV_DYNAMIC_MISSING")
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime environment key run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "beta\ntrue\nfalse\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_to_string_values() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-to-string-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-to-string-{unique}"));
    fs::write(
        &source_path,
        "val n = size(args())\nval ok = Environment#exists(head(args()))\nval nt = toString(n)\nval okt = toString(ok)\nprintln(nt)\nprintln(okt)\nprintln(\"n=\" + nt)\nprintln(\"ok=\" + okt)\nassertResult(\"1\")(nt)\nassertResult(\"true\")(okt)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime toString build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .arg("KLASSIC_NATIVE_TOSTRING_ENV")
        .env("KLASSIC_NATIVE_TOSTRING_ENV", "1")
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime toString run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "1\ntrue\nn=1\nok=true\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_line_cons() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let lines_path_holder =
        std::env::temp_dir().join(format!("klassic-native-lines-path-holder-{unique}.txt"));
    let rewritten_path_holder = std::env::temp_dir().join(format!(
        "klassic-native-rewritten-lines-path-holder-{unique}.txt"
    ));
    let prefix_path_holder =
        std::env::temp_dir().join(format!("klassic-native-prefix-path-holder-{unique}.txt"));
    let lines_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-cons-lines-{unique}.txt"));
    let rewritten_path = std::env::temp_dir().join(format!(
        "klassic-native-runtime-cons-rewritten-{unique}.txt"
    ));
    let prefix_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-cons-prefix-{unique}.txt"));
    let source_path = std::env::temp_dir().join(format!("klassic-native-runtime-cons-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-runtime-cons-{unique}"));
    let source = format!(
        r#"val linesPathHolder = "{}"
val rewrittenPathHolder = "{}"
val prefixPathHolder = "{}"
val linesPath = FileInput#all(linesPathHolder)
val rewrittenPath = FileInput#all(rewrittenPathHolder)
val prefixPath = FileInput#all(prefixPathHolder)
FileOutput#writeLines(linesPath, ["line-a", "line-b"])
val runtimeLines = FileInput#lines(linesPath)
val prefixedLines = cons("line-0")(runtimeLines)
println(prefixedLines)
println(join(prefixedLines, "|"))
println(size(prefixedLines))
println(head(prefixedLines))
println(tail(prefixedLines))
val shoutedLines = map(runtimeLines)((line) => line + "!")
val upperLines = runtimeLines.map((line) => toUpperCase(line))
val foldedLines = foldLeft(runtimeLines)("")((acc, line) => acc + "[" + line + "]")
val totalChars = foldLeft(runtimeLines)(0)((acc, line) => acc + length(line))
val longLines = foldLeft(runtimeLines)(0)((acc, line) => if(length(line) > 5) acc + 1 else acc)
val hasLineB = foldLeft(runtimeLines)(false)((acc, line) => acc || line.contains("line-b"))
val allLinePrefixed = foldLeft(runtimeLines)(true)((acc, line) => acc && startsWith(line, "line-"))
val left = "<"
val right = ">"
val decorate = (line) => left + line + right
val reduceJoin = (acc, line) => acc + left + line + right
val decoratedLines = map(runtimeLines)(decorate)
val foldedViaAlias = foldLeft(runtimeLines)("")(reduceJoin)
val runtimeLeft = FileInput#all(prefixPath)
val decorateRuntime = (line) => runtimeLeft + line + right
val runtimeDecoratedLines = runtimeLines.map(decorateRuntime)
val runtimeFoldedViaAlias = foldLeft(runtimeLines)("")((acc, line) => acc + runtimeLeft + line + right)
println(shoutedLines)
println(join(shoutedLines, "|"))
println(upperLines)
println(foldedLines)
println(totalChars)
println(longLines)
println(hasLineB)
println(allLinePrefixed)
println(decoratedLines)
println(foldedViaAlias)
println(runtimeDecoratedLines)
println(runtimeFoldedViaAlias)
assertResult(["line-0", "line-a", "line-b"])(prefixedLines)
assertResult(["line-a!", "line-b!"])(shoutedLines)
assertResult(["LINE-A", "LINE-B"])(upperLines)
assertResult("[line-a][line-b]")(foldedLines)
assertResult(12)(totalChars)
assertResult(2)(longLines)
assert(hasLineB)
assert(allLinePrefixed)
assertResult(["<line-a>", "<line-b>"])(decoratedLines)
assertResult("<line-a><line-b>")(foldedViaAlias)
assertResult(["<line-a>", "<line-b>"])(runtimeDecoratedLines)
assertResult("<line-a><line-b>")(runtimeFoldedViaAlias)
assert(prefixedLines == ["line-0", "line-a", "line-b"])
FileOutput#writeLines(rewrittenPath, prefixedLines)
println(FileInput#all(rewrittenPath))
assertResult(prefixedLines)(FileInput#lines(rewrittenPath))
FileOutput#delete(linesPath)
FileOutput#delete(rewrittenPath)
"#,
        lines_path_holder.display(),
        rewritten_path_holder.display(),
        prefix_path_holder.display()
    );
    fs::write(&source_path, source).expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime line cons build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&lines_path_holder, lines_path.to_string_lossy().as_bytes())
        .expect("lines path holder should write after native build");
    fs::write(
        &rewritten_path_holder,
        rewritten_path.to_string_lossy().as_bytes(),
    )
    .expect("rewritten path holder should write after native build");
    fs::write(
        &prefix_path_holder,
        prefix_path.to_string_lossy().as_bytes(),
    )
    .expect("prefix path holder should write after native build");
    fs::write(&prefix_path, b"<").expect("prefix file should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&lines_path_holder);
    let _ = fs::remove_file(&rewritten_path_holder);
    let _ = fs::remove_file(&prefix_path_holder);
    let _ = fs::remove_file(&lines_path);
    let _ = fs::remove_file(&rewritten_path);
    let _ = fs::remove_file(&prefix_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime line cons run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[line-0, line-a, line-b]\nline-0|line-a|line-b\n3\nline-0\n[line-a, line-b]\n[line-a!, line-b!]\nline-a!|line-b!\n[LINE-A, LINE-B]\n[line-a][line-b]\n12\n2\ntrue\ntrue\n[<line-a>, <line-b>]\n<line-a><line-b>\n[<line-a>, <line-b>]\n<line-a><line-b>\nline-0\nline-a\nline-b\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_line_map_builtin_values() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let input_path_holder = std::env::temp_dir().join(format!(
        "klassic-native-line-map-builtin-holder-{unique}.txt"
    ));
    let input_path =
        std::env::temp_dir().join(format!("klassic-native-line-map-builtin-{unique}.txt"));
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-line-map-builtin-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-line-map-builtin-{unique}"));
    fs::write(
        &source_path,
        format!(
            r#"val path = FileInput#all("{}")
val lines = FileInput#lines(path)
val trimLine = trim
val pickedUpper = {{
  println("pick mapper")
  toUpperCase
}}
val trimmed = lines.map(trimLine)
val directUpper = map(lines)(toUpperCase)
val pickedUpperLines = lines.map(pickedUpper)
println(trimmed)
println(directUpper)
println(pickedUpperLines)
assertResult(["alpha", "beta"])(trimmed)
assertResult(["  ALPHA  ", "BETA"])(directUpper)
assertResult(["  ALPHA  ", "BETA"])(pickedUpperLines)
"#,
            input_path_holder.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime line map builtin value build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path_holder, input_path.to_string_lossy().as_bytes())
        .expect("input path holder should write after native build");
    fs::write(&input_path, "  alpha  \nbeta").expect("input should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&input_path_holder);
    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime line map builtin value run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "pick mapper\n[alpha, beta]\n[  ALPHA  , BETA]\n[  ALPHA  , BETA]\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_method_style_fold_left() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let input_path_holder =
        std::env::temp_dir().join(format!("klassic-native-method-fold-holder-{unique}.txt"));
    let input_path = std::env::temp_dir().join(format!("klassic-native-method-fold-{unique}.txt"));
    let source_path = std::env::temp_dir().join(format!("klassic-native-method-fold-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-method-fold-{unique}"));
    fs::write(
        &source_path,
        format!(
            r#"val path = FileInput#all("{}")
val lines = FileInput#lines(path)
val staticSum = [1, 2, 3].foldLeft(0, (acc, item) => acc + item)
val staticText = ["a", "b"].foldLeft("", (acc, item) => acc + item)
val folded = lines.foldLeft("", (acc, line) => acc + "<" + line + ">")
val totalChars = lines.foldLeft(0, (acc, line) => acc + length(line))
val allLinePrefixed = lines.foldLeft(true, (acc, line) => acc && startsWith(line, "line-"))
println(staticSum)
println(staticText)
println(folded)
println(totalChars)
println(allLinePrefixed)
assertResult(6)(staticSum)
assertResult("ab")(staticText)
assertResult("<line-a><line-b>")(folded)
assertResult(12)(totalChars)
assert(allLinePrefixed)
"#,
            input_path_holder.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "method-style foldLeft build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path_holder, input_path.to_string_lossy().as_bytes())
        .expect("input path holder should write after native build");
    fs::write(&input_path, "line-a\nline-b").expect("input should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&input_path_holder);
    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "method-style foldLeft run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "6\nab\n<line-a><line-b>\n12\ntrue\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_line_to_string() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-line-to-string-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-line-to-string-{unique}"));
    fs::write(
        &source_path,
        "val probe = head(args())\nval lines = tail(args())\nval trailing = (probe + \",\").split(\",\")\nval empty = split(\"\", \",\")\nprintln(toString(lines))\nprintln(\"lines=\" + lines)\nprintln(toString(trailing))\nprintln(\"empty=\" + empty)\nprintln(lines.contains(\"beta\"))\nprintln(contains(lines)(\"gamma\"))\nprintln(lines.contains(probe))\nprintln(lines.contains(length(probe)))\nassertResult(\"[beta, gamma]\")(toString(lines))\nassertResult(\"lines=[beta, gamma]\")(\"lines=\" + lines)\nassertResult(\"[alpha, ]\")(toString(trailing))\nassertResult(\"empty=[]\")(\"empty=\" + empty)\nassert(lines.contains(\"beta\"))\nassert(contains(lines)(\"gamma\"))\nassert(!lines.contains(probe))\nassert(!lines.contains(length(probe)))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime line toString build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .arg("alpha")
        .arg("beta")
        .arg("gamma")
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime line toString run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[beta, gamma]\nlines=[beta, gamma]\n[alpha, ]\nempty=[]\ntrue\ntrue\nfalse\nfalse\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_line_csv_processing() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let csv_path_holder =
        std::env::temp_dir().join(format!("klassic-native-csv-path-holder-{unique}.txt"));
    let multi_path_holder =
        std::env::temp_dir().join(format!("klassic-native-multi-path-holder-{unique}.txt"));
    let delimiter_path_holder =
        std::env::temp_dir().join(format!("klassic-native-delimiter-path-holder-{unique}.txt"));
    let chars_path_holder =
        std::env::temp_dir().join(format!("klassic-native-chars-path-holder-{unique}.txt"));
    let empty_path_holder =
        std::env::temp_dir().join(format!("klassic-native-empty-path-holder-{unique}.txt"));
    let csv_path = std::env::temp_dir().join(format!("klassic-native-runtime-csv-{unique}.csv"));
    let multi_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-multi-{unique}.txt"));
    let delimiter_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-delimiter-{unique}.txt"));
    let chars_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-chars-{unique}.txt"));
    let empty_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-empty-{unique}.txt"));
    let source_path = std::env::temp_dir().join(format!("klassic-native-runtime-csv-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-runtime-csv-{unique}"));
    let source = format!(
        r#"val csvPathHolder = "{}"
val multiPathHolder = "{}"
val delimiterPathHolder = "{}"
val charsPathHolder = "{}"
val emptyPathHolder = "{}"
val csvPath = FileInput#all(csvPathHolder)
val multiPath = FileInput#all(multiPathHolder)
val delimiterPath = FileInput#all(delimiterPathHolder)
val charsPath = FileInput#all(charsPathHolder)
val emptyPath = FileInput#all(emptyPathHolder)
val lines = FileInput#lines(csvPath)
val data = tail(lines)
val formatRow = (row) => {{
  val fields = split(row, ",")
  val name = head(fields)
  val age = head(tail(fields))
  val city = head(tail(tail(fields)))
  name + ":" + age + "@" + city
}}
val results = map(data)(formatRow)
println(results)
println(join(results, "|"))
assertResult(["Alice:30@Tokyo", "Bob:25@Kyoto"])(results)
val words = split(FileInput#all(multiPath), "--")
println(words)
println(join(words, "|"))
println(size(words))
assertResult(["red", "green", "", "blue", ""])(words)
assertResult("red|green||blue|")(join(words, "|"))
assertResult(5)(size(words))
val wordTail = tail(words)
val decoratedWords = map(words)((word) => "<" + word + ">")
val foldedWords = foldLeft(words)("")((acc, word) => acc + "[" + word + "]")
println(wordTail)
println(decoratedWords)
println(foldedWords)
println(join(wordTail, ":"))
println(size(wordTail))
assertResult(["green", "", "blue", ""])(wordTail)
assertResult(["<red>", "<green>", "<>", "<blue>", "<>"])(decoratedWords)
assertResult("[red][green][][blue][]")(foldedWords)
assertResult("green::blue:")(join(wordTail, ":"))
assertResult(4)(size(wordTail))
val dynamicWords = split(FileInput#all(multiPath), FileInput#all(delimiterPath))
println(dynamicWords)
println(join(dynamicWords, "/"))
println(join(dynamicWords, FileInput#all(delimiterPath)))
println(join(dynamicWords, FileInput#all(emptyPath)))
assertResult(["red", "green", "", "blue", ""])(dynamicWords)
assertResult("red/green//blue/")(join(dynamicWords, "/"))
assertResult("red--green----blue--")(join(dynamicWords, FileInput#all(delimiterPath)))
assertResult("redgreenblue")(join(dynamicWords, FileInput#all(emptyPath)))
val chars = split(FileInput#all(charsPath), "")
println(chars)
assertResult(["h", "é", "!"])(chars)
val dynamicChars = split(FileInput#all(charsPath), FileInput#all(emptyPath))
println(dynamicChars)
assertResult(["h", "é", "!"])(dynamicChars)
val emptySplit = split(FileInput#all(emptyPath), "--")
val emptyChars = split(FileInput#all(emptyPath), "")
println(size(emptySplit))
println(size(emptyChars))
assertResult([""])(emptySplit)
assertResult(1)(size(emptySplit))
assertResult("")(head(emptySplit))
assertResult(0)(size(emptyChars))
"#,
        csv_path_holder.display(),
        multi_path_holder.display(),
        delimiter_path_holder.display(),
        chars_path_holder.display(),
        empty_path_holder.display()
    );
    fs::write(&source_path, source).expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime CSV build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&csv_path_holder, csv_path.to_string_lossy().as_bytes())
        .expect("csv path holder should write after native build");
    fs::write(&multi_path_holder, multi_path.to_string_lossy().as_bytes())
        .expect("multi path holder should write after native build");
    fs::write(
        &delimiter_path_holder,
        delimiter_path.to_string_lossy().as_bytes(),
    )
    .expect("delimiter path holder should write after native build");
    fs::write(&chars_path_holder, chars_path.to_string_lossy().as_bytes())
        .expect("chars path holder should write after native build");
    fs::write(&empty_path_holder, empty_path.to_string_lossy().as_bytes())
        .expect("empty path holder should write after native build");
    fs::write(&csv_path, b"name,age,city\nAlice,30,Tokyo\nBob,25,Kyoto")
        .expect("csv file should write after native build");
    fs::write(&multi_path, b"red--green----blue--")
        .expect("multi delimiter file should write after native build");
    fs::write(&delimiter_path, b"--").expect("delimiter file should write after native build");
    fs::write(&chars_path, "hé!".as_bytes()).expect("chars file should write after native build");
    fs::write(&empty_path, b"").expect("empty file should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&csv_path_holder);
    let _ = fs::remove_file(&multi_path_holder);
    let _ = fs::remove_file(&delimiter_path_holder);
    let _ = fs::remove_file(&chars_path_holder);
    let _ = fs::remove_file(&empty_path_holder);
    let _ = fs::remove_file(&csv_path);
    let _ = fs::remove_file(&multi_path);
    let _ = fs::remove_file(&delimiter_path);
    let _ = fs::remove_file(&chars_path);
    let _ = fs::remove_file(&empty_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime CSV run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[Alice:30@Tokyo, Bob:25@Kyoto]\nAlice:30@Tokyo|Bob:25@Kyoto\n[red, green, , blue, ]\nred|green||blue|\n5\n[green, , blue, ]\n[<red>, <green>, <>, <blue>, <>]\n[red][green][][blue][]\ngreen::blue:\n4\n[red, green, , blue, ]\nred/green//blue/\nred--green----blue--\nredgreenblue\n[h, é, !]\n[h, é, !]\n1\n0\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_file_input_errors() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let missing_path =
        std::env::temp_dir().join(format!("klassic-native-missing-input-{unique}.txt"));
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-input-error-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-input-error-{unique}"));
    fs::write(
        &source_path,
        format!("println(FileInput#all(\"{}\"))\n", missing_path.display()),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime file input error build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(!run.status.success());
    assert!(run.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&run.stderr),
        format!(
            "{}:1:1: FileInput#all failed to open file\n",
            source_path.display()
        )
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_file_input_binding_overflow_error() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let input_path = std::env::temp_dir().join(format!(
        "klassic-native-runtime-input-overflow-{unique}.txt"
    ));
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-input-overflow-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-input-overflow-{unique}"));
    fs::write(
        &source_path,
        format!(
            "val text = FileInput#all(\"{}\")\nprintln(text)\n",
            input_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime file input overflow build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    fs::write(&input_path, "x".repeat(65_537)).expect("large input should write after build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(!run.status.success());
    assert!(run.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&run.stderr)
            .contains("FileInput#all runtime string exceeds 65536 bytes")
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_cleanup_return_value_preservation() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-cleanup-value-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-cleanup-value-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nval x = { hits += 1; 10 } cleanup { hits += 10 }\nval ok = { hits += 1; true } cleanup { hits += 10 }\nprintln(x)\nprintln(ok)\nprintln(hits)\nassertResult(10)(x)\nassert(ok)\nassertResult(22)(hits)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "10\ntrue\n22\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_file_input_open_preserves_side_effecting_callback() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-file-input-effect-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-file-input-effect-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nval text = \"src/test/resources/hello.txt\" FileInput#open {(stream) =>\n  FileInput#readAll(stream) cleanup { hits += 1 }\n}\nprintln(hits)\nprintln(text)\nassertResult(1)(hits)\nassertResult(\"Hello, World!\")(text)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "1\nHello, World!\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_dir_helpers() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let base = std::env::temp_dir().join(format!("klassic-native-dir-{unique}-work"));
    let nested = base.join("nested");
    let file = nested.join("a.txt");
    let copied = nested.join("b.txt");
    let moved = nested.join("c.txt");
    let source_path = std::env::temp_dir().join(format!("klassic-native-dir-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-dir-{unique}-bin"));
    fs::write(
        &source_path,
        format!(
            "val base = \"{}\"\nval nested = \"{}\"\nval file = \"{}\"\nval copied = \"{}\"\nval moved = \"{}\"\nprintln(Dir#exists(base))\nDir#mkdir(base)\nprintln(Dir#isDirectory(base))\nDir#mkdirs(nested)\nFileOutput#write(file, \"hello\")\nprintln(Dir#isFile(file))\nprintln(Dir#list(nested))\nprintln(Dir#listFull(nested))\nDir#copy(file, copied)\nprintln(FileInput#all(copied))\nDir#move(copied, moved)\nprintln(Dir#isFile(moved))\nFileOutput#delete(file)\nFileOutput#delete(moved)\nDir#delete(nested)\nDir#delete(base)\nprintln(Dir#exists(base))\n",
            base.display(),
            nested.display(),
            file.display(),
            copied.display(),
            moved.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "dir helper build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);
    let _ = fs::remove_dir_all(&base);

    assert!(
        run.status.success(),
        "dir helper run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        format!(
            "false\ntrue\ntrue\n[a.txt]\n[{}]\nhello\ntrue\nfalse\n",
            file.display()
        )
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dir_builtin_function_values() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let base = std::env::temp_dir().join(format!("klassic-native-dir-values-{unique}-work"));
    let nested = base.join("nested");
    let file = nested.join("a.txt");
    let source_path = std::env::temp_dir().join(format!("klassic-native-dir-values-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-dir-values-{unique}-bin"));
    fs::write(
        &source_path,
        format!(
            "val base = \"{}\"\nval nested = \"{}\"\nval file = \"{}\"\n({{ println(\"pick mkdirs\"); Dir#mkdirs }})(nested)\n({{ println(\"pick write\"); FileOutput#write }})(file, \"hello\")\nprintln(({{ println(\"pick list\"); Dir#list }})(nested))\nprintln(({{ println(\"pick listFull\"); Dir#listFull }})(nested))\nprintln(({{ println(\"pick isDir\"); Dir#isDirectory }})(nested))\nprintln(({{ println(\"pick isFile\"); Dir#isFile }})(file))\n({{ println(\"pick delete file\"); FileOutput#delete }})(file)\n({{ println(\"pick delete nested\"); Dir#delete }})(nested)\n({{ println(\"pick delete base\"); Dir#delete }})(base)\nprintln(({{ println(\"pick exists\"); Dir#exists }})(base))\n",
            base.display(),
            nested.display(),
            file.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "dir builtin value build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);
    let _ = fs::remove_dir_all(&base);

    assert!(
        run.status.success(),
        "dir builtin value run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        format!(
            "pick mkdirs\npick write\npick list\n[a.txt]\npick listFull\n[{}]\npick isDir\ntrue\npick isFile\ntrue\npick delete file\npick delete nested\npick delete base\npick exists\nfalse\n",
            file.display()
        )
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_mutable_loop_then_static_ternary() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test-programs/ternary-expression.kl");
    let output_path = std::env::temp_dir().join(format!("klassic-native-ternary-{unique}"));

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert!(run.stdout.is_empty());
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_record_sample() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-programs/record.kl");
    let output_path = std::env::temp_dir().join(format!("klassic-native-record-sample-{unique}"));

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert!(run.stdout.is_empty());
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_thread_sample() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("test-programs/builtin_functions-thread.kl");
    let output_path = std::env::temp_dir().join(format!("klassic-native-thread-{unique}"));

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "Hello from main thread.\nHello from another thread.\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_thread_and_stopwatch_lambda_values() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-thread-lambda-value-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-thread-lambda-value-{unique}"));
    fs::write(
        &source_path,
        r#"mutable hits = 0
val job = () => {
  hits += 1
  println("job " + hits)
}
val pickedThread = {
  println("pick thread")
  () => {
    hits += 10
    println("picked " + hits)
  }
}
thread(job)
thread(pickedThread)
val measured = () => {
  hits += 100
  hits
}
val elapsed = stopwatch(measured)
val pickedElapsed = stopwatch({
  println("pick stopwatch")
  () => {
    hits += 1000
    hits
  }
})
println(elapsed >= 0)
println(pickedElapsed >= 0)
println(hits)
assert(elapsed >= 0)
assert(pickedElapsed >= 0)
assertResult(1100)(hits)
"#,
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "thread/stopwatch lambda value build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "thread/stopwatch lambda value run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "pick thread\npick stopwatch\ntrue\ntrue\n1100\njob 1101\npicked 1111\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_thread_block_local_mutable_capture() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-thread-capture-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-thread-capture-{unique}"));
    fs::write(
        &source_path,
        "println({\n  mutable x = 0\n  thread(() => {\n    x = x + 2\n    println(x)\n  })\n  thread(() => {\n    x = x + 3\n    println(x)\n  })\n  0\n})\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "0\n2\n5\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_thread_function_local_mutable_capture() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-thread-fn-capture-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-thread-fn-capture-{unique}"));
    fs::write(
        &source_path,
        "def enqueue() = {\n  mutable x = 10\n  thread(() => {\n    x = x + 1\n    println(x)\n  })\n  thread(() => {\n    x = x + 2\n    println(x)\n  })\n  0\n}\nprintln(enqueue())\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "0\n11\n13\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_thread_alias_inside_function() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-thread-alias-fn-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-thread-alias-fn-{unique}"));
    fs::write(
        &source_path,
        "val spawn = thread\nval spawnLater = spawn\ndef enqueue() = {\n  mutable x = 10\n  spawnLater(() => {\n    x = x + 7\n    println(x)\n  })\n  0\n}\nprintln(enqueue())\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "0\n17\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_thread_alias_inside_lambda_value() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-thread-alias-lambda-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-thread-alias-lambda-{unique}"));
    fs::write(
        &source_path,
        "val spawn = thread\nval enqueue = () => {\n  mutable x = 3\n  spawn(() => {\n    x = x * 5\n    println(x)\n  })\n  0\n}\nprintln(enqueue())\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "0\n15\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_thread_foreach_iteration_capture() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-thread-foreach-capture-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-thread-foreach-capture-{unique}"));
    fs::write(
        &source_path,
        "foreach(i in [1, 2, 3]) {\n  thread(() => println(i))\n}\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "1\n2\n3\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_curried_fold_sample() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-programs/functions.kl");
    let output_path = std::env::temp_dir().join(format!("klassic-native-functions-{unique}"));

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert!(run.stdout.is_empty());
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_int_lists() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-list-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-list-{unique}"));
    fs::write(
        &source_path,
        "println([1, 2, -3])\nprintln(\"size = \" + size([1 2 3]))\nprintln(\"head = \" + head([42, 100]))\nprintln(\"tail = \" + tail([1, 2, 3]))\nprintln(\"cons = \" + cons(0)([1, 2]))\nprintln(\"words = \" + cons(\"a\")([\"b\", \"c\"]))\nprintln(\"infix = \" + (9 #cons [10]))\nprintln(\"reverse = \" + foldLeft([1, 2, 3])([])((acc, e) => e #cons acc))\nprintln(\"map = \" + map([1, 2, 3])((x) => x * 2 + 1))\nprintln(\"sum = \" + foldLeft([1, 2, 3])(0)((r, e) => r + e))\nprintln(\"empty? \" + isEmpty([1]))\nassertResult([2, 3])(tail([1, 2, 3]))\nassertResult([\"a\", \"b\", \"c\"])(cons(\"a\")([\"b\", \"c\"]))\nassertResult([9, 10])(9 #cons [10])\nassertResult([3, 2, 1])(foldLeft([1, 2, 3])([])((acc, e) => e #cons acc))\nassertResult([3, 5, 7])(map([1, 2, 3])((x) => x * 2 + 1))\nassertResult(6)(foldLeft([1, 2, 3])(0)((r, e) => r + e))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[1, 2, -3]\nsize = 3\nhead = 42\ntail = [2, 3]\ncons = [0, 1, 2]\nwords = [a, b, c]\ninfix = [9, 10]\nreverse = [3, 2, 1]\nmap = [3, 5, 7]\nsum = 6\nempty? false\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_cons_argument_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-cons-side-effects-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-cons-side-effects-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nval words = cons({ hits += 1; \"a\" })({ hits += 1; [\"b\"] })\nval nums = ({ hits += 1; 0 }) #cons ({ hits += 1; [1] })\nprintln(hits)\nprintln(words)\nprintln(nums)\nassertResult(4)(hits)\nassertResult([\"a\", \"b\"])(words)\nassertResult([0, 1])(nums)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "4\n[a, b]\n[0, 1]\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_list_and_record_access_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-access-side-effects-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-access-side-effects-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nval size1 = size({ hits += 1; [1, 2] })\nval head1 = head({ hits += 1; [9, 10] })\nval tail1 = tail({ hits += 1; [9, 10] })\nval empty1 = isEmpty({ hits += 1; [] })\nval methodSize = ({ hits += 1; [3, 4] }).size()\nval methodHead = ({ hits += 1; [5, 6] }).head()\nval methodTail = ({ hits += 1; [7, 8] }).tail()\nval field = ({ hits += 1; record { x: 11 } }).x\nprintln(hits)\nprintln(size1)\nprintln(head1)\nprintln(tail1)\nprintln(empty1)\nprintln(methodSize)\nprintln(methodHead)\nprintln(methodTail)\nprintln(field)\nassertResult(8)(hits)\nassertResult(2)(size1)\nassertResult(9)(head1)\nassertResult([10])(tail1)\nassertResult(true)(empty1)\nassertResult(2)(methodSize)\nassertResult(5)(methodHead)\nassertResult([8])(methodTail)\nassertResult(11)(field)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "8\n2\n9\n[10]\ntrue\n2\n5\n[8]\n11\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_method_map_and_foreach_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-method-map-side-effects-{unique}.kl"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-method-map-side-effects-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nforeach(x in { hits += 1; [1, 2] }) {\n  hits += x\n  val pair = [x, x + 1]\n  val row = record { value: x, next: x + 1 }\n  println(pair)\n  assertResult([x, x + 1])(pair)\n  assertResult(x + 1)(row.next)\n}\nval ys = ({ hits += 1; [1, 2] }).map((x) => { hits += 1; x + 1 })\nprintln(hits)\nprintln(ys)\nassertResult(7)(hits)\nassertResult([2, 3])(ys)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[1, 2]\n[2, 3]\n7\n[2, 3]\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_map_and_fold_lambda_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-map-fold-side-effects-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-map-fold-side-effects-{unique}"));
    fs::write(
        &source_path,
        "def makeAdder(n: Int) = (x: Int) => x + n\nval add2 = makeAdder(2)\nmutable hits = 0\nval ys = map([1, 2, 3])((x) => { hits += 1; x + 1 })\nval zs = map([1, 2])((x) => add2(x))\nval total = foldLeft([1, 2, 3])({ hits += 1; 0 })((acc, e) => { hits += 1; acc + e })\nprintln(hits)\nprintln(ys)\nprintln(zs)\nprintln(total)\nassertResult(7)(hits)\nassertResult([2, 3, 4])(ys)\nassertResult([3, 4])(zs)\nassertResult(6)(total)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "7\n[2, 3, 4]\n[3, 4]\n6\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_string_and_list_bindings() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-static-bind-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-static-bind-{unique}"));
    fs::write(
        &source_path,
        "val greeting = \"hello\"\nval xs = [5, 8, 13]\nval ys = cons(3)(tail(xs))\nval doubled = map(xs)((x) => x * 2)\nval sum = foldLeft(xs)(0)((r, e) => r + e)\nprintln(greeting)\nprintln(\"greeting = \" + greeting)\nprintln(xs)\nprintln(\"size = \" + size(xs))\nprintln(\"head = \" + head(xs))\nprintln(\"tail = \" + tail(xs))\nprintln(\"tail size = \" + size(tail(xs)))\nprintln(\"ys = \" + ys)\nprintln(\"doubled = \" + doubled)\nprintln(\"sum = \" + sum)\nassertResult(\"hello\")(greeting)\nassertResult([8, 13])(tail(xs))\nassertResult([3, 8, 13])(ys)\nassertResult([10, 16, 26])(doubled)\nassertResult(26)(sum)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "hello\ngreeting = hello\n[5, 8, 13]\nsize = 3\nhead = 5\ntail = [8, 13]\ntail size = 2\nys = [3, 8, 13]\ndoubled = [10, 16, 26]\nsum = 26\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_straight_line_mutable_static_values() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-static-mutable-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-static-mutable-{unique}"));
    fs::write(
        &source_path,
        "mutable s = \"FOO\"\ns = s + s\nmutable xs = [\"a\"]\nxs = cons(\"b\")(xs)\nprintln(s)\nprintln(xs)\nassertResult(\"FOOFOO\")(s)\nassertResult([\"b\", \"a\"])(xs)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "FOOFOO\n[b, a]\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_annotated_string_lambda_parameters() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-string-lambda-{unique}.kl"));
    let input_path =
        std::env::temp_dir().join(format!("klassic-native-string-lambda-{unique}.txt"));
    let lines_path =
        std::env::temp_dir().join(format!("klassic-native-string-lambda-lines-{unique}.txt"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-string-lambda-{unique}"));
    fs::write(
        &source_path,
        format!(
            r#"val textLength = (s: String) => length(s)
val lineCount = (lines: List<String>) => lines.size()
val prefix = "p:"
val baseLines = ["root"]
def prefixedLength(s: String): Int = length(prefix + s)
def totalLines(lines: List<String>): Int = lineCount(lines) + baseLines.size()
val text = FileInput#all("{}")
val lines = FileInput#lines("{}")
println(textLength(text))
println(textLength("abc"))
println(lineCount(lines))
println(lineCount(["x", "y"]))
println(prefixedLength(text))
println(totalLines(lines))
assertResult(7)(textLength(text))
assertResult(3)(textLength("abc"))
assertResult(3)(lineCount(lines))
assertResult(2)(lineCount(["x", "y"]))
assertResult(9)(prefixedLength(text))
assertResult(4)(totalLines(lines))
"#,
            input_path.display(),
            lines_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "annotated string lambda build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    fs::write(&input_path, "dynamic").expect("input source should write after native build");
    fs::write(&lines_path, "a\nb\nc").expect("lines source should write after native build");
    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&lines_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "annotated string lambda run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "7\n3\n3\n2\n9\n4\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_placeholder_callable_aliases() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-placeholder-alias-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-placeholder-alias-{unique}"));
    fs::write(
        &source_path,
        "val id = _\nval add = _ + _\ndef inc(x) = x + 1\ndef sum(acc, e) = acc + e\nprintln(map([1])(id))\nprintln(map([1, 2, 3])(inc))\nprintln(foldLeft([1 2 3])(0)(add))\nprintln(foldLeft([1 2 3])(0)(sum))\nassertResult([1])(map([1])(id))\nassertResult([2, 3, 4])(map([1, 2, 3])(inc))\nassertResult(6)(foldLeft([1 2 3])(0)(add))\nassertResult(6)(foldLeft([1 2 3])(0)(sum))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[1]\n[2, 3, 4]\n6\n6\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_lambda_return_values() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-lambda-return-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-lambda-return-{unique}"));
    fs::write(
        &source_path,
        "def f(x) = _\ndef makeAdder(n: Int) = (x: Int) => x + n\nval make = (x) => _\nval add2 = makeAdder(2)\nmutable hits = 0\nprintln(((x) => x + 1)(2))\nprintln(map([1])(f(1)))\nprintln(map([1])(make(1)))\nprintln(add2({ hits += 1; 3 }))\nassertResult(3)(((x) => x + 1)(2))\nassertResult([1])(map([1])(f(1)))\nassertResult([1])(map([1])(make(1)))\nassertResult(5)(add2({ hits += 1; 3 }))\nassertResult(2)(hits)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3\n[1]\n[1]\n5\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_inline_lambda_runtime_arguments() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-lambda-runtime-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-lambda-runtime-{unique}"));
    fs::write(
        &source_path,
        "val elapsed = stopwatch( => 1)\nassert(((x) => x + 1)(elapsed) >= 1)\nassert(((x) => x >= 0)(elapsed))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert!(run.stdout.is_empty());
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_inline_lambda_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-lambda-side-effect-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-lambda-side-effect-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nval result = ((x) => {\n  hits += 1\n  x\n})(1)\nval elapsed = stopwatch( => 1)\nassert(hits + elapsed >= elapsed + 1)\nassertResult(1)(result)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert!(run.stdout.is_empty());
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_record_lambda_method_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-record-method-effect-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-record-method-effect-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nval r = record { call: (x) => {\n  hits += 1\n  x\n}}\nval result = r.call(1)\nval result2 = ({ hits += 1; record { call: (x) => { hits += 1; x + 1 } } }).call({ hits += 1; 1 })\nprintln(hits)\nprintln(result2)\nassertResult(1)(result)\nassertResult(4)(hits)\nassertResult(2)(result2)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "4\n2\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_if_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-static-if-effect-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-static-if-effect-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nif(true) {\n  hits += 1\n}\nval xs = if(true) { hits += 1; [1, 2] } else { hits += 10; [9] }\nval s = if(false) { hits += 10; \"bad\" } else { hits += 1; \"ok\" }\nval n = if({ hits += 1; true }) { hits += 1; 7 } else { hits += 10; 0 }\nprintln(hits)\nprintln(xs)\nprintln(s)\nprintln(n)\nassertResult(5)(hits)\nassertResult([1, 2])(xs)\nassertResult(\"ok\")(s)\nassertResult(7)(n)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "5\n[1, 2]\nok\n7\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dynamic_branch_assignment_state() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-branch-state-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-branch-state-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nval flag = stopwatch( => 1) >= 0\nif(flag) {\n  hits = 1\n} else {\n  hits = 2\n}\nprintln(hits)\nassertResult(1)(hits)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "1\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dynamic_if_static_value_merges() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-dynamic-if-static-merge-{unique}.kl"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-if-static-merge-{unique}"));
    fs::write(
        &source_path,
        "val flag = stopwatch( => 1) >= 0\nval label = if(flag) {\n  println(\"then\")\n  \"same\"\n} else {\n  println(\"else\")\n  \"same\"\n}\nprintln(label)\nmutable alias = \"old\"\nif(flag) {\n  alias = \"merged\"\n} else {\n  alias = \"merged\"\n}\nprintln(alias)\nmutable xs = [0]\nif(flag) {\n  xs = [1, 2]\n} else {\n  xs = [1, 2]\n}\nprintln(xs)\nassertResult(\"same\")(label)\nassertResult(\"merged\")(alias)\nassertResult([1, 2])(xs)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "then\nsame\nmerged\n[1, 2]\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dynamic_if_string_branch_results() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-dynamic-if-string-result-{unique}.kl"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-if-string-result-{unique}"));
    fs::write(
        &source_path,
        "val flag = head(args()) == \"then\"\nval label = if(flag) {\n  println(\"then branch\")\n  \"alpha\"\n} else {\n  println(\"else branch\")\n  \"beta\"\n}\nprintln(label)\nprintln(\"tag=\" + label)\nif(flag) {\n  assertResult(\"alpha\")(label)\n} else {\n  assertResult(\"beta\")(label)\n}\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "dynamic if string result build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let then_run = Command::new(&output_path)
        .arg("then")
        .output()
        .expect("generated executable should run then branch");
    let else_run = Command::new(&output_path)
        .arg("else")
        .output()
        .expect("generated executable should run else branch");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        then_run.status.success(),
        "dynamic if string then run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&then_run.stdout),
        String::from_utf8_lossy(&then_run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&then_run.stdout),
        "then branch\nalpha\ntag=alpha\n"
    );
    assert!(then_run.stderr.is_empty());

    assert!(
        else_run.status.success(),
        "dynamic if string else run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&else_run.stdout),
        String::from_utf8_lossy(&else_run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&else_run.stdout),
        "else branch\nbeta\ntag=beta\n"
    );
    assert!(else_run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dynamic_if_runtime_line_branch_results() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-dynamic-if-runtime-lines-{unique}.kl"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-if-runtime-lines-{unique}"));
    fs::write(
        &source_path,
        "val chooseArgs = head(args()) == \"args\"\nval lines = if(chooseArgs) {\n  tail(args())\n} else {\n  (toString(size(args())) + \"\\nblue\").split(\"\\n\")\n}\nprintln(size(lines))\nprintln(lines.head())\nprintln(lines.join(\"|\"))\nif(chooseArgs) {\n  assertResult(\"first\")(lines.head())\n  assertResult(\"first|second\")(lines.join(\"|\"))\n} else {\n  assertResult(\"1\")(lines.head())\n  assertResult(\"1|blue\")(lines.join(\"|\"))\n}\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "dynamic if runtime line result build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let args_run = Command::new(&output_path)
        .arg("args")
        .arg("first")
        .arg("second")
        .output()
        .expect("generated executable should run args branch");
    let split_run = Command::new(&output_path)
        .arg("split")
        .output()
        .expect("generated executable should run split branch");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        args_run.status.success(),
        "dynamic if runtime line args run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&args_run.stdout),
        String::from_utf8_lossy(&args_run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&args_run.stdout),
        "2\nfirst\nfirst|second\n"
    );
    assert!(args_run.stderr.is_empty());

    assert!(
        split_run.status.success(),
        "dynamic if runtime line split run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&split_run.stdout),
        String::from_utf8_lossy(&split_run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&split_run.stdout), "2\n1\n1|blue\n");
    assert!(split_run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dynamic_if_function_value_merges() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-dynamic-if-function-merge-{unique}.kl"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-if-function-merge-{unique}"));
    fs::write(
        &source_path,
        "val flag = stopwatch(() => 1) >= 0\nval f = if(flag) {\n  (x) => x + 1\n} else {\n  (x) => x + 1\n}\nprintln(f(4))\nval sub = if(flag) {\n  println(\"pick then\")\n  substring\n} else {\n  println(\"pick else\")\n  substring\n}\nprintln(sub(\"abcdef\", 1, 4))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "5\npick then\nbcd\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dynamic_if_branch_local_closure_capture() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-dynamic-if-closure-capture-{unique}.kl"
    ));
    let output_path = std::env::temp_dir().join(format!(
        "klassic-native-dynamic-if-closure-capture-{unique}"
    ));
    fs::write(
        &source_path,
        "val flag = stopwatch(() => 1) >= 0\nval f = if(flag) {\n  mutable x = 0\n  (y) => {\n    x = x + y\n    x\n  }\n} else {\n  mutable x = 0\n  (y) => {\n    x = x + y\n    x\n  }\n}\nprintln(f(2))\nprintln(f(3))\nassertResult(5)(f(0))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "2\n5\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dynamic_if_branch_local_thread_capture() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-dynamic-if-thread-capture-{unique}.kl"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-if-thread-capture-{unique}"));
    fs::write(
        &source_path,
        "val flag = stopwatch(() => 1) >= 0\nif(flag) {\n  mutable x = 1\n  thread(() => {\n    x = x + 1\n    println(x)\n  })\n} else {\n  mutable x = 1\n  thread(() => {\n    x = x + 1\n    println(x)\n  })\n}\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "2\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dynamic_if_branch_local_record_closures() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-dynamic-if-record-closures-{unique}.kl"
    ));
    let output_path = std::env::temp_dir().join(format!(
        "klassic-native-dynamic-if-record-closures-{unique}"
    ));
    fs::write(
        &source_path,
        "val flag = stopwatch(() => 1) >= 0\nval pair = if(flag) {\n  mutable x = 0\n  val inc = (y) => {\n    x = x + y\n    x\n  }\n  val get = () => x\n  record { inc: inc, get: get }\n} else {\n  mutable x = 0\n  val inc = (y) => {\n    x = x + y\n    x\n  }\n  val get = () => x\n  record { inc: inc, get: get }\n}\nprintln(pair.inc(2))\nprintln(pair.get())\nprintln(pair.inc(3))\nprintln(pair.get())\nassertResult(5)(pair.get())\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "2\n2\n5\n5\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dynamic_if_virtual_file_state_merges() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-file-merge-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-file-merge-{unique}"));
    let file_path = std::env::temp_dir().join(format!("klassic-native-merged-{unique}.txt"));
    let dir_path = std::env::temp_dir().join(format!("klassic-native-merged-dir-{unique}"));
    let dir_file_path = dir_path.join("merged.txt");
    fs::write(
        &source_path,
        format!(
            "val flag = stopwatch( => 1) < 0\nif(flag) {{\n  FileOutput#write(\"{}\", \"same\")\n}} else {{\n  FileOutput#write(\"{}\", \"same\")\n}}\nprintln(FileInput#all(\"{}\"))\nDir#mkdir(\"{}\")\nif(flag) {{\n  FileOutput#write(\"{}\", \"same\")\n}} else {{\n  FileOutput#write(\"{}\", \"same\")\n}}\nval entries = Dir#list(\"{}\")\nprintln(entries)\nassertResult([\"merged.txt\"])(entries)\nFileOutput#delete(\"{}\")\nDir#delete(\"{}\")\nFileOutput#delete(\"{}\")\n",
            file_path.display(),
            file_path.display(),
            file_path.display(),
            dir_path.display(),
            dir_file_path.display(),
            dir_file_path.display(),
            dir_path.display(),
            dir_file_path.display(),
            dir_path.display(),
            file_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);
    let _ = fs::remove_file(&file_path);
    let _ = fs::remove_file(&dir_file_path);
    let _ = fs::remove_dir(&dir_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "same\n[merged.txt]\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dynamic_if_branch_local_list_closure() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-dynamic-if-list-closure-{unique}.kl"
    ));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-if-list-closure-{unique}"));
    fs::write(
        &source_path,
        "val flag = stopwatch(() => 1) >= 0\nval fs = if(flag) {\n  mutable x = 0\n  [(y) => {\n    x = x + y\n    x\n  }]\n} else {\n  mutable x = 0\n  [(y) => {\n    x = x + y\n    x\n  }]\n}\nval f = head(fs)\nprintln(f(2))\nprintln(f(3))\nassertResult(5)(f(0))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "2\n5\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dynamic_if_branch_local_map_closure() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-if-map-closure-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-if-map-closure-{unique}"));
    fs::write(
        &source_path,
        "val flag = stopwatch(() => 1) >= 0\nval table = if(flag) {\n  mutable x = 1\n  %[\"inc\": (y) => {\n    x = x + y\n    x\n  }]\n} else {\n  mutable x = 1\n  %[\"inc\": (y) => {\n    x = x + y\n    x\n  }]\n}\nval f = Map#get(table, \"inc\")\nprintln(f(2))\nprintln(f(3))\nassertResult(6)(f(0))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3\n6\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_build_uses_runtime_read_after_dynamic_if_virtual_file_state() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-file-leak-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-file-leak-{unique}"));
    let file_path = std::env::temp_dir().join(format!("klassic-native-leaked-{unique}.txt"));
    fs::write(
        &source_path,
        format!(
            "val flag = stopwatch( => 1) < 0\nif(flag) {{\n  FileOutput#write(\"{}\", \"then\")\n}} else {{\n  ()\n}}\nval text = FileInput#all(\"{}\")\nprintln(text)\n",
            file_path.display(),
            file_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "dynamic file runtime read build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);
    let _ = fs::remove_file(&file_path);

    assert!(!run.status.success());
    assert!(run.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("FileInput#all failed to open file"),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_build_uses_runtime_lines_after_dynamic_if_virtual_file_state() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-file-lines-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-file-lines-{unique}"));
    let file_path = std::env::temp_dir().join(format!("klassic-native-lines-{unique}.txt"));
    fs::write(
        &source_path,
        format!(
            "val flag = stopwatch( => 1) >= 0\nif(flag) {{\n  FileOutput#writeLines(\"{}\", [\"then\", \"branch\"])\n}} else {{\n  ()\n}}\nval lines = FileInput#lines(\"{}\")\nval readLines = FileInput#readLines(\"{}\")\nprintln(lines)\nprintln(join(readLines, \"|\"))\nassertResult([\"then\", \"branch\"])(lines)\nassertResult(lines)(readLines)\nFileOutput#delete(\"{}\")\n",
            file_path.display(),
            file_path.display(),
            file_path.display(),
            file_path.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "dynamic file runtime lines build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);
    let _ = fs::remove_file(&file_path);

    assert!(
        run.status.success(),
        "dynamic file runtime lines run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[then, branch]\nthen|branch\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_build_rejects_dynamic_if_divergent_thread_queues() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-thread-leak-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-thread-leak-{unique}"));
    fs::write(
        &source_path,
        "val flag = stopwatch(() => 1) < 0\nif(flag) {\n  thread(() => println(\"then\"))\n} else {\n  ()\n}\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(!build.status.success());
    assert!(build.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&build.stderr).contains("divergent dynamic branches"),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dynamic_while_assignment_state() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-while-state-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-dynamic-while-state-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nmutable once = 0\nwhile(stopwatch( => 1) >= 0 && once == 0) {\n  hits = 1\n  once = 1\n}\nprintln(hits)\nassertResult(1)(hits)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "1\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dynamic_while_condition_assignment_state() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!(
        "klassic-native-dynamic-while-condition-state-{unique}.kl"
    ));
    let output_path = std::env::temp_dir().join(format!(
        "klassic-native-dynamic-while-condition-state-{unique}"
    ));
    fs::write(
        &source_path,
        "mutable hits = 0\nmutable once = 0\nwhile(({ hits = 1; once == 0 }) && stopwatch( => 1) >= 0) {\n  once = 1\n}\nprintln(hits)\nassertResult(1)(hits)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "1\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_statically_skipped_while_body() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-skipped-while-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-skipped-while-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nwhile(false) {\n  mutable skipped = \"x\"\n  skipped = skipped + \"y\"\n  hits += 100\n}\nwhile({ hits += 1; false }) {\n  mutable skipped = [1]\n  skipped = [2]\n  hits += 100\n}\nprintln(hits)\nassertResult(1)(hits)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "skipped while build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "1\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_binary_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-binary-effect-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-binary-effect-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nval sum = {\n  hits += 1\n  1\n} + 1\nval same = {\n  hits += 1\n  2\n} == 2\nval mixedSame = { hits += 1; 1 } == { hits += 1; 1.0 }\nval mixedDifferent = { hits += 1; 1 } != { hits += 1; 2.0F }\nval text = { hits += 1; \"a\" } + { hits += 1; \"b\" }\nval more = { hits += 1; 1 } + { hits += 1; 2 }\nval doubleSum = { hits += 1; 1.5 } + { hits += 1; 2.5 }\nval floatProduct = { hits += 1; 3.0F } * { hits += 1; 2.0F }\nval doubleGreater = { hits += 1; 4.0 } > { hits += 1; 3.0 }\nval skippedAnd = false && { hits += 1; true }\nval skippedOr = true || { hits += 1; false }\nval runAnd = true && { hits += 1; true }\nval runOr = false || { hits += 1; true }\nprintln(hits)\nprintln(text)\nprintln(more)\nprintln(doubleSum)\nprintln(floatProduct)\nprintln(doubleGreater)\nprintln(mixedSame)\nprintln(mixedDifferent)\nassertResult(2)(sum)\nassertResult(true)(same)\nassertResult(true)(mixedSame)\nassertResult(true)(mixedDifferent)\nassertResult(\"ab\")(text)\nassertResult(3)(more)\nassertResult(4.0)(doubleSum)\nassertResult(6.0F)(floatProduct)\nassertResult(true)(doubleGreater)\nassertResult(false)(skippedAnd)\nassertResult(true)(skippedOr)\nassertResult(true)(runAnd)\nassertResult(true)(runOr)\nassertResult(18)(hits)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "18\nab\n3\n4.0\n6.0\ntrue\ntrue\ntrue\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_call_argument_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-call-argument-effect-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-call-argument-effect-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nval id = (x) => x\nval direct = ((x) => x)({\n  hits += 1\n  1\n})\nassertResult(1)({\n  hits += 1\n  1\n})\nval viaBinding = id({\n  hits += 1\n  1\n})\nprintln(hits)\nassertResult(1)(direct)\nassertResult(1)(viaBinding)\nassertResult(3)(hits)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dir_helper_argument_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let work_dir = std::env::temp_dir().join(format!("klassic-native-dir-effect-{unique}"));
    fs::create_dir(&work_dir).expect("temp work dir should be created");
    let source_path = work_dir.join("dir-effect.kl");
    let output_path = work_dir.join("dir-effect-bin");
    fs::write(
        &source_path,
        "mutable hits = 0\nDir#mkdir({ hits += 1; \"base\" })\nDir#mkdirs({ hits += 1; \"base/nested\" })\nFileOutput#write({ hits += 1; \"base/nested/a.txt\" }, { hits += 1; \"hello\" })\nval exists = Dir#exists({ hits += 1; \"base\" })\nval isDir = Dir#isDirectory({ hits += 1; \"base/nested\" })\nval isFile = Dir#isFile({ hits += 1; \"base/nested/a.txt\" })\nval listed = Dir#list({ hits += 1; \"base/nested\" })\nval listedFull = Dir#listFull({ hits += 1; \"base/nested\" })\nDir#copy({ hits += 1; \"base/nested/a.txt\" }, { hits += 1; \"base/nested/b.txt\" })\nDir#move({ hits += 1; \"base/nested/b.txt\" }, { hits += 1; \"base/nested/c.txt\" })\nFileOutput#delete({ hits += 1; \"base/nested/a.txt\" })\nFileOutput#delete({ hits += 1; \"base/nested/c.txt\" })\nDir#delete({ hits += 1; \"base/nested\" })\nDir#delete({ hits += 1; \"base\" })\nprintln(hits)\nassertResult(17)(hits)\nassert(exists)\nassert(isDir)\nassert(isFile)\nassertResult([\"a.txt\"])(listed)\nassert(size(listedFull) == 1)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .current_dir(&work_dir)
        .output()
        .expect("generated executable should run");

    let leftover_base = work_dir.join("base").exists();
    let _ = fs::remove_dir_all(&work_dir);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "17\n");
    assert!(run.stderr.is_empty());
    assert!(!leftover_base);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_records_and_field_access() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-record-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-record-{unique}"));
    fs::write(
        &source_path,
        "record Person {\n  name: String\n  age: Int\n  active: Boolean\n  scores: List<Int>\n}\nval p = #Person(\"Alice\", 30, true, [1, 2, 3])\nval point = record { x: 3, y: 4, label: \"P\" }\nprintln(p.name)\nprintln(\"age = \" + p.age)\nprintln(\"active = \" + p.active)\nprintln(\"scores = \" + p.scores)\nprintln(\"point = \" + point.label + \":\" + point.x)\nprintln(p)\nprintln(point)\nassertResult(\"Alice\")(p.name)\nassertResult(30)(p.age)\nassertResult(true)(p.active)\nassertResult([1, 2, 3])(p.scores)\nassertResult(record { x: 3, y: 4, label: \"P\" })(point)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "Alice\nage = 30\nactive = true\nscores = [1, 2, 3]\npoint = P:3\n#Person(Alice, 30, true, [1, 2, 3])\n#(3, 4, P)\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_literal_argument_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-literal-side-effects-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-literal-side-effects-{unique}"));
    fs::write(
        &source_path,
        "record User {\n  name: String\n  age: Int\n}\nmutable hits = 0\nval xs = [{ hits += 1; 1 }, { hits += 1; 2 }]\nval tags = %({ hits += 1; \"red\" }, { hits += 1; \"blue\" }, { hits += 1; \"red\" })\nval ages = %[{ hits += 1; \"Alice\" }: { hits += 1; 30 }]\nval rec = record { name: { hits += 1; \"Bob\" }, age: { hits += 1; 28 } }\nval user = #User({ hits += 1; \"Carol\" }, { hits += 1; 31 })\nprintln(hits)\nprintln(xs)\nprintln(tags)\nprintln(ages)\nprintln(rec.name)\nprintln(user.age)\nassertResult(11)(hits)\nassertResult([1, 2])(xs)\nassertResult(%(\"red\", \"blue\"))(tags)\nassertResult(%[\"Alice\": 30])(ages)\nassertResult(\"Bob\")(rec.name)\nassertResult(31)(user.age)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "11\n[1, 2]\n%(red, blue)\n%[Alice: 30]\nBob\n31\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_maps_and_sets() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-map-set-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-map-set-{unique}"));
    fs::write(
        &source_path,
        "val ages = %[\"Alice\": 30, \"Bob\": 28]\nval tags = %(\"red\", \"blue\", \"red\")\nval nested = %[\"xs\": [1, 2], \"empty\": []]\nprintln(ages)\nprintln(tags)\nprintln(nested)\nprintln(\"ages = \" + ages)\nprintln(\"tags = \" + tags)\nprintln(\"map size = \" + Map#size(ages))\nprintln(\"set size = \" + Set#size(tags))\nprintln(\"has Alice? \" + Map#containsKey(ages, \"Alice\"))\nprintln(\"has 30? \" + Map#containsValue(ages, 30))\nprintln(\"Alice = \" + Map#get(ages, \"Alice\"))\nprintln(\"missing = \" + Map#get(ages, \"Carol\"))\nprintln(\"has blue? \" + Set#contains(tags, \"blue\"))\nprintln(\"Bob = \" + ages.get(\"Bob\"))\nprintln(\"has red? \" + tags.contains(\"red\"))\nassertResult(%[\"Alice\": 30, \"Bob\": 28])(ages)\nassertResult(%(\"red\", \"blue\"))(tags)\nassertResult(%[\"xs\": [1, 2], \"empty\": []])(nested)\nassertResult(true)(Map#containsKey(ages, \"Alice\"))\nassertResult(false)(Map#containsKey(ages, \"Carol\"))\nassertResult(30)(Map#get(ages, \"Alice\"))\nassertResult(null)(Map#get(ages, \"Carol\"))\nassertResult(true)(Set#contains(tags, \"blue\"))\nassertResult(28)(ages.get(\"Bob\"))\nassertResult(true)(tags.contains(\"red\"))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "%[Alice: 30, Bob: 28]\n%(red, blue)\n%[xs: [1, 2], empty: []]\nages = %[Alice: 30, Bob: 28]\ntags = %(red, blue)\nmap size = 2\nset size = 2\nhas Alice? true\nhas 30? true\nAlice = 30\nmissing = null\nhas blue? true\nBob = 28\nhas red? true\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_string_collection_runtime_membership() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-membership-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-membership-{unique}"));
    fs::write(
        &source_path,
        "val keyword = head(args())\nval op = head(tail(args()))\nval kind = head(tail(tail(args())))\nval keywordLen = length(keyword)\nval keywordKnown = keyword == \"if\"\nval keywords = %(\"if\", \"else\", \"while\")\nval precedence = %[\"+\": 10, \"*\": 20]\nval kinds = %[\"if\": \"keyword\", \"+\": \"operator\"]\nval lengthKeys = %[2: \"short\", 5: \"wide\"]\nval lengthValues = %[\"short\": 2, \"kind\": 7]\nval boolKeys = %[true: \"known\", false: \"unknown\"]\nval boolValues = %[\"known\": true]\nprintln(Set#contains(keywords, keyword))\nprintln(keywords.contains(keyword))\nprintln(keywords.contains(\"return\"))\nprintln(Map#containsKey(precedence, op))\nprintln(precedence.containsKey(\"/\"))\nprintln(Map#containsValue(kinds, kind))\nprintln(kinds.containsValue(kind))\nprintln([1, 2, 3].contains(keywordLen))\nprintln(%(1, 2, 3).contains(keywordLen))\nprintln(Map#containsKey(lengthKeys, keywordLen))\nprintln(Map#containsValue(lengthValues, keywordLen))\nprintln(Map#containsKey(boolKeys, keywordKnown))\nprintln(Map#containsValue(boolValues, keywordKnown))\nassert(Set#contains(keywords, keyword))\nassert(keywords.contains(keyword))\nassert(!keywords.contains(\"return\"))\nassert(Map#containsKey(precedence, op))\nassert(!precedence.containsKey(\"/\"))\nassert(Map#containsValue(kinds, kind))\nassert(kinds.containsValue(kind))\nassert([1, 2, 3].contains(keywordLen))\nassert(%(1, 2, 3).contains(keywordLen))\nassert(Map#containsKey(lengthKeys, keywordLen))\nassert(Map#containsValue(lengthValues, keywordLen))\nassert(Map#containsKey(boolKeys, keywordKnown))\nassert(Map#containsValue(boolValues, keywordKnown))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime membership build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .arg("if")
        .arg("+")
        .arg("keyword")
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime membership run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "true\ntrue\nfalse\ntrue\nfalse\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_module_import_aliases() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-import-alias-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-import-alias-{unique}"));
    fs::write(
        &source_path,
        "import Map as M\nimport Map.{size}\nimport Set as S\nimport Set.{contains}\nimport FileInput as FI\nval aliasedSize = M#size\nval readAll = FI#readAll\nprintln(M#size(%[\"a\": 1]))\nprintln(size(%[\"b\": 2, \"c\": 3]))\nprintln(aliasedSize(%[\"d\": 4]))\nprintln(S#size(%(\"x\")))\nprintln(contains(%(\"x\"))(\"x\"))\nprintln(readAll(\"src/test/resources/hello.txt\"))\nassertResult(1)(M#size(%[\"a\": 1]))\nassertResult(2)(size(%[\"b\": 2, \"c\": 3]))\nassertResult(1)(aliasedSize(%[\"d\": 4]))\nassertResult(1)(S#size(%(\"x\")))\nassert(contains(%(\"x\"))(\"x\"))\nassertResult(\"Hello, World!\")(readAll(\"src/test/resources/hello.txt\"))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "1\n2\n1\n1\ntrue\nHello, World!\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_helper_argument_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-helper-arg-effect-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-helper-arg-effect-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nval tags = %(\"red\", \"blue\")\nval ages = %[\"Alice\": 30]\nval textOk = \"abc\".contains({ hits += 1; \"a\" })\nval setOk = tags.contains({ hits += 1; \"red\" })\nval moduleSetOk = Set#contains(tags, { hits += 1; \"blue\" })\nval age = Map#get(ages, { hits += 1; \"Alice\" })\nval hasAge = Map#containsValue(ages, { hits += 1; 30 })\nval piece = substring({ hits += 1; \"abcd\" }, { hits += 1; 1 }, { hits += 1; 3 })\nval shouted = replace({ hits += 1; \"aba\" }, { hits += 1; \"a\" }, { hits += 1; \"x\" })\nval repeated = repeat({ hits += 1; \"ha\" }, { hits += 1; 2 })\nval first = at({ hits += 1; \"xy\" }, { hits += 1; 0 })\nval matched = matches({ hits += 1; \"123\" }, { hits += 1; \"[0-9]+\" })\nval splitParts = split({ hits += 1; \"a,b\" }, { hits += 1; \",\" })\nval joined = join(splitParts, { hits += 1; \"-\" })\nval trimmed = trim({ hits += 1; \" ok \" })\nval lower = toLowerCase({ hits += 1; \"AB\" })\nval starts = startsWith({ hits += 1; \"abc\" }, { hits += 1; \"a\" })\nval idx = indexOf({ hits += 1; \"abc\" }, { hits += 1; \"b\" })\nval len = length({ hits += 1; \"hé\" })\nprintln(hits)\nassertResult(27)(hits)\nassert(textOk)\nassert(setOk)\nassert(moduleSetOk)\nassertResult(30)(age)\nassert(hasAge)\nassertResult(\"bc\")(piece)\nassertResult(\"xba\")(shouted)\nassertResult(\"haha\")(repeated)\nassertResult(\"x\")(first)\nassert(matched)\nassertResult([\"a\", \"b\"])(splitParts)\nassertResult(\"a-b\")(joined)\nassertResult(\"ok\")(trimmed)\nassertResult(\"ab\")(lower)\nassert(starts)\nassertResult(1)(idx)\nassertResult(2)(len)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "27\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_value_equality() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-equality-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-equality-{unique}"));
    fs::write(
        &source_path,
        "val parts = split(\"a,b\", \",\")\nval genericInts = map(parts)((x) => 1)\nval rec = record { name: \"Alice\", age: 30 }\nval map = %[\"a\": [1, 2], \"b\": [3]]\nval set = %(\"red\", \"blue\", \"red\")\nval unitText = \"unit=\" + ()\nmutable flag = true\nprintln(())\nprintln(unitText)\nprintln(\"unit eq = \" + (() == ()))\nprintln(\"string eq = \" + (\"a\" == \"a\"))\nprintln(\"list ne = \" + (parts != [\"a\", \"c\"]))\nprintln(\"record eq = \" + (rec == record { name: \"Alice\", age: 30 }))\nprintln(\"map eq = \" + (map == %[\"a\": [1, 2], \"b\": [3]]))\nprintln(\"set eq = \" + (set == %(\"red\", \"blue\")))\nprintln(\"null ne = \" + (null != Map#get(%[\"x\": 1], \"missing\")))\nprintln(\"flag eq = \" + (flag == true))\nassertResult(())(())\nassertResult(\"unit=()\")(unitText)\nassert(() == ())\nassert(\"a\" == \"a\")\nassert(parts == [\"a\", \"b\"])\nassertResult([1, 1])(genericInts)\nassert(genericInts == [1, 1])\nassert(rec != record { name: \"Alice\", age: 31 })\nassert(map == %[\"a\": [1, 2], \"b\": [3]])\nassert(set == %(\"red\", \"blue\"))\nassert(null == Map#get(%[\"x\": 1], \"missing\"))\nassert(flag == true)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "()\nunit=()\nunit eq = true\nstring eq = true\nlist ne = true\nrecord eq = true\nmap eq = true\nset eq = true\nnull ne = false\nflag eq = true\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_user_visible_function_equality() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-function-equality-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-function-equality-{unique}"));
    fs::write(
        &source_path,
        "val f = (x) => x\nprintln(f == f)\nprintln([f] == [f])\nprintln(println == println)\nprintln([println] == [println])\nassertResult(false)(f == f)\nassertResult(false)([f] == [f])\nassertResult(false)(println == println)\nassertResult(false)([println] == [println])\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "false\nfalse\nfalse\nfalse\n"
    );
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_todo_runtime_error() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-todo-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-todo-{unique}"));
    fs::write(&source_path, "ToDo()\n").expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(!run.status.success());
    assert!(run.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&run.stderr),
        format!("{}:1:1: not implemented yet\n", source_path.display())
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_assertion_runtime_errors() {
    let cases = [
        ("assert(false)\n", 1, 1, "assertion failed"),
        (
            "assertResult(1)(2)\n",
            1,
            1,
            "assertResult failed: expected 1 but got 2",
        ),
        (
            "mutable x = 1\nassertResult(x)(2)\n",
            2,
            1,
            "assertResult failed: expected 1 but got 2",
        ),
        (
            "assertResult([1])([2])\n",
            1,
            1,
            "assertResult failed: expected [1] but got [2]",
        ),
        ("head([])\n", 1, 1, "head expects a non-empty list"),
        (
            "val h = head\nh([])\n",
            2,
            1,
            "head expects a non-empty list",
        ),
        (
            "at(\"abc\", -1)\n",
            1,
            1,
            "at expects a non-negative integer index",
        ),
        (
            "substring(\"abc\", -1, 2)\n",
            1,
            1,
            "substring expects a non-negative integer index",
        ),
        (
            "repeat(\"a\", -1)\n",
            1,
            1,
            "repeat expects a non-negative integer index",
        ),
        (
            "sleep(-1)\n",
            1,
            1,
            "sleep expects a non-negative integer index",
        ),
        (
            "mutable millis = -1\nsleep(millis)\n",
            2,
            1,
            "sleep expects a non-negative integer index",
        ),
    ];
    for (index, (source, line, column, expected_message)) in cases.iter().enumerate() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        let source_path =
            std::env::temp_dir().join(format!("klassic-native-assert-error-{index}-{unique}.kl"));
        let output_path =
            std::env::temp_dir().join(format!("klassic-native-assert-error-{index}-{unique}"));
        fs::write(&source_path, source).expect("source should write");

        let build = Command::new(klassic_bin())
            .args([
                "build",
                source_path.to_string_lossy().as_ref(),
                "-o",
                output_path.to_string_lossy().as_ref(),
            ])
            .output()
            .expect("klassic build should run");

        assert!(
            build.status.success(),
            "assertion error build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&build.stdout),
            String::from_utf8_lossy(&build.stderr)
        );
        assert!(build.stdout.is_empty());
        assert!(build.stderr.is_empty());

        let run = Command::new(&output_path)
            .output()
            .expect("generated executable should run");

        let _ = fs::remove_file(&source_path);
        let _ = fs::remove_file(&output_path);

        assert!(!run.status.success());
        assert!(run.stdout.is_empty());
        assert_eq!(
            String::from_utf8_lossy(&run.stderr),
            format!(
                "{}:{line}:{column}: {expected_message}\n",
                source_path.display()
            )
        );
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_file_output_runtime_errors() {
    let cases = [
        ("FileOutput#write", "FileOutput#write failed to open file"),
        ("FileOutput#append", "FileOutput#append failed to open file"),
        (
            "FileOutput#writeLines",
            "FileOutput#writeLines failed to open file",
        ),
    ];
    for (index, (helper, expected_message)) in cases.iter().enumerate() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        let source_path =
            std::env::temp_dir().join(format!("klassic-native-file-error-{index}-{unique}.kl"));
        let output_path =
            std::env::temp_dir().join(format!("klassic-native-file-error-{index}-{unique}"));
        let missing_parent =
            std::env::temp_dir().join(format!("klassic-native-missing-parent-{index}-{unique}"));
        let target_path = missing_parent.join("out.txt");
        let source = if *helper == "FileOutput#writeLines" {
            format!("{helper}(\"{}\", [\"x\"])\n", target_path.display())
        } else {
            format!("{helper}(\"{}\", \"x\")\n", target_path.display())
        };
        fs::write(&source_path, source).expect("source should write");

        let build = Command::new(klassic_bin())
            .args([
                "build",
                source_path.to_string_lossy().as_ref(),
                "-o",
                output_path.to_string_lossy().as_ref(),
            ])
            .output()
            .expect("klassic build should run");

        assert!(
            build.status.success(),
            "file error build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&build.stdout),
            String::from_utf8_lossy(&build.stderr)
        );
        assert!(build.stdout.is_empty());
        assert!(build.stderr.is_empty());

        let run = Command::new(&output_path)
            .output()
            .expect("generated executable should run");

        let _ = fs::remove_file(&source_path);
        let _ = fs::remove_file(&output_path);

        assert!(!run.status.success());
        assert!(run.stdout.is_empty());
        assert_eq!(
            String::from_utf8_lossy(&run.stderr),
            format!("{}:1:1: {expected_message}\n", source_path.display())
        );
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_dir_runtime_errors() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let existing_dir = std::env::temp_dir().join(format!("klassic-native-existing-dir-{unique}"));
    let nonempty_dir = std::env::temp_dir().join(format!("klassic-native-nonempty-dir-{unique}"));
    let nonempty_file = nonempty_dir.join("inside.txt");
    let missing_source = std::env::temp_dir().join(format!("klassic-native-missing-move-{unique}"));
    let move_target = std::env::temp_dir().join(format!("klassic-native-move-target-{unique}"));
    let missing_copy_source =
        std::env::temp_dir().join(format!("klassic-native-missing-copy-{unique}"));
    let copy_target = std::env::temp_dir().join(format!("klassic-native-copy-target-{unique}"));
    fs::create_dir_all(&existing_dir).expect("existing dir should be created");

    let cases = [
        (
            format!("Dir#mkdir(\"{}\")\n", existing_dir.display()),
            1,
            "Dir#mkdir failed to create directory",
        ),
        (
            format!(
                "Dir#mkdirs(\"{}\")\nFileOutput#write(\"{}\", \"x\")\nDir#delete(\"{}\")\n",
                nonempty_dir.display(),
                nonempty_file.display(),
                nonempty_dir.display()
            ),
            3,
            "Dir#delete failed to delete directory",
        ),
        (
            format!(
                "Dir#move(\"{}\", \"{}\")\n",
                missing_source.display(),
                move_target.display()
            ),
            1,
            "Dir#move failed to move path",
        ),
        (
            format!(
                "Dir#copy(\"{}\", \"{}\")\n",
                missing_copy_source.display(),
                copy_target.display()
            ),
            1,
            "Dir#copy failed to open source file",
        ),
    ];

    for (index, (source, line, expected_message)) in cases.iter().enumerate() {
        let source_path =
            std::env::temp_dir().join(format!("klassic-native-dir-error-{index}-{unique}.kl"));
        let output_path =
            std::env::temp_dir().join(format!("klassic-native-dir-error-{index}-{unique}"));
        fs::write(&source_path, source).expect("source should write");

        let build = Command::new(klassic_bin())
            .args([
                "build",
                source_path.to_string_lossy().as_ref(),
                "-o",
                output_path.to_string_lossy().as_ref(),
            ])
            .output()
            .expect("klassic build should run");

        assert!(
            build.status.success(),
            "dir error build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&build.stdout),
            String::from_utf8_lossy(&build.stderr)
        );
        assert!(build.stdout.is_empty());
        assert!(build.stderr.is_empty());

        let run = Command::new(&output_path)
            .output()
            .expect("generated executable should run");

        let _ = fs::remove_file(&source_path);
        let _ = fs::remove_file(&output_path);

        assert!(!run.status.success());
        assert!(run.stdout.is_empty());
        assert_eq!(
            String::from_utf8_lossy(&run.stderr),
            format!("{}:{line}:1: {expected_message}\n", source_path.display())
        );
    }

    let _ = fs::remove_dir_all(&existing_dir);
    let _ = fs::remove_dir_all(&nonempty_dir);
    let _ = fs::remove_file(&move_target);
    let _ = fs::remove_file(&copy_target);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_dir_copy() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_file = std::env::temp_dir().join(format!("klassic-native-copy-source-{unique}.txt"));
    let target_file = std::env::temp_dir().join(format!("klassic-native-copy-target-{unique}.txt"));
    let source_path = std::env::temp_dir().join(format!("klassic-native-runtime-copy-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-runtime-copy-{unique}"));
    fs::write(&source_file, "copy me").expect("source file should write");
    fs::write(
        &source_path,
        format!(
            "Dir#copy(\"{}\", \"{}\")\nprintln(\"copied\")\n",
            source_file.display(),
            target_file.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime copy build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let copied = fs::read_to_string(&target_file).unwrap_or_default();
    let _ = fs::remove_file(&source_file);
    let _ = fs::remove_file(&target_file);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime copy run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "copied\n");
    assert!(run.stderr.is_empty());
    assert_eq!(copied, "copy me");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_dir_move() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_file = std::env::temp_dir().join(format!("klassic-native-move-source-{unique}.txt"));
    let target_file = std::env::temp_dir().join(format!("klassic-native-move-target-{unique}.txt"));
    let source_path = std::env::temp_dir().join(format!("klassic-native-runtime-move-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-runtime-move-{unique}"));
    fs::write(&source_file, "move me").expect("source file should write");
    fs::write(
        &source_path,
        format!(
            "Dir#move(\"{}\", \"{}\")\nprintln(\"moved\")\n",
            source_file.display(),
            target_file.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "runtime move build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let moved = fs::read_to_string(&target_file).unwrap_or_default();
    let source_still_exists = source_file.exists();
    let _ = fs::remove_file(&source_file);
    let _ = fs::remove_file(&target_file);
    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(
        run.status.success(),
        "runtime move run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "moved\n");
    assert!(run.stderr.is_empty());
    assert!(!source_still_exists);
    assert_eq!(moved, "move me");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_file_delete_and_mkdirs_keep_evaluator_success_cases() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let existing_dir =
        std::env::temp_dir().join(format!("klassic-native-existing-mkdirs-{unique}"));
    let missing_file = existing_dir.join("missing.txt");
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-allowed-file-dir-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-allowed-file-dir-{unique}"));
    fs::create_dir_all(&existing_dir).expect("existing dir should be created");
    fs::write(
        &source_path,
        format!(
            "Dir#mkdirs(\"{}\")\nFileOutput#delete(\"{}\")\nprintln(\"ok\")\n",
            existing_dir.display(),
            missing_file.display()
        ),
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(
        build.status.success(),
        "allowed file/dir build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);
    let _ = fs::remove_dir_all(&existing_dir);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_equality_side_effects() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-equality-side-effects-{unique}.kl"));
    let output_path =
        std::env::temp_dir().join(format!("klassic-native-equality-side-effects-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nassertResult({ hits += 1; [1, 2] })({ hits += 1; [1, 2] })\nassert({ hits += 1; \"a\" } == { hits += 1; \"a\" })\nassert({ hits += 1; record { x: 1 } } != { hits += 1; record { x: 2 } })\nassert({ hits += 1; %[\"a\": 1] } == { hits += 1; %[\"a\": 1] })\nassert({ hits += 1; %(\"x\") } == { hits += 1; %(\"x\") })\nprintln(hits)\nassertResult(10)(hits)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "10\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_static_sleep() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-sleep-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-sleep-{unique}"));
    fs::write(
        &source_path,
        "println(\"before\")\nsleep(0)\nprintln(\"after\")\nassertResult(())(sleep(0))\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "before\nafter\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_runtime_sleep_argument() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path =
        std::env::temp_dir().join(format!("klassic-native-runtime-sleep-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-runtime-sleep-{unique}"));
    fs::write(
        &source_path,
        "mutable hits = 0\nval ms = stopwatch( => 1)\nsleep(ms)\nsleep({ hits += 1; 0 })\nprintln(hits)\nassertResult(1)(hits)\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "1\n");
    assert!(run.stderr.is_empty());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn builds_native_executable_for_stopwatch() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_nanos();
    let source_path = std::env::temp_dir().join(format!("klassic-native-stopwatch-{unique}.kl"));
    let output_path = std::env::temp_dir().join(format!("klassic-native-stopwatch-{unique}"));
    fs::write(
        &source_path,
        "val elapsed = stopwatch( => {\n  sleep(0)\n  42\n})\nassert(elapsed >= 0)\nprintln(\"elapsed ok\")\n",
    )
    .expect("source should write");

    let build = Command::new(klassic_bin())
        .args([
            "build",
            source_path.to_string_lossy().as_ref(),
            "-o",
            output_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("klassic build should run");

    assert!(build.status.success());
    assert!(build.stdout.is_empty());
    assert!(build.stderr.is_empty());

    let run = Command::new(&output_path)
        .output()
        .expect("generated executable should run");

    let _ = fs::remove_file(&source_path);
    let _ = fs::remove_file(&output_path);

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "elapsed ok\n");
    assert!(run.stderr.is_empty());
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
