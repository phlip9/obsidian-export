#![allow(clippy::shadow_unrelated)]

use core::str;
use std::collections::BTreeSet;
use std::fs::{create_dir, read_to_string, set_permissions, File, Permissions};
use std::io::prelude::*;
#[cfg(not(target_os = "windows"))]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use obsidian_export::{ExportError, Exporter, FrontmatterStrategy, InternalLinkFormat};
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use walkdir::{DirEntry, WalkDir};

fn diff_export_dirs(expected_dir: &Path, actual_dir: &Path) {
    // A type that wraps a path with its vault-relative path. It uses the
    // relative path for comparisons.
    struct FileEntry {
        path: PathBuf,
        rel_path: PathBuf,
    }
    impl Eq for FileEntry {}
    impl PartialEq for FileEntry {
        fn eq(&self, other: &Self) -> bool {
            self.rel_path == other.rel_path
        }
    }
    impl Ord for FileEntry {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.rel_path.cmp(&other.rel_path)
        }
    }
    impl PartialOrd for FileEntry {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    // Filter out directories and then compute the vault-relative path.
    fn filter_entry(root_dir: &Path, entry: walkdir::Result<DirEntry>) -> Option<FileEntry> {
        let entry = entry.unwrap();
        if entry.metadata().unwrap().is_dir() {
            return None;
        }
        let path = entry.into_path();
        let rel_path = path.strip_prefix(root_dir).unwrap().to_path_buf();
        Some(FileEntry { path, rel_path })
    }

    // Expected fileset
    let expected_fileset = WalkDir::new(expected_dir)
        .into_iter()
        .filter_map(|entry| filter_entry(expected_dir, entry))
        .collect::<BTreeSet<_>>();

    // Actual generated fileset
    let actual_fileset = WalkDir::new(actual_dir)
        .into_iter()
        .filter_map(|entry| filter_entry(actual_dir, entry))
        .collect::<BTreeSet<_>>();

    // Check for missing files or extra files in tmp_dir
    let missing_fileset = expected_fileset
        .difference(&actual_fileset)
        .map(|entry| &entry.rel_path)
        .collect::<Vec<_>>();
    let extra_fileset = actual_fileset
        .difference(&expected_fileset)
        .map(|entry| &entry.rel_path)
        .collect::<Vec<_>>();
    assert!(
        missing_fileset.is_empty() && extra_fileset.is_empty(),
        "Missing files in temporary exportdir: {:?}\n\
         Extra files in temporary exportdir: {:?}",
        missing_fileset,
        extra_fileset
    );
    assert_eq!(expected_fileset.len(), actual_fileset.len());

    // Check contents of generated files match
    for (expected_entry, actual_entry) in expected_fileset.into_iter().zip(actual_fileset) {
        assert_eq!(&expected_entry.rel_path, &actual_entry.rel_path);
        let expected = std::fs::read(&expected_entry.path).unwrap_or_else(|_| {
            panic!(
                "failed to read {} from testdata/expected/main-samples/",
                expected_entry.rel_path.display()
            )
        });
        let actual = std::fs::read(&actual_entry.path).unwrap_or_else(|_| {
            panic!(
                "failed to read {} from the temporary exportdir",
                actual_entry.rel_path.display()
            )
        });
        if let Ok(expected) = str::from_utf8(&expected) {
            let actual = str::from_utf8(&actual).unwrap();
            assert_eq!(
                expected,
                actual,
                "'{}' does not have expected content",
                expected_entry.rel_path.display()
            );
        } else {
            assert_eq!(
                expected,
                actual,
                "'{}' does not have expected content",
                expected_entry.rel_path.display()
            );
        }
    }
}

#[test]
fn test_main_variants_with_default_options() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");
    let input_dir = Path::new("tests/testdata/input/main-samples/");
    let expected_dir = Path::new("tests/testdata/expected/main-samples/");
    Exporter::new(input_dir.to_owned(), tmp_dir.path().to_owned())
        .run()
        .expect("exporter returned error");
    diff_export_dirs(expected_dir, tmp_dir.path());
}

