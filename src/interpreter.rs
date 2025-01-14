use std::rc::Rc;

use crate::types::prelude::*;

pub fn interpret(src: &Syntax) -> SResult<Pointer> {
    inner_interpret(src, rc_mut_new(State::new()))
}

pub fn inner_interpret(src: &Syntax, state: RcMut<State>) -> SResult<Pointer> {
    match src {
        Syntax::Statement(false, content, _) => {
            inner_interpret(content, state.clone())?;
            Ok(state.borrow().undefined.clone())
        }
        Syntax::Statement(true, content, level) => {
            if *level >= 3 {
                println!("{content:?}");
            }
            let evaluated = inner_interpret(content, state)?;
            if *level >= 2 {
                println!("{evaluated:?}");
            } else {
                println!("{evaluated}");
            }
            Ok(evaluated)
        }
        Syntax::Negate(content) => {
            let evaluated = inner_interpret(content, state)?;
            Ok(-evaluated)
        }
        Syntax::Operation(lhs, op, rhs) => interpret_operation(lhs, *op, rhs, state),
        Syntax::Block(statements) => {
            let state = rc_mut_new(State::from_parent(state));
            let mut iter = statements.iter();
            let Some(last) = iter.next_back() else {
                return Ok(state.borrow().undefined.clone())
            };
            for syn in iter {
                inner_interpret(syn, state.clone())?;
            }
            let res = inner_interpret(last, state)?;
            Ok(res)
        }
        Syntax::Declare(var_type, ident, value) => {
            let val = inner_interpret(value, state.clone())?;
            state
                .borrow_mut()
                .insert(ident.clone(), val.convert(*var_type));
            // println!("{state:#?}");
            Ok(state.borrow().undefined.clone())
        }
        Syntax::String(str) => {
            let mut string_buf = String::new();
            for segment in str {
                match segment {
                    StringSegment::Ident(ident) => {
                        string_buf.push_str(&state.borrow_mut().get(ident.clone()).to_string());
                    }
                    StringSegment::String(str) => string_buf.push_str(str),
                }
            }
            Ok(Pointer::from(string_buf.as_ref()))
        }
        Syntax::Call(func, args) => {
            let func = state.borrow_mut().get(func.clone());
            interpret_function(&func, args, state)
        }
        Syntax::Ident(ident) => Ok(state.borrow_mut().get(ident.clone())),
        Syntax::Function(args, body) => {
            Ok(Pointer::from(Value::Function(args.clone(), *body.clone())))
        }
    }
}

fn interpret_operation(
    lhs: &Syntax,
    op: Operation,
    rhs: &Syntax,
    state: RcMut<State>,
) -> SResult<Pointer> {
    let mut lhs_eval = inner_interpret(lhs, state.clone())?;
    if let (Value::Object(_), Operation::Dot, Syntax::Ident(ident)) =
        (&*lhs_eval.as_const(), op, rhs)
    {
        let inner_var = lhs_eval.as_var();
        let Value::Object(ref mut obj) = *inner_var.borrow_mut() else { panic!("Internal Compiler Error at {}:{}", file!(), line!()) };
        let key = Value::from(ident.clone());
        if let Some(val) = obj.get(&key) {
            // println!("{val:?}");
            return Ok(val.clone());
        }
        let ptr = state.borrow().undefined.convert(VarType::VarVar);
        // println!("{ptr:?}");
        obj.insert(key, ptr.clone());
        return Ok(ptr);
    }
    let rhs_eval = inner_interpret(rhs, state)?;
    // println!("{lhs:?} op {rhs:?}");
    // println!("{lhs_eval:?} op {rhs_eval:?}");
    match op {
        Operation::Equal(1) => {
            lhs_eval.assign(&rhs_eval)?;
            Ok(rhs_eval)
        }
        Operation::Equal(precision) => Ok(lhs_eval.eq(&rhs_eval, precision - 1)),
        Operation::Add => Ok(lhs_eval + rhs_eval),
        Operation::Sub => Ok(lhs_eval - rhs_eval),
        Operation::Mul => Ok(lhs_eval * rhs_eval),
        Operation::Div => Ok(lhs_eval / rhs_eval),
        Operation::Mod => Ok(lhs_eval % rhs_eval),
        Operation::Dot => rhs_eval.with_ref(|rhs_eval| lhs_eval.dot(rhs_eval)),
        Operation::And => Ok(lhs_eval & rhs_eval),
        Operation::Or => Ok(lhs_eval | rhs_eval),
        Operation::AddEq => {
            lhs_eval += rhs_eval;
            Ok(lhs_eval)
        }
        Operation::SubEq => {
            lhs_eval -= rhs_eval;
            Ok(lhs_eval)
        }
        Operation::MulEq => {
            lhs_eval *= rhs_eval;
            Ok(lhs_eval)
        }
        Operation::DivEq => {
            lhs_eval /= rhs_eval;
            Ok(lhs_eval)
        }
        Operation::ModEq => {
            lhs_eval %= rhs_eval;
            Ok(lhs_eval)
        }
        Operation::Lt => Ok(Pointer::from(lhs_eval < rhs_eval)),
        Operation::Le => Ok(Pointer::from(lhs_eval <= rhs_eval)),
        Operation::Gt => Ok(Pointer::from(lhs_eval > rhs_eval)),
        Operation::Ge => Ok(Pointer::from(lhs_eval >= rhs_eval)),
        Operation::Arrow => todo!(),
    }
}

