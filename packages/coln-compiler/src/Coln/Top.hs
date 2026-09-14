module Coln.Top
  ( loadRealmsFromFile
  , generateTs
  , generateIr
  , irPretty
  )
where

import Coln.Common
import Coln.Core.Globals
import Coln.Diagnostics (ColnCode)
import Coln.FLIR.Top (sirToFLIR)
import Coln.FLIR.Value qualified as FLIR
import Coln.Frontend.Parser (topFromText)
import Coln.MIR.Top (coreToMIR, interpGlobals)
import Coln.SIR.Top (mirToSIR)
import Coln.SIR.Realm qualified as SIR
import Coln.Backend.TypeScript.Generate (genRealmModule)
import Coln.Backend.TypeScript.Assemble (asm)

import Prettyprinter.Render.Text (renderStrict)

loadRealmsFromFile :: Reporter ColnCode -> File -> IO (OMap Name SIR.Realm)
loadRealmsFromFile r f = do
  globals <- topFromText r f
  let globalEnv = interpGlobals globals
  pure $ fmap (mirToSIR . coreToMIR globalEnv) globals.realms

render :: DDoc -> Text
render = renderStrict . layoutPretty defaultLayoutOptions

generateTs :: Name -> SIR.Realm -> Text
generateTs x = render . asm . genRealmModule x

generateIr :: SIR.Realm -> FLIR.Realm
generateIr = sirToFLIR

irPretty :: FLIR.Realm -> Text
irPretty = render . dpretty