#[test]
fn test_frontmatter_never() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");
    let mut exporter = Exporter::new(
        PathBuf::from("tests/testdata/input/main-samples/"),
        tmp_dir.path().to_path_buf(),
    );
    exporter.frontmatter_strategy(FrontmatterStrategy::Never);
    exporter.run().expect("exporter returned error");

    let expected = "Note with frontmatter.\n";
    let actual = read_to_string(
        tmp_dir
            .path()
            .join(PathBuf::from("note-with-frontmatter.md")),
    )
    .unwrap();

    assert_eq!(expected, actual);
}

#[test]
fn test_frontmatter_always() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");
    let mut exporter = Exporter::new(
        PathBuf::from("tests/testdata/input/main-samples/"),
        tmp_dir.path().to_path_buf(),
    );
    exporter.frontmatter_strategy(FrontmatterStrategy::Always);
    exporter.run().expect("exporter returned error");

    // Note without frontmatter should have empty frontmatter added.
    let expected = "---\n---\n\nNote without frontmatter.\n";
    let actual = read_to_string(
        tmp_dir
            .path()
            .join(PathBuf::from("note-without-frontmatter.md")),
    )
    .unwrap();
    assert_eq!(expected, actual);

    // Note with frontmatter should remain untouched.
    let expected = "---\nFoo: bar\n---\n\nNote with frontmatter.\n";
    let actual = read_to_string(
        tmp_dir
            .path()
            .join(PathBuf::from("note-with-frontmatter.md")),
    )
    .unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn test_exclude() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");

    Exporter::new(
        PathBuf::from("tests/testdata/input/main-samples/"),
        tmp_dir.path().to_path_buf(),
    )
    .run()
    .expect("exporter returned error");

    let excluded_note = tmp_dir.path().join(PathBuf::from("excluded-note.md"));
    assert!(
        !excluded_note.exists(),
        "exluded-note.md was found in tmpdir, but should be absent due to .export-ignore rules"
    );
}

#[test]
fn test_single_file_to_dir() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");
    Exporter::new(
        PathBuf::from("tests/testdata/input/single-file/note.md"),
        tmp_dir.path().to_path_buf(),
    )
    .run()
    .unwrap();

    assert_eq!(
        read_to_string("tests/testdata/expected/single-file/note.md").unwrap(),
        read_to_string(tmp_dir.path().join(PathBuf::from("note.md"))).unwrap(),
    );
}

#[test]
fn test_single_file_to_file() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");
    let dest = tmp_dir.path().join(PathBuf::from("export.md"));

    Exporter::new(
        PathBuf::from("tests/testdata/input/single-file/note.md"),
        dest.clone(),
    )
    .run()
    .unwrap();

    assert_eq!(
        read_to_string("tests/testdata/expected/single-file/note.md").unwrap(),
        read_to_string(&dest).unwrap(),
    );
}

#[test]
fn test_start_at_subdir() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");
    let mut exporter = Exporter::new(
        PathBuf::from("tests/testdata/input/start-at/"),
        tmp_dir.path().to_path_buf(),
    );
    exporter.start_at(PathBuf::from("tests/testdata/input/start-at/subdir"));
    exporter.run().unwrap();

    let expected = if cfg!(windows) {
        read_to_string("tests/testdata/expected/start-at/subdir/Note B.md")
            .unwrap()
            .replace('/', "\\")
    } else {
        read_to_string("tests/testdata/expected/start-at/subdir/Note B.md").unwrap()
    };

    assert_eq!(
        expected,
        read_to_string(tmp_dir.path().join(PathBuf::from("Note B.md"))).unwrap(),
    );
}

#[test]
fn test_start_at_file_within_subdir_destination_is_dir() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");
    let mut exporter = Exporter::new(
        PathBuf::from("tests/testdata/input/start-at/"),
        tmp_dir.path().to_path_buf(),
    );
    exporter.start_at(PathBuf::from(
        "tests/testdata/input/start-at/subdir/Note B.md",
    ));
    exporter.run().unwrap();

    let expected = if cfg!(windows) {
        read_to_string("tests/testdata/expected/start-at/single-file/Note B.md")
            .unwrap()
            .replace('/', "\\")
    } else {
        read_to_string("tests/testdata/expected/start-at/single-file/Note B.md").unwrap()
    };

    assert_eq!(
        expected,
        read_to_string(tmp_dir.path().join(PathBuf::from("Note B.md"))).unwrap(),
    );
}

