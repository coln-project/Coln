use crate::{host::operator::Operator, scalarial::ScalarType};
use std::fmt;

struct Predicate<Identifier, Rule> {
    /// The name (unique identifier) of the predicate.
    name: Identifier,
    /// Contains non-recursive rules.
    non_rec_rules: Vec<Rule>,
    /// Contains both self-recursive and mutually-recursive rules.
    rec_rules: Vec<Rule>,
}

impl<I: Identifier, Rule> Identifiable<I> for Predicate<I, Rule> {
    fn id(&self) -> &I {
        &self.name
    }
}

trait Identifier: Clone + fmt::Debug + fmt::Display + PartialEq + Eq + std::hash::Hash {}

trait Identifiable<Identifier> {
    fn id(&self) -> &Identifier;
}

/// A rule contains atoms and conditions, sometimes united under the umbrella
/// term _proposition_.
trait Rule: Identifiable<Self::Identifier> {
    type Identifier: Identifier;
    type Atom: Atom<Identifier = Self::Identifier>;
    type Cond: Cond;

    fn atoms(&self) -> impl Iterator<Item = &Self::Atom>;
    fn conditions(&self) -> impl Iterator<Item = &Self::Cond>;

    fn is_recursive_with(&self, identifier: &Self::Identifier) -> bool {
        self.atoms().find(|atom| atom.id() == identifier).is_some()
    }
}

trait Atom: Identifiable<Self::Identifier> {
    type Identifier: Identifier;
    type Var: TypedVar<Identifier = Self::Identifier>;
    type Lit: Lit;

    /// What's in the parenthesis, in order.
    fn bindings(&self) -> impl Iterator<Item = Bind<&Self::Var, &Self::Lit>>;

    /// All variables brought into scope by this [`Atom`].
    fn vars(&self) -> impl Iterator<Item = &Self::Var>;
}

/// Binds something to either a variable or a literal value.
enum Bind<Var, Lit> {
    Var(Var),
    Lit(Lit),
}

trait Cond {
    type Identifier: Identifier;
    type Var: TypedVar<Identifier = Self::Identifier>;
    type Lit: Lit;

    fn operator(&self) -> Operator;
    fn left(&self) -> Bind<&Self::Var, &Self::Lit>;
    fn right(&self) -> Bind<&Self::Var, &Self::Lit>;
}

trait TypedVar: Identifiable<Self::Identifier> {
    type Identifier: Identifier;

    fn name(&self) -> &Self::Identifier {
        self.id()
    }

    fn ty(&self) -> ScalarType;
}

trait Lit {}
