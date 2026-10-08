-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE OverloadedStrings #-}

{- | A port of the OCaml @test/EasyExample.ml@: a tiny language exercising the
'Scope' engine with trivial tags\/hooks\/contexts.
-}
module Main (main) where

import Control.Monad.IO.Class (liftIO)
import Data.List (intercalate)
import Data.Text qualified as Text
import System.IO (hPutStrLn, stderr)

import Yclept.Language (Language)
import Yclept.Language qualified as Language
import Yclept.Scope (ScopeT)
import Yclept.Scope qualified as Scope
import Yclept.Trie (Path)
import Yclept.Trie qualified as Trie
import Yclept.Trie.Untagged qualified as UntaggedTrie

data Decl
  = Decl Path Int
  | Import (Trie.Untagged Int) (Language ())
  | Export Path
  | Section Path [Decl]
  | PrintInt Path

type Program = [Decl]

-- data = Int, tag = (), hook = (), context = ().  The base monad is IO.
type M = ScopeT Int () () () IO

ppPath :: Path -> String
ppPath [] = "(root)"
ppPath p = intercalate "." (map Text.unpack p)

interpretDecl :: Decl -> M ()
interpretDecl d =
  case d of
    Decl p x ->
      Scope.includeSingleton Nothing Nothing (p, (x, ()))
    Import t m ->
      Scope.importSubtree Nothing Nothing m ([], Trie.retag () t)
    Export p ->
      Scope.exportVisible Nothing Nothing (Language.only p)
    Section p sec ->
      Scope.section Nothing Nothing Nothing Language.id p (interpretSection sec)
    PrintInt p -> do
      r <- Scope.resolve p
      case r of
        Nothing -> liftIO (hPutStrLn stderr ("Unbound variable " ++ ppPath p))
        Just (i, _) -> liftIO (putStrLn (ppPath p ++ " = " ++ show i))

interpretSection :: [Decl] -> M ()
interpretSection = mapM_ interpretDecl

interpret :: Program -> IO ()
interpret prog = Scope.run Scope.silence (interpretSection prog)

exampleTrie :: Trie.Untagged Int
exampleTrie = UntaggedTrie.ofSeq [(["y"], 20)]

main :: IO ()
main =
  interpret
    [ Decl ["x"] 1
    , Decl ["x"] 2
    , Import exampleTrie (Language.renaming [] ["z"])
    , Export ["z", "y"]
    , Section
        ["w"]
        [ Decl ["a"] 100
        , Export ["x"]
        ]
    , PrintInt ["z", "y"]
    , PrintInt ["w", "a"]
    , PrintInt ["w", "x"]
    ]
