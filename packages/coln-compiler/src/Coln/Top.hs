module Coln.Top where

import Coln.Common
import Coln.Core.Globals
import Coln.Diagnostics (ColnCode)
import Coln.FLIR.Top
import Coln.Frontend.Parser
import Coln.MIR.Interpret qualified as MIR
import Coln.MIR.Top
import Coln.SIR.Realm qualified as SIR
import Coln.SIR.Top

import Control.Exception
import Data.Aeson qualified as AE
import Data.Foldable (for_)
import Data.Map.Ordered qualified as OMap
import Data.Text qualified as T
import Data.Text.IO qualified as TIO
import Prettyprinter.Render.Text (hPutDoc)
import System.FilePath ((</>))
import System.IO (hPutStrLn, stderr, withFile, pattern WriteMode)

data ExitException = Exit
  deriving (Show, Eq, Ord)

instance Exception ExitException

catchExit :: IO () -> IO ()
catchExit action =
  try action >>= \case
    Right _ -> pure ()
    Left (_ :: ExitException) -> pure ()

loadFile :: FilePath -> IO (Reporter ColnCode, Globals)
loadFile fp =
  try (TIO.readFile fp) >>= \case
    Left (err :: IOError) -> do
      hPutStrLn stderr $ "could not read file " ++ fp ++ " error: " ++ show err
      throw Exit
    Right contents -> do
      let (rep, g) = compile fp contents
      g' <- g
      pure (rep, g')

loadRealms :: FilePath -> IO (Reporter ColnCode, OMap Name SIR.Realm)
loadRealms fp = do
  (rep, g) <- loadFile fp
  let realmsCore = OMap.assocs g.realms
  let globalsMIR = MIR.interpGlobals g
  let realmsMIR = [(rId, coreToMIR globalsMIR rId r) | (rId, r) <- realmsCore]
  let realmsSIR = [(rId, mirToSIR rId r) | (rId, r) <- realmsMIR]
  pure (rep, OMap.fromList realmsSIR)

compile :: FilePath -> T.Text -> (Reporter ColnCode, IO Globals)
compile fp contents = do
  let reporter = fileReporter stderr
  let f = newFile fp contents
  let top = topFromText reporter f
  (reporter, top)

writeFLIR :: FilePath -> Reporter ColnCode -> OMap Name SIR.Realm -> IO ()
writeFLIR fp _ realms = for_ (OMap.assocs realms) $ \(rId, r) -> do
  let flir = sirToFLIR rId r
  let fn = fp </> mangleToString rId <> ".json"
  AE.encodeFile fn flir
  let pn = fp </> mangleToString rId <> ".pretty"
  withFile pn WriteMode $ \h -> hPutDoc h $ dpretty flir
