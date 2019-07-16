#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Revision(u64);

static mut THREAD_REVISION: Revision = Revision(0);

#[derive(Debug)]
pub struct ThreadGraph;

pub trait Graph {
    fn revision(&self) -> Revision;
    fn mark_as_modified(&mut self, other: &mut Revision);
}

impl Graph for ThreadGraph {
    fn revision(&self) -> Revision {
        unsafe { THREAD_REVISION }
    }

    fn mark_as_modified(&mut self, other: &mut Revision) {
        unsafe {
            THREAD_REVISION.0 += 1;
            *other = THREAD_REVISION;
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

    struct E<G> {
        a: Leaf<u32, G>,
        b: Leaf<u32, G>,
        c: Leaf<u32, G>,
        sum_a_b: Branch<u32, G>,
        sum_a_b_computation_count: u32,
        mul_c_sum_a_b: Branch<u32, G>,
        mul_c_sum_a_b_computation_count: u32,
        lhs: Leaf<ABC, G>,
        rhs: Leaf<ABC, G>,
        sum_dynamic: Branch<u32, G>,
        sum_dynamic_computation_count: u32,
    }

    impl<G: Graph> E<G> {
        fn sum_a_b(&mut self) -> Revision {
            if self.sum_a_b.verify() {
                let last_modified = std::cmp::max(self.a.last_modified, self.b.last_modified);

                if self.sum_a_b.last_modified < last_modified {
                    self.sum_a_b.last_modified = last_modified;

                    self.sum_a_b_computation_count += 1;

                    self.sum_a_b.cached = *self.a + *self.b;
                }
            }
            self.sum_a_b.last_modified
        }

        fn mul_c_sum_a_b(&mut self) -> Revision {
            if self.mul_c_sum_a_b.verify() {
                let last_modified = std::cmp::max(self.c.last_modified, self.sum_a_b());

                if self.mul_c_sum_a_b.last_modified < last_modified {
                    self.mul_c_sum_a_b.last_modified = last_modified;

                    self.mul_c_sum_a_b_computation_count += 1;

                    self.mul_c_sum_a_b.cached = *self.c * *self.sum_a_b
                }
            }
            self.mul_c_sum_a_b.last_modified
        }

        fn sum_dynamic(&mut self) -> Revision {
            if self.sum_dynamic.verify() {
                let last_modified = std::cmp::max(
                    std::cmp::max(
                        self.lhs.last_modified,
                        match *self.lhs {
                            ABC::A => self.a.last_modified,
                            ABC::B => self.b.last_modified,
                            ABC::C => self.c.last_modified,
                            ABC::SumAB => self.sum_a_b(),
                        },
                    ),
                    std::cmp::max(
                        self.rhs.last_modified,
                        match *self.rhs {
                            ABC::A => self.a.last_modified,
                            ABC::B => self.b.last_modified,
                            ABC::C => self.c.last_modified,
                            ABC::SumAB => self.sum_a_b(),
                        },
                    ),
                );

                if self.sum_dynamic.last_modified < last_modified {
                    self.sum_dynamic.last_modified = last_modified;

                    self.sum_dynamic_computation_count += 1;

                    self.sum_dynamic.cached = match *self.lhs {
                        ABC::A => *self.a,
                        ABC::B => *self.b,
                        ABC::C => *self.c,
                        ABC::SumAB => *self.sum_a_b,
                    } + match *self.rhs {
                        ABC::A => *self.a,
                        ABC::B => *self.b,
                        ABC::C => *self.c,
                        ABC::SumAB => *self.sum_a_b,
                    };
                }
            }
            self.sum_dynamic.last_modified
        }
    }

    let mut e = {
        E {
            a: Leaf::new(1, ThreadGraph),
            b: Leaf::new(2, ThreadGraph),
            c: Leaf::new(3, ThreadGraph),
            sum_a_b: Branch::new(1 + 2, ThreadGraph),
            sum_a_b_computation_count: 0,
            mul_c_sum_a_b: Branch::new(3 * (1 + 2), ThreadGraph),
            mul_c_sum_a_b_computation_count: 0,
            lhs: Leaf::new(ABC::A, ThreadGraph),
            rhs: Leaf::new(ABC::B, ThreadGraph),
            sum_dynamic: Branch::new(1 + 2, ThreadGraph),
            sum_dynamic_computation_count: 0,
        }
    };

    // a = 1
    // b = 2
    // c = 3
    assert_eq!(9, {
        e.mul_c_sum_a_b();
        *e.mul_c_sum_a_b
    });
    assert_eq!(0, e.sum_a_b_computation_count);
    assert_eq!(0, e.mul_c_sum_a_b_computation_count);
    assert_eq!(0, e.sum_dynamic_computation_count);

    e.b.modify(6);

    // a = 1
    // b = 6
    // c = 3
    assert_eq!((7, 21), {
        e.mul_c_sum_a_b();
        (*e.sum_a_b, *e.mul_c_sum_a_b)
    });
    assert_eq!(1, e.sum_a_b_computation_count);
    assert_eq!(1, e.mul_c_sum_a_b_computation_count);
    assert_eq!(0, e.sum_dynamic_computation_count);

    e.lhs.modify(ABC::C);

    // a = 1
    // b = 6
    // c = 3
    assert_eq!(9, {
        e.sum_dynamic();
        *e.sum_dynamic
    });
    assert_eq!(1, e.sum_a_b_computation_count);
    assert_eq!(1, e.mul_c_sum_a_b_computation_count);
    assert_eq!(1, e.sum_dynamic_computation_count);

    e.rhs.modify(ABC::SumAB);

    assert_eq!(10, {
        e.sum_dynamic();
        *e.sum_dynamic
    });
    assert_eq!(1, e.sum_a_b_computation_count);
    assert_eq!(1, e.mul_c_sum_a_b_computation_count);
    assert_eq!(2, e.sum_dynamic_computation_count);

    e.a.modify(20);

    assert_eq!(29, {
        e.sum_dynamic();
        *e.sum_dynamic
    });
    assert_eq!(2, e.sum_a_b_computation_count);
    assert_eq!(1, e.mul_c_sum_a_b_computation_count);
    assert_eq!(3, e.sum_dynamic_computation_count);
}

struct Leaf<T, G> {
    value: T,
    last_modified: Revision,
    graph: G,
}

impl<T: Copy + PartialEq, G: Graph> Leaf<T, G> {
    fn new(value: T, graph: G) -> Self {
        Self {
            value,
            last_modified: graph.revision(),
            graph,
        }
    }

    fn modify(&mut self, value: T) {
        if self.value != value {
            self.graph.mark_as_modified(&mut self.last_modified);
            self.value = value;
        }
    }
}

impl<T, G> std::ops::Deref for Leaf<T, G> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

pub struct Branch<T, G> {
    cached: T,
    last_modified: Revision,
    last_verified: Revision,
    graph: G,
}

impl<T, G: Graph> Branch<T, G> {
    pub fn new(cached: T, graph: G) -> Self {
        Self {
            cached,
            last_modified: graph.revision(),
            last_verified: graph.revision(),
            graph,
        }
    }

    pub fn verify(&mut self) -> bool {
        let revision = self.graph.revision();
        if self.last_verified < revision {
            self.last_verified = revision;
            true
        } else {
            false
        }
    }
}

impl<T, G: Graph> std::ops::Deref for Branch<T, G> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        let revision = self.graph.revision();
        if self.last_verified < revision {
            panic!("Dereferenced outdated branch!");
        }
        &self.cached
    }
}
