# Dependent Types + Proof Terms Design Draft

This document sketches a lightweight, Klassic-style dependent type system with
proof terms, trust propagation, and erasure. The goal is to keep syntax simple
while enabling practical, explicit proofs.

## Goals
- Keep Klassic syntax lightweight and familiar (no tactic language).
- Add dependent function types and dependent data (GADT-style enums).
- Support proof terms with explicit trust/axiom markers.
- Track proof status (Verified vs Trusted) and allow strict builds.
- Erase proofs at runtime and keep normal execution behavior unchanged.

## Non-goals (initial)
- Full type-in-type / universe polymorphism.
- Automation/tactics, SMT, or proof search.
- Implicit coercions from Prop to Bool in runtime contexts.

## Core Ideas
### Prop vs Bool (with auto-lift)
- Prop is the type of propositions (proof-only, erasable).
- Bool is a runtime value type (used by if/while/etc).
- In proof positions, Bool expressions are auto-lifted to Prop via `IsTrue`.
  This keeps writing proofs lightweight without collapsing Prop and Bool.

Example (auto-lift in theorem signatures):
```klassic
theorem sortedAppend(xs: List<Int>, ys: List<Int>):
  { isSorted(append(xs, ys)) } =
  ...
```
Internally, this is treated as:
```klassic
theorem sortedAppend(...): IsTrue(isSorted(append(xs, ys))) = ...
```

### Proof status
Each proof-carrying definition is labeled:
- Verified: uses only verified definitions and total computation.
- Trusted: depends on a trusted axiom/theorem or a trust-marked definition.

Strict mode:
- `--deny-trust` rejects any use of Trusted proofs.
- `--warn-trust` emits warnings but allows them.

## Syntax Additions (lightweight)
### Theorem and axiom
```klassic
theorem name(params...): { proposition } = expr
trust theorem name(params...): { proposition } = expr
axiom name(params...): { proposition }
```
Notes:
- `theorem` requires a proof term and passes totality checks.
- `trust theorem` accepts a proof term but marks the result Trusted.
- `axiom` has no body and is always Trusted.

### Dependent function types
No new syntax; return type may refer to value parameters:
```klassic
def head(xs: Vec<'a, n>): Vec<'a, n - 1> = ...
```

### Dependent records
Record field types can reference earlier fields:
```klassic
record SortedList<'a> {
  list: List<'a>;
  proof: { isSorted(list) };
}
```

### GADT-style enum constructors
Allow explicit constructor result types:
```klassic
enum Vec<'a, n: Nat> {
  | Nil: Vec<'a, 0>
  | Cons(x: 'a, xs: Vec<'a, n>): Vec<'a, n + 1>
}
```

### Pattern matching
Add a lightweight `match` expression:
```klassic
match xs {
  Nil -> ...
  Cons(x, xs2) -> ...
}
```
Patterns (initial):
- `_` wildcard
- identifier binder
- constructor pattern `Ctor(p1, p2, ...)`
- literal patterns (Int, Bool, String)

### Dependent GADT indices
`Vec<'a, n>` is valid and the index `n` may appear in constructor result types. The typer tracks index relations by normalizing scrutinee indices against branch annotations (e.g. `Cons(x, xs): Vec<'a, n + 1>` when matching a `Vec<'a, n>`), so recursive proofs can reason about how values evolve.

To keep the syntax lightweight, the parser treats `Vec<'a, n>` as a GADT indexed over a small `Nat` language. The enum itself looks like:

```klassic
enum Vec<'a, n: Nat> {
  | Nil: Vec<'a, Zero>
  | Cons(x: 'a, xs: Vec<'a, n>): Vec<'a, Succ<n>>
}
```

The `Nat` type (e.g., `Zero`, `Succ(n)`) is special-cased within the typer so we can normalize index expressions before checking constructors: `Cons` is only allowed when the enclosing `Vec` index is `Succ<n>`. The parser supports inline `Nat` expressions in type arguments (e.g., `Vec<'a, n + 1>`) as long as they reduce to a `Nat` constant; this is the minimal index-term language we need for `Vec` lengths without introducing full dependent arithmetic.

## Type-level Terms and Indices
Dependent types require term expressions inside types. Proposed shape:
- Extend `Type` to carry term arguments alongside type arguments.
- Introduce a restricted "index term" language for type-level computation.

