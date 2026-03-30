/// Expression evaluator for the simulation engine.

use crate::sim::expr::*;

/// Evaluate an expression in the given environment.
pub fn evaluate(expr: &Expr, env: &Env) -> Result<Value, EvalError> {
    match expr {
        Expr::Literal(v) => Ok(v.clone()),

        Expr::Var(name) => env
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::new(format!("undefined variable `{}`", name))),

        Expr::BinaryOp { op, lhs, rhs } => {
            let l = evaluate(lhs, env)?;
            let r = evaluate(rhs, env)?;
            eval_binary(*op, &l, &r)
        }

        Expr::UnaryOp { op, operand } => {
            let v = evaluate(operand, env)?;
            eval_unary(*op, &v)
        }

        Expr::FunctionCall { name, args } => {
            let values: Result<Vec<Value>, _> =
                args.iter().map(|a| evaluate(a, env)).collect();
            eval_function(name, &values?)
        }
    }
}

/// Evaluate a constraint expression, expecting a boolean result.
pub fn evaluate_constraint(expr: &Expr, env: &Env) -> Result<bool, EvalError> {
    let result = evaluate(expr, env)?;
    result
        .as_bool()
        .ok_or_else(|| EvalError::new(format!("constraint did not evaluate to bool, got {}", result)))
}

/// Evaluate a calc expression, expecting a numeric result.
pub fn evaluate_calc(expr: &Expr, env: &Env) -> Result<f64, EvalError> {
    let result = evaluate(expr, env)?;
    result
        .as_number()
        .ok_or_else(|| EvalError::new(format!("calc did not evaluate to number, got {}", result)))
}

fn eval_binary(op: BinOp, lhs: &Value, rhs: &Value) -> Result<Value, EvalError> {
    match (op, lhs, rhs) {
        // Arithmetic on numbers
        (BinOp::Add, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
        (BinOp::Sub, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a - b)),
        (BinOp::Mul, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
        (BinOp::Div, Value::Number(_), Value::Number(b)) if *b == 0.0 => {
            Err(EvalError::new("division by zero"))
        }
        (BinOp::Div, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a / b)),
        (BinOp::Mod, Value::Number(_), Value::Number(b)) if *b == 0.0 => {
            Err(EvalError::new("modulo by zero"))
        }
        (BinOp::Mod, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a % b)),
        (BinOp::Pow, Value::Number(a), Value::Number(b)) => Ok(Value::Number(a.powf(*b))),

        // String concatenation
        (BinOp::Add, Value::String(a), Value::String(b)) => {
            Ok(Value::String(format!("{}{}", a, b)))
        }

        // Numeric comparison
        (BinOp::Lt, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a < b)),
        (BinOp::Gt, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a > b)),
        (BinOp::Lte, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a <= b)),
        (BinOp::Gte, Value::Number(a), Value::Number(b)) => Ok(Value::Bool(a >= b)),

        // Equality (works on any same-type pair)
        (BinOp::Eq, a, b) => Ok(Value::Bool(a == b)),
        (BinOp::Neq, a, b) => Ok(Value::Bool(a != b)),

        // Logical operators
        (BinOp::And, a, b) => {
            let la = a.as_bool().ok_or_else(|| EvalError::new(format!("cannot apply `and` to {}", a)))?;
            let lb = b.as_bool().ok_or_else(|| EvalError::new(format!("cannot apply `and` to {}", b)))?;
            Ok(Value::Bool(la && lb))
        }
        (BinOp::Or, a, b) => {
            let la = a.as_bool().ok_or_else(|| EvalError::new(format!("cannot apply `or` to {}", a)))?;
            let lb = b.as_bool().ok_or_else(|| EvalError::new(format!("cannot apply `or` to {}", b)))?;
            Ok(Value::Bool(la || lb))
        }
        (BinOp::Xor, a, b) => {
            let la = a.as_bool().ok_or_else(|| EvalError::new(format!("cannot apply `xor` to {}", a)))?;
            let lb = b.as_bool().ok_or_else(|| EvalError::new(format!("cannot apply `xor` to {}", b)))?;
            Ok(Value::Bool(la ^ lb))
        }
        (BinOp::Implies, a, b) => {
            let la = a.as_bool().ok_or_else(|| EvalError::new(format!("cannot apply `implies` to {}", a)))?;
            let lb = b.as_bool().ok_or_else(|| EvalError::new(format!("cannot apply `implies` to {}", b)))?;
            Ok(Value::Bool(!la || lb))
        }

        _ => Err(EvalError::new(format!(
            "cannot apply `{}` to {} and {}",
            op, lhs, rhs
        ))),
    }
}