#[test]
fn test_start_at_file_within_subdir_destination_is_file() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");
    let dest = tmp_dir.path().join(PathBuf::from("note.md"));
    let mut exporter = Exporter::new(
        PathBuf::from("tests/testdata/input/start-at/"),
        dest.clone(),
    );
    exporter.start_at(PathBuf::from(
        "tests/testdata/input/start-at/subdir/Note B.md",
    ));
    exporter.run().unwrap();

    let expected = if cfg!(windows) {
        read_to_string("tests/testdata/expected/start-at/single-file/Note B.md")
            .unwrap()
            .replace('/', "\\")
    } else {
        read_to_string("tests/testdata/expected/start-at/single-file/Note B.md").unwrap()
    };
    assert_eq!(expected, read_to_string(dest).unwrap(),);
}

#[test]
fn test_not_existing_source() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");

    let err = Exporter::new(
        PathBuf::from("tests/testdata/no-such-file.md"),
        tmp_dir.path().to_path_buf(),
    )
    .run()
    .unwrap_err();

    match err {
        ExportError::PathDoesNotExist { .. } => {}
        _ => panic!("Wrong error variant: {:?}", err),
    }
}

#[test]
fn test_not_existing_destination_with_source_dir() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");

    let err = Exporter::new(
        PathBuf::from("tests/testdata/input/main-samples/"),
        tmp_dir.path().to_path_buf().join("does-not-exist"),
    )
    .run()
    .unwrap_err();

    match err {
        ExportError::PathDoesNotExist { .. } => {}
        _ => panic!("Wrong error variant: {:?}", err),
    }
}

#[test]
// This test ensures that when source is a file, but destination points to a
// regular file inside of a non-existent directory, an error is raised instead
// of that directory path being created (like `mkdir -p`)
fn test_not_existing_destination_with_source_file() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");

    let err = Exporter::new(
        PathBuf::from("tests/testdata/input/main-samples/obsidian-wikilinks.md"),
        tmp_dir.path().to_path_buf().join("subdir/does-not-exist"),
    )
    .run()
    .unwrap_err();

    match err {
        ExportError::PathDoesNotExist { .. } => {}
        _ => panic!("Wrong error variant: {:?}", err),
    }
}

#[cfg(not(target_os = "windows"))]
#[test]
fn test_source_no_permissions() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");
    let src = tmp_dir.path().to_path_buf().join("source.md");
    let dest = tmp_dir.path().to_path_buf().join("dest.md");

    let mut file = File::create(&src).unwrap();
    file.write_all(b"Foo").unwrap();
    set_permissions(&src, Permissions::from_mode(0o000)).unwrap();

    match Exporter::new(src, dest).run().unwrap_err() {
        ExportError::FileExportError { source, .. } => match *source {
            ExportError::ReadError { .. } => {}
            _ => panic!("Wrong error variant for source, got: {:?}", source),
        },
        err => panic!("Wrong error variant: {:?}", err),
    }
}

#[cfg(not(target_os = "windows"))]
#[test]
fn test_dest_no_permissions() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");
    let src = tmp_dir.path().to_path_buf().join("source.md");
    let dest = tmp_dir.path().to_path_buf().join("dest");

    let mut file = File::create(&src).unwrap();
    file.write_all(b"Foo").unwrap();

    create_dir(&dest).unwrap();
    set_permissions(&dest, Permissions::from_mode(0o555)).unwrap();

    match Exporter::new(src, dest).run().unwrap_err() {
        ExportError::FileExportError { source, .. } => match *source {
            ExportError::WriteError { .. } => {}
            _ => panic!("Wrong error variant for source, got: {:?}", source),
        },
        err => panic!("Wrong error variant: {:?}", err),
    }
}

#[test]
fn test_infinite_recursion() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");

    let err = Exporter::new(
        PathBuf::from("tests/testdata/input/infinite-recursion/"),
        tmp_dir.path().to_path_buf(),
    )
    .run()
    .unwrap_err();

    match err {
        ExportError::FileExportError { source, .. } => match *source {
            ExportError::RecursionLimitExceeded { .. } => {}
            _ => panic!("Wrong error variant for source, got: {:?}", source),
        },
        err => panic!("Wrong error variant: {:?}", err),
    }
}

