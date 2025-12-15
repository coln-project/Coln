\documentclass{acmart}

\geometry{twoside=false}
\usepackage{../macros}
\usepackage{agda}
\DeclareUnicodeCharacter{225F}{=^?}

\begin{document}

\title{Lecture 2 Notes}
\author{Owen Lynch}

\maketitle

\section{Recap from last time}

Last week we discussed:

\begin{enumerate}
  \item The theses behind the geolog research program
  \item The ``doctrinal'' view of categorical logic
  \item Theories as functors from the 2-category of models of a doctrine into $\ms{Cat}$
  \item The doctrine of regular hyperdoctrines
\end{enumerate}

\section{Overview of this lecture}

\begin{enumerate}
  \item A syntactic approach to regular theories
  \item Finite models of regular theories
  \item A type theory for regular theories
  \item What the geolog architecture could look like
\end{enumerate}

\section{Regular theories and finite models}

\begin{code}
open import Data.Unit
open import Data.Bool using (Bool)
import Data.Bool as Bool
open import Data.Fin using (Fin)
import Data.Fin as Fin
open import Data.Nat
open import Relation.Nullary.Decidable using (Dec)
\end{code}

\begin{code}
data Con (S : Set) : Set where
  • : Con S
  _▷_ : Con S -> S -> Con S

data Var {S : Set} : Con S -> S -> Set where
  q : {A : S} -> {Γ : Con S} -> Var (Γ ▷ A) A
  p_ : {A B : S} -> {Γ : Con S} -> Var Γ A -> Var (Γ ▷ B) A

module RegularLogic (Sort : Set) (Rel : Con Sort -> Set) where
  variable Γ Δ : Con Sort
  variable A B : Sort

  Tm = Var

  Sub : Con Sort -> Con Sort -> Set
  Sub Γ Δ = (A : Sort) -> Tm Δ A -> Tm Γ A
  
  data Prop : Con Sort -> Set where
    true : Prop Γ
    _$_ : Rel Δ -> Sub Γ Δ -> Prop Γ
    _∧_ : Prop Γ -> Prop Γ -> Prop Γ
    _===_ : Tm Γ A -> Tm Γ A -> Prop Γ
    ∃ : Prop (Γ ▷ A) -> Prop Γ

  record Axiom : Set where
    field
      con : Con Sort
      ante : Prop con
      cons : Prop con

record Sig : Set where
  field
    sorts : Con ⊤
    rels : Con (Con (Var sorts tt))

Sort : (S : Sig) -> Set
Sort S = Var (Sig.sorts S) tt

Rel : (S : Sig) -> Con (Sort S) -> Set
Rel S = Var (Sig.rels S)

record Theory : Set where
  field
    sig : Sig
    axioms : Con (RegularLogic.Axiom (Sort sig) (Rel sig))

Tuple : (S : Sig) -> (Sort S -> ℕ) -> Con (Sort S) -> Set
Tuple S elts Γ = (A : Sort S) -> Var Γ A -> Fin (elts A)
  
record ModelData (S : Sig) : Set where
  field
    elts : Sort S -> ℕ
    sat : {Γ : Con (Sort S)} -> Rel S Γ -> Tuple S elts Γ -> Bool

