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
//!   is written and nothing is deleted. Before the first file is written,
//!   every destination is inspected without following links, from the crate
//!   folder down; the crate folder and its ancestors are trusted and not
//!   inspected. A destination outside the crate folder, a symbolic link or a
//!   non-folder on the way to a file, or a file that is a symbolic link, is
//!   not a regular file or, on Unix, has another hard link is refused, and
//!   then nothing is written. Each file is inspected again just before it is
//!   written. The inspections are not atomic with the writes, so a link that
//!   a concurrent writer puts in place between an inspection and its write
//!   is followed.
//! - With `--check`, nothing is written. Each generated file is compared
//!   byte for byte with the committed one, read through
//!   `golden::read_corpus_input` for a case input file and directly for
//!   `corpus.zwr` and `manifest.tsv`. No other file is read. Before either
//!   of those two is opened, it is inspected without following links, from
//!   the crate folder down as a destination is: a symbolic link, a
//!   non-folder or nothing on the way to it, or a file that is a symbolic
//!   link, is not a regular file or is not as long as the generated text,
//!   is refused without being opened, and the read stops one byte past that
//!   length. The inspection is not atomic with the open, so a link or FIFO
//!   that a concurrent writer puts in place between the two is followed or
//!   blocks the open.
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
//! generated file that is missing, refused by that inspection, unreadable,
//! rejected by the case input reader or unequal, then
//! `check: 233 files equal` or `check: <k> of 233 files differ`. Paths are
//! shown relative to the crate folder.
//!
//! The exit status is 0 when the files were written or all compared equal.
//! It is 1 when the fixtures or `SPEC.md` cannot be read, the generation
//! fails, the generated file list is not the 233 corpus files, a destination
//! is refused, a file cannot be written, or a compared file differs; each
//! failure but the last prints `corpus_gen: <cause>` to standard error, for
//! a refused or unwritable file `corpus_gen: <path>: <cause>`. A failed
//! generation, and a destination refused before the first write, write
//! nothing; a refusal or write failure during the writes stops them at that
//! file, the files before it already written. It is 2, after
//! `usage: corpus_gen [--check]` on standard error, for any other argument.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Display;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use ifcap_po_inquiry::{corpus, golden, store};

/// The manifest's file name in the corpus folder.
const MANIFEST_NAME: &str = "manifest.tsv";

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

/// Writes every generated file, creating its folder first. Every
/// destination passes `check_destination` before the first file is written,
/// so a refused one stops the run with nothing written. Each is checked
/// again just before its own write, and the run stops at the first file that
/// is refused then or cannot be written, the files before it already
/// written. The checks are not atomic with the writes: a link that a
/// concurrent writer puts in place between a check and its write is
/// followed. Unchanged files are written too.
fn write_files(files: &[(PathBuf, String)]) -> ExitCode {
    for (path, _) in files {
        if let Err(cause) = check_destination(path) {
            return fail(format_args!("{}: {cause}", shown(path)));
        }
    }
    for (path, text) in files {
        let written = check_destination(path)
            .and_then(|()| write_file(path, text).map_err(|error| error.to_string()));
        if let Err(cause) = written {
            return fail(format_args!("{}: {cause}", shown(path)));
        }
    }
    ExitCode::SUCCESS
}