Possible representation:
```rust
sealed trait TypeArg
case class TypeArgType(tpe: Type) extends TypeArg
case class TypeArgTerm(term: Ast.Node) extends TypeArg

case class TConstructor(name: String, args: List[TypeArg], kind: Kind)
case class TPropTerm(term: Ast.Node) extends Type
case object TProp extends Type("Prop")
```

Index terms should be:
- Pure (no IO, no mutation)
- Total (termination-checked when using recursion)
- Normalizable (beta/delta/iota)

### Nat-indexed Vec semantics
- Introduce a lightweight `Nat` enum whose constructors (`Zero`, `Succ(n)`) serve both as values and as index-level expressions. The parser treats `Nat` constructors as ordinary enums so they can be constructed/inspected at runtime, but the typer also recognizes `Nat` literals when they appear inside type arguments.
- `Vec<'a, n>` is declared as a regular enum with constructors that specify their resulting index:
  ```klassic
  enum Vec<'a, n: Nat> {
    | Nil: Vec<'a, Zero>
    | Cons(x: 'a, xs: Vec<'a, n>): Vec<'a, Succ<n>>
  }
  ```
- The constructor result types (`resultType` in `DataConstructor`) carry Nat expressions; during typing we normalize them by evaluating any inline additions or `Succ`/`Zero` constructors to maintain a canonical `Nat` representation (e.g., `Succ<Succ<Zero>>`).
- Matching on `Vec` must compare the scrutinee index with each branch’s expected index (e.g., `Cons` requires that the scrutinee index unifies with `Succ<k>`). We keep a small bag of index constraints so each branch refines the scrutinee type before checking the body.
- Pattern binders introduced by constructors (like `x` and `xs` in `Cons`) are added via the existing `extendEnvForPattern` helper but now also carry the refined type info computed from the Nat constraint. This lets theorem bodies refer to the reified `n` inside the branch without extra hacking.

### Trust levels and authority
`trust theorem` and `axiom` nodes are tagged with a trusted level. A theorem depending only on Verified proofs remains level `trusted<0>` (i.e. Verified). If it uses a `trust theorem` of level `trusted<k>`, the caller escalates to `trusted<k+1>` (transitive escalation). `axiom` declarations start at `trusted<1>`, so any theorem depending directly on an axiom becomes `trusted<2>`.

`--deny-trust` rejects any theorem whose dependency graph contains a node with level ≥ 1. `--warn-trust` logs each time a verified theorem gains a trusted ancestor, reporting the level so users can know how deep the trust chain is. This encourages keeping trusted hops minimal while still allowing staged trust in practice.

Level tracking feeds back into the CLI: warnings now mention `trusted<k>` explicitly, and `--deny-trust` behaves like `trusted<0>` mode so any non-zero level leads to a type error. When a user imports a proof from another module, the metadata carries the highest level seen in the dependency closure so that trust information is not accidentally erased.

## Totality and Termination
Theorem bodies must be total:
- No `while`, `foreach`, or mutation.
- Only recursive calls that are structurally smaller.
- Only total functions can be called from theorems.

Implementation strategy:
- Add `total def` (optional) for proof-safe helpers.
- Enforce structural recursion on an inductive parameter via pattern matching.
- Reject calls into non-total defs inside theorem bodies.

## Equality and Rewriting
Provide `Eq` as a definable inductive type (or builtin for v1):
```klassic
enum Eq<'a, x: 'a, y: 'a> {
  | Refl: Eq<'a, x, x>
}

theorem symm<'a, x: 'a, y: 'a>(p: Eq<'a, x, y>): Eq<'a, y, x> =
  match p {
    Refl -> Refl
  }
```
Dependent match allows rewriting by pattern matching on `Refl`.

## Definitional Equality (Normalization)
Type equality extends beyond syntactic equality:
- Beta: apply lambdas
- Delta: unfold total definitions
- Iota: reduce `match` on constructors

Normalization runs on the restricted index-term subset to keep it predictable.

## Proof Erasure
Proof terms are erased before runtime:
- Values of Prop are removed or compiled to Unit.
- Proof fields in records are erased from runtime layouts.
- Runtime evaluation never executes proof-only terms after erasure.