module Interp (S : Sig) (M : ModelData S) where
  open RegularLogic (Sort S) (Rel S)

  t = Tuple S (ModelData.elts M)

  Elt : Sort S -> Set
  Elt A = Fin (ModelData.elts M A)

  extend : (Γ : Con (Sort S)) -> t Γ -> (A : Sort S) -> Elt A -> t (Γ ▷ A)
  extend = {!!}

  any : {n : ℕ} -> (Fin n -> Bool) -> Bool
  any = {!!}

  allTuples : (Γ : Con (Sort S)) -> (t Γ -> Bool) -> Bool
  allTuples = {!!}

  eval : {Γ : Con (Sort S)} -> {A : Sort S} -> Tm Γ A -> t Γ -> Elt A
  eval {Γ} {A} v f = f A v

  evalSub : {Γ Δ : Con (Sort S)} -> Sub Γ Δ -> t Γ -> t Δ
  evalSub s f A v = f A (s A v)

  interp : {Γ : Con (Sort S)} -> Prop Γ -> t Γ -> Bool
  interp true _ = Bool.true
  interp (r $ xs) f = ModelData.sat M r (evalSub xs f)
  interp (ϕ ∧ ψ) f = interp ϕ f Bool.∧ interp ψ f
  interp (x₁ === x₂) f = Dec.does (eval x₁ f Fin.≟ eval x₂ f)
  interp (∃ {Γ} {A} ϕ) f = any (λ i -> interp ϕ (extend Γ f A i))

  satisfies : Axiom -> Bool
  satisfies a = allTuples
    (Axiom.con a)
    (λ f -> Dec.does (interp (Axiom.ante a) f Bool.≤? interp (Axiom.cons a) f))
\end{code}

\section{A type theory for regular logic}

The query engine for geolog will probably accept an IR format that looks something like an extended version of the above. However, for the user we would like to have the ability to define higher-level abstractions.

These higher-level abstractions take the form of a type theory.

To develop this type theory, we will first look to denotational semantics. The premise here is that the denotational semantics will give us a first pass at figuring out which rules are admissable in the type theory; we will then have to refine the type theory in order to

\begin{enumerate}
  \item Make elaboration feasible (which will involve switching to intensional equality)
  \item Allow the user more control over operational semantics
\end{enumerate}

In the denotational semantics, each context in the type theory will correspond to a \emph{semantic regular theory}.

\begin{definition}
  A \defcase{semantic regular theory} consists of a representable 2-functor from $\ms{Reg}$, the 2-category of regular categories, to $\ms{Cat}$.
\end{definition}

If $\Gamma \colon \ms{Reg} \to \ms{Cat}$ is a representable 2-functor, we will denote its representing object by $S[\Gamma]$.

Substitutions correspond to 2-natural transformations.

A theory $T$ in a context $\Gamma$ corresponds to a representable functor from $\int \Gamma$ to $\ms{Cat}$. A model $t \colon T$ is a section of that functor, e.g. $t((\mc{C},M)) \in T((\mc{C},M))$ for each $(\mc{C},M) \in \int \Gamma$.

We can thus start building up rules.

\begin{ttrule}[The object classifier]
\[\inferrule{}{\yields \ms{Type} \jdg{theory}}\]
This is the forgetful functor $\ms{Reg} \to \ms{Cat}$. It is representable by $\ms{Fin}\op$.
\end{ttrule}

\begin{ttrule}[Theory of an element]
\[\inferrule{\Gamma \yields A \colon \ms{Type}}{\Gamma \yields \ms{Elt}(A) \jdg{theory}}\]
We have $A(\mc{C},M) \in \mc{C}$ for all $(\mc{C},M) \in \int \Gamma$; $\ms{Elt}(A)(\mc{C},M) = \mc{C}(1, A(\mc{C},M))$. It is representable by $S[\Gamma]/A(S[\Gamma], M[\Gamma])$.
\end{ttrule}

\begin{ttrule}[The proposition classifier]
\[\inferrule{}{\yields \ms{Prop} \jdg{theory}}\]
This is the functor $\ms{Reg} \to \ms{Cat}$ that sends $\mc{C}$ to $\ms{Sub}(1)$. It is representable by $\mb{2}$, the walking arrow.
\end{ttrule}

\begin{ttrule}[Propositions as types]
  \[\inferrule{\Gamma \yields P \colon \ms{Prop}}{\Gamma \yields \ms{Proof}(P) \colon \ms{Type}}\]
  Any subobject of $1$ is also an object.
\end{ttrule}

\begin{ttrule}[Sigma types]
\[\inferrule{}{}\]
\end{ttrule}

\end{document}