/// Whether `path` may be written. It must lie inside the crate folder, each
/// folder between the crate folder and the file must be a folder or not
/// exist yet, and the file must not exist yet or be a regular file with no
/// other hard link. Each part is inspected through `fs::symlink_metadata`,
/// which describes a symbolic link itself rather than what it points to, so
/// a link anywhere on the way is refused, a dangling one included. The crate
/// folder and its ancestors are trusted and not inspected. `Err` gives the
/// cause.
fn check_destination(path: &Path) -> Result<(), String> {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let relative = path.strip_prefix(crate_dir).ok().filter(|relative| {
        relative.file_name().is_some()
            && relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
    });
    let Some(relative) = relative else {
        return Err("destination is not a file inside the crate folder".to_string());
    };
    let mut folder = crate_dir.to_path_buf();
    for part in relative.parent().into_iter().flat_map(Path::components) {
        folder.push(part);
        match fs::symlink_metadata(&folder) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!("folder {} is a symbolic link", shown(&folder)));
            }
            Ok(meta) if !meta.is_dir() => {
                return Err(format!("{} is not a folder", shown(&folder)));
            }
            Ok(_) => {}
            // A missing folder is created by the write, and nothing can lie
            // below it yet.
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("{}: {error}", shown(&folder))),
        }
    }
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err("destination is a symbolic link".to_string())
        }
        Ok(meta) if !meta.is_file() => Err("destination is not a regular file".to_string()),
        Ok(meta) => match hard_links(&meta) {
            Some(links) => Err(format!("destination has {links} hard links")),
            None => Ok(()),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/// The hard-link count of the file `meta` describes, when it is more than
/// one, so that a write cannot reach another name of the same file.
#[cfg(unix)]
fn hard_links(meta: &fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(meta.nlink()).filter(|&links| links > 1)
}

/// Off Unix the standard library gives no hard-link count, so no hard link
/// is reported and none is refused.
#[cfg(not(unix))]
fn hard_links(_meta: &fs::Metadata) -> Option<u64> {
    None
}

/// Writes `text` to `path`, creating the folders above it as needed. These
/// are the generator's only two file-system writes, and both follow links,
/// so `write_files` calls `check_destination` first.
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
/// through `read_committed`, which first inspects them without following
/// links and refuses, unopened, a link or non-folder on the way, a file that
/// is a link or not a regular file, or one whose length is not the generated
/// text's. That inspection is not atomic with the open. A file that is
/// refused or cannot be read differs.
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
            None => read_committed(path, text.len()),
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

/// The committed text of `corpus.zwr` or the manifest at `path`, read
/// directly, or `None` when it is refused or cannot be read, so that it
/// differs. Before anything is opened, each part from the crate folder down
/// to the file is inspected through `fs::symlink_metadata`, which describes
/// a symbolic link itself rather than what it points to. A path outside the
/// crate folder, a symbolic link, a non-folder or nothing on the way, or a
/// file that is a symbolic link, dangling or not, is not a regular file (a
/// FIFO, socket, device or folder) or is not `length` bytes long is refused
/// without being opened; `length` is the generated text's, the only length
/// that can compare equal. The crate folder and its ancestors are trusted
/// and not inspected, and a hard link is not refused, since a read changes
/// nothing. The read stops one byte past `length`, so a file grown since its
/// inspection is never read whole and still differs. The inspection is not
/// atomic with the open, so a link or FIFO that a concurrent writer puts in
/// place between the two is followed or blocks the open.
fn read_committed(path: &Path, length: usize) -> Option<String> {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let relative = path.strip_prefix(crate_dir).ok()?;
    if !relative
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        return None;
    }
    let mut folder = crate_dir.to_path_buf();
    for part in relative.parent().into_iter().flat_map(Path::components) {
        folder.push(part);
        // A symbolic link's own metadata is never a folder's.
        if !fs::symlink_metadata(&folder).ok()?.is_dir() {
            return None;
        }
    }
    let budget = u64::try_from(length).ok()?;
    // Nor is it a regular file's, so a link at the file is refused here too.
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() != budget {
        return None;
    }
    let mut text = String::with_capacity(length);
    let mut file = fs::File::open(path).ok()?.take(budget.saturating_add(1));
    file.read_to_string(&mut text).ok()?;
    Some(text)
}

/// `path` relative to the crate folder where it lies inside it, else as
/// given.
fn shown(path: &Path) -> String {
    path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
        .unwrap_or(path)
        .display()
        .to_string()
}