#[test]
fn test_no_recursive_embeds() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");

    let mut exporter = Exporter::new(
        PathBuf::from("tests/testdata/input/infinite-recursion/"),
        tmp_dir.path().to_path_buf(),
    );
    exporter.process_embeds_recursively(false);
    exporter.run().expect("exporter returned error");

    assert_eq!(
        read_to_string("tests/testdata/expected/infinite-recursion/Note A.md").unwrap(),
        read_to_string(tmp_dir.path().join(PathBuf::from("Note A.md"))).unwrap(),
    );
}

#[test]
fn test_preserve_mtime() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");

    let mut exporter = Exporter::new(
        PathBuf::from("tests/testdata/input/main-samples/"),
        tmp_dir.path().to_path_buf(),
    );
    exporter.preserve_mtime(true);
    exporter.run().expect("exporter returned error");

    let src = "tests/testdata/input/main-samples/obsidian-wikilinks.md";
    let dest = tmp_dir.path().join(PathBuf::from("obsidian-wikilinks.md"));
    let src_meta = std::fs::metadata(src).unwrap();
    let dest_meta = std::fs::metadata(dest).unwrap();

    assert_eq!(src_meta.modified().unwrap(), dest_meta.modified().unwrap());
}

#[test]
fn test_no_preserve_mtime() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");

    let mut exporter = Exporter::new(
        PathBuf::from("tests/testdata/input/main-samples/"),
        tmp_dir.path().to_path_buf(),
    );
    exporter.preserve_mtime(false);
    exporter.run().expect("exporter returned error");

    let src = "tests/testdata/input/main-samples/obsidian-wikilinks.md";
    let dest = tmp_dir.path().join(PathBuf::from("obsidian-wikilinks.md"));
    let src_meta = std::fs::metadata(src).unwrap();
    let dest_meta = std::fs::metadata(dest).unwrap();

    assert_ne!(src_meta.modified().unwrap(), dest_meta.modified().unwrap());
}

#[test]
fn test_non_ascii_filenames() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");

    Exporter::new(
        PathBuf::from("tests/testdata/input/non-ascii/"),
        tmp_dir.path().to_path_buf(),
    )
    .run()
    .expect("exporter returned error");

    let walker = WalkDir::new("tests/testdata/expected/non-ascii/")
        // Without sorting here, different test runs may trigger the first assertion failure in
        // unpredictable order.
        .sort_by(|a, b| a.file_name().cmp(b.file_name()))
        .into_iter();
    for entry in walker {
        let entry = entry.unwrap();
        if entry.metadata().unwrap().is_dir() {
            continue;
        };
        let filename = entry.file_name().to_string_lossy().into_owned();
        let expected = read_to_string(entry.path()).unwrap_or_else(|_| {
            panic!(
                "failed to read {} from testdata/expected/non-ascii/",
                entry.path().display()
            )
        });
        let actual = read_to_string(tmp_dir.path().join(PathBuf::from(&filename)))
            .unwrap_or_else(|_| panic!("failed to read {} from temporary exportdir", filename));

        assert_eq!(
            expected, actual,
            "{} does not have expected content",
            filename
        );
    }
}

#[test]
fn test_same_filename_different_directories() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");
    Exporter::new(
        PathBuf::from("tests/testdata/input/same-filename-different-directories"),
        tmp_dir.path().to_path_buf(),
    )
    .run()
    .unwrap();

    let expected = if cfg!(windows) {
        read_to_string("tests/testdata/expected/same-filename-different-directories/Note.md")
            .unwrap()
            .replace('/', "\\")
    } else {
        read_to_string("tests/testdata/expected/same-filename-different-directories/Note.md")
            .unwrap()
    };

    let actual = read_to_string(tmp_dir.path().join(PathBuf::from("Note.md"))).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn test_zola_internal_links() {
    let tmp_dir = TempDir::new().expect("failed to make tempdir");
    let input_dir = Path::new("tests/testdata/input/zola-links");
    let expected_dir = Path::new("tests/testdata/expected/zola-links");
    Exporter::new(input_dir.to_owned(), tmp_dir.path().to_owned())
        .internal_link_format(InternalLinkFormat::Zola)
        .run()
        .expect("exporter returned error");
    diff_export_dirs(expected_dir, tmp_dir.path());
}
