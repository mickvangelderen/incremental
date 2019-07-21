use incremental::{Global, Revision};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

type SourceIndex = usize;

#[derive(Debug, Clone)]
enum Token {
    Literal(String),
    Include(PathBuf),
}

type Tokens = Vec<Token>;

#[derive(Debug)]
struct Disk {
    files: HashMap<PathBuf, Tokens>,
}

impl Disk {
    fn read(&self, path: impl AsRef<Path>) -> Tokens {
        self.files.get(path.as_ref()).cloned().expect("No such file.")
    }
}

#[derive(Debug)]
struct Variables {
    attenuation_mode: u32,
    render_technique: u32,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
enum SourceReader {
    File(PathBuf),
    AttenuationMode,
    RenderTechnique,
}

impl SourceReader {
    fn read(&self, tokens: &mut Tokens, disk: &Disk, vars: &Variables) {
        match *self {
            SourceReader::File(ref path) => {
                *tokens = disk.read(path);
            },
            SourceReader::AttenuationMode => {
                *tokens = vec![
                    Token::Literal(format!("#define ATTENUATION_MODE {}\n", vars.attenuation_mode)),
                ];
            }
            SourceReader::RenderTechnique => {
                *tokens = vec![
                    Token::Literal(format!("#define RENDER_TECHNIQUE {}\n", vars.render_technique)),
                ];
            }
        }
    }
}

#[derive(Debug)]
struct Source {
    reader: SourceReader,
    last_modified: Revision,
    last_computed: Revision,
    tokens: Tokens,
}

impl Source {
    fn update(&mut self, disk: &Disk, vars: &Variables) {
        if self.last_computed == self.last_modified {
            println!("No need to update {:?}, file has not been modified.", self);
            // Tokens is up-to-date.
        } else {
            debug_assert!(self.last_computed < self.last_modified);
            self.last_computed = self.last_modified;

            self.reader.read(&mut self.tokens, disk, vars);

            println!("Updated {:?}.", self);
        }
    }

    fn tokens(&self, parent: &mut Revision) -> &Tokens {
        assert_eq!(self.last_computed, self.last_modified);

        if *parent < self.last_computed {
            *parent = self.last_computed;
        }

        &self.tokens
    }
}

#[derive(Debug)]
struct Memory {
    path_to_file_index: HashMap<PathBuf, SourceIndex>,
    files: Vec<Rc<Source>>,
}

impl Memory {
    fn file_index(&mut self, global: &Global, path: impl AsRef<Path>) -> SourceIndex {
        let path = path.as_ref();
        match self.path_to_file_index.get(path) {
            Some(&file_index) => file_index,
            None => {
                let file_index = self.files.len();
                self.files.push(Rc::new(Source {
                    reader: SourceReader::File(PathBuf::from(path)),
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
    file_index: SourceIndex,
    last_verified: Revision,
    last_computed: Revision,
    contents: String,
    included: Vec<SourceIndex>,
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
    fn update(&mut self, global: &Global, mem: &mut Memory, disk: &Disk, vars: &Variables) {
        if self.last_verified == global.revision {
            println!("No need to update {:?}, already verified.", self);
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
            println!("No need to update {:?}, all dependencies up-to-date.", self);
            return;
        }

        self.contents.clear();
        self.included.clear();

        process(self, global, mem, disk, vars, self.file_index);

        fn process(ep: &mut EntryPoint, global: &Global, mem: &mut Memory, disk: &Disk, vars: &Variables, file_index: SourceIndex) {
            // Stop processing if we've already included this file.
            if let Presence::Duplicate = vec_set_add(&mut ep.included, file_index) {
                return;
            }

            let file = Rc::get_mut(&mut mem.files[file_index]).unwrap();
            file.update(disk, vars);

            // Clone the file rc so we can access tokens while mutating the tokens vec.
            let file = Rc::clone(&mem.files[file_index]);

            for token in file.tokens(&mut ep.last_computed).iter() {
                match *token {
                    Token::Literal(ref lit) => {
                        ep.contents.push_str(lit);
                    }
                    Token::Include(ref path) => {
                        let file_index = mem.file_index(global, path);
                        process(ep, global, mem, disk, vars, file_index);
                    }
                }
            }
        }

        println!("Updated {:?}.", self);
    }
}

fn main() {
    let attenuation_mode_path = PathBuf::from("native/ATTENUATION_MODE.glsl");
    let render_technique_path = PathBuf::from("native/RENDER_TECHNIQUE.glsl");

    let mut disk = Disk {
        files: vec![
            (
                PathBuf::from("a.txt"),
                vec![
                    Token::Include(attenuation_mode_path.clone()),
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

    let vars = Variables {
        attenuation_mode: 1,
        render_technique: 6,
    };

    let attenuation_mode_index = mem.files.len();
    mem.files.push(Rc::new(Source {
        reader: SourceReader::AttenuationMode,
        last_modified: global.revision,
        last_computed: Revision::DIRTY,
        tokens: Vec::new(),
    }));
    mem.path_to_file_index.insert(attenuation_mode_path, attenuation_mode_index);

    let render_technique_index = mem.files.len();
    mem.files.push(Rc::new(Source {
        reader: SourceReader::RenderTechnique,
        last_modified: global.revision,
        last_computed: Revision::DIRTY,
        tokens: Vec::new(),
    }));
    mem.path_to_file_index.insert(render_technique_path, render_technique_index);

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

    entry.update(&global, &mut mem, &disk, &vars);
    entry.update(&global, &mut mem, &disk, &vars);
    global.revision.0 += 1;
    entry.update(&global, &mut mem, &disk, &vars);

    println!("{}", &entry.contents);

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

    entry.update(&global, &mut mem, &disk, &vars);

    println!("{}", &entry.contents);
}
