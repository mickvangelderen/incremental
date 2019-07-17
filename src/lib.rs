pub mod ic {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
    pub struct Revision(u64);

    #[derive(Debug)]
    pub struct Graph {
        pub revision: Revision,
    }

    impl Graph {
        pub fn new() -> Self {
            Self {
                revision: Revision(0),
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
                self.inc();
                leaf.last_modified = self.revision;
                std::mem::replace(&mut leaf.value, value)
            }
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
                inner: std::cell::RefCell::new(BranchInner {
                    value,
                    last_verified: self.revision,
                    last_modified: self.revision,
                }),
            }
        }
    }

    pub struct Leaf<T> {
        pub value: T,
        pub last_modified: Revision,
    }

    impl<T> Leaf<T> {
        pub fn read(&self, mut token: impl Token) -> &T {
            token.update(self.last_modified);
            &self.value
        }
    }

    struct BranchInner<T> {
        pub value: T,
        pub last_modified: Revision,
        pub last_verified: Revision,
    }

    pub struct Branch<T> {
        inner: std::cell::RefCell<BranchInner<T>>,
    }

    impl<T> Branch<T> {
        pub fn verify<'a>(
            &'a self,
            graph: &'a Graph,
            mut token: impl Token,
            f: impl FnOnce(ParentToken<T>),
        ) -> BranchRef<'a, T> {
            match self.inner.try_borrow_mut() {
                Ok(mut borrow) => {
                    if borrow.last_verified < graph.revision {
                        borrow.last_verified = graph.revision;

                        f(ParentToken {
                            last_modified: borrow.last_modified,
                            borrow,
                        })
                    }
                }
                Err(_) => {
                    // Branch has already been borrowed which means that it must
                    // also have already been verified for the current graph.
                }
            }
            let borrow = self
                .inner
                .try_borrow()
                .expect("Cycle detected in dependency graph!");

            token.update(borrow.last_modified);

            BranchRef(std::cell::Ref::map(borrow, |branch| &branch.value))
        }
    }

    pub struct BranchRef<'a, T>(std::cell::Ref<'a, T>);

    impl<T> std::ops::Deref for BranchRef<'_, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    pub trait Token {
        fn update(&mut self, revision: Revision);
    }

    pub struct RootToken;

    impl Token for RootToken {
        fn update(&mut self, _revision: Revision) {
            // Do nothing.
        }
    }

    pub struct ParentToken<'a, T> {
        last_modified: Revision,
        borrow: std::cell::RefMut<'a, BranchInner<T>>,
    }

    impl<'a, T> ParentToken<'a, T> {
        pub fn compute(mut self, f: impl FnOnce(&mut T)) {
            if self.borrow.last_modified < self.last_modified {
                self.borrow.last_modified = self.last_modified;

                f(&mut self.borrow.value);
            }
        }
    }

    impl<'a, T> Token for &mut ParentToken<'a, T> {
        fn update(&mut self, revision: Revision) {
            if self.last_modified < revision {
                self.last_modified = revision
            }
        }
    }

    impl<'a, T> Drop for ParentToken<'a, T> {
        fn drop(&mut self) {
            if !std::thread::panicking() {
                if self.borrow.last_modified < self.last_modified {
                    panic!("Forgot to recompute!");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ic::{Branch, BranchRef, Graph, RootToken, Leaf, Token};

    #[test]
    fn main() {
        #[derive(Debug, Copy, Clone, Eq, PartialEq)]
        enum ABC {
            A,
            B,
            C,
            SumAB,
        }

        struct E {
            graph: Graph,
            a: Leaf<u32>,
            b: Leaf<u32>,
            c: Leaf<u32>,
            sum_a_b: Branch<u32>,
            mul_c_sum_a_b: Branch<u32>,
            lhs: Leaf<ABC>,
            rhs: Leaf<ABC>,
            sum_dynamic: Branch<u32>,
        }

        impl E {
            fn sum_a_b(&self, token: impl Token) -> BranchRef<u32> {
                self.sum_a_b.verify(&self.graph, token, |mut token| {
                    let a = *self.a.read(&mut token);
                    let b = *self.b.read(&mut token);
                    token.compute(|value| {
                        *value = a + b;
                    });
                })
            }

            fn mul_c_sum_a_b(&self, token: impl Token) -> BranchRef<u32> {
                self.mul_c_sum_a_b.verify(&self.graph, token, |mut token| {
                    let c = *self.c.read(&mut token);
                    let sum_a_b = *self.sum_a_b(&mut token);
                    token.compute(|value| {
                        *value = c * sum_a_b;
                    })
                })
            }

            fn sum_dynamic(&self, token: impl Token) -> BranchRef<u32> {
                self.sum_dynamic.verify(&self.graph, token, |mut token| {
                    let lhs = match *self.lhs.read(&mut token) {
                        ABC::A => *self.a.read(&mut token),
                        ABC::B => *self.b.read(&mut token),
                        ABC::C => *self.c.read(&mut token),
                        ABC::SumAB => *self.sum_a_b(&mut token),
                    };

                    let rhs = match *self.rhs.read(&mut token) {
                        ABC::A => *self.a.read(&mut token),
                        ABC::B => *self.b.read(&mut token),
                        ABC::C => *self.c.read(&mut token),
                        ABC::SumAB => *self.sum_a_b(&mut token),
                    };

                    token.compute(|value| *value = lhs + rhs)
                })
            }
        }

        let mut e = {
            let graph = Graph::new();
            E {
                a: graph.leaf(1),
                b: graph.leaf(2),
                c: graph.leaf(3),
                sum_a_b: graph.branch(1 + 2),
                mul_c_sum_a_b: graph.branch(3 * (1 + 2)),
                lhs: graph.leaf(ABC::A),
                rhs: graph.leaf(ABC::B),
                sum_dynamic: graph.branch(1 + 2),
                graph,
            }
        };

        // a = 1
        // b = 2
        // c = 3
        assert_eq!(3, *e.sum_a_b(RootToken));

        e.graph.replace(&mut e.b, 6);

        // a = 1
        // b = 6
        // c = 3
        assert_eq!(7, *e.sum_a_b(RootToken));
        assert_eq!(21, *e.mul_c_sum_a_b(RootToken));

        e.graph.replace(&mut e.lhs, ABC::C);

        // a = 1
        // b = 6
        // c = 3
        assert_eq!(9, *e.sum_dynamic(RootToken));

        e.graph.replace(&mut e.rhs, ABC::SumAB);

        assert_eq!(10, *e.sum_dynamic(RootToken));

        e.graph.replace(&mut e.a, 20);

        assert_eq!(29, *e.sum_dynamic(RootToken));
    }

    #[test]
    #[should_panic(expected = "Cycle detected in dependency graph!")]
    fn panic_if_dependency_graph_contains_a_cycle() {
        struct E {
            ignite: Leaf<u32>,
            a: Branch<u32>,
            b: Branch<u32>,
            graph: Graph,
        }

        impl E {
            fn a(&self, token: impl Token) -> BranchRef<u32> {
                self.a.verify(&self.graph, token, |mut token| {
                    let ignite = *self.ignite.read(&mut token);
                    let b = *self.b(&mut token);
                    token.compute(|value| {
                        *value = ignite + b;
                    });
                })
            }

            fn b(&self, token: impl Token) -> BranchRef<u32> {
                self.b.verify(&self.graph, token, |mut token| {
                    let a = *self.a(&mut token);
                    token.compute(|value| {
                        *value = a + 1;
                    });
                })
            }
        }

        let mut e = {
            let graph = Graph::new();
            E {
                ignite: graph.leaf(0),
                a: graph.branch(0),
                b: graph.branch(0),
                graph
            }
        };

        e.graph.replace(&mut e.ignite, 1);

        let _ = e.a(RootToken);
    }
}
