use incremental::{Branch, BranchRef, Graph, Leaf, RootToken, Token};

#[test]
fn work_when_we_do_a_bunch_of_things() {
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
        fn sum_a_b(&self, token: &mut impl Token) -> BranchRef<u32> {
            self.sum_a_b.verify(&self.graph, token, |token| {
                let a = *self.a.read(token);
                let b = *self.b.read(token);
                token.compute(|value| {
                    *value = a + b;
                })
            })
        }

        fn mul_c_sum_a_b(&self, token: &mut impl Token) -> BranchRef<u32> {
            self.mul_c_sum_a_b.verify(&self.graph, token, |token| {
                let c = *self.c.read(token);
                let sum_a_b = *self.sum_a_b(token);
                token.compute(|value| {
                    *value = c * sum_a_b;
                })
            })
        }

        fn sum_dynamic(&self, token: &mut impl Token) -> BranchRef<u32> {
            self.sum_dynamic.verify(&self.graph, token, |token| {
                let lhs = match *self.lhs.read(token) {
                    ABC::A => *self.a.read(token),
                    ABC::B => *self.b.read(token),
                    ABC::C => *self.c.read(token),
                    ABC::SumAB => *self.sum_a_b(token),
                };

                let rhs = match *self.rhs.read(token) {
                    ABC::A => *self.a.read(token),
                    ABC::B => *self.b.read(token),
                    ABC::C => *self.c.read(token),
                    ABC::SumAB => *self.sum_a_b(token),
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
    assert_eq!(3, *e.sum_a_b(&mut RootToken));

    e.graph.replace(&mut e.b, 6);

    // a = 1
    // b = 6
    // c = 3
    assert_eq!(7, *e.sum_a_b(&mut RootToken));
    assert_eq!(21, *e.mul_c_sum_a_b(&mut RootToken));

    e.graph.replace(&mut e.lhs, ABC::C);

    // a = 1
    // b = 6
    // c = 3
    assert_eq!(9, *e.sum_dynamic(&mut RootToken));

    e.graph.replace(&mut e.rhs, ABC::SumAB);

    assert_eq!(10, *e.sum_dynamic(&mut RootToken));

    e.graph.replace(&mut e.a, 20);

    assert_eq!(29, *e.sum_dynamic(&mut RootToken));
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
        fn a(&self, token: &mut impl Token) -> BranchRef<u32> {
            self.a.verify(&self.graph, token, |token| {
                let ignite = *self.ignite.read(token);
                let b = *self.b(token);
                token.compute(|value: &mut u32| {
                    *value = ignite + b;
                })
            })
        }

        fn b(&self, token: &mut impl Token) -> BranchRef<u32> {
            self.b.verify(&self.graph, token, |token| {
                let a = *self.a(token);
                token.compute(|value: &mut u32| {
                    *value = a + 1;
                })
            })
        }
    }

    let mut e = {
        let graph = Graph::new();
        E {
            ignite: graph.leaf(0),
            a: graph.branch(0),
            b: graph.branch(0),
            graph,
        }
    };

    e.graph.replace(&mut e.ignite, 1);

    let _ = e.a(&mut RootToken);
}

#[cfg(debug_assertions)]
mod debug {
    use super::*;

    #[test]
    #[should_panic(expected = "Forgot to call compute!")]
    fn panic_when_we_forget_to_call_compute() {
        struct E {
            ignite: Leaf<u32>,
            a: Branch<u32>,
            graph: Graph,
        }

        impl E {
            fn a(&self, token: &mut impl Token) -> BranchRef<u32> {
                self.a.verify(&self.graph, token, |token| {
                    let _ignite = *self.ignite.read(token);
                    // token.compute omitted intentionally
                })
            }
        }

        let mut e = {
            let graph = Graph::new();
            E {
                ignite: graph.leaf(0),
                a: graph.branch(0),
                graph,
            }
        };

        e.graph.replace(&mut e.ignite, 1);

        let _ = e.a(&mut RootToken);
    }
}
