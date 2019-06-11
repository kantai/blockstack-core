use vm::errors::{Error, ErrType, InterpreterResult as Result};
use vm::types::{Value};
use vm::representations::{SymbolicExpression,SymbolicExpressionType};
use vm::{LocalContext, Environment, eval};

pub fn tuple_cons(args: &[SymbolicExpression], env: &mut Environment, context: &LocalContext) -> Result<Value> {
    // (tuple #arg-name value
    //        #arg-name value ...)

    // or actually:
    //    (tuple (arg-name value)
    //           (arg-name value))
    use super::parse_eval_bindings;

    if args.len() < 1 {
        return Err(Error::new(ErrType::InvalidArguments(format!("Tuples must be constructed with named-arguments and corresponding values"))))
    }

    let bindings = parse_eval_bindings(args, env, context)?;

    Value::tuple_from_data(bindings)
}

pub fn tuple_get(args: &[SymbolicExpression], env: &mut Environment, context: &LocalContext) -> Result<Value> {
    // (get arg-name (tuple ...))
    //    if the tuple argument is an option type, then return option(field-name).

    if args.len() != 2 {
        return Err(Error::new(ErrType::InvalidArguments(format!("(get ..) requires exactly 2 arguments"))))
    }
    let arg_name = match args[0].expr {
        SymbolicExpressionType::Atom(ref name) => Ok(name),
        _ => Err(Error::new(ErrType::InvalidArguments(format!("Second argument to (get ..) must be a name, found: {:?}", args[0]))))
    }?;

    let value = eval(&args[1], env, context)?;

    match value {
        Value::Optional(ref opt_data) => {
            match opt_data.data {
                Some(ref data) => {
                    let data: &Value = data;
                    if let Value::Tuple(tuple_data) = data {
                        Ok(Value::some(tuple_data.get(arg_name)?))
                    } else {
                        Err(Error::new(ErrType::TypeError("TupleType".to_string(), data.clone())))
                    }
                },
                None => Ok(value.clone()) // just pass through none-types.
            }
        },
        Value::Tuple(tuple_data) => tuple_data.get(arg_name),
        _ => Err(Error::new(ErrType::TypeError("TupleType".to_string(), value.clone())))
    }
}