//! This is more of a philosophy than a library

pub trait Dependee {
    fn revision(&self) -> Revision;
}

#[derive(Debug)]
pub struct Current(Revision);

impl Current {
    pub fn new() -> Self {
        Self(Revision::INITIAL_CURRENT)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Revision(u64);

impl Revision {
    const DIRTY: Revision = Revision(0);
    const INITIAL_CURRENT: Revision = Revision(1);
}

#[derive(Debug)]
pub struct LastVerified(Revision);

impl LastVerified {
    pub fn clean(current: &Current) -> Self {
        Self(current.0)
    }

    pub fn dirty() -> Self {
        Self(Revision::DIRTY)
    }

    pub fn should_verify(&self, current: &Current) -> bool {
        self.0 < current.0
    }

    pub fn update_to(&mut self, current: &Current) {
        debug_assert!(self.0 < current.0);
        self.0 = current.0;
    }

    pub fn verify_with(&mut self, current: &Current, f: impl FnOnce()) {
        if self.should_verify(&current) {
            self.update_to(&current);
            f()
        }
    }
}

#[derive(Debug)]
pub struct LastModified(Revision);

impl LastModified {
    pub fn new(current: &Current) -> Self {
        Self(current.0)
    }

    pub fn modify(&mut self, current: &mut Current) {
        (current.0).0 += 1;
        self.0 = current.0;
    }
}

impl Dependee for LastModified {
    fn revision(&self) -> Revision {
        self.0
    }
}

#[derive(Debug)]
pub struct LastComputed(Revision);

impl LastComputed {
    pub fn clean(current: &Current) -> Self {
        Self(current.0)
    }

    pub fn dirty() -> Self {
        Self(Revision::DIRTY)
    }

    pub fn should_compute(&self, dependee: &impl Dependee) -> bool {
        self.0 < dependee.revision()
    }

    pub fn update_to(&mut self, dependee: &impl Dependee) {
        let revision = dependee.revision();
        if self.0 < revision {
            self.0 = revision
        }
    }
}

impl Dependee for LastComputed {
    fn revision(&self) -> Revision {
        self.0
    }
}
