module Main (main) where

import Control.Monad
import Control.Monad.State.Strict
import Data.Bifunctor
import Data.Char
import Data.Foldable
import Data.Function
import Data.Functor
import Data.List hiding (lookup)
import Data.Text qualified as T
import Data.Text.IO qualified as T
import Diagnostician
import FNotation
import FNotation qualified as N
import Geolog.Core
import Geolog.CoreOperations hiding (eval)
import Geolog.Diagnostics
import Geolog.Elaborator
import Geolog.Notation
import Prettyprinter
import Prettyprinter.Render.Text
import System.Console.Repline
import System.IO
import Prelude hiding (lex, lookup)

type Repl = HaskelineT (StateT GlobalEnv IO)

main :: IO ()
main =
  flip evalStateT emptyGlobalEnv $
    evalRepl banner runCmd opts (Just cmdPrefix) (Just multiCmd) completer start finish
 where
  banner = \case
    SingleLine -> pure "geolog> "
    MultiLine -> pure "| "
  runCmd = dontCrash . eval . newFile "<interactive>" . T.pack
  opts =
    map
      (second \f s -> dontCrash $ f $ strip s)
      [
        ( "load"
        , \fp -> eval . newFile fp =<< liftIO (T.readFile fp)
        )
      ,
        ( "theories"
        , const $ liftIO . putDoc . (<> line) . vcat . map (dpretty . fst) . globalEntries =<< get
        )
      ]
   where
    strip = dropWhileEnd isSpace . dropWhile isSpace
  cmdPrefix = ':'
  multiCmd = "multiline"
  completer =
    Combine
      ( Prefix
          (wordCompleter \_ -> pure [])
          [ (":load", fileCompleter)
          , (":theories", \_ -> pure ("", []))
          ]
      )
      ( Word0 \s -> do
          names <- gets $ map fst . globalEntries
          let nameStrings = map (\n -> mconcat ((<> "/") . T.unpack <$> n.init) <> T.unpack n.last) names
          pure $ filter (s `isPrefixOf`) $ cmdStrings <> nameStrings
      )
   where
    cmdStrings = map (cmdPrefix :) $ map fst opts <> [multiCmd]
  start = liftIO $ putStrLn "Welcome to the Geolog REPL!"
  finish = liftIO (putStrLn "Goodbye!") >> pure Exit

eval :: File -> Repl ()
eval file = do
  ns <- liftIO $ parse parseConfig (reporter ParserCode) file =<< lex lexConfig (reporter LexerCode) file
  let (decls, exprs) =
        ns & partition \case
          N.Decl{} -> True
          _ -> False
  let ?diagnosticCtx = DiagnosticCtx{reporter = reporter ElaboratorCode, file}
  -- register declarations
  ge <-
    liftIO
      . flip (foldlM \ge' n -> let ?globalEnv = ge' in uncurry (insertEntry ge') <$> elabDecl n) decls
      =<< get
  when (not $ null decls) $ liftIO $ putStrLn $ show (length decls) <> " declarations added."
  put ge
  -- evaluate expressions
  forM_ exprs \n ->
    let ?globalEnv = ge
     in liftIO $
          synK emptyCtx n
            >>= \(v, t) -> putDoc $ prtVal mempty v.val <+> ":" <+> prtVal mempty t <> line
 where
  reporter translator = ReporterFor{translator, reporter = Reporter{handle = stdout, fancy = True}}
