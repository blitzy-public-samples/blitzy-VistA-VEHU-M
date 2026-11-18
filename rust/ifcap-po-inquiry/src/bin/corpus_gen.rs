//! The `corpus_gen` binary: generates the deterministic parity corpus of
//! `SPEC.md` "Parity corpus", its 231 cases, from the fixed seed
//! `corpus::SEED` and the world A fixtures of `tests/fixtures/ifcap`. The
//! logic is all in `ifcap_po_inquiry::corpus`; this file loads, prints,
//! writes or compares, and turns the outcome into an exit status.
//!
//! Usage: `corpus_gen [--check]`.
//!
//! Both modes read the world A fixtures through `store::load_fixtures` and
//! `SPEC.md` beside `Cargo.toml`, and generate the corpus in memory. The
//! generated files are 233: `tests/fixtures/ifcap-corpus/corpus.zwr`,
//! `manifest.tsv` in the corpus folder `golden::corpus_dir()`, and one case
//! input file in each case's folder of the corpus folder.
//! - With no argument, those files are written, their folders created as
//!   needed, and only once the whole generation has succeeded. Nothing else
//!   is written and nothing is deleted.
//! - With `--check`, nothing is written. Each generated file is compared
//!   byte for byte with the committed one, read through
//!   `golden::read_corpus_input` for a case input file and directly for
//!   `corpus.zwr` and `manifest.tsv`. No other file is read.
//!
//! Both modes first print the seven lines of `Corpus::summary` to standard
//! output, each filled from the generated model:
//! - `specified: 200 CP, 26 CE (16 IN, 10 BOUNDARY), 5 CR, 50 multi-item orders`
//! - `generated: <cp> CP, <ce> CE (<i> IN, <b> BOUNDARY), <cr> CR, <m> multi-item orders`
//! - `moved: <case> <from>-><to> <branch>; ...`, or `moved: none`
//! - `dropped variants: <n>`
//! - `findings: <n>`
//! - `regeneration rounds: <r>`
//! - `largest item count: <k>; worst $Y: <y> (bound 45)`
//!
//! With `--check` they are followed by one `differs: <path>` line for each
//! generated file that is missing, unreadable, rejected by the case input
//! reader or unequal, then `check: 233 files equal` or
//! `check: <k> of 233 files differ`. Paths are shown relative to the crate
//! folder.
//!
//! The exit status is 0 when the files were written or all compared equal.
//! It is 1 when the fixtures or `SPEC.md` cannot be read, the generation
//! fails, the generated file list is not the 233 corpus files, a file cannot
//! be written, or a compared file differs; each failure but the last prints
//! `corpus_gen: <cause>` to standard error, and a failed generation writes
//! nothing. It is 2, after `usage: corpus_gen [--check]` on standard error,
//! for any other argument.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Display;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ifcap_po_inquiry::{corpus, golden, store};

/// The manifest's file name in the corpus folder.
const MANIFEST_NAME: &str = "manifest.tsv";

/// What the command line asks for.
enum Mode {
    /// Generate and write the corpus files.
    Write,
    /// Generate and compare with the committed files, writing nothing.
    Check,
}

fn main() -> ExitCode {
    // Every line is written with its error dropped, as in the bridge binary:
    // a stream that cannot take a line never changes the exit status.
    let Some(mode) = parse_mode() else {
        let _ = writeln!(io::stderr(), "usage: corpus_gen [--check]");
        return ExitCode::from(2);
    };
    let world_a = match store::load_fixtures(&corpus::world_a_dir()) {
        Ok(world_a) => world_a,
        Err(error) => return fail(error),
    };
    let spec_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("SPEC.md");
    let spec = match fs::read_to_string(&spec_path) {
        Ok(spec) => spec,
        Err(error) => return fail(format_args!("{}: {error}", shown(&spec_path))),
    };
    let generated = match corpus::generate(&world_a, &spec) {
        Ok(generated) => generated,
        Err(error) => return fail(error),
    };
    let files = generated.files();
    let Some(inputs) = case_inputs(&files) else {
        return fail(format_args!(
            "generated file list is not the {} corpus files",
            corpus_file_count()
        ));
    };
    let mut out = io::stdout().lock();
    for line in generated.summary() {
        let _ = writeln!(out, "{line}");
    }
    let status = match mode {
        Mode::Write => write_files(&files),
        Mode::Check => check_files(&files, &inputs, &mut out),
    };
    let _ = out.flush();
    status
}

