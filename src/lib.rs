//! This minimalistic crate provides some tools to facilitate implementing
//! single threaded incremental computation. There are other projects like salsa
//! and adapton which might better suit your needs.
//!
//! I had a need for something simple in my OpenGL application where I wanted to
//! recompile shaders on the fly whenever files or configuration changes.
//!
//! To do incremental computation you'll likely create a struct to hold your
//! graph. On this struct you will implement methods that query values from the
//! graph and recompute them if needed. It is best to check out the tests to see
//! how this library can help you achieve that ergonomically.
//!
//! My initial designs did not use RefCell. It is possible to implement things
//! by separating the updating pass and the reference obtaining pass. The
//! resulting code is easy to mess up because of this duplication. On the
//! reference obtaining pass we lose some performance because we are checking if
//! the values are all actually up-to-date to prevent programming mistakes. We
//! would also have to find a way to get element level borrow checking when
//! using collections. You're now paying heavily in ergonomics and it will
//! become tempting to work around the incremental computation system. With
//! these realizations I finally felt comfortable resorting to run-time borrow
//! checking by integrating RefCell.

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Revision(pub u64);

impl Revision {
    pub const DIRTY: Revision = Revision(0);
}

const INITIAL_GRAPH: Revision = Revision(1);

/// A Global represents a network of values which are lazily recomputed.
/// Global itself only keeps track of its global revision.
///
/// It is on you to ensure leaves and branches are only used with the graph
/// they were created in. If you are creating more than one but a fixed
/// number of graphs it might be worthwile to add a zero sized type
/// parameter to keep the graphs apart. This isn't part of the core library
/// but might be some day.

#[derive(Debug)]
pub struct Global {
    pub revision: Revision,
}

impl Global {
    pub fn new() -> Self {
        Self {
            revision: INITIAL_GRAPH,
        }
    }

    fn inc(&mut self) {
        self.revision.0 += 1;
    }

    #[inline]
    pub fn replace<T: PartialEq>(&mut self, leaf: &mut Leaf<T>, value: T) -> T {
        if value == leaf.value {
            value
        } else {
            self.replace_always(leaf, value)
        }
    }

    #[inline]
    pub fn replace_always<T>(&mut self, leaf: &mut Leaf<T>, value: T) -> T {
        self.inc();
        leaf.last_modified = self.revision;
        std::mem::replace(&mut leaf.value, value)
    }

    #[inline]
    pub fn leaf<T>(&self, value: T) -> Leaf<T> {
        Leaf {
            value,
            last_modified: self.revision,
        }
    }

    /// A dirty branch has a last_verified and last_modified that is guaranteed
    /// to be before any leaf's last_modified, causing it to always recompute on
    /// the first access.
    #[inline]
    pub fn branch<T>(&self, value: T) -> Branch<T> {
        Branch {
            value,
            last_verified: Revision::DIRTY,
            last_computed: Revision::DIRTY,
        }
    }

    /// A clean branch will not recompute until dependencies are modified.
    #[inline]
    pub fn clean_branch<T>(&self, value: T) -> Branch<T> {
        Branch {
            value,
            last_verified: self.revision,
            last_computed: self.revision,
        }
    }

    #[inline]
    pub fn verify<'a, T, F>(&self, branch: &'a mut Branch<T>, f: F) where F: FnOnce(&mut Parent<'a, T>) {
        if branch.last_verified < self.revision {
            branch.last_verified = self.revision;

            let mut parent = Parent {
                revision: branch.last_computed,
                branch
            };

            f(&mut parent)
        }
    }
}

/// A leaf represents an input. It can be directly modified. Doing so will
/// increment the revision of the graph, ensuring branches will recompute
/// their values when queried if necessary.
#[derive(Debug)]
pub struct Leaf<T> {
    pub value: T,
    pub last_modified: Revision,
}

#[derive(Debug)]
pub struct Branch<T> {
    pub value: T,
    pub last_verified: Revision,
    pub last_computed: Revision,
}

#[derive(Debug)]
pub struct Parent<'a, T> {
    pub revision: Revision,
    pub branch: &'a mut Branch<T>,
}

impl<'a, T> Parent<'a, T> {
    pub fn read<'l, U>(&mut self, leaf: &'l Leaf<U>) -> &'l U {
        if self.revision < leaf.last_modified {
            self.revision = leaf.last_modified
        }
        &leaf.value
    }

    /// Will execute the passed closure when one of the dependencies has
    /// been modified since the last execution.
    pub fn compute(&mut self, f: impl FnOnce(&mut T)) {
        if self.branch.last_computed < self.revision {
            self.branch.last_computed = self.revision;

            f(&mut self.branch.value)
        }
    }
}