fn eval_unary(op: UnOp, val: &Value) -> Result<Value, EvalError> {
    match (op, val) {
        (UnOp::Neg, Value::Number(n)) => Ok(Value::Number(-n)),
        (UnOp::Not, v) => {
            let b = v.as_bool().ok_or_else(|| EvalError::new(format!("cannot apply `not` to {}", v)))?;
            Ok(Value::Bool(!b))
        }
        _ => Err(EvalError::new(format!("cannot apply `{}` to {}", op, val))),
    }
}

fn eval_function(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    match name {
        "abs" => {
            require_args(name, args, 1)?;
            let n = require_number(name, &args[0])?;
            Ok(Value::Number(n.abs()))
        }
        "sqrt" => {
            require_args(name, args, 1)?;
            let n = require_number(name, &args[0])?;
            if n < 0.0 {
                Err(EvalError::new("sqrt of negative number"))
            } else {
                Ok(Value::Number(n.sqrt()))
            }
        }
        "floor" => {
            require_args(name, args, 1)?;
            let n = require_number(name, &args[0])?;
            Ok(Value::Number(n.floor()))
        }
        "ceil" => {
            require_args(name, args, 1)?;
            let n = require_number(name, &args[0])?;
            Ok(Value::Number(n.ceil()))
        }
        "round" => {
            require_args(name, args, 1)?;
            let n = require_number(name, &args[0])?;
            Ok(Value::Number(n.round()))
        }
        "min" => {
            if args.is_empty() {
                return Err(EvalError::new("min requires at least 1 argument"));
            }
            let mut result = require_number(name, &args[0])?;
            for a in &args[1..] {
                let n = require_number(name, a)?;
                if n < result {
                    result = n;
                }
            }
            Ok(Value::Number(result))
        }
        "max" => {
            if args.is_empty() {
                return Err(EvalError::new("max requires at least 1 argument"));
            }
            let mut result = require_number(name, &args[0])?;
            for a in &args[1..] {
                let n = require_number(name, a)?;
                if n > result {
                    result = n;
                }
            }
            Ok(Value::Number(result))
        }
        "sum" => {
            let mut total = 0.0;
            for a in args {
                total += require_number(name, a)?;
            }
            Ok(Value::Number(total))
        }
        "product" => {
            if args.is_empty() {
                return Err(EvalError::new("product requires at least 1 argument"));
            }
            let mut result = 1.0;
            for a in args {
                result *= require_number(name, a)?;
            }
            Ok(Value::Number(result))
        }
        "mean" | "avg" => {
            if args.is_empty() {
                return Err(EvalError::new("mean requires at least 1 argument"));
            }
            let mut total = 0.0;
            for a in args {
                total += require_number(name, a)?;
            }
            Ok(Value::Number(total / args.len() as f64))
        }
        "rss" => {
            // Root-sum-of-squares: sqrt(a^2 + b^2 + c^2 + ...)
            if args.is_empty() {
                return Err(EvalError::new("rss requires at least 1 argument"));
            }
            let mut sum_sq = 0.0;
            for a in args {
                let n = require_number(name, a)?;
                sum_sq += n * n;
            }
            Ok(Value::Number(sum_sq.sqrt()))
        }
        "count" => Ok(Value::Number(args.len() as f64)),
        "clamp" => {
            require_args(name, args, 3)?;
            let v = require_number(name, &args[0])?;
            let lo = require_number(name, &args[1])?;
            let hi = require_number(name, &args[2])?;
            Ok(Value::Number(v.max(lo).min(hi)))
        }
        "convert" => {
            require_args(name, args, 3)?;
            let value = require_number(name, &args[0])?;
            let from = match &args[1] {
                Value::String(s) => s.clone(),
                _ => return Err(EvalError::new("convert: second argument must be a unit string")),
            };
            let to = match &args[2] {
                Value::String(s) => s.clone(),
                _ => return Err(EvalError::new("convert: third argument must be a unit string")),
            };
            crate::sim::units::convert(value, &from, &to).map(Value::Number)
        }
        _ => Err(EvalError::new(format!("unknown function `{}`", name))),
    }
}

