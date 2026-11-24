-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE OverloadedRecordDot #-}
{-# LANGUAGE OverloadedStrings #-}

{- | A port of the OCaml @test/Example.ml@: a tiny language exercising the full
'Scope' engine — the @not_found@ \/ @shadow@ \/ @hook@ handler bundle, tags,
contexts, and 'Scope.tryWith'.
-}
module Main (main) where

import Data.List (intercalate)
import Data.Text qualified as Text

import Yclept.Bwd (Bwd (Emp), toList)
import Yclept.Language (Language)
import Yclept.Language qualified as Language
import Yclept.Scope (Handlers (..), ScopeT)
import Yclept.Scope qualified as Scope
import Yclept.Trie (BwdPath, Path)
import Yclept.Trie qualified as Trie
import Yclept.Trie.Untagged qualified as UntaggedTrie

-- A modifier hook command.
data ModifierCmd = Print

-- Tags distinguishing where a binding came from.
data Tag = Imported | Local

-- Contexts, so handlers can tell which namespace an effect came from.
data Ctx = CVisible | CExport

data Decl
  = -- | a declaration
    Decl Path Int
  | -- | … suppressing the shadow warning
    ShadowingDecl Path Int
  | -- | import a trie after a modifier
    Import (Trie.Untagged Int) (Language ModifierCmd)
  | -- | print all visible bindings
    PrintVisible
  | -- | export a binding
    Export Path
  | -- | a section
    Section Path [Decl]

type Program = [Decl]

-- data = Int, tag = Tag, hook = ModifierCmd, context = Ctx.  Base monad IO.
type M = ScopeT Int Tag ModifierCmd Ctx IO

-- ---------------------------------------------------------------------------
-- Pretty-printing used by the handlers
-- ---------------------------------------------------------------------------

ppPath :: BwdPath -> String
ppPath Emp = "(root)"
ppPath p = intercalate "." (map Text.unpack (toList p))

ppContext :: Maybe Ctx -> String
ppContext (Just CVisible) = " in the visible namespace"
ppContext (Just CExport) = " in the export namespace"
ppContext Nothing = ""

ppItem :: (Int, Tag) -> String
ppItem (x, Imported) = show x ++ " (imported)"
ppItem (x, Local) = show x ++ " (local)"

-- ---------------------------------------------------------------------------
-- The handler bundle
-- ---------------------------------------------------------------------------

handler :: Handlers IO Int Tag ModifierCmd Ctx
handler =
  Handlers
    { notFound = \ctx prefix ->
        putStrLn $
          "[Warning] Could not find any data within the subtree at "
            ++ ppPath prefix
            ++ ppContext ctx
            ++ "."
    , shadow = \ctx path x y -> do
        putStrLn $
          "[Warning] Data "
            ++ ppItem x
            ++ " assigned at "
            ++ ppPath path
            ++ " was shadowed by data "
            ++ ppItem y
            ++ ppContext ctx
            ++ "."
        pure y
    , hook = \ctx prefix Print input -> do
        putStrLn $ "[Info] Got the following bindings at " ++ ppPath prefix ++ ppContext ctx ++ ":"
        mapM_
          (\(pth, x) -> putStrLn ("  " ++ ppPath pth ++ " => " ++ ppItem x))
          (Trie.toSeqWithBwdPaths Emp input)
        pure input
    }

-- ---------------------------------------------------------------------------
-- The interpreter
-- ---------------------------------------------------------------------------

interpretDecl :: Decl -> M ()
interpretDecl d =
  case d of
    Decl p x ->
      Scope.includeSingleton (Just CVisible) (Just CExport) (p, (x, Local))
    ShadowingDecl p x ->
      -- Intercept just the shadow effect, silencing it, and re-perform the rest.
      Scope.tryWith (\hs -> hs{shadow = Scope.silence.shadow}) $
        Scope.includeSingleton Nothing Nothing (p, (x, Local))
    Import t m ->
      Scope.importSubtree Nothing Nothing m ([], UntaggedTrie.tag Imported t)
    PrintVisible ->
      Scope.modifyVisible Nothing (Language.hook Print)
    Export p ->
      Scope.exportVisible Nothing Nothing (Language.only p)
    Section p sec ->
      Scope.section Nothing Nothing Nothing Language.id p (interpretSection sec)

interpretSection :: [Decl] -> M ()
interpretSection = mapM_ interpretDecl

interpret :: Program -> IO ()
interpret prog = Scope.run handler (interpretSection prog)

main :: IO ()
main =
  interpret
    [ Decl ["x"] 1
    , PrintVisible
    , Decl ["x"] 2
    , PrintVisible
    , ShadowingDecl ["x"] 10
    , PrintVisible
    , Import (UntaggedTrie.ofSeq [(["y"], 20)]) (Language.renaming [] ["z"])
    , PrintVisible
    , Export ["z", "y"]
    , Section
        ["w"]
        [ Decl ["a"] 100
        , PrintVisible
        , Export ["x"]
        , Export ["x"]
        ]
    , PrintVisible
    ]
