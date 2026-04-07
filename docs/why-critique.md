# Claude Opus's critique of "Why Geolog?"

## Section 1: National security case for formal verification

- The argument flows well from AI-found vulnerabilities → offense-defense imbalance
  → destabilisation. But there's a significant gap: formal verification proves conformance to
  a specification, and the specification itself can be wrong or incomplete. A formally verified
  kernel can still have vulnerabilities if the spec doesn't capture the right security properties.
  You acknowledge this parenthetically in line 176 ("provided that the proof checker and its axioms
  are correct") but it deserves attention here in Section 1, where you're making the strongest
  security claims.
- The scope claim is also arguably too strong. Even a provably secure kernel doesn't help with
  application-layer bugs, misconfigurations, social engineering, or supply chain attacks — which
  account for a large share of real-world breaches. Acknowledging that formal verification of core
  infrastructure is necessary but not sufficient would make the argument more credible, not weaker.

## Section 2: Case for a new proof assistant

- The core hypothesis — that Lean won't scale — is the crux of the whole document, but the evidence
  is thin. The three bullet points (no multi-agent collaboration, slow LSP, text-based) describe
  engineering limitations that could arguably be fixed within Lean's ecosystem. The document doesn't
  address why extending Lean is harder than building from scratch. This is the classic
  build-vs-extend question, and a skeptical reader (especially one already invested in Lean) will
  find the case for "build" under-argued.
- The assumption that many parallel agents are necessary is used as a premise here, but you don't
  acknowledge until Section 5 (line 216) that this is actually an open question. A reader who doubts
  the parallel-agents premise will find the rest of the argument unpersuasive. Consider flagging the
  uncertainty earlier, or at least saying "if we are right that parallelism is needed, then..."
- The version control argument (line 81–82) — that concurrent work on Lean files would lead to messy
  Git merge conflicts — is plausible but hand-wavy. Thousands of large software projects manage
  concurrent Git work. What specifically makes proof merges harder than code merges? The structured
  nature of proofs (where a small textual conflict can invalidate a large proof tree) might be the
  answer, but it's not stated.

## Section 3: Case for a new DBMS

- This section is well-argued and clearly structured. The progression from "server-centric DB won't
  work" → "Git is closer but text-only" → "Automerge is closest but JSON-only" → "build on Automerge
  with a new data model" is convincing.
- One tension: the local-first argument (avoid network round-trips for speed) sits somewhat
  uncomfortably with the later discussion of peer-to-peer replication and BFT verification (Section
  5). If agents ultimately need to coordinate and merge, the coordination overhead isn't eliminated
  — it's deferred. Is deferred coordination actually faster for proof search than synchronous
  coordination? That's worth a sentence.

## Section 4: Case for a new data model

- The argument that SQL's constraints are too weak for local-first merging is the strongest and most
  novel part of the document. The connection between proof checking and constraint checking (lines
  174–176) is elegant and should arguably appear earlier — it's the deep insight that unifies the
  "proof assistant" and "database" aspects of Geolog.
- The claim on line 195 that "anything that can be proved in Lean can also be proved in Geolog" is
  strong and unsubstantiated. It needs either a justification or a softer qualifier ("we believe
  that...").
- The geometric logic restriction is well-motivated (incremental checking, monotonicity), but
  a skeptical reader will want to know: what can't you express? One concrete example of a useful
  constraint that geometric logic excludes, and how you work around it, would help readers
  calibrate.
- The compositionality of theories (lines 206–211) is compelling but comes late and reads as an
  afterthought. It might deserve more prominence.

## Section 5: Research roadmap

- The honesty about the early stage and untested hypotheses is a strength. But the SNARK paragraph
  (lines 290–295) feels like a tangent. It's flagged as "slightly wild," and the computational cost
  of generating SNARKs for a proof of kernel-scale would be astronomical with current technology.
  Including it risks undermining the credibility of the more grounded arguments. Consider cutting it
  or moving it to a "future directions" aside.
- The "10-year database" argument (line 107, echoed in line 135) cuts both ways. You use it to argue
  that Automerge gives you a head start, but a skeptic could equally say it strengthens the case for
  using an existing, mature database rather than building a new one. You might want to preempt that
  reading.

## Structural / rhetorical observations

- The document's greatest structural weakness is that Sections 2, 3, and 4 each argue for "why new
  X?" but they feel somewhat independent. The unifying insight — that proof checking is database
  constraint checking, and that this unification is what makes a combined system more than the sum
  of its parts — only emerges in Section 4 (lines 174–176). Foreshadowing this earlier would
  strengthen the narrative arc.
- There is no discussion of other proof assistants besides Lean (Rocq, Isabelle) or other approaches
  to scaling verification (modular verification, contract-based design, compositional reasoning).
  Even a brief paragraph explaining why these don't suffice would preempt an obvious objection.
- The document is at its most convincing when it's concrete (the MLIR example, the Linux kernel
  locking primitive, the seL4 statistics) and weakest when it's abstract. More concrete examples
  throughout would help.