## Compiler/Typechecker Changes (High Level)
### Parser
Files: `crates/klassic-syntax/src/lib.rs`
- Add keywords: `theorem`, `axiom`, `trust`, `match`.
- Add theorem/axiom definitions with proposition annotations.
- Add GADT constructor result types.
- Add `match` expression and pattern grammar.

### AST
Files: `crates/klassic-syntax/src/lib.rs`
- Add `TheoremDefinition` and `AxiomDeclaration` (or extend `FunctionDefinition` with flags).
- Add `MatchExpression` and pattern nodes.
- Extend `EnumDeclaration` and `DataConstructor` with result types.

### Types
Files: `crates/klassic-types/src/lib.rs`
- Add `TProp`, `TPropTerm`, `TypeArg` structure.
- Extend `TConstructor` to accept term arguments.

### Typer
Files: `crates/klassic-types/src/lib.rs`
- Add totality checker for theorem bodies.
- Track proof status (Verified/Trusted).
- Extend unification/equality with normalization on term arguments.
- Support constructor-pattern matches: compute refined scrutinee types for Nat-indexed `Vec` constructors, solve the small index constraint set, and allow constructors to introduce new bindings with the refined types.
- Enforce Prop-only theorem results (Bool auto-lift).

### Typed IR + Runtime
Files: `crates/klassic-types/src/lib.rs`, `crates/klassic-eval/src/lib.rs`
- Carry proof-status metadata for bindings.
- Erase proofs before runtime evaluation.
- Add `match` evaluation rules.
 
## Implementation Tasks Overview
1. **Parser / Syntax**
   - Add keywords `theorem`, `trust`, `axiom`, `match`.
   - Parse theorem/axiom definitions with proposition blocks `: { ... }`.
   - Extend `enum` parsing so constructors can carry dependent result types (e.g. `Cons(...): Vec<'a, n + 1>`).
   - Parse `match` expressions and the lightweight pattern grammar (constructors, literals, `_`, binders).
2. **AST / Representation**
   - Introduce `TheoremDefinition`/`AxiomDeclaration` nodes with proof bodies and trust metadata.
   - Extend `EnumDeclaration`/`DataConstructor` to accept explicit `resultType: Option[Type]`.
   - Add `MatchExpression` along with `Pattern`/`PatternBinder` nodes for typing.
3. **Type System**
   - Add `TProp`, `TPropTerm`, and `TypeArg` to represent type vs term arguments.
   - Allow constructors to accept term indices and normalize them during equality checking.
   - Auto-lift Bool expressions to `IsTrue` in theorem return types.
4. **Typer & Totality**
   - Track proof status (Verified/Trusted + level) per definition and propagate over dependencies.
   - Enforce that theorem bodies only call verified total defs and check structural recursion.
   - Build a dependency graph to compute trusted levels and reject `--deny-trust` violations.
   - Normalize dependent indices in `match` via branch constraints so pattern matching refines types.
5. **Typed IR & Runtime**
   - Carry proof metadata for erasure decisions.
   - Evaluate `match` expressions while erasing prop-only branches.
   - Ensure total helpers are recognized as pure for normalization and erasure.
6. **Stdlib, Tests & CLI**
   - Provide `Nat`, `Vec`, `Eq`, `Sorted`, etc., plus example theorems.
   - Add regression tests for trust leveling, totality checking, match normalization, and CLI flags.
   - Document proof patterns and CLI options in README/examples.

## Examples
### Length of append (sketch)
```klassic
enum Nat {
  | Z: Nat
  | S(n: Nat): Nat
}

def add(n: Nat, m: Nat): Nat =
  match n {
    Z -> m
    S(n1) -> S(add(n1, m))
  }

theorem addAssoc(n: Nat, m: Nat, k: Nat):
  { add(add(n, m), k) == add(n, add(m, k)) } =
  match n {
    Z -> Refl
    S(n1) -> ...
  }
```

## Implementation Plan (Phased)
1) Parser + AST + Type nodes for theorem/axiom/match and Prop terms.
2) Typechecker support for dependent types and Prop/Bool lifting.
3) Totality/termination checker and proof-status tracking.
4) Runtime support for match and proof erasure.
5) Stdlib additions (Nat, Eq, Vec) and tests.

## Open Questions
- How strict should the index-term subset be (allow arithmetic on Nat only)?
- How much definitional equality is needed initially (delta unfolding depth)?
- Exact syntax for term arguments in type application (implicit vs explicit marker).
