pub trait Visitor<T> {
    fn visit_number(&mut self, val: f64) -> T;
    fn visit_add(&mut self, left: &Expr, right: &Expr) -> T;
}

pub enum Expr {
    Number(f64),
    Add(Box<Expr>, Box<Expr>),
}

impl Expr {
    pub fn accept<T>(&self, visitor: &mut dyn Visitor<T>) -> T {
        match self {
            Expr::Number(val) => visitor.visit_number(*val),
            Expr::Add(l, r) => visitor.visit_add(l, r),
        }
    }
}

pub struct Evaluator;
impl Visitor<f64> for Evaluator {
    fn visit_number(&mut self, val: f64) -> f64 { val }
    fn visit_add(&mut self, left: &Expr, right: &Expr) -> f64 {
        left.accept(self) + right.accept(self)
    }
}