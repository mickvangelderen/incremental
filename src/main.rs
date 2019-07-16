#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
struct Revision(u64);

#[derive(Debug)]
struct Graph {
    revision: Revision,
}

impl Graph {
    fn new() -> Self {
        Self {
            revision: Revision(0),
        }
    }

    fn leaf<T>(&self, value: T) -> Leaf<T> {
        Leaf {
            value,
            last_modified: self.revision,
        }
    }

    fn branch<T>(&self, cached: T) -> Branch<T> {
        Branch {
            cached,
            last_modified: self.revision,
            last_verified: self.revision,
        }
    }

    fn modify<T>(&mut self, leaf: &mut Leaf<T>, value: T)
    where
        T: Copy + PartialEq,
    {
        if leaf.value != value {
            self.revision.0 += 1;
            leaf.last_modified = self.revision;
            leaf.value = value;
        }
    }
}

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
        sum_a_b_computation_count: u32,
        mul_c_sum_a_b: Branch<u32>,
        mul_c_sum_a_b_computation_count: u32,
        lhs: Leaf<ABC>,
        rhs: Leaf<ABC>,
        sum_dynamic: Branch<u32>,
        sum_dynamic_computation_count: u32,
    }

    impl E {
        fn sum_a_b(&mut self) -> Value<u32> {
            if self.sum_a_b.last_verified < self.graph.revision {
                self.sum_a_b.last_verified = self.graph.revision;

                let last_modified = std::cmp::max(self.a.last_modified, self.b.last_modified);

                if self.sum_a_b.last_modified < last_modified {
                    self.sum_a_b.last_modified = last_modified;

                    self.sum_a_b_computation_count += 1;

                    self.sum_a_b.cached = self.a.value + self.b.value;
                }
            }
            Value {
                value: self.sum_a_b.cached,
                last_modified: self.sum_a_b.last_modified,
            }
        }

        fn mul_c_sum_a_b(&mut self) -> Value<u32> {
            if self.mul_c_sum_a_b.last_verified < self.graph.revision {
                self.mul_c_sum_a_b.last_verified = self.graph.revision;

                let sum_a_b = self.sum_a_b();

                let last_modified = std::cmp::max(self.c.last_modified, sum_a_b.last_modified);

                if self.mul_c_sum_a_b.last_modified < last_modified {
                    self.mul_c_sum_a_b.last_modified = last_modified;

                    self.mul_c_sum_a_b_computation_count += 1;

                    self.mul_c_sum_a_b.cached = self.c.value * sum_a_b.value;
                }
            }
            Value {
                value: self.mul_c_sum_a_b.cached,
                last_modified: self.mul_c_sum_a_b.last_modified,
            }
        }

        fn sum_dynamic(&mut self) -> Value<u32> {
            if self.sum_dynamic.last_verified < self.graph.revision {
                self.sum_dynamic.last_verified = self.graph.revision;

                let lhs = match self.lhs.value {
                    ABC::A => Value {
                        value: self.a.value,
                        last_modified: self.a.last_modified,
                    },
                    ABC::B => Value {
                        value: self.b.value,
                        last_modified: self.b.last_modified,
                    },
                    ABC::C => Value {
                        value: self.c.value,
                        last_modified: self.c.last_modified,
                    },
                    ABC::SumAB => self.sum_a_b(),
                };

                let rhs = match self.rhs.value {
                    ABC::A => Value {
                        value: self.a.value,
                        last_modified: self.a.last_modified,
                    },
                    ABC::B => Value {
                        value: self.b.value,
                        last_modified: self.b.last_modified,
                    },
                    ABC::C => Value {
                        value: self.c.value,
                        last_modified: self.c.last_modified,
                    },
                    ABC::SumAB => self.sum_a_b(),
                };

                let last_modified = [
                    self.lhs.last_modified,
                    lhs.last_modified,
                    self.rhs.last_modified,
                    rhs.last_modified,
                ]
                .iter()
                .cloned()
                .max()
                .unwrap();

                if self.sum_dynamic.last_modified < last_modified {
                    self.sum_dynamic.last_modified = last_modified;

                    self.sum_dynamic_computation_count += 1;

                    self.sum_dynamic.cached = lhs.value + rhs.value;
                }
            }
            Value {
                value: self.sum_dynamic.cached,
                last_modified: self.sum_dynamic.last_modified,
            }
        }
    }

    let mut e = {
        let graph = Graph::new();
        E {
            a: graph.leaf(1),
            b: graph.leaf(2),
            c: graph.leaf(3),
            sum_a_b: graph.branch(1 + 2),
            sum_a_b_computation_count: 0,
            mul_c_sum_a_b: graph.branch(3 * (1 + 2)),
            mul_c_sum_a_b_computation_count: 0,
            lhs: graph.leaf(ABC::A),
            rhs: graph.leaf(ABC::B),
            sum_dynamic: graph.branch(1 + 2),
            sum_dynamic_computation_count: 0,
            graph,
        }
    };

    // a = 1
    // b = 2
    // c = 3
    assert_eq!(9, *e.mul_c_sum_a_b());
    assert_eq!(0, e.sum_a_b_computation_count);
    assert_eq!(0, e.mul_c_sum_a_b_computation_count);
    assert_eq!(0, e.sum_dynamic_computation_count);

    e.graph.modify(&mut e.b, 6);

    // a = 1
    // b = 6
    // c = 3
    assert_eq!(21, *e.mul_c_sum_a_b());
    assert_eq!(1, e.sum_a_b_computation_count);
    assert_eq!(1, e.mul_c_sum_a_b_computation_count);
    assert_eq!(0, e.sum_dynamic_computation_count);

    e.graph.modify(&mut e.lhs, ABC::C);

    // a = 1
    // b = 6
    // c = 3
    assert_eq!(9, *e.sum_dynamic());
    assert_eq!(1, e.sum_a_b_computation_count);
    assert_eq!(1, e.mul_c_sum_a_b_computation_count);
    assert_eq!(1, e.sum_dynamic_computation_count);

    e.graph.modify(&mut e.rhs, ABC::SumAB);

    assert_eq!(10, *e.sum_dynamic());
    assert_eq!(1, e.sum_a_b_computation_count);
    assert_eq!(1, e.mul_c_sum_a_b_computation_count);
    assert_eq!(2, e.sum_dynamic_computation_count);

    e.graph.modify(&mut e.a, 20);

    assert_eq!(29, *e.sum_dynamic());
    assert_eq!(2, e.sum_a_b_computation_count);
    assert_eq!(1, e.mul_c_sum_a_b_computation_count);
    assert_eq!(3, e.sum_dynamic_computation_count);
}

struct Leaf<T> {
    value: T,
    last_modified: Revision,
}

struct Branch<T> {
    cached: T,
    last_modified: Revision,
    last_verified: Revision,
}

struct Value<T> {
    value: T,
    last_modified: Revision,
}

impl<T> std::ops::Deref for Value<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}
