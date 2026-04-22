module Test.FNotation.Common where

import Control.Exception
import Data.ByteString.Lazy qualified as LBS
import Data.Functor.Contravariant (contramap, (>$<))
import Data.Map (Map)
import Data.Map qualified as Map
import Data.Text.IO.Utf8 qualified as T
import Data.Text.Lazy qualified as TL
import Data.Text.Lazy.Encoding qualified as TLE
import Data.Vector qualified as V
import Diagnostician
import FNotation
import FNotation.Tokens qualified as K
import Prettyprinter
import Prettyprinter.Render.Text
import System.FilePath (replaceExtension, takeBaseName)
import System.IO
import System.IO.Temp (withSystemTempFile)
import Test.QuickCheck (ioProperty)
import Test.Tasty (TestTree, defaultMain, testGroup)
import Test.Tasty.Golden (findByExtension, goldenVsString)
import Test.Tasty.QuickCheck (testProperty)
import Prelude hiding (lex)

lexConfig :: ConfTable Kind
lexConfig =
  confTableFromList
    [ ("sig", K.Block)
    , ("struct", K.Block)
    , ("sum", K.Block)
    , ("match", K.Block)
    , ("theory", K.Decl)
    , ("def", K.Decl)
    , ("type", K.Decl)
    , ("let", K.Decl)
    , ("open", K.Decl)
    , ("import", K.Decl)
    , ("end", K.End)
    , ("Type", K.AKeyword)
    , ("Int", K.AKeyword)
    , ("String", K.AKeyword)
    , (":=", K.SKeyword)
    , ("=", K.SKeyword)
    , (":", K.SKeyword)
    , ("->", K.SKeyword)
    , ("=>", K.SKeyword)
    ]

parseConfig :: ConfTable Prec
parseConfig =
  confTableFromList
    [ (":=", Prec 10 AssocNon)
    , (":", Prec 20 AssocNon)
    , ("->", Prec 30 AssocR)
    , ("=>", Prec 30 AssocR)
    , ("=", Prec 40 AssocNon)
    , ("+", Prec 50 AssocL)
    , ("-", Prec 50 AssocL)
    , ("*", Prec 60 AssocL)
    , ("/", Prec 60 AssocL)
    ]

