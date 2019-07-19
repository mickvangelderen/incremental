use incremental::{Revision, Global};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

type FileIndex = usize;

#[derive(Debug, Clone)]
enum Token {
    Literal(String),
    Include(PathBuf),
}

type File = Vec<Token>;

#[derive(Debug)]
struct Disk {
    files: HashMap<PathBuf, File>,
}

impl Disk {
    fn read(&self, path: impl AsRef<Path>) -> File {
        self.files.get(path.as_ref()).cloned().expect("No such file.")
    }
}

#[derive(Debug)]
struct CachedFile {
    path: PathBuf,
    last_modified: Revision,
    last_computed: Revision,
    tokens: File,
}

impl CachedFile {
    fn update(&mut self, disk: &Disk) {
        if self.last_computed == self.last_modified {
            // File is up-to-date.
        } else {
            debug_assert!(self.last_computed < self.last_modified);
            self.last_computed = self.last_modified;

            // Read file from disk.
            self.tokens = disk.read(&self.path);
        }
    }
}

#[derive(Debug)]
struct Memory {
    path_to_file_index: HashMap<PathBuf, FileIndex>,
    files: Vec<Rc<CachedFile>>,
}

impl Memory {
    fn file_index(&mut self, global: &Global, path: impl AsRef<Path>) -> FileIndex {
        let path = path.as_ref();
        match self.path_to_file_index.get(path) {
            Some(&file_index) => file_index,
            None => {
                let file_index = self.files.len();
                self.files.push(Rc::new(CachedFile {
                    path: PathBuf::from(path),
                    last_modified: global.revision,
                    last_computed: Revision::DIRTY,
                    tokens: Vec::new(),
                }));
                self.path_to_file_index.insert(PathBuf::from(path), file_index);
                file_index
            }
        }
    }
}

#[derive(Debug)]
struct EntryPoint {
    file_index: FileIndex,
    last_verified: Revision,
    last_computed: Revision,
    contents: String,
    included: Vec<FileIndex>,
}

enum Presence {
    Unique,
    Duplicate,
}

fn vec_set_add<T: Copy + PartialEq>(vec: &mut Vec<T>, val: T) -> Presence {
    if vec.iter().find(|&&x| x == val).is_some() {
        Presence::Duplicate
    } else {
        vec.push(val);
        Presence::Unique
    }
}

impl EntryPoint {
    fn update(&mut self, global: &Global, mem: &mut Memory, disk: &Disk) {
        if self.last_verified == global.revision {
            return;
        } else {
            debug_assert!(self.last_verified < global.revision);
            self.last_verified = global.revision;
        }

        let mut should_recompute = false;

        for &include in self.included.iter() {
            let file = &mem.files[include];
            if self.last_computed < file.last_modified {
                should_recompute = true;
                break;
            }
        }

        if should_recompute == false {
            return;
        }

        self.contents.clear();
        self.included.clear();

        process(self, global, mem, disk, self.file_index);

        fn process(ep: &mut EntryPoint, global: &Global, mem: &mut Memory, disk: &Disk, file_index: FileIndex) {
            // Stop processing if we've already included this file.
            if let Presence::Duplicate = vec_set_add(&mut ep.included, file_index) {
                return;
            }

            let file = Rc::get_mut(&mut mem.files[file_index]).unwrap();
            file.update(disk);

            // Clone the file rc so we can access tokens while mutating the tokens vec.
            let file = Rc::clone(&mem.files[file_index]);
            if ep.last_computed < file.last_modified {
                ep.last_computed = file.last_modified;
            }
            for token in file.tokens.iter() {
                match *token {
                    Token::Literal(ref lit) => {
                        ep.contents.push_str(lit);
                    }
                    Token::Include(ref path) => {
                        let file_index = mem.file_index(global, path);
                        process(ep, global, mem, disk, file_index);
                    }
                }
            }
        }
    }
}

fn main() {
    let mut disk = Disk {
        files: vec![
            (
                PathBuf::from("a.txt"),
                vec![
                    Token::Literal("a.txt:1\n".to_string()),
                    Token::Include(PathBuf::from("b.txt")),
                    Token::Literal("a.txt:3\n".to_string()),
                    Token::Include(PathBuf::from("c.txt")),
                    Token::Literal("a.txt:5\n".to_string()),
                ],
            ),
            (
                PathBuf::from("b.txt"),
                vec![
                    Token::Literal("b.txt:1\nb.txt:2\n".to_string()),
                    Token::Include(PathBuf::from("c.txt")),
                ],
            ),
            (
                PathBuf::from("c.txt"),
                vec![
                    Token::Include(PathBuf::from("a.txt")),
                    Token::Literal("c.txt:1\nc.txt:2\n".to_string()),
                ],
            ),
        ]
        .into_iter()
        .collect(),
    };

    let mut global = Global::new();

    let mut mem = Memory {
        path_to_file_index: HashMap::new(),
        files: Vec::new(),
    };

    let mut entry = {
        let file_index = mem.file_index(&global, "a.txt");
        EntryPoint {
            file_index,
            last_verified: Revision::DIRTY,
            last_computed: Revision::DIRTY,
            contents: String::new(),
            included: vec![file_index],
        }
    };

    entry.update(&global, &mut mem, &disk);

    println!("{:#?}", &mem);
    println!("{}", &entry.contents);
    println!("{:?}", &entry.included);

    // Simulate some IO, a new file d.txt is added and a.txt is changed.
    disk.files
        .insert(PathBuf::from("d.txt"), vec![Token::Literal("d.txt:1\n".to_string())]);

    std::mem::replace(
        disk.files.get_mut(Path::new("a.txt")).unwrap(),
        vec![
            Token::Literal("a.txt:1\n".to_string()),
            Token::Include(PathBuf::from("d.txt")),
        ],
    );

    global.revision.0 += 1;
    Rc::get_mut(&mut mem.files[entry.file_index]).unwrap().last_modified = global.revision;

    entry.update(&global, &mut mem, &disk);

    println!("{:#?}", &mem);
    println!("{}", &entry.contents);
    println!("{:?}", &entry.included);
}
