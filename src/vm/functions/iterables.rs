use vm::costs::{cost_functions, CostOverflowingMath};
use vm::errors::{CheckErrors, RuntimeErrorType, InterpreterResult as Result, check_argument_count};
use vm::types::{Value, ListData, signatures::ListTypeData, TypeSignature::BoolType, TypeSignature};
use vm::representations::{SymbolicExpression, SymbolicExpressionType};
use vm::{LocalContext, Environment, eval, apply, lookup_function};
use std::convert::TryInto;
use std::cmp;

pub fn list_cons(args: &[SymbolicExpression], env: &mut Environment, context: &LocalContext) -> Result<Value> {
    let eval_tried: Result<Vec<Value>> =
        args.iter().map(|x| eval(x, env, context)).collect();
    let args = eval_tried?;

    let mut arg_size = 0;
    for a in args.iter() {
        arg_size = arg_size.cost_overflow_add(a.size().into())?;
    }

    runtime_cost!(cost_functions::LIST_CONS, env, arg_size)?;

    Value::list_from(args)
}

pub fn special_filter(args: &[SymbolicExpression], env: &mut Environment, context: &LocalContext) -> Result<Value> {
    check_argument_count(2, args)?;

    runtime_cost!(cost_functions::FILTER, env, 0)?;

    let function_name = args[0].match_atom()
        .ok_or(CheckErrors::ExpectedName)?;

    let function = lookup_function(&function_name, env)?;
    let iterable = eval(&args[1], env, context)?;

    match iterable {
        Value::List(mut list) => {
            let mut filtered_vec = Vec::new();
            for x in list.data.drain(..) {
                let argument = [ SymbolicExpression::atom_value(x.clone()) ];
                let filter_eval = apply(&function, &argument, env, context)?;
                if let Value::Bool(include) = filter_eval {
                    if include {
                        filtered_vec.push(x);
                    } // else, filter out.
                } else {
                    return Err(CheckErrors::TypeValueError(BoolType, filter_eval).into())
                }
            }
            Value::list_with_type(filtered_vec, list.type_signature)
        },
        Value::Buffer(mut buff) => {
            let mut filtered_vec = Vec::new();
            for x in buff.data.drain(..) {
                let v = Value::buff_from(vec![x.clone()])?;
                let argument = [ SymbolicExpression::atom_value(v) ];
                let filter_eval = apply(&function, &argument, env, context)?;
                if let Value::Bool(include) = filter_eval {
                    if include {
                        filtered_vec.push(x);
                    } // else, filter out.
                } else {
                    return Err(CheckErrors::TypeValueError(BoolType, filter_eval).into())
                }
            }
            Value::buff_from(filtered_vec)
        },
        _ => Err(CheckErrors::ExpectedListOrBuffer(TypeSignature::type_of(&iterable)).into())
    }
}

pub fn special_fold(args: &[SymbolicExpression], env: &mut Environment, context: &LocalContext) -> Result<Value> {
    check_argument_count(3, args)?;

    runtime_cost!(cost_functions::FILTER, env, 0)?;

    let function_name = args[0].match_atom()
        .ok_or(CheckErrors::ExpectedName)?;

    let function = lookup_function(&function_name, env)?;
    let iterable = eval(&args[1], env, context)?;
    let initial = eval(&args[2], env, context)?;

    let mapped_args: Vec<_> = match iterable {
        Value::List(mut list) => {
            list.data.drain(..).map(|x| {
                SymbolicExpression::atom_value(x)
            }).collect()
        },
        Value::Buffer(mut buff) => {
            buff.data.drain(..).map(|x| {
                SymbolicExpression::atom_value(Value::buff_from_byte(x))
            }).collect()
        },
        _ => return Err(CheckErrors::ExpectedListOrBuffer(TypeSignature::type_of(&iterable)).into())
    };
    mapped_args.iter().try_fold(initial, |acc, x| {
        apply(&function, &[x.clone(), SymbolicExpression::atom_value(acc)], env, context)
    })
}

