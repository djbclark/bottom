//! A process command split into the parts shown when `processes.split_command`
//! is on: the command name, the working directory, and the options.

use std::path::Path;

/// A process command split for display. See the module docs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SplitCommand {
    /// The file name of the executable. For a script interpreter (`python`,
    /// `node`, `bash`, ...), the interpreter followed by the script's file name
    /// (or the module, for `python -m`), e.g. `python3.15 s1_search.py`.
    pub name: String,

    /// The working directory, with the home directory shown as `~`. `None` if
    /// it could not be read (usually a process of another user).
    pub cwd: Option<String>,

    /// The remaining arguments, joined with spaces. Arguments that contain
    /// whitespace or quotes are single-quoted.
    pub args: String,
}

/// How to find the script in an interpreter's arguments.
struct Interpreter {
    /// Options that take the next argument as their value.
    value_options: &'static [&'static str],
    /// Options whose value is a module to run; that value is used as the script.
    module_options: &'static [&'static str],
    /// Options that run inline code, so there is no script.
    inline_options: &'static [&'static str],
    /// Subcommands that may come before the script (`bun run x.ts`).
    subcommands: &'static [&'static str],
}

const PYTHON: Interpreter = Interpreter {
    value_options: &["-W", "-X"],
    module_options: &["-m"],
    inline_options: &["-c"],
    subcommands: &[],
};

const NODE: Interpreter = Interpreter {
    value_options: &[
        "-r",
        "--require",
        "--import",
        "--loader",
        "--experimental-loader",
        "-C",
        "--conditions",
    ],
    module_options: &[],
    inline_options: &["-e", "--eval", "-p", "--print"],
    subcommands: &["run"],
};

const RUBY: Interpreter = Interpreter {
    value_options: &["-r", "-I", "-E"],
    module_options: &[],
    inline_options: &["-e"],
    subcommands: &[],
};

const PERL: Interpreter = Interpreter {
    value_options: &[],
    module_options: &[],
    inline_options: &["-e", "-E"],
    subcommands: &[],
};

const PHP: Interpreter = Interpreter {
    value_options: &["-c", "-d"],
    module_options: &[],
    inline_options: &["-r"],
    subcommands: &[],
};

const SHELL: Interpreter = Interpreter {
    value_options: &["-o", "+o"],
    module_options: &[],
    inline_options: &["-c"],
    subcommands: &[],
};

const OTHER: Interpreter = Interpreter {
    value_options: &[],
    module_options: &[],
    inline_options: &["-e"],
    subcommands: &[],
};

/// The interpreter named by an executable's file name, ignoring a version
/// suffix (`python3.15`, `node22`) and case (`Python`).
fn interpreter(name: &str) -> Option<&'static Interpreter> {
    let stem = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.' || c == '-');
    let interpreter = match stem.to_ascii_lowercase().as_str() {
        "python" | "pypy" => &PYTHON,
        "node" | "nodejs" | "bun" | "deno" => &NODE,
        "ruby" => &RUBY,
        "perl" => &PERL,
        "php" => &PHP,
        "sh" | "bash" | "zsh" | "dash" | "ksh" | "mksh" | "fish" | "csh" | "tcsh" => &SHELL,
        "lua" | "luajit" | "tclsh" | "wish" | "rscript" | "osascript" => &OTHER,
        _ => return None,
    };
    Some(interpreter)
}

/// Index into `args` (the arguments after the interpreter) of the script.
fn script_index(interpreter: &Interpreter, args: &[&str]) -> Option<usize> {
    let mut seen_subcommand = false;
    let mut index = 0;
    while let Some(&arg) = args.get(index) {
        if arg == "--" {
            return (index + 1 < args.len()).then_some(index + 1);
        }
        // `-` is a script read from standard input: there is no file name.
        if arg == "-" || interpreter.inline_options.contains(&arg) {
            return None;
        }
        if interpreter.module_options.contains(&arg) {
            return (index + 1 < args.len()).then_some(index + 1);
        }
        if arg.len() > 1 && (arg.starts_with('-') || arg.starts_with('+')) {
            index += if interpreter.value_options.contains(&arg) {
                2
            } else {
                1
            };
            continue;
        }
        if !seen_subcommand && interpreter.subcommands.contains(&arg) {
            seen_subcommand = true;
            index += 1;
            continue;
        }
        return Some(index);
    }
    None
}

