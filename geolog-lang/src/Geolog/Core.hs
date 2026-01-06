module Geolog.Core where

import Geolog.Common

data Abs a = Abs Name a

-- How do we deal with the fact that, in general, a top-level binding is for a
-- judgment in the theory?

-- I guess evaluation by default returns a VTmJ/VTyJ, and then we coerce it
-- to a VTm when necessary?

-- Alternatively, we could have JLam as a constructor of VTm (VJLam).

-- What is the right way to handle argument telescopes in nbe?

-- Thought: named theory telescopes are the natural unit of subtyping. If we
-- want theories with arguments, they should desugar to specializations of
-- telescopes.

data Tm
  = Local BId
  | Global QName
  | Code Ty
  | App Tm Tm
  | Lam (Abs Tm)
  | Proj Tm Name
  | Cons [(Name, Tm)]

data Ty
  = TyGlobal QName
  | TyApp Ty Tm
  | Elt Tm
  | U
  | Pi Tm (Abs Ty)
  | Record [(Name, Tm)]

type Env = Bwd VTm

data Spine
  = SId
  | SApp Spine VTm
  | SProj Spine Name

data VTm
  = VLocal FId Spine
  | VCode VTy
  | VLam Name (VTm -> VTm)
  | VCons [(Name, VTm)]

data VTmJ
  = JTm VTm
  | JLam Name (VTm -> VTmJ)

data VTy
  = VElt VTm
  | VU
  | VPi VTm Name (VTm -> VTy)
  | VRecord [(Name, VTm -> VTy)]

data VTyJ
  = JTy VTy
  | JPi VTy Name (VTm -> VTyJ)
