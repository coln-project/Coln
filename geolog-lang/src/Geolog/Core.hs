module Geolog.Core where

import Geolog.Common

data Abs a = Abs Name a

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
