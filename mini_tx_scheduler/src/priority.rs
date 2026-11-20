use std::cmp::Ordering;

/// How urgently a transaction should run. A larger priority goes first.
///
/// It is a fee *rate*: fee divided by compute units. Ranking by the fee
/// alone would favour large transactions that happen to pay a lot in total
/// while paying little for each unit of the block they occupy. Space in a
/// block is the scarce thing, so the price per unit of space is what counts.
///
/// When two rates are equal, the one that arrived first wins.
#[derive(Debug, Clone, Copy)]
pub struct Priority {
    fee: u64,
    units: u64,
    arrival: u64,
}

impl Priority {
    /// The priority of something paying `fee` for `units` of work that
    /// arrived as number `arrival`.
    pub fn new(fee: u64, units: u64, arrival: u64) -> Self {
        Self {
            fee,
            units: units.max(1),
            arrival,
        }
    }

    /// The rate as a decimal number, for display only. It is never used to
    /// decide an order.
    pub fn rate(&self) -> f64 {
        self.fee as f64 / self.units as f64
    }
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare a/b with c/d without dividing: a/b > c/d exactly when
        // a*d > c*b. Division would round, and two different rates could
        // then tie or even swap. The products are computed in 128 bits so
        // they cannot overflow.
        let left = u128::from(self.fee) * u128::from(other.units);
        let right = u128::from(other.fee) * u128::from(self.units);
        // On equal rates the earlier arrival ranks higher, hence the swap.
        left.cmp(&right)
            .then_with(|| other.arrival.cmp(&self.arrival))
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Priority {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Priority {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_higher_rate_outranks_a_higher_fee() {
        // 100 for 50 units is 2.0 per unit; 300 for 600 units is 0.5.
        let small_and_generous = Priority::new(100, 50, 9);
        let big_and_stingy = Priority::new(300, 600, 1);
        assert!(small_and_generous > big_and_stingy);
        assert_eq!(small_and_generous.rate(), 2.0);
    }

    #[test]
    fn equal_rates_go_in_order_of_arrival() {
        // 10/5 and 20/10 are the same rate.
        let earlier = Priority::new(10, 5, 3);
        let later = Priority::new(20, 10, 4);
        assert!(earlier > later);
        assert_ne!(earlier, later);
        assert_eq!(earlier, Priority::new(30, 15, 3));
    }

    #[test]
    fn rates_are_compared_exactly_not_after_rounding() {
        // 1/3 and 333333/1000000 differ in the seventh decimal place.
        let third = Priority::new(1, 3, 0);
        let nearly = Priority::new(333_333, 1_000_000, 0);
        assert!(third > nearly);
        // Numbers too large to multiply in 64 bits still compare correctly.
        let huge = Priority::new(u64::MAX, u64::MAX - 1, 0);
        let huger = Priority::new(u64::MAX, u64::MAX - 2, 0);
        assert!(huger > huge);
    }

    #[test]
    fn zero_units_do_not_divide_by_zero() {
        let odd = Priority::new(5, 0, 0);
        assert_eq!(odd.rate(), 5.0);
        assert!(odd > Priority::new(4, 1, 0));
    }
}