/// The mode the arguments select: none is `Write`, `--check` alone is
/// `Check`, and anything else, a non-UTF-8 argument included, is `None`.
fn parse_mode() -> Option<Mode> {
    let mut args = env::args_os().skip(1);
    match (args.next(), args.next()) {
        (None, _) => Some(Mode::Write),
        (Some(arg), None) if arg == "--check" => Some(Mode::Check),
        _ => None,
    }
}

/// Prints `corpus_gen: <cause>` to standard error and gives exit status 1.
fn fail(cause: impl Display) -> ExitCode {
    let _ = writeln!(io::stderr(), "corpus_gen: {cause}");
    ExitCode::FAILURE
}

/// The number of corpus files: the corpus ZWR file, the manifest and one
/// case input file per case.
fn corpus_file_count() -> usize {
    golden::corpus_case_ids().len() + 2
}

/// Each case input file's path mapped to its case id, when `files` holds
/// exactly the corpus files: distinct paths, one of them the corpus ZWR
/// file, one the manifest in the corpus folder, and the case input file of
/// every corpus case id. `None` otherwise, so that nothing outside those
/// paths is ever written, and so that the check reads each file by the
/// reader its path calls for, whatever order the list is in.
fn case_inputs(files: &[(PathBuf, String)]) -> Option<BTreeMap<PathBuf, String>> {
    let folder = golden::corpus_dir();
    let inputs: BTreeMap<PathBuf, String> = golden::corpus_case_ids()
        .into_iter()
        .map(|id| (golden::corpus_input_path(&folder, &id), id))
        .collect();
    let paths: BTreeSet<&Path> = files.iter().map(|(path, _)| path.as_path()).collect();
    let zwr = corpus::corpus_zwr_path();
    let manifest = folder.join(MANIFEST_NAME);
    let complete = files.len() == corpus_file_count()
        && paths.len() == files.len()
        && inputs.len() + 2 == files.len()
        && paths.contains(zwr.as_path())
        && paths.contains(manifest.as_path())
        && inputs.keys().all(|path| paths.contains(path.as_path()));
    complete.then_some(inputs)
}

/// Writes every generated file, creating its folder first, and stops at the
/// first file that cannot be written. Unchanged files are written too.
fn write_files(files: &[(PathBuf, String)]) -> ExitCode {
    for (path, text) in files {
        if let Err(error) = write_file(path, text) {
            return fail(format_args!("{}: {error}", shown(path)));
        }
    }
    ExitCode::SUCCESS
}

/// Writes `text` to `path`, creating the folders above it as needed. These
/// are the generator's only two file-system writes.
fn write_file(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, text)
}

/// Compares every generated file with the committed one and prints a
/// `differs:` line for each that is not equal, then the `check:` line. A
/// case input file is read through `golden::read_corpus_input`, so one its
/// integrity rules reject differs; `corpus.zwr` and the manifest are read
/// whole. A file that cannot be read differs.
fn check_files(
    files: &[(PathBuf, String)],
    inputs: &BTreeMap<PathBuf, String>,
    out: &mut impl Write,
) -> ExitCode {
    let folder = golden::corpus_dir();
    let mut differing = 0usize;
    for (path, text) in files {
        let committed = match inputs.get(path) {
            Some(id) => golden::read_corpus_input(&folder, id).ok(),
            None => fs::read_to_string(path).ok(),
        };
        if committed.as_deref() != Some(text.as_str()) {
            differing += 1;
            let _ = writeln!(out, "differs: {}", shown(path));
        }
    }
    let total = files.len();
    if differing == 0 {
        let _ = writeln!(out, "check: {total} files equal");
        ExitCode::SUCCESS
    } else {
        let _ = writeln!(out, "check: {differing} of {total} files differ");
        ExitCode::FAILURE
    }
}

/// `path` relative to the crate folder where it lies inside it, else as
/// given.
fn shown(path: &Path) -> String {
    path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
        .unwrap_or(path)
        .display()
        .to_string()
}
