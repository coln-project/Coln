module Main (main) where

import Coln.Common
import Coln.Core.Globals
import Coln.Core.Params (CtxShape (..), N)
import Coln.Core.Print (DPrettyWithNames (dprettyWithNames), prtIn)
import Coln.Core.Realm (Realm)
import Coln.Diagnostics
import Coln.Elaborator.Environment (emptyElabEnv)
import Coln.Elaborator.Judgment (Syn (..))
import Coln.Frontend.Driver (decl', syn)
import Coln.Frontend.Notation
import Coln.Report (DiagnosticEnv (DiagnosticEnv))
import Control.Monad
import Control.Monad.State.Strict
import Control.Monad.Writer
import Data.Bifunctor
import Data.Char
import Data.Foldable
import Data.Function
import Data.Functor
import Data.Functor.Contravariant
import Data.List hiding (lookup)
import Data.Map.Ordered qualified as OMap
import Data.Text qualified as T
import Data.Text.IO qualified as T
import FNotation
import Prettyprinter
import Prettyprinter.Render.Text
import System.Console.Repline
import System.IO
import Prelude hiding (lex, lookup)

type Repl = HaskelineT (StateT Globals IO)

main :: IO ()
main =
  flip evalStateT emptyGlobals $
    evalRepl banner runCmd opts (Just cmdPrefix) (Just multiCmd) completer start finish
 where
  banner = \case
    SingleLine -> pure "coln> "
    MultiLine -> pure "| "
  runCmd = dontCrash . eval . newFile "<interactive>" . (<> "\n") . T.pack
  opts =
    map
      (second \f s -> dontCrash $ f $ strip s)
      [ ("source", \fp -> eval . newFile fp =<< liftIO (T.readFile fp))
      , ("list", const $ liftIO . putDoc . (<+> "\n") . prettyDecls =<< get)
      ]
   where
    strip = dropWhileEnd isSpace . dropWhile isSpace
  cmdPrefix = ':'
  multiCmd = "multiline"
  completer =
    Prefix
      ( wordCompleter \s -> do
          names <- gets $ fmap fst . OMap.assocs . (.entries)
          let nameStrings = map (\n -> mconcat ((<> "/") . T.unpack <$> n.init) <> T.unpack n.last) names
          pure $ filter (s `isPrefixOf`) $ cmdStrings <> nameStrings
      )
      [ (":source", fileCompleter)
      , (":list", \_ -> pure ("", []))
      ]
   where
    cmdStrings = map (cmdPrefix :) $ map fst opts <> [multiCmd]
  start = liftIO $ putStrLn "Welcome to the Coln REPL!"
  finish = liftIO (putStrLn "Goodbye!") >> pure Exit

eval :: File -> Repl ()
eval file = do
  ntns <- liftIO $ parse parseConfig (reporter ParserCode) file =<< lex lexConfig (reporter LexerCode) file
  ((), newDeclNames) <- runWriterT $ for_ ntns \ntn -> do
    ge <- get
    let
      diagEnv = envFor id
    case ntn of
      -- register declaration
      Decl name _ _ -> do
        put =<< liftIO (decl' diagEnv ge ntn)
        tell [name]
      -- ignore realms
      Block "realm" _ _ _ -> pure ()
      -- evaluate expression
      _ ->
        liftIO do
          let elabEnv = emptyElabEnv (envFor ElaboratorCode) ge
          ntnSyn <- (syn @N) diagEnv "" ntn
          (t, m) <- ntnSyn.elab elabEnv
          putDoc $ prtIn elabEnv m <+> ":" <+> prtIn elabEnv t <+> "\n"

  when (not $ null newDeclNames) $ liftIO $ putStrLn $ show (length newDeclNames) <> " declarations added."
 where
  envFor :: (Code a') => (a -> a') -> DiagnosticEnv a
  envFor f = DiagnosticEnv (reporter f) file
  reporter translator = contramap translator $ Reporter{reportIO = putDoc . dpretty}

prettyEntry :: (Name, GlobalEntry) -> DDoc
prettyEntry (x, GlobalEntry t _ a) =
  vsep
    [ "global entry named" <+> dpretty x
    , "type:" <+> prtIn (CtxShape 0 BwdNil) a
    , "value:" <+> dprettyWithNames mempty t
    ]

prettyRealm :: (Name, Realm) -> DDoc
prettyRealm (x, r) =
  vsep
    [ "realm named" <+> dpretty x
    , "generators:" <+> dpretty r
    ]

prettyDecls :: Globals -> DDoc
prettyDecls ge =
  vsep $
    (prettyEntry <$> OMap.assocs ge.entries)
      ++ (prettyRealm <$> OMap.assocs ge.realms)