pub fn special_map(args: &[SymbolicExpression], env: &mut Environment, context: &LocalContext) -> Result<Value> {
    check_argument_count(2, args)?;

    runtime_cost!(cost_functions::MAP, env, 0)?;

    let function_name = args[0].match_atom()
        .ok_or(CheckErrors::ExpectedName)?;
    let iterable = eval(&args[1], env, context)?;
    let function = lookup_function(&function_name, env)?;

    let mapped_args: Vec<_> = match iterable {
        Value::List(mut list) => {
            list.data.drain(..).map(|x| {
                vec![SymbolicExpression::atom_value(x)]
            }).collect()
        },
        Value::Buffer(mut buff) => {
            buff.data.drain(..).map(|x| {
                vec![SymbolicExpression::atom_value(Value::buff_from_byte(x))]
            }).collect()
        },
        _ => return Err(CheckErrors::ExpectedListOrBuffer(TypeSignature::type_of(&iterable)).into())
    };
    let mapped_vec: Result<Vec<_>> =
        mapped_args.iter().map(|argument| apply(&function, &argument, env, context)).collect();
    Value::list_from(mapped_vec?)
}

pub fn special_append(args: &[SymbolicExpression], env: &mut Environment, context: &LocalContext) -> Result<Value> {
    check_argument_count(2, args)?;

    let iterable = eval(&args[0], env, context)?;
    match iterable {
        Value::List(list) => {
            let element =  eval(&args[1], env, context)?;
            let ListData { mut data, type_signature } = list;
            let (entry_type, size) = type_signature.destruct();
            let element_type = TypeSignature::type_of(&element); 
            runtime_cost!(cost_functions::APPEND, env,
                          u64::from(cmp::max(entry_type.size(), element_type.size())))?;
            if entry_type.is_no_type() {
                assert_eq!(size, 0);
                return Value::list_from(vec![ element ])
            }
            if let Ok(next_entry_type) = TypeSignature::least_supertype(&entry_type, &element_type) {
                let next_type_signature = ListTypeData::new_list(next_entry_type, size + 1)?;
                data.push(element);
                Ok(Value::List(ListData {
                    type_signature: next_type_signature,
                    data }))
            } else {
                Err(CheckErrors::TypeValueError(entry_type, element).into())
            }
        },
        _ => Err(CheckErrors::ExpectedListApplication.into())
    }
}

pub fn special_concat(args: &[SymbolicExpression], env: &mut Environment, context: &LocalContext) -> Result<Value> {
    check_argument_count(2, args)?;

    let lhs = eval(&args[0], env, context)?;
    let rhs = eval(&args[1], env, context)?;

    runtime_cost!(cost_functions::CONCAT, env,
                  u64::from(lhs.size()).cost_overflow_add(
                      u64::from(rhs.size()))?)?;

    match (lhs, rhs) {
        (Value::List(lhs_data), Value::List(mut rhs_data)) => {
            let mut data = lhs_data.data;
            data.append(&mut rhs_data.data);
            Value::list_from(data)
        },
        (Value::Buffer(lhs_data), Value::Buffer(mut rhs_data)) => {
            let mut data = lhs_data.data;
            data.append(&mut rhs_data.data);
            Value::buff_from(data)
        },
        (_, _) => {
            Err(RuntimeErrorType::BadTypeConstruction.into())
        }
    }
}

pub fn special_as_max_len(args: &[SymbolicExpression], env: &mut Environment, context: &LocalContext) -> Result<Value> {
    check_argument_count(2, args)?;

    let mut iterable = eval(&args[0], env, context)?;

    runtime_cost!(cost_functions::AS_MAX_LEN, env, 0)?;

    if let Some(Value::UInt(expected_len)) = args[1].match_literal_value() {
        let iterable_len = match iterable {
            Value::List(ref list) => list.data.len(),
            Value::Buffer(ref buff) => buff.data.len(),
            _ => return Err(CheckErrors::ExpectedListOrBuffer(TypeSignature::type_of(&iterable)).into())
        };
        if iterable_len as u128 > *expected_len {
            Ok(Value::none())
        } else {
            if let Value::List(ref mut list) = iterable {
                list.type_signature.reduce_max_len(*expected_len as u32);
            }
            Ok(Value::some(iterable)?)
        }
    } else {
        let actual_len = eval(&args[1], env, context)?;
        Err(CheckErrors::TypeError(TypeSignature::UIntType, TypeSignature::type_of(&actual_len)).into())
    }
}

pub fn native_len(iterable: Value) -> Result<Value> {
    match iterable {
        Value::List(list) => Ok(Value::UInt(list.data.len() as u128)),
        Value::Buffer(buff) => Ok(Value::UInt(buff.data.len() as u128)),
        _ => Err(CheckErrors::ExpectedListOrBuffer(TypeSignature::type_of(&iterable)).into())
    }
}
