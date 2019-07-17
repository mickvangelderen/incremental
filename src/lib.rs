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
        pub fn as_current<'a>(&'a self) -> Current<'a, T> {
            Current {
                reference: Ref::Leaf(&self.value),
                last_modified: self.last_modified,
            }
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
            f: impl FnOnce(&mut T, &mut Revision),
        ) -> Current<'a, T> {
            match self.inner.try_borrow_mut() {
                Ok(mut borrow) => {
                    if borrow.last_verified < graph.revision {
                        borrow.last_verified = graph.revision;

                        let (mut value, mut last_modified) =
                            std::cell::RefMut::map_split(borrow, |borrow| {
                                (&mut borrow.value, &mut borrow.last_modified)
                            });
                        f(&mut value, &mut last_modified);
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
                .expect("Acyclic dependency graph detected!");
            Current {
                last_modified: borrow.last_modified,
                reference: Ref::Branch(std::cell::Ref::map(borrow, |branch| &branch.value)),
            }
        }
    }

    enum Ref<'a, T>
    where
        T: ?Sized,
    {
        Leaf(&'a T),
        Branch(std::cell::Ref<'a, T>),
    }

    pub struct Current<'a, T>
    where
        T: ?Sized,
    {
        reference: Ref<'a, T>,
        last_modified: Revision,
    }

    impl<'a, T> Current<'a, T> {
        pub fn last_modified(&self) -> Revision {
            self.last_modified
        }
    }

    impl<T: ?Sized> std::ops::Deref for Current<'_, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            match self.reference {
                Ref::Leaf(leaf) => leaf,
                Ref::Branch(ref branch) => &*branch,
            }
        }
    }

    // impl<T, G: Graph> Branch<T, G> {
    //     pub fn new(cached: T, graph: G) -> Self {
    //         Self {
    //             cached,
    //             last_modified: graph.revision(),
    //             last_verified: graph.revision(),
    //             graph,
    //         }
    //     }

    //     pub fn compute(
    //         &mut self,
    //         compute_dependencies: impl FnOnce() -> Revision,
    //         compute_self: impl FnOnce(&mut T),
    //     ) -> Revision {
    //         let revision = self.graph.revision();
    //         if self.last_verified < revision {
    //             self.last_verified = revision;

    //             let last_modified = compute_dependencies();

    //             if self.last_modified < last_modified {
    //                 self.last_modified = last_modified;

    //                 compute_self(&mut self.cached)
    //             }
    //         }
    //         self.last_modified
    //     }
    // }
}

#[cfg(test)]
mod tests {
    use super::ic::{Branch, Current, Graph, Leaf};

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
            fn sum_a_b(&self) -> Current<u32> {
                self.sum_a_b.verify(&self.graph, |value, last_modified| {
                    let a = self.a.as_current();
                    let b = self.b.as_current();
                    let revision = std::cmp::max(a.last_modified(), b.last_modified());
                    if *last_modified < revision {
                        *last_modified = revision;
                        *value = *a + *b;
                    }
                })
            }

            fn mul_c_sum_a_b(&self) -> Current<u32> {
                self.mul_c_sum_a_b
                    .verify(&self.graph, |value, last_modified| {
                        let c = self.c.as_current();
                        let sum_a_b = self.sum_a_b();
                        let revision = std::cmp::max(c.last_modified(), sum_a_b.last_modified());
                        if *last_modified < revision {
                            *last_modified = revision;
                            *value = *c * *sum_a_b;
                        }
                    })
            }

            fn sum_dynamic(&self) -> Current<u32> {
                self.sum_dynamic
                    .verify(&self.graph, |value, last_modified| {
                        let lhs_sel = self.lhs.as_current();
                        let lhs_val = match *lhs_sel {
                            ABC::A => self.a.as_current(),
                            ABC::B => self.b.as_current(),
                            ABC::C => self.c.as_current(),
                            ABC::SumAB => self.sum_a_b(),
                        };
                        let rhs_sel = self.rhs.as_current();
                        let rhs_val = match *rhs_sel {
                            ABC::A => self.a.as_current(),
                            ABC::B => self.b.as_current(),
                            ABC::C => self.c.as_current(),
                            ABC::SumAB => self.sum_a_b(),
                        };

                        let revision = std::cmp::max(
                            std::cmp::max(lhs_sel.last_modified(), lhs_val.last_modified()),
                            std::cmp::max(rhs_sel.last_modified(), rhs_val.last_modified()),
                        );

                        if *last_modified < revision {
                            *last_modified = revision;
                            *value = *lhs_val + *rhs_val;
                        }
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
        assert_eq!(3, *e.sum_a_b());

        e.graph.replace(&mut e.b, 6);

        // a = 1
        // b = 6
        // c = 3
        assert_eq!(7, *e.sum_a_b());
        assert_eq!(21, *e.mul_c_sum_a_b());

        e.graph.replace(&mut e.lhs, ABC::C);

        // a = 1
        // b = 6
        // c = 3
        assert_eq!(9, *e.sum_dynamic());

        e.graph.replace(&mut e.rhs, ABC::SumAB);

        assert_eq!(10, *e.sum_dynamic());

        e.graph.replace(&mut e.a, 20);

        assert_eq!(29, *e.sum_dynamic());
    }
}
