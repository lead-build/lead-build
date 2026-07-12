use super::*;
use super::{
    super::parser::parse_str,
    super::testvalue::{FRef, TestValue},
    ExprType,
};
use std::collections::HashSet;

fn eval(code: &str) -> Expr<TestValue, FRef> {
    let expr: Expr<TestValue, FRef> =
        ExprType::Bind(ExprSet::new(), parse_str(code, &1).unwrap()).builtin();
    expr.eval().unwrap();
    expr
}

fn referenced_vars(code: &str) -> HashSet<String> {
    parse_str::<TestValue, FRef>(code, &1)
        .unwrap()
        .referenced_vars()
}

#[test]
fn test_referenced_vars_simple() {
    assert_eq!(
        referenced_vars("a + b"),
        HashSet::from(["a".into(), "b".into()])
    );
}

#[test]
fn test_referenced_vars_let_bindings_are_not_free() {
    assert_eq!(
        referenced_vars("let a = x; in (a + y)"),
        HashSet::from(["a".into(), "x".into(), "y".into()])
    );
}

#[test]
fn test_referenced_vars_function_matcher_scope() {
    assert_eq!(
        referenced_vars(
            "let func = |{a = ax, b ? def, ...} @ obj| ax + b + obj.inner + cap; in (func input)"
        ),
        HashSet::from([
            "ax".into(),
            "b".into(),
            "def".into(),
            "obj".into(),
            "cap".into(),
            "func".into(),
            "input".into(),
        ])
    );
}

#[test]
fn test_referenced_vars_shadowing() {
    assert_eq!(
        referenced_vars("let x = a; in (let x = b; in (x + c))"),
        HashSet::from(["x".into(), "a".into(), "b".into(), "c".into()])
    );
}

#[test]
fn test_referenced_in_matcher() {
    let referenced = referenced_vars("|{a ? x, ...}| a");
    assert!(referenced.contains("x"));
}

#[test]
fn test_resolve() -> Result<(), FRef> {
    let expr = parse_str(
        r#"
                {
                    stuff = "hello";
                    something = "hej";
                }
            "#,
        &1,
    )
    .unwrap();
    let value = expr.get_item("stuff")?;
    assert_eq!(
        value,
        ExprType::from(TestValue::String("hello".into())).builtin()
    );
    Ok(())
}

#[test]
fn test_eval_obj() {
    assert_eq!(eval("{ a = (let x = 3; in x); }"), eval("{ a = 3; }"));
}

#[test]
fn test_eval_list() {
    assert_eq!(eval("[ (let x = 3; in x) ]"), eval("[ 3 ]"));
}

#[test]
fn test_func_in_let_res() {
    assert_eq!(eval("let a = |x| (x+1); in (a 12)"), eval("13"));
}

#[test]
fn test_func_with_let() {
    assert_eq!(
        eval("let a = |x| let a = (x+1); in a; in (a 12)"),
        eval("13")
    );
}

#[test]
fn test_stacked_func_defaults() {
    assert_eq!(
        eval("let f = |{a = ax ? 1}| |{b = bx ? ax}| (ax+bx); in (f {} {})"),
        eval("2")
    );
}

#[test]
fn test_resolve_deep() -> Result<(), FRef> {
    // This also tests "inner" as prefixed for reserved keyword "in" is ok
    let expr = parse_str(
        r#"
                {
                    stuff = "hello";
                    something = {
                        inner = 55;
                    };
                }
            "#,
        &1,
    )
    .unwrap();
    let value = expr.get_item("something")?.get_item("inner")?;
    assert_eq!(value, ExprType::from(TestValue::Int(55)).builtin());
    Ok(())
}

#[test]
fn test_let() {
    assert_eq!(
        eval("let x = 12; in (let a = 21; b = 37; in (a+x))"),
        eval("33")
    );
}

#[test]
fn test_let_pattern() {
    assert_eq!(
        eval(
            r#"
            let
                x = { b = 32; a = 12; };
                { a = newvar, ... } = x;
            in
                newvar
            "#
        ),
        eval("12")
    );
}

