# Contributing to Blockstack Core

Blockstack Core is open-source software written in Rust.  Contributions
should adhere to the following best practices.

## Simplicity of implementation

The most important consideration when accepting or rejecting a contribution is
the simplicity (i.e. ease of understanding) of its implementation.
Contributions that are "clever" or introduce functionality beyond the scope of
the immediate problem they are meant to solve will be rejected.

### Type simplicity

Simplicity of implementation includes simplicity of types.  Type parameters
and associated types should only be used if there are at
least two possible implementations of those types.

Lifetime parameters should only be introduced if the compiler cannot deduce them
on its own.

## Builds with a stable Rust compiler

We use a recent, stable Rust compiler.  Contributions should _not_
require nightly Rust features to build and run.

## Each file should include relevant unit tests

Each Rust file should contain a `mod test {}` definition, in which unit tests
should be supplied for the file's methods.  Unit tests should cover a maximal
amount of code paths.

## Use built-in logging facilities

Blockstack Core implements logging macros in `util::log`.  If your code needs to
output data, it should use these macros _exclusively_ for doing so.  The only
exception is code that is explicitly user-facing, such as help documentation.

## Minimal dependencies

Adding new package dependencies is very much discouraged.  Exceptions will be
granted on a case-by-case basis, and only if deemed absolutely necessary.

## Minimal global macros

Adding new global macros is discouraged.  Exceptions will only be given if
absolutely necessary.

## Minimal compiler warnings

Contributions should not trigger compiler warnings if possible, and should not
mask compiler warnings with macros.  Common sources of compiler warnings that
will not be accepted include, but are not limited to:

* unnecessary imports
* unused code
* variable naming conventions
* unhandled return types

## Minimal `unsafe` code

Contributions should not contain `unsafe` blocks if at all possible.

## Code block consistency

Surrounding code blocks with `{` and `}` is encouraged, even when the enclosed
block is a single statement.  Blocks in the same lexical scope must use
consistent conventions.  For example, consider the following:

```
match foo {
   1..2 => {
      // this is a single statement, but it is surrounded
      // with { and } because the other blocks in the match
      // statement need them.
      Ok(true)
   },
   3..4 => {
      error!("Bad value for foo");
      Err(Error::BadFoo)
   },
   _ => {
      // similarly, this block uses { }
      Ok(true)
   }
}

// conversely, all of the arms of this match statement
// have one-statement blocks, so { and } can be elided.
match bar {
   1..2 => Some("abc"),
   3..4 => Some("def"),
   _ => None
}  
```

## Error definitions

Each module should include an `Error` enumeration in its `mod.rs` that encodes
errors specific to the module.  All error code paths in the module should return
an `Err` type with one of the module's errors.

## Whitespace

All contributions should use the same whitespacing as the rest of the project.
Moreover, Pull requests where a large number of changes only deal with whitespace will be
rejected.

## Licensing and contributor license agreement.

Blockstack Core is released under the terms of the GPL version 3.  Contributions
that are not licensed under compatible terms will be rejected.  Moreover,
contributions will not be accepted unless _all_ authors accept the project's
contributor license agreement.


