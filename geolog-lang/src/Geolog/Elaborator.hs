module Geolog.Elaborator where

import Geolog.Common
import Geolog.Core
import Geolog.CoreOperations
import Geolog.Diagnostics
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

typ :: Elab (Maybe Level -> Ntn -> IO (TyG K))
typ (Just l) n = case universeFor l of
  Just u -> decode u <$> chkK (VU u) n
  Nothing -> do
    (g, a) <- synK n
    case a of
      VU u -> decode u g
      _ -> unimplemented

synK :: Elab (Ntn -> IO (ElG K, TyG K))
synK = unimplemented

synP :: Elab (Ntn -> IO (ElG P, TyG K))
synP = unimplemented

chkK :: Elab (TyV K -> Ntn -> IO (ElG K))
chkK = unimplemented

chkP :: Elab (TyV K -> Ntn -> IO (ElG P))
chkP = unimplemented
