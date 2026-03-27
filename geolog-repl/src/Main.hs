module Main (main) where

import Control.Monad
import Control.Monad.State.Strict
import Control.Monad.Writer
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
import Geolog.Common
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
        [ ( "load",
            \fp -> eval . newFile fp =<< liftIO (T.readFile fp)
          ),
          ( "list",
            const $ liftIO . putDoc . (<> line) . vcat . map (dpretty . fst) . globalEntries =<< get
          )
        ]
      where
        strip = dropWhileEnd isSpace . dropWhile isSpace
    cmdPrefix = ':'
    multiCmd = "multiline"
    completer =
      Prefix
        ( wordCompleter \s -> do
            names <- gets $ map fst . globalEntries
            let nameStrings = map (\n -> mconcat ((<> "/") . T.unpack <$> n.init) <> T.unpack n.last) names
            pure $ filter (s `isPrefixOf`) $ cmdStrings <> nameStrings
        )
        [ (":load", fileCompleter),
          (":list", \_ -> pure ("", []))
        ]
      where
        cmdStrings = map (cmdPrefix :) $ map fst opts <> [multiCmd]
    start = liftIO $ putStrLn "Welcome to the Geolog REPL!"
    finish = liftIO (putStrLn "Goodbye!") >> pure Exit

eval :: File -> Repl ()
eval file = do
  ntns <- liftIO $ parse parseConfig (reporter ParserCode) file =<< lex lexConfig (reporter LexerCode) file
  let ?diagnosticCtx = DiagnosticCtx {reporter = reporter ElaboratorCode, file}
  ((), newDeclNames) <- runWriterT $ for_ ntns \ntn -> do
    ge <- get
    let ?globalEnv = ge
    case ntn of
      -- register declaration
      Decl name _ _ -> do
        put =<< liftIO (uncurry (insertEntry ge) <$> elabDecl ntn)
        tell [name]
      -- look up name
      Ident name _ -> lift do
        gets (flip lookup name) >>= \case
          Nothing -> liftIO $ putStrLn $ "Not in scope: " <> show name
          Just r -> case r of
            PEntry _ v a -> p v a
            KEntry _ v a -> p v a
            where
              p v a = liftIO $ putDoc $ prtVal mempty a <> line <> prtVal mempty v <> line
      -- evaluate expression
      _ ->
        liftIO $
          synK emptyCtx ntn
            >>= \(v, t) -> putDoc $ prtVal mempty v.val <+> ":" <+> prtVal mempty t <> line
  when (not $ null newDeclNames) $ liftIO $ putStrLn $ show (length newDeclNames) <> " declarations added."
  where
    reporter translator = ReporterFor {translator, reporter = Reporter {handle = stdout, fancy = True}}
