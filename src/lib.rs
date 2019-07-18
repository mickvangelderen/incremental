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

use std::cell::{Ref, RefCell, RefMut};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Revision(u64);

/// A Graph represents a network of values which are lazily recomputed.
/// Graph itself only keeps track of its global revision.
///
/// It is on you to ensure leaves and branches are only used with the graph
/// they were created in. If you are creating more than one but a fixed
/// number of graphs it might be worthwile to add a zero sized type
/// parameter to keep the graphs apart. This isn't part of the core library
/// but might be some day.

#[derive(Debug)]
pub struct Graph {
    revision: Revision,
}

impl Graph {
    pub fn new() -> Self {
        Self { revision: Revision(0) }
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

    #[inline]
    pub fn branch<T>(&self, value: T) -> Branch<T> {
        Branch {
            inner: RefCell::new(BranchInner {
                value,
                last_verified: self.revision,
                last_modified: self.revision,
            }),
        }
    }
}

/// A leaf represents an input. It can be directly modified. Doing so will
/// increment the revision of the graph, ensuring branches will recompute
/// their values when queried if necessary.
#[derive(Debug)]
pub struct Leaf<T> {
    value: T,
    last_modified: Revision,
}

impl<T> Leaf<T> {
    pub fn read(&self, token: &mut impl Token) -> &T {
        token.update(self.last_modified);
        &self.value
    }
}

#[derive(Debug)]
struct BranchInner<T> {
    value: T,
    last_modified: Revision,
    last_verified: Revision,
}

/// A branch represents a lazily computed cached value. Usually a function
/// will be dedicated to computing the cased value. This function should
/// call `verify`.
#[derive(Debug)]
pub struct Branch<T> {
    inner: RefCell<BranchInner<T>>,
}

impl<T> Branch<T> {
    /// The passed closure will be called recomputation is required. A
    /// branch keeps track of the last time verify was called so that we can
    /// stop recomputation early when the graph has not been updated at all.
    ///
    /// If the graph has been updated since the last verification, the
    /// passed closure will be called. The closure receives a `ParentToken`
    /// that should be used to obtain depended-upon Leaf or Branch
    /// references.
    ///
    /// After obtaining all dependencies, `compute` must be called on the
    /// token.
    pub fn verify<'a>(
        &'a self,
        graph: &'a Graph,
        token: &mut impl Token,
        f: impl FnOnce(&mut ParentToken<T>),
    ) -> Ref<'a, T> {
        match self.inner.try_borrow_mut() {
            Ok(mut borrow) => {
                if borrow.last_verified < graph.revision {
                    borrow.last_verified = graph.revision;

                    let mut token = ParentToken {
                        last_modified: borrow.last_modified,
                        borrow,
                    };

                    f(&mut token);

                    debug_assert!(
                        token.borrow.last_modified == token.last_modified,
                        "Forgot to call compute!"
                    );
                }
            }
            Err(_) => {
                // Branch has already been borrowed which means that it must
                // also have already been verified for the current graph.
            }
        }
        let borrow = self.inner.try_borrow().expect("Cycle detected in dependency graph!");

        token.update(borrow.last_modified);

        Ref::map(borrow, |&BranchInner { ref value, .. }| value)
    }
}

/// Abstracts the tracking of the latest last_modified value of all
/// dependencies.
pub trait Token {
    fn update(&mut self, revision: Revision);
}

/// Graph queries need to start somewhere. Do not use this inside
/// incremental computations. The verify function provides you with a
/// ParentToken.
#[derive(Debug)]
pub struct RootToken;

impl Token for RootToken {
    fn update(&mut self, _revision: Revision) {
        // Do nothing.
    }
}

/// Tracks the latest last_modified value of all dependencies. After
/// obtaining references to all dependencies, the compute function must be
/// called.
#[derive(Debug)]
pub struct ParentToken<'a, T> {
    last_modified: Revision,
    borrow: RefMut<'a, BranchInner<T>>,
}

impl<'a, T> ParentToken<'a, T> {
    /// Will execute the passed closure when one of the dependencies has
    /// been modified since the last execution.
    pub fn compute(&mut self, f: impl FnOnce(&mut T)) {
        if self.borrow.last_modified < self.last_modified {
            self.borrow.last_modified = self.last_modified;

            f(&mut self.borrow.value);
        }
    }
}

impl<T> Token for ParentToken<'_, T> {
    fn update(&mut self, revision: Revision) {
        if self.last_modified < revision {
            self.last_modified = revision
        }
    }
}