/// The last component of a path, splitting on `/` (and `\` on Windows).
fn file_name(path: &str) -> &str {
    let separators: &[char] = if cfg!(windows) { &['/', '\\'] } else { &['/'] };
    let trimmed = path.trim_end_matches(separators);
    match trimmed.rfind(separators) {
        Some(index) => &trimmed[index + 1..],
        None => trimmed,
    }
}

/// Where a single-string argv (a process that rewrote its arguments into one
/// space-separated string) has its first option: the first ` -` followed by a
/// non-space. `None` if there is none, as an executable path alone can
/// contain spaces.
fn rewritten_option_start(only: &str) -> Option<usize> {
    only.match_indices(" -")
        .map(|(index, _)| index)
        .find(|&index| {
            only[index + 2..]
                .chars()
                .next()
                .is_some_and(|c| !c.is_whitespace())
        })
}

/// Appends `arg` to `out` as one more space-separated argument, single-quoting
/// it if needed so the result reads like a shell command. Control characters
/// (such as the newlines of a `bash -c` script) become spaces, as a table cell
/// is one line.
fn push_arg(out: &mut String, arg: &str) {
    if !out.is_empty() {
        out.push(' ');
    }

    let needs_quotes =
        arg.is_empty() || arg.contains(|c: char| c.is_whitespace() || c == '\'' || c == '"');
    if needs_quotes {
        out.push('\'');
    }
    for c in arg.chars() {
        match c {
            '\'' if needs_quotes => out.push_str("'\\''"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    if needs_quotes {
        out.push('\'');
    }
}

/// `cwd` as text, with `home` (and anything under it) shown as `~`.
fn display_cwd(cwd: &Path, home: Option<&str>) -> String {
    let cwd = cwd.to_string_lossy();
    if let Some(home) = home.filter(|home| !home.is_empty() && *home != "/")
        && let Some(rest) = cwd.strip_prefix(home)
        && (rest.is_empty() || rest.starts_with(['/', std::path::MAIN_SEPARATOR]))
    {
        let mut shown = String::with_capacity(rest.len() + 1);
        shown.push('~');
        shown.push_str(rest);
        return shown;
    }
    cwd.into_owned()
}

impl SplitCommand {
    /// Splits a process's `argv`.
    ///
    /// `fallback_name` is used when `argv` is empty (kernel threads, or a
    /// process whose arguments cannot be read). `home` is the user's home
    /// directory, shown as `~` in the working directory.
    pub fn new<S: AsRef<str>>(
        argv: &[S], cwd: Option<&Path>, home: Option<&str>, fallback_name: &str,
    ) -> Self {
        let argv: Vec<&str> = match argv {
            [only] => {
                let only = only.as_ref();
                match rewritten_option_start(only) {
                    Some(split_at) => std::iter::once(&only[..split_at])
                        .chain(only[split_at..].split_whitespace())
                        .collect(),
                    None => vec![only],
                }
            }
            _ => argv.iter().map(AsRef::as_ref).collect(),
        };

        let cwd = cwd.map(|cwd| display_cwd(cwd, home));

        let Some((&argv0, rest)) = argv.split_first() else {
            return SplitCommand {
                name: fallback_name.to_string(),
                cwd,
                args: String::new(),
            };
        };

        let executable = match file_name(argv0) {
            "" => fallback_name,
            name => name,
        };

        let script =
            interpreter(executable).and_then(|interpreter| script_index(interpreter, rest));

        let mut name = executable.to_string();
        if let Some(script) = script {
            name.push(' ');
            name.push_str(file_name(rest[script]));
        }

        let mut args = String::new();
        for (index, arg) in rest.iter().enumerate() {
            if Some(index) != script {
                push_arg(&mut args, arg);
            }
        }

        SplitCommand { name, cwd, args }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::SplitCommand;

    fn split(argv: &[&str]) -> SplitCommand {
        SplitCommand::new(argv, None, None, "fallback")
    }

    #[test]
    fn executable_name_and_options() {
        let parts = split(&[
            "/Users/me/.local/bin/herdr",
            "server",
            "--handoff-import",
            "a b",
        ]);
        assert_eq!(parts.name, "herdr");
        assert_eq!(parts.args, "server --handoff-import 'a b'");
    }

    #[test]
    fn interpreter_name_includes_the_script() {
        let parts = split(&[
            "/opt/homebrew/bin/python3.15",
            "-u",
            "/Users/me/ops/bin/s1_search.py",
            "index",
        ]);
        assert_eq!(parts.name, "python3.15 s1_search.py");
        assert_eq!(parts.args, "-u index");

        let parts = split(&[
            "node",
            "--require",
            "ts-node/register",
            "./server.ts",
            "--port",
            "80",
        ]);
        assert_eq!(parts.name, "node server.ts");
        assert_eq!(parts.args, "--require ts-node/register --port 80");

        let parts = split(&["bun", "run", "dev.ts"]);
        assert_eq!(parts.name, "bun dev.ts");
        assert_eq!(parts.args, "run");
    }

    #[test]
    fn interpreter_module_and_inline_code() {
        let parts = split(&["python3", "-m", "http.server", "8000"]);
        assert_eq!(parts.name, "python3 http.server");
        assert_eq!(parts.args, "-m 8000");

        // Inline code is not a script; it stays in the options.
        let parts = split(&["bash", "-c", "echo hi\nsleep 1"]);
        assert_eq!(parts.name, "bash");
        assert_eq!(parts.args, "-c 'echo hi sleep 1'");

        // A script read from standard input has no file name.
        let parts = split(&["python3", "-", "100.124.55.39", "8022"]);
        assert_eq!(parts.name, "python3");
        assert_eq!(parts.args, "- 100.124.55.39 8022");

        // A login shell has no script.
        let parts = split(&["-zsh"]);
        assert_eq!(parts.name, "-zsh");
        assert_eq!(parts.args, "");
    }

    #[test]
    fn rewritten_single_string_command() {
        let parts = split(&["/opt/google/chrome/chrome --type=renderer --lang=en"]);
        assert_eq!(parts.name, "chrome");
        assert_eq!(parts.args, "--type=renderer --lang=en");

        // Without an option, a single argument is a path that may have spaces.
        let parts = split(&["/Applications/Foo - Bar.app/Contents/MacOS/Foo Helper"]);
        assert_eq!(parts.name, "Foo Helper");
        assert_eq!(parts.args, "");
    }

    #[test]
    fn empty_argv_uses_fallback() {
        let parts = SplitCommand::new::<&str>(&[], None, None, "[kworker/0:1]");
        assert_eq!(parts.name, "[kworker/0:1]");
        assert_eq!(parts.args, "");
    }

    #[test]
    fn home_is_shown_as_tilde() {
        let home = Some("/Users/me");
        let cwd = |path: &str| SplitCommand::new(&["x"], Some(Path::new(path)), home, "x").cwd;

        assert_eq!(cwd("/Users/me").as_deref(), Some("~"));
        assert_eq!(cwd("/Users/me/src/bottom").as_deref(), Some("~/src/bottom"));
        assert_eq!(cwd("/Users/meg/src").as_deref(), Some("/Users/meg/src"));
        assert_eq!(cwd("/tmp").as_deref(), Some("/tmp"));
    }
}