#[test]
fn test_invalid_var() -> Result<(), FRef> {
    let expr: Expr<TestValue, FRef> =
        ExprType::Bind(ExprSet::new(), parse_str("invalid_var", &1).unwrap()).builtin();
    if let Err(Error { msg, .. }) = expr.resolve() {
        assert_eq!(msg.as_str(), "Unknown variable invalid_var");
    } else {
        panic!("expected resolve() to return an error");
    }
    Ok(())
}

#[test]
fn test_resolve_error_keeps_original_expr() {
    let expr: Expr<TestValue, FRef> =
        ExprType::Bind(ExprSet::new(), parse_str("invalid_var", &1).unwrap()).builtin();
    let before = expr.inner_ref().tok.clone();

    assert!(expr.resolve().is_err());
    assert_eq!(expr.inner_ref().tok, before);
}

#[test]
fn test_let_set_var() {
    assert_eq! {
        eval(r#"
                let
                    a = 12;
                in
                {
                    stuff = a;
                }
            "#),
        eval("{ stuff = 12; }"),
    }
}

#[test]
fn test_let_set_var_seq() {
    assert_eq! {
        eval(r#"
                let
                    a = 12;
                    b = a;
                in
                {
                    stuff = b;
                }
            "#),
        eval("{ stuff = 12; }"),
    }
}

#[test]
fn test_func_call() {
    let func_a = parse_str("|var| 13", &1).unwrap();
    let func_b = parse_str("|var| 42", &1).unwrap();
    let call = parse_str("func_b 32", &1).unwrap();
    let varscope = ExprSet::from([("func_a".into(), func_a), ("func_b".into(), func_b)]);
    let value: Expr<TestValue, FRef> = ExprType::Bind(varscope, call).builtin();
    value.resolve().unwrap();
    assert_eq!(value, ExprType::from(TestValue::Int(42)).builtin());
}

#[test]
fn test_func_call_var_arg() {
    let func_var = parse_str("|var| var", &1).unwrap();
    let arg_var = parse_str("32", &1).unwrap();
    let call = parse_str("func arg", &1).unwrap();
    let varscope = ExprSet::from([("func".into(), func_var), ("arg".into(), arg_var)]);
    let value: Expr<TestValue, FRef> = ExprType::Bind(varscope, call).builtin();
    value.resolve().unwrap();
    assert_eq!(value, ExprType::from(TestValue::Int(32)).builtin());
}

#[test]
fn test_func_call_order() {
    assert_eq! {
        eval("let f = |a| |b| (a+b); in (f 3 4)"),
        eval("7"),
    }
}

#[test]
fn test_func_call_resolved() {
    assert_eq! {
        eval(r#"
                let
                    a = 12;
                    func = |test| {
                        var = test;
                    };
                in
                {
                    stuff = func a;
                }
            "#),
        eval("{ stuff = { var = 12; }; }"),
    }
}

#[test]
fn test_func_call_bound() {
    assert_eq! {
        eval(r#"
                let
                    a = 12;
                    func = |test| {
                        var = a;
                    };
                in
                {
                    stuff = func 77;
                }
            "#),
        eval("{ stuff = { var = 12; }; }"),
    }
}

#[test]
fn test_func_call_resolved_stacked_let() {
    assert_eq! {
        eval(r#"
                let
                    a = 12;
                in
                let
                    func = |test| {
                        var = test;
                    };
                in
                {
                    stuff = func a;
                }
            "#),
        eval("{ stuff = { var = 12; }; }"),
    }
}

#[test]
fn test_func_call_pattern() {
    assert_eq! {
        eval(r#"
                let
                    a = 12;
                    b = 13;
                    func = |{ a, b, ... }| {
                        var = b;
                    };
                in
                {
                    stuff = func {
                        a = 15;
                        b = 74;
                    };
                }
            "#),
        eval("{ stuff = { var = 74; }; }"),
    }
}

#[test]
#[should_panic]
fn test_func_call_pattern_needall() {
    eval(
        r#"
                let
                    func = |{ a, b }| {
                        var = b;
                    };
                in
                {
                    stuff = func {
                        a = 15;
                        b = 74;
                        c = 123;
                    };
                }
            "#,
    );
}

#[test]
#[should_panic]
fn test_func_call_pattern_extra_args() {
    assert_eq!(
        eval(
            r#"
                let
                    func = |{ a, b, ... }| {
                        var = b;
                    };
                in
                {
                    stuff = func {
                        a = 15;
                        b = 74;
                        c = 123;
                    };
                }
            "#,
        ),
        eval("{var = 74;}")
    );
}

#[test]
fn test_var_through_func_call() {
    assert_eq! {
        eval(r#"
                let
                    func = (|a| a+2);
                in
                let
                    myvar = 13;
                in
                    func myvar
            "#),
        eval("15"),
    }
}

#[test]
fn test_eval() {
    assert_eq! {
        eval(r#"
                let
                    a = 12;
                    b = { inner = 43; };
                    myfunc = |{target, ...}| { var = b; };
                in
                {
                    app = myfunc {
                        target = "app.elf";
                    };
                }
            "#),
        eval("{ app = { var = { inner = 43; }; }; }"),
    }
}

#[test]
fn test_arith() {
    assert_eq! {
        eval("2 * 3 + 4 * 5"),
        eval("6 + 20"),
    }
    assert_eq! {
        eval("6 + 20"),
        eval("26"),
    }
}

#[test]
fn test_bool_op() {
    assert_eq!(eval("false || 12"), eval("12"));
    assert_eq!(eval("true || 12"), eval("true"));
    assert_eq!(eval("false && 12"), eval("false"));
    assert_eq!(eval("true && 12"), eval("12"));
}

#[test]
fn test_bool_laziness() {
    assert_eq!(eval("true || invalid_var"), eval("true"));
    assert_eq!(eval("false && invalid_var"), eval("false"));
    assert_eq!(eval("false -> invalid_var"), eval("true"));
}

#[test]
fn test_bool_implication() {
    assert_eq!(eval("false -> false"), eval("true"));
    assert_eq!(eval("false -> true"), eval("true"));
    assert_eq!(eval("true -> false"), eval("false"));
    assert_eq!(eval("true -> true"), eval("true"));
    assert_eq!(eval("false -> 12"), eval("true"));
    assert_eq!(eval("true -> 12"), eval("12"));
}

#[test]
fn test_bool_not() {
    assert_eq!(eval("!true"), eval("false"));
    assert_eq!(eval("!false"), eval("true"));
}

#[test]
fn test_bool_neg() {
    assert_eq!(eval("let a = 5; in (-a) + 3"), eval("-2"));
}

#[test]
fn test_func_call_laziness() {
    // The code contains an error; myfunc, which is not a function.
    // It is intentional that the func should not be evaluated, since
    // laziness in "false && ...", and therefore not be resolved as an
    // error.
    //
    // Test evalutes that eval is successful rather than ethe actual output
    assert_eq!(
        eval(
            r#"
                let
                    myfunc = not_a_function;
                    lazy_func_call = myfunc 72;
                in
                    false && lazy_func_call
                "#
        ),
        eval("false")
    );
}

#[derive(Debug, Clone)]
struct CountingBuiltin(Rc<RefCell<i32>>);
impl CountingBuiltin {
    fn new() -> CountingBuiltin {
        CountingBuiltin(Rc::new(RefCell::new(0i32)))
    }

    fn get(&self) -> i32 {
        *self.0.borrow()
    }
}

impl ExprBuiltin<TestValue, FRef> for CountingBuiltin {
    fn get_name(&self) -> String {
        "mybuiltin".into()
    }

    fn call(&self, arg: Expr<TestValue, FRef>) -> Result<Expr<TestValue, FRef>, FRef> {
        let mut counter = self.0.borrow_mut();
        *counter += 1;
        Ok(arg)
    }
}

#[test]
fn test_parse_func_call_from_obj() {
    assert_eq!(
        eval("let lib = { func = |a| a+3; }; in (lib.func 7)"),
        eval("10")
    );
}

#[test]
fn test_parse_func_call_match_tuple() {
    assert_eq!(
        eval(
            r#"
        let
            func = |(a,b)| 10*a + b;
        in
            func (3,4)
        "#
        ),
        eval("34")
    );
}

#[test]
fn test_parse_func_call_match_default() {
    assert_eq!(
        eval(
            r#"
        let
            func = |{a, b ? 12}| a + b;
        in
            func { a = 3; }
        "#
        ),
        eval("15")
    );
}

#[test]
fn test_parse_func_call_match_default_outer_scope() {
    assert_eq!(
        eval(
            r#"
        let
            x = 12;
            func = |{a ? x}| a;
        in
            func {}
        "#
        ),
        eval("12")
    );
}

#[test]
fn test_parse_func_call_match_alias() {
    assert_eq!(
        eval(
            r#"
        let
            func = |(a,b @ bx) @ x| {
                a = a;
                b = b;
                bx = bx;
                x = x;
            };
        in
            func (3,4)
        "#
        ),
        eval(
            r#"
            {
                a = 3;
                b = 4;
                bx = 4;
                x = (3,4);
            }
        "#
        )
    );
}

#[test]
fn test_parse_func_call_match_obj_rename() {
    assert_eq!(
        eval(
            r#"
        let
            a = 10;
            b = 11;
            func = |{a = ax, b = _}| {
                a = a;
                b = b;
                ax = ax;
            };
        in
            func { a = 20; b = 21; }
        "#
        ),
        eval(
            r#"
            {
                a = 10;
                b = 11;
                ax = 20;
            }
        "#
        )
    );
}

#[test]
fn test_parse_func_no_args() {
    assert_eq!(
        eval(
            r#"
        let
            func = | | 12;
        in
            func
        "#
        ),
        eval("12")
    );
}

#[test]
fn test_parse_func_multi_args() {
    assert_eq!(
        eval(
            r#"
        let
            func = |a b c| 100*a + 10*b + 1*c;
            x = func 2 3;
        in
            x 4
        "#
        ),
        eval("234")
    );
}

#[test]
fn test_parse_func_let_priority() {
    assert_eq!(
        eval(
            r#"
        let
            func = |a| a + 3;
            x = 7;
        in
            func x
        "#
        ),
        eval("10")
    );
}

#[test]
fn test_parse_func_multi_args_syntax() {
    assert_eq!(eval("|a b c| x"), eval("|a| |b| |c| x"),);
}

#[test]
fn test_multi_level_obj() {
    assert_eq!(eval("let a = { b = { c = 3; }; }; in a.b.c"), eval("3"));
}

#[test]
fn test_object_string_key_assign_and_select() {
    assert_eq!(
        eval("let a = { \"my-key\" = 7; }; in a.{\"my-key\"}"),
        eval("7")
    );
}

#[test]
fn test_attr_select_computed_from_paren_expr() {
    assert_eq!(
        eval(
            r#"
            let
                obj = { name = 44; };
            in
                obj.{("name")}
            "#
        ),
        eval("44")
    );
}

#[test]
fn test_attr_select_computed_from_expr() {
    assert_eq!(
        eval(
            r#"
            let
                obj = { name = 99; };
            in
                obj.{"na" + "me"}
            "#
        ),
        eval("99")
    );
}

#[test]
fn test_attr_select_ident() {
    assert_eq!(
        eval("let obj = { field = 123; }; in obj.field"),
        eval("123")
    );
}

#[test]
fn test_attr_select_var_in_selection() {
    assert_eq!(
        eval(
            r#"
            let
                obj = { field = 123; incorrect = 4343; };
                config = { name = "field"; };
            in
                obj.{config.name}
        "#
        ),
        eval("123")
    );
}

#[test]
fn test_list_concat() {
    assert_eq!(
        eval("[1, 3, 5, 7] ++ [2, 4, 6, 8]"),
        eval("[1, 3, 5, 7, 2, 4, 6, 8]")
    );
}

#[test]
fn test_object_update() {
    assert_eq!(eval("{a=1; b=2;} // {b=3; c=4;}"), eval("{a=1; b=3; c=4;}"));
}

#[test]
fn test_tuple() {
    assert_eq!(eval("let a=1; b=2; in (a,b)"), eval("(1,2)"));
}

#[test]
fn test_tuple_single_item() {
    assert_ne!(eval("(1,)"), eval("(1)"));
}

#[test]
fn test_tuple_empty() {
    assert_eq!(eval("(,)"), ExprType::Tuple(vec![]).builtin());
}

#[test]
fn test_list_commas() {
    assert_eq!(eval("[1, 3, 5, 7,]"), eval("[1, 3, 5, 7]"));
}

#[test]
fn test_list_fold() {
    assert_eq!(
        eval("(|prev field| prev*10 + field <- 7 .. [1, 2, 3] )"),
        eval("7123")
    );
}

#[test]
fn test_list_fold_concat_strings() {
    assert_eq!(
        eval(
            r#"
            let
                strconcat = |strs| (|p f| (p + f) <- "" .. strs );
            in
                strconcat [ "hello", "rld" ]
            "#
        ),
        eval(r#" "hellorld" "#)
    );
}

#[test]
fn test_list_map_list_to_list() {
    assert_eq!(eval("[ |a| (a*2) <- [1, 2, 3] ]"), eval("[2, 4, 6]"));
}

#[test]
fn test_list_map_list_to_obj() {
    assert_eq!(
        eval("{ |a| (a, 3) <- [\"a\", \"b\", \"c\"] }"),
        eval("{a=3; b=3; c=3;}")
    );
}

#[test]
fn test_list_map_obj_to_list() {
    assert_eq!(
        eval("[ |(k, v)| v <- {a=1; b=2; c=3;} ]"),
        eval("[1, 2, 3]")
    );
}

#[test]
fn test_list_map_obj_to_obj() {
    assert_eq!(
        eval("{ |(k, v)| (k, v*3) <- {a=1; b=2; c=3;} }"),
        eval("{a=3; b=6; c=9;}")
    );
}

#[test]
fn test_div_precedence() {
    assert_eq!(eval("8 / 2 / 2"), eval("2"));
}

#[test]
fn test_sub_precedence() {
    assert_eq!(eval("8 - 2 - 2"), eval("4"));
}

#[test]
fn test_builtin_func() {
    let code = "mybuiltin 10";

    let builtins = ExprSet::from([(
        "mybuiltin".into(),
        Expr::new_builtin(Rc::new(CountingBuiltin::new())),
    )]);
    let expr: Expr<TestValue, FRef> =
        ExprType::Bind(builtins, parse_str(code, &1).unwrap()).builtin();
    expr.eval().unwrap();
    assert_eq!(expr, eval("10"));
}

#[test]
fn test_builtin_func_laziness_multiple_calls() {
    // Invoked in code twice should only be evaluated once
    let code = r#"
                let
                    func_call = mybuiltin 10;
                in
                {
                    a = func_call;
                    b = func_call;
                }
            "#;
    let counter = CountingBuiltin::new();
    let builtins = ExprSet::from([(
        "mybuiltin".into(),
        Expr::new_builtin(Rc::new(counter.clone())),
    )]);
    let expr: Expr<TestValue, FRef> =
        ExprType::Bind(builtins, parse_str(code, &1).unwrap()).builtin();
    expr.eval().unwrap();
    assert_eq!(expr, eval("{ a = 10; b = 10; }"));
    assert_eq!(counter.get(), 1);
}

#[test]
fn test_builtin_func_laziness_within_obj_map() {
    // Invoked in code twice should only be evaluated once
    let code = r#"
                let
                    obj = {|(k,v)| (k, mybuiltin v) <- {a=2; b=3;} };
                in
                {
                    a = obj.a;
                    b1 = obj.b;
                    b2 = obj.b;
                    b3 = obj.b;
                }
            "#;
    let counter = CountingBuiltin::new();
    let builtins = ExprSet::from([(
        "mybuiltin".into(),
        Expr::new_builtin(Rc::new(counter.clone())),
    )]);
    let expr: Expr<TestValue, FRef> =
        ExprType::Bind(builtins, parse_str(code, &1).unwrap()).builtin();
    expr.eval().unwrap();
    assert_eq!(expr, eval("{ a = 2; b1 = 3; b2 = 3; b3 = 3; }"));

    // Should only be evaluated once per branch in the list compherension
    assert_eq!(counter.get(), 2);
}

#[test]
fn test_builtin_func_laziness_within_list_map() {
    // Invoked in code twice should only be evaluated once
    let code = r#"
                let
                    obj = {|k| (k, mybuiltin k) <- ["a", "b"] };
                in
                {
                    a = obj.a;
                    b1 = obj.b;
                    b2 = obj.b;
                    b3 = obj.b;
                }
            "#;
    let counter = CountingBuiltin::new();
    let builtins = ExprSet::from([(
        "mybuiltin".into(),
        Expr::new_builtin(Rc::new(counter.clone())),
    )]);
    let expr: Expr<TestValue, FRef> =
        ExprType::Bind(builtins, parse_str(code, &1).unwrap()).builtin();
    expr.eval().unwrap();
    assert_eq!(
        expr,
        eval("{ a = \"a\"; b1 = \"b\"; b2 = \"b\"; b3 = \"b\"; }")
    );

    // Should only be evaluated once per branch in the list compherension
    assert_eq!(counter.get(), 2);
}

#[test]
fn test_builtin_func_laziness_within_obj_hardcoded() {
    // Invoked in code twice should only be evaluated once
    let code = r#"
                let
                    obj = {
                        a = mybuiltin "a";
                        b = mybuiltin "b";
                    };
                in
                {
                    a = obj.a;
                    b1 = obj.b;
                    b2 = obj.b;
                    b3 = obj.b;
                }
            "#;
    let counter = CountingBuiltin::new();
    let builtins = ExprSet::from([(
        "mybuiltin".into(),
        Expr::new_builtin(Rc::new(counter.clone())),
    )]);
    let expr: Expr<TestValue, FRef> =
        ExprType::Bind(builtins, parse_str(code, &1).unwrap()).builtin();
    expr.eval().unwrap();
    assert_eq!(
        expr,
        eval("{ a = \"a\"; b1 = \"b\"; b2 = \"b\"; b3 = \"b\"; }")
    );

    // Should only be evaluated once per branch in the list compherension
    assert_eq!(counter.get(), 2);
}

#[test]
fn test_builtin_func_laziness_no_calls() {
    // Invoked in code twice should only be evaluated once
    let code = r#"
                let
                    func_call = mybuiltin 10;
                in
                {}
            "#;
    let counter = CountingBuiltin::new();
    let builtins = ExprSet::from([(
        "mybuiltin".into(),
        Expr::new_builtin(Rc::new(counter.clone())),
    )]);
    let expr: Expr<TestValue, FRef> =
        ExprType::Bind(builtins, parse_str(code, &1).unwrap()).builtin();
    expr.eval().unwrap();
    assert_eq!(expr, eval("{}"));
    assert_eq!(counter.get(), 0);
}

#[test]
fn test_switch_value() {
    assert_eq!(
        eval(
            r#"
            switch 234 {
                123 => 1;
                234 => 2;
            }
            "#
        ),
        eval("2")
    );
}

#[test]
fn test_switch_variable() {
    assert_eq!(
        eval(
            r#"
            let a = 234; in
            switch 234 {
                123 => 1;
                a => 2;
            }
            "#
        ),
        eval("2")
    );
}

#[test]
fn test_switch_condition() {
    assert_eq!(
        eval(
            r#"
            let a = 123; in
            switch true {
                a == 323 => 1;
                a == 123 => 2;
                true => 3;
            }
            "#
        ),
        eval("2")
    );
}

#[test]
fn test_switch_none() {
    assert_eq!(
        eval(
            r#"
            switch 17 {
                1 => 1;
                2 => 2;
                _ => 3;
            }
            "#
        ),
        eval("3")
    );
}

#[test]
fn test_string_concat() {
    assert_eq!(
        eval(
            r#"
            let
                var = "foo";
            in
                "${var}aaa${var}bbb${var}"
            "#
        ),
        eval("\"fooaaafoobbbfoo\"")
    );
}