fn interpret_function(func: &Pointer, args: &[Syntax], state: RcMut<State>) -> SResult<Pointer> {
    func.with_ref(|func_eval|
        match func_eval {
            Value::Keyword(Keyword::If) => {
                let [condition, body, ..] = args else {
                        return Err(String::from("If statement requires two arguments: condition and body"))
                    };
                let condition_evaluated = inner_interpret(condition, state.clone())?;
                // println!("{condition_evaluated:?}");
                let bool = condition_evaluated.with_ref(Value::bool);
                if bool == Boolean::True {
                    inner_interpret(body, state)
                } else if let (Boolean::Maybe, Some(body)) = (bool, args.get(3)) {
                    inner_interpret(body, state)
                } else {
                    match args.get(2) {
                        Some(else_statement) => inner_interpret(else_statement, state),
                        None => Ok(state.borrow().undefined.clone())
                    }
                }
            }
            Value::Keyword(Keyword::Delete) => {
                if let [Syntax::Ident(key)] = args {
                    state.borrow_mut().delete(key.clone());
                }
                Ok(state.borrow().undefined.clone())
            }
            Value::Keyword(Keyword::Function) => {
                let [Syntax::Ident(name), args, body] = args else {
                        return Err(format!("Invalid arguments for `function`: `{args:?}`; expected name, args, and body"))
                    };
                let args = match args {
                    Syntax::Block(args) => args.clone(),
                    other => vec![other.clone()],
                };
                let args: Vec<Rc<str>> = args
                    .into_iter()
                    .map(|syn| match syn {
                        Syntax::Ident(str) => Ok(str),
                        other => Err(format!("Invalid parameter name: `{other:?}`")),
                    })
                    .collect::<Result<_, _>>()?;
                let inner_val = Value::Function(args, body.clone());
                state
                    .borrow_mut()
                    .insert(name.clone(), Pointer::from(inner_val));
                Ok(state.borrow().undefined.clone())
            }
            Value::Keyword(Keyword::Eval) => {
                let [body] = args else {
                    return Err(format!("You can only `eval` one thing at a time; got `{args:?}`"));
                };
                let text = inner_interpret(body, state.clone())?.to_string();
                // #[cfg(debug_assertions)]
                // println!("Evaluating Inner: {text}");
                let tokens = crate::lexer::tokenize(&text)?;
                // #[cfg(debug_assertions)]
                // println!("Evaluating Tokens: {tokens:?}");
                let syntax = crate::parser::parse(tokens)?;
                // #[cfg(debug_assertions)]
                // println!("Evaluating Syntax: {syntax:?}");
                inner_interpret(&syntax, state)
            }
            Value::Object(obj) => {
                let Some(call) = obj.get(&"call".into()) else {
                    return Err(format!("`Object({obj:?})` is not a function"))
                };
                let mut new_state = State::from_parent(state);
                new_state.insert("self".into(), func.clone());
                interpret_function(call, args, rc_mut_new(new_state))
            }
            Value::Function(fn_args, body) => {
                let mut inner_state = State::from_parent(state.clone());
                for (idx, ident) in fn_args.iter().enumerate() {
                    let arg_eval = if let Some(syn) = args.get(idx) {
                        inner_interpret(syn, state.clone())?
                    } else {
                        state.borrow().undefined.clone()
                    };
                    inner_state.insert(ident.clone(), arg_eval);
                }
                inner_interpret(body, rc_mut_new(inner_state))
            }
            other => Err(format!("`{other:?}` is not a function")),
        }
    )
}
