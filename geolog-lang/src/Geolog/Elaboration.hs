module Geolog.Elaboration where

import Geolog.Common
import Geolog.Core
import Geolog.Diagnostics
import Geolog.Diagnostics.Code qualified as C
import Geolog.Evaluation
import Geolog.Notation (Ntn)
import Geolog.Notation qualified as N
import Geolog.Pretty

data Ctx = Bwd (TyV K)

type CtxArg = (?ctx :: Ctx)

type Elab a = (CtxArg, CtxLenArg, NamesArg, EnvArg, GlobalEnvArg, ReporterArg) => a

data TyG e = TyG (TyS e) ~(TyV e)

data ElG e = ElG (ElS e) ~(ElV e)

instance Core ElG TyG where
  app (ElG ft fv) (ElG xt xv) = ElG (app ft xt) (app fv xv)
  proj (ElG t v) x = ElG (proj t x) (proj v x)
  code (TyG t v) = ElG (code t) (code v)
  decode u (ElG t v) = TyG (decode u t) (decode u v)
  universe u = TyG (universe u) (universe u)

typ :: Elab (Ntn -> IO (TyG K))
typ = unimplemented

syn :: Elab (Ntn -> IO (ElG K, TyG K))
syn = unimplemented

chk :: Elab (TyV K -> Ntn -> IO (ElG K))
chk = unimplemented