fn require_args(name: &str, args: &[Value], expected: usize) -> Result<(), EvalError> {
    if args.len() != expected {
        Err(EvalError::new(format!(
            "{} expects {} argument(s), got {}",
            name,
            expected,
            args.len()
        )))
    } else {
        Ok(())
    }
}

fn require_number(name: &str, val: &Value) -> Result<f64, EvalError> {
    val.as_number()
        .ok_or_else(|| EvalError::new(format!("{} expects numeric argument, got {}", name, val)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(n: f64) -> Expr {
        Expr::Literal(Value::Number(n))
    }

    fn var(name: &str) -> Expr {
        Expr::Var(name.to_string())
    }

    fn bool_lit(b: bool) -> Expr {
        Expr::Literal(Value::Bool(b))
    }

    fn binop(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
        Expr::BinaryOp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    fn unop(op: UnOp, operand: Expr) -> Expr {
        Expr::UnaryOp {
            op,
            operand: Box::new(operand),
        }
    }

    fn call(name: &str, args: Vec<Expr>) -> Expr {
        Expr::FunctionCall {
            name: name.to_string(),
            args,
        }
    }

    fn empty_env() -> Env {
        Env::new()
    }

    // --- Arithmetic ---

    #[test]
    fn eval_addition() {
        let e = binop(BinOp::Add, num(3.0), num(4.0));
        assert_eq!(evaluate(&e, &empty_env()).unwrap(), Value::Number(7.0));
    }

    #[test]
    fn eval_subtraction() {
        let e = binop(BinOp::Sub, num(10.0), num(3.0));
        assert_eq!(evaluate(&e, &empty_env()).unwrap(), Value::Number(7.0));
    }

    #[test]
    fn eval_multiplication() {
        let e = binop(BinOp::Mul, num(5.0), num(6.0));
        assert_eq!(evaluate(&e, &empty_env()).unwrap(), Value::Number(30.0));
    }

    #[test]
    fn eval_division() {
        let e = binop(BinOp::Div, num(15.0), num(3.0));
        assert_eq!(evaluate(&e, &empty_env()).unwrap(), Value::Number(5.0));
    }

    #[test]
    fn eval_division_by_zero() {
        let e = binop(BinOp::Div, num(1.0), num(0.0));
        assert!(evaluate(&e, &empty_env()).is_err());
    }

    #[test]
    fn eval_modulo() {
        let e = binop(BinOp::Mod, num(10.0), num(3.0));
        assert_eq!(evaluate(&e, &empty_env()).unwrap(), Value::Number(1.0));
    }

    #[test]
    fn eval_power() {
        let e = binop(BinOp::Pow, num(2.0), num(10.0));
        assert_eq!(evaluate(&e, &empty_env()).unwrap(), Value::Number(1024.0));
    }

    // --- Comparison ---

    #[test]
    fn eval_less_than() {
        assert_eq!(
            evaluate(&binop(BinOp::Lt, num(3.0), num(5.0)), &empty_env()).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            evaluate(&binop(BinOp::Lt, num(5.0), num(3.0)), &empty_env()).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn eval_less_equal() {
        assert_eq!(
            evaluate(&binop(BinOp::Lte, num(3.0), num(3.0)), &empty_env()).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn eval_greater_than() {
        assert_eq!(
            evaluate(&binop(BinOp::Gt, num(5.0), num(3.0)), &empty_env()).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn eval_equality() {
        assert_eq!(
            evaluate(&binop(BinOp::Eq, num(3.0), num(3.0)), &empty_env()).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            evaluate(&binop(BinOp::Neq, num(3.0), num(4.0)), &empty_env()).unwrap(),
            Value::Bool(true)
        );
    }

    // --- Logical ---

    #[test]
    fn eval_and() {
        assert_eq!(
            evaluate(&binop(BinOp::And, bool_lit(true), bool_lit(false)), &empty_env()).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            evaluate(&binop(BinOp::And, bool_lit(true), bool_lit(true)), &empty_env()).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn eval_or() {
        assert_eq!(
            evaluate(&binop(BinOp::Or, bool_lit(false), bool_lit(true)), &empty_env()).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn eval_implies() {
        // true implies false = false
        assert_eq!(
            evaluate(&binop(BinOp::Implies, bool_lit(true), bool_lit(false)), &empty_env()).unwrap(),
            Value::Bool(false)
        );
        // false implies anything = true
        assert_eq!(
            evaluate(&binop(BinOp::Implies, bool_lit(false), bool_lit(false)), &empty_env()).unwrap(),
            Value::Bool(true)
        );
    }

    // --- Unary ---

    #[test]
    fn eval_negation() {
        assert_eq!(
            evaluate(&unop(UnOp::Neg, num(5.0)), &empty_env()).unwrap(),
            Value::Number(-5.0)
        );
    }

    #[test]
    fn eval_not() {
        assert_eq!(
            evaluate(&unop(UnOp::Not, bool_lit(true)), &empty_env()).unwrap(),
            Value::Bool(false)
        );
    }

    // --- Variables ---

    #[test]
    fn eval_variable_lookup() {
        let mut env = Env::new();
        env.bind("x", Value::Number(42.0));
        assert_eq!(evaluate(&var("x"), &env).unwrap(), Value::Number(42.0));
    }

    #[test]
    fn eval_undefined_variable() {
        assert!(evaluate(&var("unknown"), &empty_env()).is_err());
    }

    // --- Functions ---

    #[test]
    fn eval_abs() {
        assert_eq!(
            evaluate(&call("abs", vec![num(-5.0)]), &empty_env()).unwrap(),
            Value::Number(5.0)
        );
    }

    #[test]
    fn eval_sqrt() {
        assert_eq!(
            evaluate(&call("sqrt", vec![num(16.0)]), &empty_env()).unwrap(),
            Value::Number(4.0)
        );
    }

    #[test]
    fn eval_sqrt_negative() {
        assert!(evaluate(&call("sqrt", vec![num(-1.0)]), &empty_env()).is_err());
    }

    #[test]
    fn eval_min_max() {
        assert_eq!(
            evaluate(&call("min", vec![num(3.0), num(1.0), num(5.0)]), &empty_env()).unwrap(),
            Value::Number(1.0)
        );
        assert_eq!(
            evaluate(&call("max", vec![num(3.0), num(1.0), num(5.0)]), &empty_env()).unwrap(),
            Value::Number(5.0)
        );
    }

    #[test]
    fn eval_sum() {
        assert_eq!(
            evaluate(&call("sum", vec![num(1.0), num(2.0), num(3.0)]), &empty_env()).unwrap(),
            Value::Number(6.0)
        );
    }

    #[test]
    fn eval_floor_ceil_round() {
        assert_eq!(
            evaluate(&call("floor", vec![num(3.7)]), &empty_env()).unwrap(),
            Value::Number(3.0)
        );
        assert_eq!(
            evaluate(&call("ceil", vec![num(3.2)]), &empty_env()).unwrap(),
            Value::Number(4.0)
        );
        assert_eq!(
            evaluate(&call("round", vec![num(3.5)]), &empty_env()).unwrap(),
            Value::Number(4.0)
        );
    }

    #[test]
    fn eval_product() {
        assert_eq!(
            evaluate(&call("product", vec![num(2.0), num(3.0), num(4.0)]), &empty_env()).unwrap(),
            Value::Number(24.0)
        );
    }

    #[test]
    fn eval_product_single() {
        assert_eq!(
            evaluate(&call("product", vec![num(7.0)]), &empty_env()).unwrap(),
            Value::Number(7.0)
        );
    }

    #[test]
    fn eval_mean() {
        assert_eq!(
            evaluate(&call("mean", vec![num(2.0), num(4.0), num(6.0)]), &empty_env()).unwrap(),
            Value::Number(4.0)
        );
    }

    #[test]
    fn eval_avg_alias() {
        assert_eq!(
            evaluate(&call("avg", vec![num(10.0), num(20.0)]), &empty_env()).unwrap(),
            Value::Number(15.0)
        );
    }

    #[test]
    fn eval_rss() {
        // sqrt(3^2 + 4^2) = 5
        let result = evaluate(&call("rss", vec![num(3.0), num(4.0)]), &empty_env()).unwrap();
        assert_eq!(result, Value::Number(5.0));
    }

    #[test]
    fn eval_rss_three_values() {
        // sqrt(1^2 + 2^2 + 2^2) = 3
        let result = evaluate(&call("rss", vec![num(1.0), num(2.0), num(2.0)]), &empty_env()).unwrap();
        assert_eq!(result, Value::Number(3.0));
    }

    #[test]
    fn eval_count() {
        assert_eq!(
            evaluate(&call("count", vec![num(1.0), num(2.0), num(3.0)]), &empty_env()).unwrap(),
            Value::Number(3.0)
        );
    }

    #[test]
    fn eval_count_empty() {
        assert_eq!(
            evaluate(&call("count", vec![]), &empty_env()).unwrap(),
            Value::Number(0.0)
        );
    }

    #[test]
    fn eval_clamp() {
        assert_eq!(
            evaluate(&call("clamp", vec![num(15.0), num(0.0), num(10.0)]), &empty_env()).unwrap(),
            Value::Number(10.0)
        );
        assert_eq!(
            evaluate(&call("clamp", vec![num(-5.0), num(0.0), num(10.0)]), &empty_env()).unwrap(),
            Value::Number(0.0)
        );
        assert_eq!(
            evaluate(&call("clamp", vec![num(5.0), num(0.0), num(10.0)]), &empty_env()).unwrap(),
            Value::Number(5.0)
        );
    }

    #[test]
    fn eval_unknown_function() {
        assert!(evaluate(&call("foobar", vec![num(1.0)]), &empty_env()).is_err());
    }

    // --- Compound expressions ---

    #[test]
    fn eval_constraint_expression() {
        // massActual <= massLimit
        let expr = binop(BinOp::Lte, var("massActual"), var("massLimit"));
        let mut env = Env::new();
        env.bind("massActual", Value::Number(1500.0));
        env.bind("massLimit", Value::Number(2000.0));
        assert_eq!(evaluate_constraint(&expr, &env).unwrap(), true);
    }

    #[test]
    fn eval_constraint_fails() {
        let expr = binop(BinOp::Lte, var("massActual"), var("massLimit"));
        let mut env = Env::new();
        env.bind("massActual", Value::Number(2500.0));
        env.bind("massLimit", Value::Number(2000.0));
        assert_eq!(evaluate_constraint(&expr, &env).unwrap(), false);
    }

    #[test]
    fn eval_calc_expression() {
        // 1 / (bsfc * mass * tpd_avg / distance)
        let expr = binop(
            BinOp::Div,
            num(1.0),
            binop(
                BinOp::Div,
                binop(
                    BinOp::Mul,
                    binop(BinOp::Mul, var("bsfc"), var("mass")),
                    var("tpd_avg"),
                ),
                var("distance"),
            ),
        );
        let mut env = Env::new();
        env.bind("bsfc", Value::Number(0.5));
        env.bind("mass", Value::Number(2.0));
        env.bind("tpd_avg", Value::Number(4.0));
        env.bind("distance", Value::Number(100.0));
        let result = evaluate_calc(&expr, &env).unwrap();
        assert!((result - 25.0).abs() < 0.001);
    }

    #[test]
    fn eval_nested_expression() {
        // abs(x - y) > 0 and z == true
        let expr = binop(
            BinOp::And,
            binop(
                BinOp::Gt,
                call("abs", vec![binop(BinOp::Sub, var("x"), var("y"))]),
                num(0.0),
            ),
            binop(BinOp::Eq, var("z"), bool_lit(true)),
        );
        let mut env = Env::new();
        env.bind("x", Value::Number(10.0));
        env.bind("y", Value::Number(3.0));
        env.bind("z", Value::Bool(true));
        assert_eq!(evaluate(&expr, &env).unwrap(), Value::Bool(true));
    }

    #[test]
    fn eval_string_concat() {
        let e = binop(
            BinOp::Add,
            Expr::Literal(Value::String("hello ".into())),
            Expr::Literal(Value::String("world".into())),
        );
        assert_eq!(
            evaluate(&e, &empty_env()).unwrap(),
            Value::String("hello world".into())
        );
    }
}
